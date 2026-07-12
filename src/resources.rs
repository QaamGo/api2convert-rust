//! One resource type per API tag. Methods are thin: build the request, call the
//! transport, hydrate a model. **Derived layer** — tracks the OpenAPI spec.

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use crate::client::Input;
use crate::data;
use crate::errors::{Api2ConvertError, Result};
use crate::models::{InputFile, Job, OutputFile, Preset};
use crate::transport::{encode_segment, Transport};
use crate::upload;

const MIN_POLL_INTERVAL: Duration = Duration::from_millis(500);
const MAX_POLL_TIMEOUT: Duration = Duration::from_secs(14400);

/// Full control over the job lifecycle. Most callers only need
/// [`Api2Convert::convert`](crate::Api2Convert::convert), which is built on these.
pub struct JobsResource {
    transport: Arc<Transport>,
}

impl JobsResource {
    pub(crate) fn new(transport: Arc<Transport>) -> Self {
        JobsResource { transport }
    }

    /// Create a job. Pass `{"process": false}` to stage it for uploads, then
    /// [`start`](Self::start) it once inputs are attached. An idempotency key
    /// makes the create retry-safe (sent as `Idempotency-Key`).
    pub fn create(&self, payload: Value, idempotency_key: Option<&str>) -> Result<Job> {
        let v =
            self.transport
                .account_request("POST", "jobs", &[], Some(payload), idempotency_key)?;
        Ok(Job::from_value(&v))
    }

    /// Fetch a job by id.
    pub fn get(&self, job_id: &str) -> Result<Job> {
        let path = format!("jobs/{}", encode_segment(job_id));
        let v = self
            .transport
            .account_request("GET", &path, &[], None, None)?;
        Ok(Job::from_value(&v))
    }

    /// List the current key's jobs (50 per page). An empty `status` lists all.
    pub fn list(&self, status: Option<&str>, page: Option<u32>) -> Result<Vec<Job>> {
        let mut query: Vec<(&str, String)> = vec![("page", page.unwrap_or(1).max(1).to_string())];
        if let Some(s) = status {
            if !s.is_empty() {
                query.push(("status", s.to_string()));
            }
        }
        let v = self
            .transport
            .account_request("GET", "jobs", &query, None, None)?;
        Ok(data::map_objects(Some(&v), Job::from_value))
    }

    /// Patch a job (e.g. `{"process": true}` to start it).
    pub fn update(&self, job_id: &str, payload: Value) -> Result<Job> {
        let path = format!("jobs/{}", encode_segment(job_id));
        let v = self
            .transport
            .account_request("PATCH", &path, &[], Some(payload), None)?;
        Ok(Job::from_value(&v))
    }

    /// Begin processing a staged job (`process: true`).
    pub fn start(&self, job_id: &str) -> Result<Job> {
        self.update(job_id, json!({ "process": true }))
    }

    /// Cancel a job (staged or processing).
    pub fn cancel(&self, job_id: &str) -> Result<()> {
        let path = format!("jobs/{}", encode_segment(job_id));
        self.transport
            .account_request("DELETE", &path, &[], None, None)?;
        Ok(())
    }

    /// Attach an input by descriptor. Pass a raw JSON map, e.g. a remote URL
    /// `add_input(id, json!({"type": "remote", "source": "https://…"}))` or a
    /// Google Drive picker
    /// `add_input(id, json!({"type": "gdrive_picker", "source": "<file-id>", "credentials": {"token": "…"}}))`,
    /// or a [`CloudInput`](crate::CloudInput) builder directly (it converts into
    /// the `{type:"cloud", …}` descriptor).
    pub fn add_input(&self, job_id: &str, descriptor: impl Into<Value>) -> Result<InputFile> {
        let path = format!("jobs/{}/input", encode_segment(job_id));
        let v =
            self.transport
                .account_request("POST", &path, &[], Some(descriptor.into()), None)?;
        Ok(InputFile::from_value(&v))
    }

    /// Upload a local file (path, bytes or a reader) to the job's upload server,
    /// authenticated with the per-job token.
    pub fn upload(
        &self,
        job: &Job,
        file: impl Into<Input>,
        filename: Option<&str>,
    ) -> Result<InputFile> {
        upload::upload(&self.transport, job, file.into(), filename)
    }

    /// The outputs produced by the job (call [`get`](Self::get) or
    /// [`wait`](Self::wait) first).
    pub fn outputs(&self, job_id: &str) -> Result<Vec<OutputFile>> {
        let path = format!("jobs/{}/output", encode_segment(job_id));
        let v = self
            .transport
            .account_request("GET", &path, &[], None, None)?;
        Ok(data::map_objects(Some(&v), OutputFile::from_value))
    }

    /// Poll with backoff until the job reaches a terminal status. Returns
    /// [`Api2ConvertError::ConversionFailed`] on a failed/canceled job (unless
    /// `throw_on_failure` is false) and [`Api2ConvertError::ConversionTimeout`]
    /// past the deadline. `timeout` defaults to the configured poll timeout; the
    /// interval is floored and the total wait capped.
    pub fn wait(
        &self,
        job_id: &str,
        timeout: Option<Duration>,
        throw_on_failure: bool,
    ) -> Result<Job> {
        let timeout = timeout
            .unwrap_or_else(|| self.transport.poll_timeout())
            .min(MAX_POLL_TIMEOUT);
        let max_interval = self.transport.poll_max_interval().max(MIN_POLL_INTERVAL);
        let mut interval = self.transport.poll_interval().max(MIN_POLL_INTERVAL);
        let deadline = Instant::now() + timeout;

        loop {
            let job = self.get(job_id)?;
            if throw_on_failure && (job.is_failed() || job.is_canceled()) {
                return Err(Api2ConvertError::ConversionFailed {
                    errors: job.errors.clone(),
                    job: Box::new(job),
                });
            }
            if job.is_terminal() {
                return Ok(job);
            }
            if Instant::now() >= deadline {
                return Err(Api2ConvertError::ConversionTimeout { job: Box::new(job) });
            }
            self.transport.poll_pause(interval);
            interval = max_interval.min(interval.mul_f64(1.5));
        }
    }
}

