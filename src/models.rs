//! Typed models for the API's resources. **Derived layer** — tracks
//! `openapi/api2convert.openapi.json`.
//!
//! Every model is hydrated by a `from_value` factory that parses defensively
//! through [`crate::data`]: a missing, `null`, or unexpectedly-typed field
//! degrades to a default instead of failing. Nullable strings map to
//! `Option<String>` and nullable numbers to `Option<i64>` (the idiomatic Rust
//! form; the Go SDK uses `""`/`*int64` only because Go lacks `Option`). [`Job`]
//! keeps the full decoded response in [`Job::raw`] for fields the SDK does not
//! surface as typed accessors.

use serde_json::{Map, Value};

use crate::cloud::OutputTarget;
use crate::data;
use crate::enums::job_status;

/// A job's status: a `code` (see [`crate::enums::job_status`]) and optional
/// human-readable `info`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub code: String,
    pub info: Option<String>,
}

impl Status {
    pub(crate) fn from_value(v: &Value) -> Self {
        Status {
            code: data::string(v.get("code"), ""),
            info: data::opt_string(v.get("info")),
        }
    }
}

/// One conversion within a job.
#[derive(Debug, Clone)]
pub struct Conversion {
    pub target: String,
    pub id: Option<String>,
    pub category: Option<String>,
    pub options: Map<String, Value>,
    pub metadata: Map<String, Value>,
    /// Cloud delivery targets for this conversion's output, if any. Empty for a
    /// conversion with a local (downloadable) output. `credentials` are never
    /// surfaced; `type`/`parameters`/`status` round-trip raw.
    pub output_target: Vec<OutputTarget>,
}

impl Conversion {
    pub(crate) fn from_value(v: &Value) -> Self {
        Conversion {
            target: data::string(v.get("target"), ""),
            id: data::opt_string(v.get("id")),
            category: data::opt_string(v.get("category")),
            options: data::object(v.get("options")),
            metadata: data::object(v.get("metadata")),
            output_target: data::map_objects(v.get("output_target"), OutputTarget::from_value),
        }
    }
}

/// An input file of a job.
#[derive(Debug, Clone)]
pub struct InputFile {
    pub id: Option<String>,
    /// The API input `type` (see [`crate::enums::input_type`]).
    pub kind: String,
    pub source: Option<String>,
    pub status: Option<String>,
    pub filename: Option<String>,
    pub size: Option<i64>,
    pub content_type: Option<String>,
    pub options: Map<String, Value>,
    /// Cloud-input locator keys (`bucket`, `file`, `host`, …) surfaced on read;
    /// empty for a non-cloud input.
    pub parameters: Map<String, Value>,
}

impl InputFile {
    pub(crate) fn from_value(v: &Value) -> Self {
        InputFile {
            id: data::opt_string(v.get("id")),
            kind: data::string(v.get("type"), ""),
            source: data::opt_string(v.get("source")),
            status: data::opt_string(v.get("status")),
            filename: data::opt_string(v.get("filename")),
            size: data::opt_i64(v.get("size")),
            content_type: data::opt_string(v.get("content_type")),
            options: data::object(v.get("options")),
            parameters: data::object(v.get("parameters")),
        }
    }
}

/// An output file of a job. `uri` is a self-contained download URL (no auth,
/// finite TTL); protect it with a download password if the job set one.
#[derive(Debug, Clone)]
pub struct OutputFile {
    pub id: Option<String>,
    pub uri: String,
    pub filename: Option<String>,
    pub size: Option<i64>,
    pub status: Option<String>,
    pub content_type: Option<String>,
    pub checksum: Option<String>,
    pub metadata: Map<String, Value>,
}

impl OutputFile {
    pub(crate) fn from_value(v: &Value) -> Self {
        OutputFile {
            id: data::opt_string(v.get("id")),
            uri: data::string(v.get("uri"), ""),
            filename: data::opt_string(v.get("filename")),
            size: data::opt_i64(v.get("size")),
            status: data::opt_string(v.get("status")),
            content_type: data::opt_string(v.get("content_type")),
            checksum: data::opt_string(v.get("checksum")),
            metadata: data::object(v.get("metadata")),
        }
    }

    /// Construct an output referring to a bare URL, for downloading a URI you
    /// already hold (e.g. from a webhook payload) without a full job.
    pub fn with_uri(uri: impl Into<String>) -> Self {
        OutputFile {
            id: None,
            uri: uri.into(),
            filename: None,
            size: None,
            status: None,
            content_type: None,
            checksum: None,
            metadata: Map::new(),
        }
    }
}

