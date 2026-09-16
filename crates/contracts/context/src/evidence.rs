//! What one source view contributes to a context, and how much of it there may be.
//!
//! Evidence is the untrusted text a view yields together with the handle and
//! the class it was authorized under. Its bounds belong beside the views.

use serde::{Deserialize, Serialize};

use crate::DataClass;

pub const MAX_CONTEXT_EVIDENCE: usize = 64;
pub const MAX_CONTEXT_EVIDENCE_BYTES: usize = 32 * 1024;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContextEvidence {
    pub source_handle: String,
    pub data_class: DataClass,
    pub untrusted_text: String,
    pub expires_at_unix_ms: u64,
}
