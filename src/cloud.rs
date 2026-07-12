//! Cloud storage connectors: the [`CloudInput`] builder and the
//! [`OutputTarget`] model, plus the [`provider`] vocabulary.
//!
//! The API imports inputs from and delivers outputs to customer-owned cloud
//! storage. This module models the two wire descriptors:
//!
//! - a **cloud input** — `{ type:"cloud", source, parameters, credentials }` —
//!   built by [`CloudInput`] and handed to
//!   [`Api2Convert::convert_cloud`](crate::Api2Convert::convert_cloud) or
//!   [`JobsResource::add_input`](crate::JobsResource::add_input);
//! - a **cloud output target** — `{ type, parameters, credentials }` —
//!   [`OutputTarget`], attached to a conversion via the `output_targets`
//!   control of [`ConvertOptions`](crate::ConvertOptions) or a raw
//!   `jobs().create` conversion map.
//!
//! [`provider`] is **build-side vocabulary only**: read models keep
//! `source`/`type`/`status` as raw strings, so an unknown provider returned by
//! the server round-trips untyped and never fails to parse.
//!
//! `credentials` ride in the plaintext request body, so both types implement a
//! manual [`std::fmt::Debug`] that masks the whole `credentials` object to
//! `[REDACTED]` and any sensitive `parameters` leaf (see [`crate::redact`]).

use std::fmt;

use serde_json::{json, Map, Value};

use crate::data;
use crate::enums::input_type;
use crate::redact;

/// The cloud storage providers the API can import inputs from and deliver
/// outputs to — the values of a cloud descriptor's `source` (input) / `type`
/// (output) field.
///
/// This is **build-side vocabulary only** (string constants, deliberately not a
/// hard `enum`): read models keep `source`/`type`/`status` as raw strings, so an
/// unknown provider string round-trips untyped and never rejects.
///
/// Import support (a [`CloudInput`] constructor) exists for [`AMAZONS3`], [`AZURE`],
/// [`FTP`] and [`GOOGLECLOUD`]. [`GDRIVE`] and [`YOUTUBE`] are **output-only** (they
/// validate as an output `type` but have no downloader); Google Drive *input*
/// uses the separate `gdrive_picker` input type via the raw
/// [`add_input`](crate::JobsResource::add_input) path.
pub mod provider {
    pub const AMAZONS3: &str = "amazons3";
    pub const AZURE: &str = "azure";
    pub const FTP: &str = "ftp";
    pub const GDRIVE: &str = "gdrive";
    pub const GOOGLECLOUD: &str = "googlecloud";
    pub const YOUTUBE: &str = "youtube";

    /// The full provider vocabulary, in canonical order.
    pub const ALL: [&str; 6] = [AMAZONS3, AZURE, FTP, GDRIVE, GOOGLECLOUD, YOUTUBE];
}

/// A cloud-storage input descriptor: `{ type:"cloud", source, parameters, credentials }`.
///
/// Hand it to [`Api2Convert::convert_cloud`](crate::Api2Convert::convert_cloud) /
/// [`convert_cloud_async`](crate::Api2Convert::convert_cloud_async) as the
/// input, or to [`JobsResource::add_input`](crate::JobsResource::add_input);
/// either way it emits the wire descriptor via [`to_value`](Self::to_value).
/// Like a remote URL, a cloud input is a **started** job, not a staged upload.
///
/// The per-provider constructors carry each provider's required keys
/// **verbatim** — flat and lowercase, exactly as the API expects (`accesskeyid`,
/// not `access_key_id`). Those keys are constructor arguments (structural
/// correctness), **not** a runtime gate: the builder never rejects a descriptor
/// the permissive, asynchronously-validating server would accept. Optional and
/// forward-compat keys go through the fluent [`parameter`](Self::parameter) /
/// [`credential`](Self::credential) setters or the generic [`of`](Self::of)
/// escape hatch.
#[derive(Clone)]
pub struct CloudInput {
    /// The provider, e.g. [`provider::AMAZONS3`]. A raw string — any forward-compat
    /// value is accepted.
    pub source: String,
    /// Non-secret locator keys (`bucket`, `file`, `host`, …).
    pub parameters: Map<String, Value>,
    /// Secret keys (access keys, passwords, tokens). Masked on `Debug`.
    pub credentials: Map<String, Value>,
}

impl CloudInput {
    /// Generic escape hatch: any provider (a [`provider`] constant or a
    /// forward-compat string) with pre-built maps. Chain
    /// [`parameter`](Self::parameter) / [`credential`](Self::credential) to add
    /// keys.
    pub fn of(source: impl Into<String>) -> Self {
        CloudInput {
            source: source.into(),
            parameters: Map::new(),
            credentials: Map::new(),
        }
    }

    /// Import from Amazon S3.
    pub fn amazon_s3(
        bucket: impl Into<String>,
        file: impl Into<String>,
        accesskeyid: impl Into<String>,
        secretaccesskey: impl Into<String>,
    ) -> Self {
        CloudInput {
            source: provider::AMAZONS3.to_string(),
            parameters: obj([("bucket", bucket.into()), ("file", file.into())]),
            credentials: obj([
                ("accesskeyid", accesskeyid.into()),
                ("secretaccesskey", secretaccesskey.into()),
            ]),
        }
    }

