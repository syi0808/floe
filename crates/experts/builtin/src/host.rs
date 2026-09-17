//! What a builtin Expert may ask of the host that runs it.
//!
//! Each Expert owns which views its judgment needs and how it composes them.
//! Acquiring a view — the grant check, the authorized read, the provenance
//! record — belongs to Access and Context behind this port, and the concrete
//! readers are injected by the composition root. No Expert reaches a source or a
//! model directly.

use std::{future::Future, pin::Pin};

use floe_agent_contract::PersonId;
use floe_agent_contract::{AgentContext, InferencePolicyDecision};
use floe_agent_contract::{AgentFailure, ExpertModel};
use floe_context_contract::{
    AttentionView, AuthorizedRead, CalendarContextView, NativeContextView, PeopleView,
    WellbeingView, WorkContextView,
};
use floe_context_contract::{ContextDependency, MemoryContextSnapshot, SourceGrant};
use floe_execution::Cancellation;
use tokio::time::Instant;
use uuid::Uuid;

use crate::{
    BuiltinContextSource, MailExpertInvocation, PersonalExpertInvocation, PortfolioExpertInvocation,
};
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
        })
    }
}

/// The host an Expert runs inside.
pub trait BuiltinExpertHost: Sync {
    type Model: ExpertModel;

    /// One source read this Expert holds open while it reasons over it.
    type SourceRead: AuthorizedRead;

    /// The model this turn runs on, whatever its placement.
    fn model(&self) -> &Self::Model;

    /// The same model, but only when it is the paired server model. An Expert
    /// whose judgment needs server capability refuses rather than silently
    /// downgrading to the device model.
    fn server_model(&self) -> Option<&Self::Model>;

    /// The same model, but only when it is the on-device model.
    fn device_model(&self) -> Option<&Self::Model>;

    fn policy(&self) -> &InferencePolicyDecision;

    /// Whether this Expert may read the named source right now.
    ///
    /// The decision belongs to whoever holds the setup; the Expert only asks,
    /// and decides for itself what a source it cannot read means for its own
    /// judgment.
    fn source_grant(&self, agent_id: &str, source: BuiltinContextSource) -> SourceGrant;

    /// Whether the named source is readable at all, for an Expert that only
    /// enriches its context when it is.
    fn source_granted(&self, agent_id: &str, source: BuiltinContextSource) -> bool {
        self.source_grant(agent_id, source).is_granted()
    }

    /// Read one authorized remote source view for this Expert.
    fn read_source_view<'a>(
        &'a self,
        request: &'a BuiltinExpertRequest,
        view_id: &'a str,
        query: serde_json::Value,
    ) -> Acquiring<'a, Self::SourceRead>;

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
    ) -> Acquiring<'a, Vec<CalendarContextView>>;

    fn work_context_views<'a>(
        &'a self,
        request: &'a BuiltinExpertRequest,
    ) -> Acquiring<'a, Vec<WorkContextView>>;

    fn people_view<'a>(&'a self, request: &'a BuiltinExpertRequest) -> Acquiring<'a, PeopleView>;

    fn confirmed_interaction_views<'a>(
        &'a self,
        request: &'a BuiltinExpertRequest,
        people: &'a PeopleView,
    ) -> Acquiring<'a, Vec<ConfirmedInteractionView>>;

    fn wellbeing_view<'a>(
        &'a self,
        request: &'a BuiltinExpertRequest,
    ) -> Acquiring<'a, WellbeingView>;

    fn attention_view<'a>(
        &'a self,
        request: &'a BuiltinExpertRequest,
    ) -> Acquiring<'a, (AttentionView, ContextDependency)>;

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
/// Confirmed memories are part of the conversation context, so an Expert that
/// was never granted them must not read them out of the shared context either.
pub fn granted_context<Host: BuiltinExpertHost + ?Sized>(
    host: &Host,
    request: &BuiltinExpertRequest,
) -> AgentContext {
    let mut context = request.context.clone();
    if !host.source_granted(&request.agent_id, BuiltinContextSource::ConfirmedMemory) {
        context.memories.clear();
    }
    context
}

/// Every builtin Expert must hold the one source its judgment cannot do without.
///
/// Each Expert asserts this for itself before it reads anything else; a missing
/// mandatory grant is a denial, not a degraded answer.
pub fn require_mandatory_source<Host: BuiltinExpertHost + ?Sized>(
    host: &Host,
    request: &BuiltinExpertRequest,
) -> Result<(), AgentFailure> {
    let expert = crate::BuiltinExpertKind::from_package_id(&request.agent_id)
        .ok_or(AgentFailure::CapabilityDenied)?;
    match host.source_grant(&request.agent_id, expert.mandatory_source()) {
        SourceGrant::Granted => Ok(()),
        // A source that is bound but down right now is a different answer from
        // one this Expert was never granted, and the Person can act on it.
        SourceGrant::Unavailable => Err(AgentFailure::CapabilityUnavailable),
        SourceGrant::NotConfigured | SourceGrant::Denied => Err(AgentFailure::CapabilityDenied),
    }
}
