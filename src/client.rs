//! The [`Api2Convert`] client and its one-call `convert` façade. **Hand-authored.**

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{json, Map, Value};

use crate::config::ClientBuilder;
use crate::convert_options::{AsyncOptions, ConvertOptions};
use crate::errors::Result;
use crate::models::{Job, OutputFile};
use crate::resources::{
    ContractsResource, ConversionsResource, JobsResource, PresetsResource, StatsResource,
};
use crate::result::{ConversionResult, FileDownload};
use crate::transport::Transport;
use crate::webhook::WebhookVerifier;

/// Something to convert: a local file path, a remote URL (`http(s)://…`), an
/// in-memory buffer, or a streaming reader.
///
/// A `&str`/`String` starting with `http://`/`https://` is treated as a URL;
/// any other string is a local path. Use [`Input::reader`] for a stream, or the
/// explicit variants / `From` conversions to remove ambiguity.
pub enum Input {
    Path(PathBuf),
    Url(String),
    Bytes(Vec<u8>),
    Reader(Box<dyn Read + Send>),
}

impl Input {
    /// Wrap a streaming reader (consumed once; a retried upload cannot replay it).
    pub fn reader(r: impl Read + Send + 'static) -> Self {
        Input::Reader(Box::new(r))
    }
}

impl From<&str> for Input {
    fn from(s: &str) -> Self {
        if is_url(s) {
            Input::Url(s.to_string())
        } else {
            Input::Path(PathBuf::from(s))
        }
    }
}

impl From<String> for Input {
    fn from(s: String) -> Self {
        if is_url(&s) {
            Input::Url(s)
        } else {
            Input::Path(PathBuf::from(s))
        }
    }
}

impl From<&Path> for Input {
    fn from(p: &Path) -> Self {
        Input::Path(p.to_path_buf())
    }
}

impl From<PathBuf> for Input {
    fn from(p: PathBuf) -> Self {
        Input::Path(p)
    }
}

impl From<Vec<u8>> for Input {
    fn from(b: Vec<u8>) -> Self {
        Input::Bytes(b)
    }
}

impl From<&[u8]> for Input {
    fn from(b: &[u8]) -> Self {
        Input::Bytes(b.to_vec())
    }
}

