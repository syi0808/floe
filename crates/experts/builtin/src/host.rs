//! What a builtin Expert may ask of the host that runs it.
//!
//! Each Expert owns which views its judgment needs and how it composes them.
//! Acquiring a view — the grant check, the authorized read, the provenance
//! record — belongs to Access and Context behind this port, and the concrete
//! readers are injected by the composition root. No Expert reaches a source or a
//! model directly.

use std::{future::Future, pin::Pin};

use floe_agent_contract::AgentFailure;
use floe_context::{
    AgentContext, AttentionView, CalendarContextView, InferencePolicyDecision, NativeContextView,
    PeopleView, SourceView, WellbeingView, WorkContextView,
};
use floe_context_contract::ContextDependency;
use floe_conversation::{ModelRunner, UsageLedger};
use floe_execution::Cancellation;
use floe_kernel::PersonId;
use tokio::time::Instant;
use uuid::Uuid;

use crate::{
    BuiltinContextSource, ConfirmedInteractionView, MailExpertInvocation,
    PersonalExpertInvocation, PortfolioExpertInvocation, communication::CommunicationView,
};

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
    pub usage: UsageLedger,
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
            usage: self.usage.clone(),
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
            usage: self.usage.clone(),
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
            usage: self.usage.clone(),
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
pub struct BuiltinExpertOutput {
    pub summary: String,
    pub data: String,
}

impl BuiltinExpertOutput {
    /// A serialized Expert result and the summary the Task carries beside it.
    pub fn from_result<Result_: serde::Serialize>(
        summary: String,
        result: &Result_,
    ) -> Result<Self, AgentFailure> {
        Ok(Self {
            summary,
            data: serde_json::to_string(result).map_err(|_| AgentFailure::InvalidModelOutput)?,
        })
    }
}

/// The host an Expert runs inside.
pub trait BuiltinExpertHost: Sync {
    type Model: ModelRunner + Sync;

    /// The model this turn runs on, whatever its placement.
    fn model(&self) -> &Self::Model;

    /// The same model, but only when it is the paired server model. An Expert
    /// whose judgment needs server capability refuses rather than silently
    /// downgrading to the device model.
    fn server_model(&self) -> Option<&Self::Model>;

    /// The same model, but only when it is the on-device model.
    fn device_model(&self) -> Option<&Self::Model>;

    fn policy(&self) -> &InferencePolicyDecision;

    /// Whether this Expert was granted the named source at setup time.
    fn source_granted(&self, agent_id: &str, source: BuiltinContextSource) -> bool;

    /// Read one authorized remote source view for this Expert.
    fn read_source_view<'a>(
        &'a self,
        request: &'a BuiltinExpertRequest,
        view_id: &'a str,
        query: serde_json::Value,
    ) -> Acquiring<'a, SourceView<serde_json::Value>>;

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

    fn wellbeing_view<'a>(&'a self, request: &'a BuiltinExpertRequest)
    -> Acquiring<'a, WellbeingView>;

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
    fn memory_context<'a>(&'a self) -> Acquiring<'a, floe_knowledge::MemoryContextSnapshot>;

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
    host.source_granted(&request.agent_id, expert.mandatory_source())
        .then_some(())
        .ok_or(AgentFailure::CapabilityDenied)
}
