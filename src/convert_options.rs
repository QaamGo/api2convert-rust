//! Extra controls for [`convert`](crate::Api2Convert::convert_with) /
//! [`convert_async`](crate::Api2Convert::convert_async_with).
//!
//! These named controls are kept **strictly separate** from the open-ended
//! conversion-options map, so an API option key can never collide with an SDK
//! control key — a contract requirement.

use std::time::Duration;

use serde_json::{Map, Value};

/// Controls for a synchronous [`convert`](crate::Api2Convert::convert_with).
#[derive(Default, Clone)]
pub struct ConvertOptions {
    pub(crate) conversion_options: Map<String, Value>,
    pub(crate) category: Option<String>,
    pub(crate) filename: Option<String>,
    pub(crate) download_password: Option<String>,
    pub(crate) output_index: usize,
    pub(crate) timeout: Option<Duration>,
}

impl ConvertOptions {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the whole target-specific conversion-options map (passed 1:1 to the
    /// API's conversion `options`). Discover valid keys via
    /// [`Api2Convert::options`](crate::Api2Convert::options).
    pub fn conversion_options(mut self, options: Map<String, Value>) -> Self {
        self.conversion_options = options;
        self
    }

    /// Add or replace a single conversion option.
    pub fn option(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.conversion_options.insert(key.into(), value.into());
        self
    }

    /// Disambiguate an ambiguous target format.
    pub fn category(mut self, category: impl Into<String>) -> Self {
        self.category = Some(category.into());
        self
    }

    /// The advertised filename for an uploaded local file / stream.
    pub fn filename(mut self, filename: impl Into<String>) -> Self {
        self.filename = Some(filename.into());
        self
    }

    /// Protect every output with a download password. It is remembered on the
    /// returned result and sent automatically on download.
    pub fn download_password(mut self, password: impl Into<String>) -> Self {
        self.download_password = Some(password.into());
        self
    }

    /// Which output file the result selects (default `0`).
    pub fn output_index(mut self, index: usize) -> Self {
        self.output_index = index;
        self
    }

    /// Override the poll timeout for this conversion.
    pub fn timeout(mut self, d: Duration) -> Self {
        self.timeout = Some(d);
        self
    }
}

/// Controls for an asynchronous
/// [`convert_async`](crate::Api2Convert::convert_async_with).
#[derive(Default, Clone)]
pub struct AsyncOptions {
    pub(crate) conversion_options: Map<String, Value>,
    pub(crate) category: Option<String>,
    pub(crate) filename: Option<String>,
    pub(crate) download_password: Option<String>,
    pub(crate) callback: Option<String>,
}

impl AsyncOptions {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the whole target-specific conversion-options map.
    pub fn conversion_options(mut self, options: Map<String, Value>) -> Self {
        self.conversion_options = options;
        self
    }

    /// Add or replace a single conversion option.
    pub fn option(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.conversion_options.insert(key.into(), value.into());
        self
    }

    /// Disambiguate an ambiguous target format.
    pub fn category(mut self, category: impl Into<String>) -> Self {
        self.category = Some(category.into());
        self
    }

    /// The advertised filename for an uploaded local file / stream.
    pub fn filename(mut self, filename: impl Into<String>) -> Self {
        self.filename = Some(filename.into());
        self
    }

    /// Protect every output with a download password (sets the job's
    /// `download_passwords`). A later download must supply it, since the
    /// returned [`Job`](crate::Job) is not a result wrapper.
    pub fn download_password(mut self, password: impl Into<String>) -> Self {
        self.download_password = Some(password.into());
        self
    }

    /// A webhook URL to notify on status change (sets `notify_status: true`).
    pub fn callback(mut self, url: impl Into<String>) -> Self {
        self.callback = Some(url.into());
        self
    }
}
