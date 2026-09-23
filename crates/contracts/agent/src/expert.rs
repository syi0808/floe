//! What one Expert is asked to do, and what it answers.
//!
//! These are the values the common delegation path carries: an assignment, the
//! context and budget it runs under, and the insights it reports. No Expert's
//! own judgment, prompt or view is named here, and neither is the registry that
//! admitted it.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use floe_context_contract::DataClass;
use floe_kernel::PersonId;

/// What a package is, and which one an assignment runs.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageKind {
    Tool,
    Expert,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PackageRef {
    pub kind: PackageKind,
    pub id: String,
    pub version: String,
}

/// The default byte bound a view read may return to an Expert.
pub const MAX_EXPERT_VIEW_BYTES: usize = 65_536;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExpertInput {
    Briefing {
        focus_minutes: u16,
    },
    ProposeFocus {
        focus_minutes: u16,
    },
    Analyze {
        request: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        focus_minutes: Option<u16>,
    },
}

#[derive(Clone, Copy)]
pub struct ExpertBudget {
    pub max_view_calls: u32,
    pub max_view_bytes: usize,
    pub max_output_bytes: usize,
    pub max_insights: usize,
    pub max_model_calls: u32,
    pub max_tool_calls: u32,
    pub max_model_tokens: u64,
    pub max_model_cost_micros: u64,
}

impl Default for ExpertBudget {
    fn default() -> Self {
        Self {
            max_view_calls: 4,
            max_view_bytes: MAX_EXPERT_VIEW_BYTES,
            max_output_bytes: 16384,
            max_insights: 8,
            max_model_calls: 10,
            max_tool_calls: 9,
            max_model_tokens: 40_960,
            max_model_cost_micros: 50_000,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExpertInsight {
    Commitment {
        evidence_handle: Uuid,
        untrusted_title: String,
        starts_at_unix_ms: u64,
        ends_at_unix_ms: u64,
    },
    FocusWindow {
        starts_at_unix_ms: u64,
        ends_at_unix_ms: u64,
    },
    NoFocusWindow,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertFocusProposal {
    pub starts_at_unix_ms: u64,
    pub ends_at_unix_ms: u64,
    pub evidence_id: Uuid,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertResult {
    pub schema_version: u32,
    pub invocation_id: Uuid,
    pub instance_id: Uuid,
    pub person_id: PersonId,
    pub assignment_id: Uuid,
    pub package: PackageRef,
    pub evidence_id: Uuid,
    pub source_handle: String,
    pub data_class: DataClass,
    pub expires_at_unix_ms: u64,
    pub insights: Vec<ExpertInsight>,
    pub action_proposals: Vec<ExpertFocusProposal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default)]
    pub model_calls: u32,
    pub state_revision: u64,
    pub view_calls: u32,
}
