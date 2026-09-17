//! What one Expert is asked to do, and what it answers.
//!
//! These are the values the common delegation path carries: an assignment, the
//! context and budget it runs under, and the insights it reports. No Expert's
//! own judgment, prompt or view is named here, and neither is the registry that
//! admitted it.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::time::Instant;
use uuid::Uuid;

use floe_context_contract::DataClass;
use floe_execution::Cancellation;
use floe_kernel::{AgentFailure, PersonId};

use crate::{AgentContext, CapabilityJournal};

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

pub struct ExpertInvocation {
    /// Where this Expert's own capability calls are recorded before they are
    /// dispatched. An Expert never writes its caller's Session state directly.
    pub capabilities: Arc<dyn CapabilityJournal>,
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

/// What the Person's registry admitted this invocation to do.
///
/// An Expert reads its own package identity and the class of data it may
/// handle; which installation, assignment and revision that came from is the
/// registry's own business.
pub struct AdmittedExpert {
    pub package: PackageRef,
    pub data_class: DataClass,
    /// The builtin Expert this assignment runs, when it runs one.
    pub builtin_expert: Option<String>,
    /// The shortest focus window this assignment's own rules ask for, already
    /// reconciled with whatever the request asked for.
    pub focus_minimum_minutes: Option<u16>,
    /// The registry revision the admission was decided against.
    pub registry_revision: u64,
}

/// The assignments an Expert runs under.
///
/// Admitting an invocation and recording that it ran belong to whoever owns the
/// Person's registry. An Expert asks, decides what it was admitted to do, and
/// reports back; it never resolves an assignment itself.
pub trait ExpertAssignments: Sync {
    /// Admit this invocation, reconciling any focus window it asked for with
    /// what the assignment's own rules allow.
    fn admit(
        &self,
        invocation: &ExpertInvocation,
        focus_minutes: Option<u16>,
    ) -> Result<AdmittedExpert, AgentFailure>;

    /// Record that the invocation ran and return the assignment's new state
    /// revision. A result that is already stale, or too large for the budget it
    /// ran under, is refused here rather than stored.
    fn settle(
        &self,
        invocation: &ExpertInvocation,
        admitted: &AdmittedExpert,
        result: &ExpertResult,
    ) -> Result<u64, AgentFailure>;
}