/// An entry of a job's `errors[]` / `warnings[]`.
#[derive(Debug, Clone)]
pub struct JobMessage {
    pub code: Option<i64>,
    pub message: String,
    pub source: Option<String>,
    pub id_source: Option<String>,
    pub details: Map<String, Value>,
}

impl JobMessage {
    pub(crate) fn from_value(v: &Value) -> Self {
        JobMessage {
            code: data::opt_i64(v.get("code")),
            message: data::string(v.get("message"), ""),
            source: data::opt_string(v.get("source")),
            id_source: data::opt_string(v.get("id_source")),
            details: data::object(v.get("details")),
        }
    }
}

/// A saved conversion preset.
#[derive(Debug, Clone)]
pub struct Preset {
    pub id: Option<String>,
    pub name: String,
    pub target: Option<String>,
    pub category: Option<String>,
    pub scope: Option<String>,
    pub options: Map<String, Value>,
}

impl Preset {
    pub(crate) fn from_value(v: &Value) -> Self {
        Preset {
            id: data::opt_string(v.get("id")),
            name: data::string(v.get("name"), ""),
            target: data::opt_string(v.get("target")),
            category: data::opt_string(v.get("category")),
            scope: data::opt_string(v.get("scope")),
            options: data::object(v.get("options")),
        }
    }
}

/// A conversion job — the central resource. Create it, (upload | add a remote
/// input), start it, poll it to a terminal status, then download its outputs.
#[derive(Debug, Clone)]
pub struct Job {
    pub id: String,
    pub status: Status,
    /// Per-job upload token (`X-Api2convert-Token`); never the account key.
    pub token: Option<String>,
    /// Per-job upload server base URL.
    pub server: Option<String>,
    pub callback: Option<String>,
    pub conversion: Vec<Conversion>,
    pub input: Vec<InputFile>,
    pub output: Vec<OutputFile>,
    pub errors: Vec<JobMessage>,
    pub warnings: Vec<JobMessage>,
    /// The full decoded response, for fields not surfaced as typed accessors.
    pub raw: Value,
}

impl Job {
    pub(crate) fn from_value(v: &Value) -> Self {
        Job {
            id: data::string(v.get("id"), ""),
            status: Status::from_value(v.get("status").unwrap_or(&Value::Null)),
            token: data::opt_string(v.get("token")),
            server: data::opt_string(v.get("server")),
            callback: data::opt_string(v.get("callback")),
            conversion: data::map_objects(v.get("conversion"), Conversion::from_value),
            input: data::map_objects(v.get("input"), InputFile::from_value),
            output: data::map_objects(v.get("output"), OutputFile::from_value),
            errors: data::map_objects(v.get("errors"), JobMessage::from_value),
            warnings: data::map_objects(v.get("warnings"), JobMessage::from_value),
            raw: v.clone(),
        }
    }

    /// Whether the job finished successfully.
    pub fn is_completed(&self) -> bool {
        self.status.code == job_status::COMPLETED
    }

    /// Whether the job failed.
    pub fn is_failed(&self) -> bool {
        self.status.code == job_status::FAILED
    }

    /// Whether the job was canceled.
    pub fn is_canceled(&self) -> bool {
        self.status.code == job_status::CANCELED
    }

    /// Whether the job reached a terminal status (`completed`/`failed`/`canceled`).
    pub fn is_terminal(&self) -> bool {
        job_status::is_terminal(&self.status.code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn job_hydrates_and_reports_terminal() {
        let v = json!({
            "id": "job1",
            "status": {"code": "completed"},
            "output": [{"uri": "https://cdn/f.png", "filename": "f.png", "size": 123}]
        });
        let job = Job::from_value(&v);
        assert_eq!(job.id, "job1");
        assert!(job.is_completed());
        assert!(job.is_terminal());
        assert_eq!(job.output.len(), 1);
        assert_eq!(job.output[0].size, Some(123));
        assert!(job.token.is_none());
    }

    #[test]
    fn unknown_status_is_not_terminal() {
        let job = Job::from_value(&json!({"id": "j", "status": {"code": "some_new_state"}}));
        assert!(!job.is_terminal());
        assert!(!job.is_completed());
    }

    #[test]
    fn missing_fields_do_not_panic() {
        let job = Job::from_value(&json!({}));
        assert_eq!(job.id, "");
        assert_eq!(job.status.code, "");
        assert!(job.output.is_empty());
        assert!(!job.is_terminal());
    }
}