    /// Import from Azure Blob Storage.
    pub fn azure(
        container: impl Into<String>,
        file: impl Into<String>,
        accountname: impl Into<String>,
        accountkey: impl Into<String>,
    ) -> Self {
        CloudInput {
            source: provider::AZURE.to_string(),
            parameters: obj([("container", container.into()), ("file", file.into())]),
            credentials: obj([
                ("accountname", accountname.into()),
                ("accountkey", accountkey.into()),
            ]),
        }
    }

    /// Import from an FTP server.
    pub fn ftp(
        host: impl Into<String>,
        file: impl Into<String>,
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Self {
        CloudInput {
            source: provider::FTP.to_string(),
            parameters: obj([("host", host.into()), ("file", file.into())]),
            credentials: obj([("username", username.into()), ("password", password.into())]),
        }
    }

    /// Import from Google Cloud Storage.
    pub fn google_cloud(
        projectid: impl Into<String>,
        bucket: impl Into<String>,
        file: impl Into<String>,
        keyfile: impl Into<String>,
    ) -> Self {
        CloudInput {
            source: provider::GOOGLECLOUD.to_string(),
            parameters: obj([
                ("projectid", projectid.into()),
                ("bucket", bucket.into()),
                ("file", file.into()),
            ]),
            credentials: obj([("keyfile", keyfile.into())]),
        }
    }

    /// Add or replace a `parameters` key (optional / forward-compat locator keys).
    pub fn parameter(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.parameters.insert(key.into(), value.into());
        self
    }

    /// Add or replace a `credentials` key (optional / forward-compat secret keys).
    pub fn credential(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.credentials.insert(key.into(), value.into());
        self
    }

    /// The wire descriptor sent to `POST /jobs` (inline `input`) or
    /// `POST /jobs/{id}/input`.
    pub fn to_value(&self) -> Value {
        json!({
            "type": input_type::CLOUD,
            "source": self.source,
            "parameters": self.parameters,
            "credentials": self.credentials,
        })
    }
}

impl fmt::Debug for CloudInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CloudInput")
            .field("type", &input_type::CLOUD)
            .field("source", &self.source)
            .field("parameters", &redact::parameters(&self.parameters))
            .field("credentials", &redact::MARKER)
            .finish()
    }
}

impl From<CloudInput> for Value {
    fn from(input: CloudInput) -> Value {
        input.to_value()
    }
}

impl From<&CloudInput> for Value {
    fn from(input: &CloudInput) -> Value {
        input.to_value()
    }
}

/// A cloud-storage delivery target for a conversion's output:
/// `{ type, parameters, credentials }`.
///
/// Attach one (or more) to a conversion via the `output_targets` control of
/// [`ConvertOptions`](crate::ConvertOptions) /
/// [`AsyncOptions`](crate::AsyncOptions), or inline in a raw
/// [`jobs().create`](crate::JobsResource::create) conversion map. When any output
/// target is set the conversion delivers straight to your storage and produces
/// **no** local output — so `convert` returns the completed job without
/// downloading.
///
/// This wave ships the **generic** shape only (`type` + free-form
/// `parameters`/`credentials`); per-provider output keys live in a separate
/// service and diverge per provider, so there are no per-provider output
/// constructors yet.
///
/// [`to_value`](Self::to_value) emits `{ type, parameters, credentials }` and
/// **omits `status`** (server-set, read-only). On read
/// ([`from_value`](Self::from_value)) `type`, `parameters` and `status`
/// round-trip as raw values; `credentials` are **never** surfaced (the API
/// returns them empty).
#[derive(Clone)]
pub struct OutputTarget {
    /// The provider, e.g. [`provider::FTP`] — a raw string (an unknown provider
    /// round-trips untyped).
    pub kind: String,
    /// Delivery locator keys (provider-specific).
    pub parameters: Map<String, Value>,
    /// Secret keys — never surfaced on read; masked on `Debug`.
    pub credentials: Map<String, Value>,
    /// Server-set delivery status on read (`waiting|uploading|completed|failed`);
    /// never sent on create.
    pub status: Option<String>,
}

impl OutputTarget {
    /// A generic output target for `kind` (a [`provider`] constant or a
    /// forward-compat string). Chain [`parameter`](Self::parameter) /
    /// [`credential`](Self::credential) to add keys.
    pub fn of(kind: impl Into<String>) -> Self {
        OutputTarget {
            kind: kind.into(),
            parameters: Map::new(),
            credentials: Map::new(),
            status: None,
        }
    }

    /// A generic output target built from pre-assembled `parameters` /
    /// `credentials` maps.
    pub fn new(
        kind: impl Into<String>,
        parameters: Map<String, Value>,
        credentials: Map<String, Value>,
    ) -> Self {
        OutputTarget {
            kind: kind.into(),
            parameters,
            credentials,
            status: None,
        }
    }

