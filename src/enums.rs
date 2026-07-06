//! String-valued enumerations from the API. Unknown values are preserved as-is
//! (never rejected), so a newly-added status or input type does not break the
//! SDK — forward compatibility is a contract requirement.

/// Job status codes (`status.code`). A job is *terminal* only when
/// `completed`, `failed` or `canceled`; every other code — including any
/// unknown one — is non-terminal.
pub mod job_status {
    pub const CREATED: &str = "created";
    pub const INCOMPLETE: &str = "incomplete";
    pub const DOWNLOADING: &str = "downloading";
    pub const QUEUED: &str = "queued";
    pub const PROCESSING: &str = "processing";
    pub const COMPLETED: &str = "completed";
    pub const FAILED: &str = "failed";
    pub const CANCELED: &str = "canceled";

    /// Whether `code` is a terminal status. Unknown codes are non-terminal.
    pub fn is_terminal(code: &str) -> bool {
        matches!(code, COMPLETED | FAILED | CANCELED)
    }

    /// Whether `code` is an unsuccessful terminal status (`failed`/`canceled`).
    pub fn is_unsuccessful(code: &str) -> bool {
        matches!(code, FAILED | CANCELED)
    }
}

/// Input `type` values. The SDK auto-builds `remote` (URL) and `upload` (local
/// file) inputs; the others are reachable by passing a descriptor to
/// [`crate::JobsResource::add_input`].
pub mod input_type {
    pub const UPLOAD: &str = "upload";
    pub const REMOTE: &str = "remote";
    pub const OUTPUT: &str = "output";
    pub const INPUT_ID: &str = "input_id";
    pub const GDRIVE_PICKER: &str = "gdrive_picker";
    pub const BASE64: &str = "base64";
    pub const CLOUD: &str = "cloud";
}
