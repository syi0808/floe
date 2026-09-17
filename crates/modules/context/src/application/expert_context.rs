//! The context one Expert run may see, assembled and re-authorized.
//!
//! An Expert may declare that it needs the Person's own tasks and notes; it
//! does not go and get them. Context acquires them, records what it could not
//! read as an issue rather than failing the run, and re-authorizes the whole
//! context against the run's policy before it leaves.

use chrono::{DateTime, Utc};
use floe_agent_contract::{
    AgentContext, AgentFailure, ContextSource, DataClass, InferencePolicyDecision, ModelPlacement,
    SessionProtection,
};
use floe_context_contract::{PersonId, acquire_optional_source, record_source_issue};
use floe_day::TimelineRepository;
use floe_execution::Cancellation;
use tokio::time::Instant;
use uuid::Uuid;

use crate::application::day_context_views::{note_context_view, task_context_view};
use crate::native_context_evidence;

/// How much of the Person's day one Expert run may carry.
const MAX_DAY_CONTEXT_ITEMS: usize = 16;
const MAX_DAY_CONTEXT_BYTES: usize = 8 * 1024;

/// What one Expert run is allowed to see, and how long it may spend getting it.
#[derive(Clone)]
pub struct ExpertContextRequest<'a> {
    pub person_id: PersonId,
    pub policy: &'a InferencePolicyDecision,
    pub placement: ModelPlacement,
    pub protection: SessionProtection,
    pub now: DateTime<Utc>,
    pub deadline: Instant,
    pub cancellation: Cancellation,
}

impl ExpertContextRequest<'_> {
    /// Whether this run may see the Person's own day at all.
    fn carries_personal_day(&self) -> bool {
        self.policy.data_classes.contains(&DataClass::Personal)
    }

    fn authorize(&self, context: &AgentContext) -> Result<(), AgentFailure> {
        self.policy.authorize(
            self.placement,
            self.protection,
            context,
            u64::try_from(self.now.timestamp_millis()).map_err(|_| AgentFailure::StaleContext)?,
        )
    }
}

/// Add the Person's own tasks and notes to an Expert's context.
///
/// Whatever the caller staged is authorized first, so an Expert never adds to a
/// context the run was not allowed to carry in the first place; the day is then
/// acquired and the result authorized again.
pub async fn prepare_expert_context(
    context: &mut AgentContext,
    repository: &impl TimelineRepository,
    request: ExpertContextRequest<'_>,
) -> Result<(), AgentFailure> {
    if request.carries_personal_day() {
        // Whatever the caller staged about the day is re-read here, not reused.
        context.evidence.retain(|evidence| {
            !evidence.source_handle.starts_with("floe.tasks:")
                && !evidence.source_handle.starts_with("floe.notes:")
        });
    }
    request.authorize(context)?;
    if !request.carries_personal_day() {
        return Ok(());
    }
    let tasks = bounded(
        &request,
        acquire_optional_source(
            ContextSource::Tasks,
            task_context_view(
                repository,
                request.person_id,
                day_handle(request.person_id, b"floe.tasks"),
                request.now,
                MAX_DAY_CONTEXT_ITEMS,
                MAX_DAY_CONTEXT_BYTES,
            ),
        ),
    )
    .await?;
    let notes = bounded(
        &request,
        acquire_optional_source(
            ContextSource::Notes,
            note_context_view(
                repository,
                request.person_id,
                day_handle(request.person_id, b"floe.notes"),
                request.now,
                MAX_DAY_CONTEXT_ITEMS,
                MAX_DAY_CONTEXT_BYTES,
            ),
        ),
    )
    .await?;
    record_source_issue(
        &mut context.optional_context_issues,
        ContextSource::Tasks,
        tasks.issue.map(|issue| issue.reason),
    );
    record_source_issue(
        &mut context.optional_context_issues,
        ContextSource::Notes,
        notes.issue.map(|issue| issue.reason),
    );
    for view in [tasks.value, notes.value].into_iter().flatten() {
        if !view.items.is_empty() {
            context.evidence.push(native_context_evidence(&view)?);
        }
    }
    request.authorize(context)
}

/// The handle one of this Person's day views is recorded under.
fn day_handle(person_id: PersonId, name: &[u8]) -> Uuid {
    Uuid::new_v5(&person_id.0, name)
}

async fn bounded<Value>(
    request: &ExpertContextRequest<'_>,
    future: impl Future<Output = Result<Value, AgentFailure>>,
) -> Result<Value, AgentFailure> {
    if request.cancellation.is_cancelled() {
        return Err(AgentFailure::Cancelled);
    }
    if Instant::now() >= request.deadline {
        return Err(AgentFailure::DeadlineExceeded);
    }
    tokio::select! {
        biased;
        _ = request.cancellation.cancelled() => Err(AgentFailure::Cancelled),
        _ = tokio::time::sleep_until(request.deadline) => Err(AgentFailure::DeadlineExceeded),
        result = future => result,
    }
}
