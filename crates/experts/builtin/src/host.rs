//! What a builtin Expert may ask of the host that runs it.
//!
//! Each Expert owns which views its judgment needs and how it composes them.
//! Acquiring a view — the grant check, the authorized read, the provenance
//! record — belongs to Access and Context behind this port, and the concrete
//! readers are injected by the composition root. No Expert reaches a source or a
//! model directly.

use std::{future::Future, pin::Pin};

use floe_agent_contract::PersonId;
use floe_agent_contract::{
    AgentContext, Artifact, ArtifactPart, DependencyCoverage, EndpointSettlement,
    InferencePolicyDecision,
};
use floe_agent_contract::{AgentFailure, ExpertModel};
use floe_context_contract::ContextDependency;
use floe_context_contract::SourceReadOutcome;
use floe_context_contract::{AuthorizedRead, CalendarViewQuery, NativeContextView};
use floe_execution::Cancellation;
use tokio::time::Instant;
use uuid::Uuid;

use crate::{MailExpertInvocation, PersonalExpertInvocation, PortfolioExpertInvocation};
use floe_context_contract::CommunicationView;

/// Every builtin Expert runs one bounded model call with the same ceiling.
const MAX_EXPERT_MODEL_TOKENS: u64 = 40_960;
const MAX_EXPERT_MODEL_COST_MICROS: u64 = 50_000;

/// A view acquisition in flight on behalf of one Expert.
pub type Acquiring<'a, Value> =
    Pin<Box<dyn Future<Output = Result<Value, AgentFailure>> + Send + 'a>>;

/// One caller assignment handed to a builtin Expert.
pub struct BuiltinExpertRequest {
    pub agent_id: String,
    pub person_id: PersonId,
    /// The delegated Task result whose source coverage this run records.
    pub task_id: Uuid,
    /// The exact invocation key settled by stateful Experts.
    pub invocation_id: Uuid,
    pub assignment: String,
    pub current_time_unix_ms: i64,
    /// The conversation context this Expert is allowed to see.
    pub context: AgentContext,
    pub context_inputs_available: bool,
    pub staged_task_views: Vec<NativeContextView>,
    pub max_output_bytes: usize,
    pub deadline: Instant,
    pub cancellation: Cancellation,
}

pub struct DeclaredSourceRead<Read> {
    payload: serde_json::Value,
    dependencies: Vec<ContextDependency>,
    held: Option<Read>,
}

impl<Read> DeclaredSourceRead<Read> {
    pub fn new(
        payload: serde_json::Value,
        dependencies: Vec<ContextDependency>,
        held: Option<Read>,
    ) -> Self {
        Self {
            payload,
            dependencies,
            held,
        }
    }

    pub fn payload(&self) -> &serde_json::Value {
        &self.payload
    }

    pub fn dependencies(&self) -> &[ContextDependency] {
        &self.dependencies
    }

    pub fn held(&self) -> Option<&Read> {
        self.held.as_ref()
    }
}

impl BuiltinExpertRequest {
    pub fn nearby_calendar_query(&self) -> Result<CalendarViewQuery, AgentFailure> {
        CalendarViewQuery::try_new(
            self.current_time_unix_ms.saturating_sub(86_400_000),
            self.current_time_unix_ms.saturating_add(86_400_000),
            None,
            floe_context_contract::MAX_CALENDAR_CONTEXT_ITEMS,
        )
    }

    pub fn mail_invocation(
        &self,
        context: AgentContext,
        view: CommunicationView,
    ) -> MailExpertInvocation {
        MailExpertInvocation {
            person_id: self.person_id,
            invocation_id: self.invocation_id,
            assignment: self.assignment.clone(),
            current_time_unix_ms: self.current_time_unix_ms,
            context,
            view,
            max_output_bytes: self.max_output_bytes,
            max_model_tokens: MAX_EXPERT_MODEL_TOKENS,
            max_model_cost_micros: MAX_EXPERT_MODEL_COST_MICROS,
            deadline: self.deadline,
            cancellation: self.cancellation.clone(),
        }
    }

    pub fn portfolio_invocation(&self, context: AgentContext) -> PortfolioExpertInvocation {
        PortfolioExpertInvocation {
            person_id: self.person_id,
            invocation_id: self.invocation_id,
            assignment: self.assignment.clone(),
            current_time_unix_ms: self.current_time_unix_ms,
            context,
            max_output_bytes: self.max_output_bytes,
            max_model_tokens: MAX_EXPERT_MODEL_TOKENS,
            max_model_cost_micros: MAX_EXPERT_MODEL_COST_MICROS,
            deadline: self.deadline,
            cancellation: self.cancellation.clone(),
        }
    }

    pub fn personal_invocation(&self, context: AgentContext) -> PersonalExpertInvocation {
        PersonalExpertInvocation {
            person_id: self.person_id,
            invocation_id: self.invocation_id,
            assignment: self.assignment.clone(),
            current_time_unix_ms: self.current_time_unix_ms,
            context,
            max_output_bytes: self.max_output_bytes,
            max_model_tokens: MAX_EXPERT_MODEL_TOKENS,
            max_model_cost_micros: MAX_EXPERT_MODEL_COST_MICROS,
            deadline: self.deadline,
            cancellation: self.cancellation.clone(),
        }
    }
}

