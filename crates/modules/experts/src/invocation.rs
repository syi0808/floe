//! Role-neutral Expert invocation and result values.
//!
//! These are the values the common delegation path carries. A specific
//! Expert's judgment, prompts and views live in its own crate.

use serde::{Deserialize, Serialize};
use tokio::time::Instant;
use uuid::Uuid;

use floe_agent_contract::{AgentFailure, DataClass};
use floe_context::AgentContext;
use floe_execution::Cancellation;
use floe_execution::budget::UsageLedger;
use floe_kernel::PersonId;

use crate::PackageRef;

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

pub struct ExpertInvocation {
    pub usage: UsageLedger,
    pub context: AgentContext,
    pub schema_version: u32,
    pub invocation_id: Uuid,
    pub instance_id: Uuid,
    pub person_id: PersonId,
    pub assignment_id: Uuid,
    pub expected_registry_revision: u64,
    pub granted_view_handles: Vec<Uuid>,
    pub allowed_data_classes: Vec<DataClass>,
    pub current_time_unix_ms: u64,
    pub timezone_offset_seconds: i32,
    pub suggested_range_start_unix_ms: Option<u64>,
    pub suggested_range_end_unix_ms: Option<u64>,
    pub input: ExpertInput,
    pub budget: ExpertBudget,
    pub deadline: Instant,
    pub cancellation: Cancellation,
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
    pub view_handle: Uuid,
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
    pub view_handle: Uuid,
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

pub fn check_running(invocation: &ExpertInvocation) -> Result<(), AgentFailure> {
    if invocation.cancellation.is_cancelled() {
        Err(AgentFailure::Cancelled)
    } else if invocation.deadline <= Instant::now() {
        Err(AgentFailure::DeadlineExceeded)
    } else {
        Ok(())
    }
}

pub struct ViewCancellation(pub Cancellation);

impl Drop for ViewCancellation {
    fn drop(&mut self) {
        self.0.cancel();
    }
}
