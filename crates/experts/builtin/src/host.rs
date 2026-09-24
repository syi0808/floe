//! What a builtin Expert may ask of the host that runs it.
//!
//! Each Expert owns which views its judgment needs and how it composes them.
//! Acquiring a view — the grant check, the authorized read, the provenance
//! record — belongs to Access and Context behind this port, and the concrete
//! readers are injected by the composition root. No Expert reaches a source or a
//! model directly.

use std::{future::Future, pin::Pin};

use floe_agent_contract::DataClass;
use floe_agent_contract::PersonId;
use floe_agent_contract::{AgentContext, Artifact, EndpointSettlement, InferencePolicyDecision};
use floe_agent_contract::{AgentFailure, ExpertInsight, ExpertModel};
use floe_context_contract::SourceReadOutcome;
use floe_context_contract::{
    AttentionView, AuthorizedRead, CalendarContextView, CalendarViewQuery, NativeContextView,
    PeopleView, WellbeingView, WorkContextView,
};
use floe_context_contract::{ContextDependency, MemoryContextSnapshot};
use floe_execution::Cancellation;
use tokio::time::Instant;
use uuid::Uuid;

use crate::{MailExpertInvocation, PersonalExpertInvocation, PortfolioExpertInvocation};
use floe_context_contract::{CommunicationView, ConfirmedInteractionView};

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
    pub max_output_bytes: usize,
    pub deadline: Instant,
    pub cancellation: Cancellation,
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
    pub artifact_name: String,
    pub summary: String,
    pub data: String,
    pub artifacts: Vec<Artifact>,
    pub settlement: Option<EndpointSettlement>,
}

#[derive(serde::Serialize)]
pub struct StatefulFocusProposal {
    pub starts_at_unix_ms: u64,
    pub ends_at_unix_ms: u64,
}

#[derive(serde::Serialize)]
pub struct StatefulExpertDraft {
    pub source_handle: String,
    pub data_class: DataClass,
    pub expires_at_unix_ms: u64,
    pub insights: Vec<ExpertInsight>,
    pub action_proposals: Vec<StatefulFocusProposal>,
    pub summary: String,
    pub model_calls: u32,
    pub view_calls: u32,
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
    /// A serialized Expert result and the summary the Task carries beside it.
    pub fn from_result<Result_: serde::Serialize>(
        artifact_name: &str,
        summary: String,
        result: &Result_,
    ) -> Result<Self, AgentFailure> {
        Ok(Self {
            artifact_name: artifact_name.to_owned(),
            summary,
            data: serde_json::to_string(result).map_err(|_| AgentFailure::InvalidModelOutput)?,
            artifacts: vec![],
            settlement: None,
        })
    }

    /// A blocked-domain report for a mandatory source that could not be
    /// read: deterministic, evidence-free, and Task Completed. The trusted
    /// host attaches the durable refs from its own capture; the report
    /// itself proposes no requirement.
    pub fn from_blocked(
        artifact_name: &str,
        status: BlockedExpertStatus,
        summary: String,
    ) -> Result<Self, AgentFailure> {
        if summary.trim().is_empty() || summary.len() > 512 {
            return Err(AgentFailure::InvalidInput);
        }
        Self::from_result(
            artifact_name,
            summary.clone(),
            &BlockedExpertResult {
                schema_version: floe_agent_contract::AGENT_VERSION,
                status,
                summary,
            },
        )
    }

    pub fn with_artifacts(mut self, artifacts: Vec<Artifact>) -> Self {
        self.artifacts = artifacts;
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

    /// Read one authorized remote source view for this Expert. A
    /// recoverable blocker arrives typed; only hard failures raise.
    fn read_source_view<'a>(
        &'a self,
        request: &'a BuiltinExpertRequest,
        view_id: &'a str,
        query: serde_json::Value,
    ) -> Acquiring<'a, SourceReadOutcome<Self::SourceRead>>;

    /// Record that this Expert's result depends on a source it read.
    fn record_dependency(
        &self,
        turn_id: Uuid,
        result_id: Uuid,
        dependency: ContextDependency,
    ) -> Result<(), AgentFailure>;

    fn calendar_views<'a>(
        &'a self,
        request: &'a BuiltinExpertRequest,
        query: CalendarViewQuery,
    ) -> Acquiring<'a, SourceReadOutcome<Vec<CalendarContextView>>>;

    fn settle_stateful_result<'a>(
        &'a self,
        request: &'a BuiltinExpertRequest,
        draft: StatefulExpertDraft,
    ) -> Acquiring<'a, BuiltinExpertOutput>;

    fn work_context_views<'a>(
        &'a self,
        request: &'a BuiltinExpertRequest,
    ) -> Acquiring<'a, SourceReadOutcome<Vec<WorkContextView>>>;

    fn people_view<'a>(
        &'a self,
        request: &'a BuiltinExpertRequest,
    ) -> Acquiring<'a, SourceReadOutcome<PeopleView>>;

    fn confirmed_interaction_views<'a>(
        &'a self,
        request: &'a BuiltinExpertRequest,
        people: &'a PeopleView,
    ) -> Acquiring<'a, Vec<ConfirmedInteractionView>>;

    fn wellbeing_view<'a>(
        &'a self,
        request: &'a BuiltinExpertRequest,
    ) -> Acquiring<'a, SourceReadOutcome<WellbeingView>>;

    fn attention_view<'a>(
        &'a self,
        request: &'a BuiltinExpertRequest,
    ) -> Acquiring<'a, SourceReadOutcome<(AttentionView, ContextDependency)>>;

    /// Whether this turn can read the Person's own conversation context at all.
    ///
    /// An Expert that would enrich its context skips the enrichment when the
    /// reader is absent, exactly as it does when the source was never granted.
    fn conversation_context_available(&self) -> bool;

    /// The confirmed memories this Person has, for an Expert granted them.
    fn memory_context<'a>(&'a self) -> Acquiring<'a, MemoryContextSnapshot>;

    /// The Person's own task view, for an Expert granted it.
    fn task_view<'a>(&'a self) -> Acquiring<'a, NativeContextView>;

    /// The task views already settled for this turn.
    fn staged_task_views(&self) -> &[NativeContextView];
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