/// What one Expert reports back to the common Task path.
///
/// The Expert names its own result artifact; the common path carries the name
/// it was given rather than deriving one from the agent id.
pub struct BuiltinExpertOutput {
    pub result: String,
    pub artifacts: Vec<Artifact>,
    pub settlement: Option<EndpointSettlement>,
}

pub struct StatefulExpertDraft {
    pub result: String,
    pub artifacts: Vec<Artifact>,
    pub calendar_proposal: Option<floe_actions::ExpertCalendarProposalDraft>,
}

/// Why a mandatory source left an Expert with no judgment to make.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockedExpertStatus {
    NeedsUserAction,
    Unavailable,
}

/// A deterministic blocked-domain report: the status the Manager explains,
/// and no conclusion, no evidence and no model call behind it.
#[derive(serde::Serialize)]
pub struct BlockedExpertResult {
    pub schema_version: u32,
    pub status: BlockedExpertStatus,
    pub summary: String,
}

impl BuiltinExpertOutput {
    pub fn data_part(&self, media_type: &str) -> Option<&str> {
        self.artifacts
            .iter()
            .flat_map(|artifact| &artifact.parts)
            .find_map(|part| match part {
                ArtifactPart::Data {
                    media_type: found,
                    data,
                } if found == media_type => Some(data.as_str()),
                _ => None,
            })
    }

    /// Serialize a package-owned result as an unsettled domain artifact.
    pub fn from_result<Result_: serde::Serialize>(
        artifact_name: &str,
        media_type: &str,
        result_text: String,
        result: &Result_,
    ) -> Result<Self, AgentFailure> {
        Ok(Self {
            result: result_text,
            artifacts: vec![Artifact {
                artifact_id: Uuid::new_v4(),
                name: artifact_name.to_owned(),
                parts: vec![ArtifactPart::Data {
                    media_type: media_type.to_owned(),
                    data: serde_json::to_string(result)
                        .map_err(|_| AgentFailure::InvalidModelOutput)?,
                }],
                coverage: DependencyCoverage::Unknown,
            }],
            settlement: None,
        })
    }

    /// A blocked-domain report for a mandatory source that could not be
    /// read: deterministic, evidence-free, and Task Completed. The trusted
    /// host attaches the durable refs from its own capture; the report
    /// itself proposes no requirement.
    pub fn from_blocked(
        artifact_name: &str,
        media_type: &str,
        status: BlockedExpertStatus,
        summary: String,
    ) -> Result<Self, AgentFailure> {
        if summary.trim().is_empty() || summary.len() > 512 {
            return Err(AgentFailure::InvalidInput);
        }
        Self::from_result(
            artifact_name,
            media_type,
            summary.clone(),
            &BlockedExpertResult {
                schema_version: floe_agent_contract::AGENT_VERSION,
                status,
                summary,
            },
        )
    }

    pub fn with_artifacts(mut self, artifacts: Vec<Artifact>) -> Self {
        self.artifacts.extend(artifacts);
        self
    }

    pub fn with_settlement(mut self, settlement: EndpointSettlement) -> Self {
        self.settlement = Some(settlement);
        self
    }
}

/// The host an Expert runs inside.
pub trait BuiltinExpertHost: Sync {
    type Model: ExpertModel;

    /// One source read this Expert holds open while it reasons over it.
    type SourceRead: AuthorizedRead;

    /// The model this turn runs on. Experts state the execution class they
    /// require on each call; which provider satisfies it is the model
    /// owner's answer, never a host-side selection.
    fn model(&self) -> &Self::Model;

    fn policy(&self) -> &InferencePolicyDecision;

    fn read_requirement<'a>(
        &'a self,
        request: &'a BuiltinExpertRequest,
        key: &'a str,
        query: serde_json::Value,
    ) -> Acquiring<'a, SourceReadOutcome<DeclaredSourceRead<Self::SourceRead>>>;

    /// Record that this Expert's result depends on a source it read.
    fn record_dependency(
        &self,
        turn_id: Uuid,
        result_id: Uuid,
        dependency: ContextDependency,
    ) -> Result<(), AgentFailure>;

    fn settle_stateful_result<'a>(
        &'a self,
        request: &'a BuiltinExpertRequest,
        draft: StatefulExpertDraft,
    ) -> Acquiring<'a, BuiltinExpertOutput>;
}

/// The context one Expert may see.
///
/// Conversation context supplied to the Expert is already authorized context.
/// This is a plain bounded clone; source admission happens at the actual read.
pub fn granted_context<Host: BuiltinExpertHost + ?Sized>(
    _host: &Host,
    request: &BuiltinExpertRequest,
) -> AgentContext {
    request.context.clone()
}