    /// Add or replace a `parameters` key.
    pub fn parameter(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.parameters.insert(key.into(), value.into());
        self
    }

    /// Add or replace a `credentials` key.
    pub fn credential(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.credentials.insert(key.into(), value.into());
        self
    }

    /// The wire descriptor sent on create — `{ type, parameters, credentials }`,
    /// with `status` omitted (server-set, read-only).
    pub fn to_value(&self) -> Value {
        json!({
            "type": self.kind,
            "parameters": self.parameters,
            "credentials": self.credentials,
        })
    }

    /// Hydrate from a `GET /jobs/{id}` `output_target[]` element. `type`/`status`
    /// stay raw strings (an unknown provider round-trips untyped);
    /// `credentials` are deliberately not surfaced (left empty).
    pub(crate) fn from_value(v: &Value) -> Self {
        OutputTarget {
            kind: data::string(v.get("type"), ""),
            parameters: data::object(v.get("parameters")),
            credentials: Map::new(),
            status: data::opt_string(v.get("status")),
        }
    }
}

impl fmt::Debug for OutputTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OutputTarget")
            .field("type", &self.kind)
            .field("parameters", &redact::parameters(&self.parameters))
            .field("credentials", &redact::MARKER)
            .field("status", &self.status)
            .finish()
    }
}

/// Build a JSON object map from `(key, string-value)` pairs, in insertion order.
fn obj<const N: usize>(pairs: [(&str, String); N]) -> Map<String, Value> {
    let mut m = Map::new();
    for (k, v) in pairs {
        m.insert(k.to_string(), Value::String(v));
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_vocabulary_in_order() {
        assert_eq!(
            provider::ALL,
            [
                "amazons3",
                "azure",
                "ftp",
                "gdrive",
                "googlecloud",
                "youtube"
            ]
        );
    }

    #[test]
    fn amazon_s3_descriptor_has_flat_lowercase_keys() {
        let v = CloudInput::amazon_s3("my-bucket", "in/photo.png", "AKIA_TEST", "SECRET_TEST")
            .to_value();
        assert_eq!(v["type"], "cloud");
        assert_eq!(v["source"], "amazons3");
        assert_eq!(
            v["parameters"],
            json!({"bucket": "my-bucket", "file": "in/photo.png"})
        );
        assert_eq!(
            v["credentials"],
            json!({"accesskeyid": "AKIA_TEST", "secretaccesskey": "SECRET_TEST"})
        );
    }

    #[test]
    fn per_provider_constructors_carry_keys_verbatim() {
        assert_eq!(
            CloudInput::azure("c", "f", "n", "k").to_value(),
            json!({
                "type": "cloud", "source": "azure",
                "parameters": {"container": "c", "file": "f"},
                "credentials": {"accountname": "n", "accountkey": "k"}
            })
        );
        assert_eq!(
            CloudInput::google_cloud("p", "b", "f", "kf").to_value(),
            json!({
                "type": "cloud", "source": "googlecloud",
                "parameters": {"projectid": "p", "bucket": "b", "file": "f"},
                "credentials": {"keyfile": "kf"}
            })
        );
    }

    #[test]
    fn fluent_setters_carry_forward_compat_keys() {
        let v = CloudInput::amazon_s3("b", "f", "id", "sec")
            .parameter("region", "eu")
            .credential("sessiontoken", "t")
            .to_value();
        assert_eq!(
            v["parameters"],
            json!({"bucket": "b", "file": "f", "region": "eu"})
        );
        assert_eq!(
            v["credentials"],
            json!({"accesskeyid": "id", "secretaccesskey": "sec", "sessiontoken": "t"})
        );
    }

    #[test]
    fn output_target_omits_status_on_serialize_but_hydrates_it_on_read() {
        let created = OutputTarget::of("ftp")
            .parameter("host", "h")
            .credential("username", "u");
        let v = created.to_value();
        assert!(v.get("status").is_none());
        assert_eq!(v["type"], "ftp");

        let read = OutputTarget::from_value(&json!({
            "type": "ftp", "parameters": {"host": "h"}, "credentials": {"x": "y"}, "status": "completed"
        }));
        assert_eq!(read.kind, "ftp");
        assert_eq!(read.status.as_deref(), Some("completed"));
        // credentials are never surfaced on read.
        assert!(read.credentials.is_empty());
    }

    #[test]
    fn debug_masks_credentials_and_sensitive_parameters() {
        let input = CloudInput::amazon_s3("b", "f", "AKIA", "SUPERSECRET123");
        let dbg = format!("{input:?}");
        assert!(!dbg.contains("SUPERSECRET123"));
        assert!(dbg.contains("[REDACTED]"));

        let leaf = CloudInput::of("amazons3")
            .parameter("token", "PARAMSECRET")
            .parameter("bucket", "b");
        let dbg = format!("{leaf:?}");
        assert!(!dbg.contains("PARAMSECRET"));
        assert!(dbg.contains("[REDACTED]"));
        assert!(dbg.contains("bucket"));
    }
}