fn is_url(s: &str) -> bool {
    let lower = s.trim_start().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

/// The API2Convert client. Cheap to clone (shares one transport).
#[derive(Clone)]
pub struct Api2Convert {
    transport: Arc<Transport>,
}

impl Api2Convert {
    /// Build a client with the given API key and default configuration.
    pub fn new(api_key: impl Into<String>) -> Result<Self> {
        ClientBuilder::new().api_key(api_key).build()
    }

    /// Build a client, taking the API key from the `API2CONVERT_API_KEY`
    /// environment variable.
    pub fn from_env() -> Result<Self> {
        ClientBuilder::new().build()
    }

    /// Start configuring a client.
    pub fn builder() -> ClientBuilder {
        ClientBuilder::new()
    }

    pub(crate) fn from_transport(transport: Arc<Transport>) -> Self {
        Api2Convert { transport }
    }

    #[cfg(test)]
    pub(crate) fn debug_base_url(&self) -> String {
        self.transport.debug_base_url()
    }

    /// The jobs resource (full lifecycle control).
    pub fn jobs(&self) -> JobsResource {
        JobsResource::new(Arc::clone(&self.transport))
    }

    /// The conversions catalog.
    pub fn conversions(&self) -> ConversionsResource {
        ConversionsResource::new(Arc::clone(&self.transport))
    }

    /// The presets resource.
    pub fn presets(&self) -> PresetsResource {
        PresetsResource::new(Arc::clone(&self.transport))
    }

    /// The stats resource.
    pub fn stats(&self) -> StatsResource {
        StatsResource::new(Arc::clone(&self.transport))
    }

    /// The contracts resource.
    pub fn contracts(&self) -> ContractsResource {
        ContractsResource::new(Arc::clone(&self.transport))
    }

    /// A webhook verifier — usable without a configured client.
    pub fn webhooks() -> WebhookVerifier {
        WebhookVerifier
    }

    /// One-call convert: create → (upload | remote input) → start → poll to
    /// completion → return a [`ConversionResult`].
    pub fn convert(&self, input: impl Into<Input>, to: &str) -> Result<ConversionResult> {
        self.convert_with(input, to, ConvertOptions::new())
    }

    /// [`convert`](Self::convert) with extra controls.
    pub fn convert_with(
        &self,
        input: impl Into<Input>,
        to: &str,
        opts: ConvertOptions,
    ) -> Result<ConversionResult> {
        let job = self.start_conversion(
            input.into(),
            to,
            opts.conversion_options.clone(),
            opts.category.as_deref(),
            false,
            None,
            opts.filename.as_deref(),
            opts.download_password.as_deref(),
        )?;
        let job = self.jobs().wait(&job.id, opts.timeout, true)?;
        Ok(ConversionResult::new(
            Arc::clone(&self.transport),
            job,
            opts.output_index,
            opts.download_password,
        ))
    }

    /// Start a conversion without polling; returns once the job is started.
    pub fn convert_async(&self, input: impl Into<Input>, to: &str) -> Result<Job> {
        self.convert_async_with(input, to, AsyncOptions::new())
    }

    /// [`convert_async`](Self::convert_async) with extra controls.
    pub fn convert_async_with(
        &self,
        input: impl Into<Input>,
        to: &str,
        opts: AsyncOptions,
    ) -> Result<Job> {
        self.start_conversion(
            input.into(),
            to,
            opts.conversion_options,
            opts.category.as_deref(),
            true,
            opts.callback.as_deref(),
            opts.filename.as_deref(),
            opts.download_password.as_deref(),
        )
    }

    /// Build a download handle for an output (optionally with a download password).
    pub fn download(&self, output: &OutputFile, download_password: Option<&str>) -> FileDownload {
        FileDownload::new(
            Arc::clone(&self.transport),
            output.clone(),
            download_password.map(|s| s.to_string()),
        )
    }

    /// Discover the valid conversion options for a target.
    pub fn options(&self, target: &str, category: Option<&str>) -> Result<Map<String, Value>> {
        self.conversions().options(target, category)
    }

    #[allow(clippy::too_many_arguments)]
    fn start_conversion(
        &self,
        input: Input,
        to: &str,
        options: Map<String, Value>,
        category: Option<&str>,
        is_async: bool,
        callback: Option<&str>,
        filename: Option<&str>,
        download_password: Option<&str>,
    ) -> Result<Job> {
        let mut conversion = Map::new();
        conversion.insert("target".to_string(), Value::String(to.to_string()));
        if let Some(cat) = category {
            conversion.insert("category".to_string(), Value::String(cat.to_string()));
        }
        if !options.is_empty() {
            conversion.insert("options".to_string(), Value::Object(options));
        }

        let mut payload = Map::new();
        payload.insert(
            "conversion".to_string(),
            Value::Array(vec![Value::Object(conversion)]),
        );
        if is_async {
            if let Some(cb) = callback {
                payload.insert("callback".to_string(), Value::String(cb.to_string()));
                payload.insert("notify_status".to_string(), Value::Bool(true));
            }
        }
        if let Some(pw) = download_password {
            payload.insert(
                "download_passwords".to_string(),
                Value::Array(vec![Value::String(pw.to_string())]),
            );
        }

        match input {
            Input::Url(url) => {
                payload.insert("process".to_string(), Value::Bool(true));
                payload.insert(
                    "input".to_string(),
                    json!([{ "type": "remote", "source": url }]),
                );
                self.jobs().create(Value::Object(payload), None)
            }
            other => {
                payload.insert("process".to_string(), Value::Bool(false));
                let job = self.jobs().create(Value::Object(payload), None)?;
                self.jobs().upload(&job, other, filename)?;
                self.jobs().start(&job.id)
            }
        }
    }
}