/// The conversions catalog (`GET /conversions`) — which targets exist and which
/// options each accepts.
pub struct ConversionsResource {
    transport: Arc<Transport>,
}

impl ConversionsResource {
    pub(crate) fn new(transport: Arc<Transport>) -> Self {
        ConversionsResource { transport }
    }

    /// List supported conversions, optionally filtered by category/target. Each
    /// entry is a raw object `{id, category, target, options}`.
    pub fn list(
        &self,
        category: Option<&str>,
        target: Option<&str>,
        page: Option<u32>,
    ) -> Result<Vec<Value>> {
        let mut query: Vec<(&str, String)> = vec![("page", page.unwrap_or(1).max(1).to_string())];
        if let Some(c) = category {
            if !c.is_empty() {
                query.push(("category", c.to_string()));
            }
        }
        if let Some(t) = target {
            if !t.is_empty() {
                query.push(("target", t.to_string()));
            }
        }
        let v = self
            .transport
            .account_request("GET", "conversions", &query, None, None)?;
        Ok(data::list(Some(&v)))
    }

    /// The option schema (type / enum / default / range) for a single target.
    /// An optional category disambiguates an ambiguous target.
    pub fn options(&self, target: &str, category: Option<&str>) -> Result<Map<String, Value>> {
        let rows = self.list(category, Some(target), None)?;
        match rows.first() {
            Some(row) => Ok(data::object(row.get("options"))),
            None => Ok(Map::new()),
        }
    }
}

/// Manage saved conversion presets (reusable named target + options).
pub struct PresetsResource {
    transport: Arc<Transport>,
}

impl PresetsResource {
    pub(crate) fn new(transport: Arc<Transport>) -> Self {
        PresetsResource { transport }
    }

    /// List presets, optionally filtered by category / target / filter.
    pub fn list(
        &self,
        category: Option<&str>,
        target: Option<&str>,
        filter: Option<&str>,
    ) -> Result<Vec<Preset>> {
        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(c) = category {
            if !c.is_empty() {
                query.push(("category", c.to_string()));
            }
        }
        if let Some(t) = target {
            if !t.is_empty() {
                query.push(("target", t.to_string()));
            }
        }
        if let Some(fl) = filter {
            if !fl.is_empty() {
                query.push(("filter", fl.to_string()));
            }
        }
        let v = self
            .transport
            .account_request("GET", "presets", &query, None, None)?;
        Ok(data::map_objects(Some(&v), Preset::from_value))
    }

    /// Create a preset (`{name, target, options, scope?, category?}`).
    pub fn create(&self, payload: Value) -> Result<Preset> {
        let v = self
            .transport
            .account_request("POST", "presets", &[], Some(payload), None)?;
        Ok(Preset::from_value(&v))
    }

    /// Fetch a preset by id.
    pub fn get(&self, preset_id: &str) -> Result<Preset> {
        let path = format!("presets/{}", encode_segment(preset_id));
        let v = self
            .transport
            .account_request("GET", &path, &[], None, None)?;
        Ok(Preset::from_value(&v))
    }

    /// Patch a preset.
    pub fn update(&self, preset_id: &str, payload: Value) -> Result<Preset> {
        let path = format!("presets/{}", encode_segment(preset_id));
        let v = self
            .transport
            .account_request("PATCH", &path, &[], Some(payload), None)?;
        Ok(Preset::from_value(&v))
    }

    /// Delete a preset.
    pub fn delete(&self, preset_id: &str) -> Result<()> {
        let path = format!("presets/{}", encode_segment(preset_id));
        self.transport
            .account_request("DELETE", &path, &[], None, None)?;
        Ok(())
    }
}

/// Usage statistics (`GET /stats/{period}/{value}/{filter}`). Returns free-form
/// JSON.
pub struct StatsResource {
    transport: Arc<Transport>,
}

impl StatsResource {
    pub(crate) fn new(transport: Arc<Transport>) -> Self {
        StatsResource { transport }
    }

    fn fetch(&self, period: &str, value: &str, filter: Option<&str>) -> Result<Value> {
        let filter = filter.filter(|f| !f.is_empty()).unwrap_or("all");
        let path = format!(
            "stats/{}/{}/{}",
            encode_segment(period),
            encode_segment(value),
            encode_segment(filter)
        );
        self.transport
            .account_request("GET", &path, &[], None, None)
    }

    /// Stats for a day (`YYYY-MM-DD`).
    pub fn day(&self, day: &str, filter: Option<&str>) -> Result<Value> {
        self.fetch("day", day, filter)
    }

    /// Stats for a month (`YYYY-MM`).
    pub fn month(&self, month: &str, filter: Option<&str>) -> Result<Value> {
        self.fetch("month", month, filter)
    }

    /// Stats for a year (`YYYY`).
    pub fn year(&self, year: &str, filter: Option<&str>) -> Result<Value> {
        self.fetch("year", year, filter)
    }
}

/// The account's contract / plan details (`GET /contracts`). Returns free-form JSON.
pub struct ContractsResource {
    transport: Arc<Transport>,
}

impl ContractsResource {
    pub(crate) fn new(transport: Arc<Transport>) -> Self {
        ContractsResource { transport }
    }

    /// Fetch the account contract details.
    pub fn get(&self) -> Result<Value> {
        self.transport
            .account_request("GET", "contracts", &[], None, None)
    }
}
