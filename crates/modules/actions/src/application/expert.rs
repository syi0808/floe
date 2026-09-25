use chrono::{DateTime, Utc};
use floe_context_contract::CalendarProvider;
use floe_context_contract::ContextDependency;
use floe_context_contract::DataClass;
use floe_day::{PersonId, TimedSchedule};
use floe_execution::Cancellation;
use floe_kernel::AgentFailure;
use tokio::time::Instant;
use uuid::Uuid;

use crate::{
    ActionAuthorityMode, ActionBlockReason, ActionError, ActionErrorCode, ActionFailure,
    ActionRepository, ActionService, AgentActionEnvelope, AgentActionOrigin, CalendarAction,
    CalendarActionPolicy, CalendarActionProvider, CalendarActionState, ExpertActionStore,
    ExpertProposalReference,
    ExpertCalendarProposal,
};

pub struct ExpertCalendarDestination {
    pub provider: CalendarProvider,
    pub calendar_id: String,
    pub connection_revision: u64,
    pub timezone: String,
}

pub struct ExpertCalendarRequest {
    pub reference: ExpertProposalReference,
    pub destination: ExpertCalendarDestination,
    pub cancellation: Cancellation,
    pub deadline: Instant,
}

pub struct ExpertCalendarInspection {
    pub reference: ExpertProposalReference,
    pub cancellation: Cancellation,
    pub deadline: Instant,
}

/// What the source actually looked like when the action was proposed.
///
/// An external write is only dispatched against the observation it was reviewed
/// against, so the fence answers with the subject fingerprint that observation
/// carried, or refuses.
pub trait ObservationFence: Send + Sync {
    fn observation(&self, dependency: &ContextDependency) -> Result<String, AgentFailure>;
}

/// Expert-originated proposals: inspection, publication, approval, dispatch and
/// the uncertain-result recovery path, all bound to recorded expert evidence.
pub struct ExpertActionService<'a, Repository: ActionRepository + ?Sized, Fence: ObservationFence> {
    pub(crate) repository: &'a Repository,
    pub(crate) fence: &'a Fence,
}

impl<'a, Repository: ActionRepository + ?Sized, Fence: ObservationFence>
    ExpertActionService<'a, Repository, Fence>
{
    pub fn new(repository: &'a Repository, fence: &'a Fence) -> Self {
        Self { repository, fence }
    }

    fn actions(&self) -> ActionService<'_, Repository> {
        ActionService::new(self.repository)
    }

    pub async fn inspect_expert_calendar_action(
        &self,
        store: &impl ExpertActionStore,
        request: ExpertCalendarInspection,
    ) -> Result<Option<CalendarAction>, AgentFailure> {
        let deadline = request
            .deadline
            .min(Instant::now() + std::time::Duration::from_secs(30));
        let session_id = request.reference.session_id;
        let reference = request.reference.clone();
        let result = tokio::select! {
            biased;
            _ = request.cancellation.cancelled() => Err(AgentFailure::Cancelled),
            _ = tokio::time::sleep_until(deadline) => Err(AgentFailure::DeadlineExceeded),
            result = store.with_recorded_expert_proposal(&request.reference, |evidence| async move {
                if !matches!(evidence.data_class, DataClass::Synthetic | DataClass::Personal) {
                    return Err(AgentFailure::PolicyDenied);
                }
                let Some(action) = self
                    .repository
                    .bounded_expert_calendar_action(evidence.person_id, evidence.task_id)
                    .await? else {
                    let dependency = store
                        .expert_proposal_dependency(&reference, &evidence)
                        .await?;
                    if dependency.observation_id() != evidence.evidence_id {
                        return Err(AgentFailure::Conflict);
                    }
                    return Ok(None);
                };
                let Some(origin) = action.agent_origin.as_ref() else {
                    return Err(AgentFailure::Conflict);
                };
                let proposal = &evidence.draft;
                if !origin.valid_for(&action)
                    || origin.instance_id != evidence.instance_id
                    || origin.session_id != session_id
                    || origin.invocation_id != evidence.task_id
                    || origin.assignment_id != evidence.assignment_id
                    || origin.package != evidence.package
                    || origin.evidence_id != evidence.evidence_id
                    || origin.state_revision != evidence.state_revision
                    || origin.data_class != evidence.data_class
                    || action.title != "Focus time"
                    || action.schedule.starts_at != timestamp(proposal.starts_at_unix_ms)?
                    || action.schedule.ends_at != timestamp(proposal.ends_at_unix_ms)?
                    || action.expires_at > timestamp(evidence.expires_at_unix_ms)?
                {
                    return Err(AgentFailure::Conflict);
                }
                Ok(Some(action))
            }) => result,
        };
        if request.cancellation.is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        if Instant::now() >= deadline {
            return Err(AgentFailure::DeadlineExceeded);
        }
        result
    }

    pub async fn prepare_expert_calendar_action(
        &self,
        store: &impl ExpertActionStore,
        mut request: ExpertCalendarRequest,
        clock: impl Fn() -> DateTime<Utc>,
    ) -> Result<CalendarAction, AgentFailure> {
        if request.destination.calendar_id.trim().is_empty()
            || request.destination.calendar_id.len() > 512
            || request.destination.timezone.len() > 128
        {
            return Err(AgentFailure::InvalidInput);
        }
        request.deadline = request
            .deadline
            .min(Instant::now() + std::time::Duration::from_secs(30));
        let result = tokio::select! {
            biased;
            _ = request.cancellation.cancelled() => Err(AgentFailure::Cancelled),
            _ = tokio::time::sleep_until(request.deadline) => Err(AgentFailure::DeadlineExceeded),
            result = store.with_expert_proposal(&request.reference, |evidence| async move {
                Ok(evidence)
            }) => match result {
                Ok(evidence) => {
                    self.publish_expert_calendar_action(store, &request, evidence, &clock).await
                }
                Err(failure) => Err(failure),
            },
        };
        if request.cancellation.is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        if Instant::now() >= request.deadline {
            return Err(AgentFailure::DeadlineExceeded);
        }
        result
    }

    pub async fn execute_expert_calendar_action(
        &self,
        store: &impl ExpertActionStore,
        person_id: PersonId,
        execution_id: Uuid,
        policy: &CalendarActionPolicy,
        provider: &impl CalendarActionProvider,
        clock: impl Fn() -> DateTime<Utc>,
    ) -> Result<CalendarAction, AgentFailure> {
        self.execute_expert_calendar_action_with_cancellation(
            store,
            person_id,
            execution_id,
            policy,
            provider,
            clock,
            Cancellation::default(),
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn execute_expert_calendar_action_with_cancellation(
        &self,
        store: &impl ExpertActionStore,
        person_id: PersonId,
        execution_id: Uuid,
        policy: &CalendarActionPolicy,
        provider: &impl CalendarActionProvider,
        clock: impl Fn() -> DateTime<Utc>,
        cancellation: Cancellation,
    ) -> Result<CalendarAction, AgentFailure> {
        if cancellation.is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        let stored = store.agent_action_admission(execution_id).await?;
        let action = stored.envelope.action.clone();
        if action.person_id != person_id
            || action.state != CalendarActionState::Approved
            || !policy.allow_create
            || policy.person_id != person_id
            || policy.provider != action.provider
            || !policy.allowed_calendar_ids.contains(&action.calendar_id)
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let native_subject = if matches!(
            action.provider,
            CalendarProvider::EventKit | CalendarProvider::Android
        ) {
            let subject = self.fence.observation(&stored.envelope.dependency)?;
            provider
                .validate_source(&action, &stored.envelope.dependency, &subject)
                .await
                .map_err(|_| AgentFailure::StaleContext)?;
            Some(subject)
        } else {
            None
        };
        let local_events = self
            .repository
            .list_events(person_id)
            .await
            .map_err(agent_error)?;
        let preflight = tokio::select! {
            biased;
            _ = cancellation.cancelled() => return Err(AgentFailure::Cancelled),
            result = provider.preflight(&action, &local_events) => result
                .map_err(|_| AgentFailure::CapabilityUnavailable)?,
        };
        if preflight.person_id != person_id
            || preflight.provider != action.provider
            || preflight.calendar_id != action.calendar_id
            || !preflight.permission_granted
            || !preflight.can_create
            || !preflight.timezone_valid
            || preflight.has_conflict
        {
            return Err(AgentFailure::PolicyDenied);
        }
        if let Some(expected_subject) = &native_subject {
            let subject = self.fence.observation(&stored.envelope.dependency)?;
            if &subject != expected_subject {
                return Err(AgentFailure::StaleContext);
            }
            provider
                .validate_source(&action, &stored.envelope.dependency, &subject)
                .await
                .map_err(|_| AgentFailure::StaleContext)?;
        }
        if cancellation.is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        let admission = store
            .admit_agent_action_dispatch_with_cancellation_and_fence(
                execution_id,
                &stored.digest,
                clock(),
                cancellation,
                || {
                    if let Some(expected_subject) = &native_subject {
                        let subject = self.fence.observation(&stored.envelope.dependency)?;
                        if &subject != expected_subject {
                            return Err(AgentFailure::StaleContext);
                        }
                    }
                    Ok(())
                },
            )
            .await?;
        let mut executing = admission.envelope.action.clone();
        executing.state = CalendarActionState::Executing;
        self.repository
            .save_calendar_action(&executing, None)
            .await
            .map_err(agent_error)?;
        let state = match provider.create(&executing).await {
            Ok(receipt) if receipt.matches(&executing) => CalendarActionState::Succeeded {
                external_id: receipt.external_id,
            },
            Ok(_) => CalendarActionState::Unknown {
                reason: ActionFailure::UncertainResult,
            },
            Err(reason) => CalendarActionState::Unknown { reason },
        };
        let settled = store.settle_agent_action(&admission, state).await?;
        self.repository
            .save_calendar_action(&settled, Some(&executing))
            .await
            .map_err(agent_error)?;
        Ok(settled)
    }

    pub async fn decide_expert_calendar_action(
        &self,
        store: &impl ExpertActionStore,
        person_id: PersonId,
        execution_id: Uuid,
        approve: bool,
        now: DateTime<Utc>,
    ) -> Result<CalendarAction, AgentFailure> {
        let current = store.agent_action_admission(execution_id).await?;
        if current.envelope.action.person_id != person_id {
            return Err(AgentFailure::NotFound);
        }
        let decided = store
            .decide_agent_action(execution_id, &current.digest, approve, now)
            .await?;
        self.project_agent_action(person_id, decided.envelope.action)
            .await
    }

    pub async fn cancel_expert_calendar_action(
        &self,
        store: &impl ExpertActionStore,
        person_id: PersonId,
        execution_id: Uuid,
    ) -> Result<CalendarAction, AgentFailure> {
        let current = store.agent_action_admission(execution_id).await?;
        if current.envelope.action.person_id != person_id {
            return Err(AgentFailure::NotFound);
        }
        let cancelled = store
            .cancel_agent_action(execution_id, &current.digest)
            .await?;
        self.project_agent_action(person_id, cancelled.envelope.action)
            .await
    }

    pub async fn recover_expert_calendar_action(
        &self,
        store: &impl ExpertActionStore,
        person_id: PersonId,
        execution_id: Uuid,
        provider: &impl CalendarActionProvider,
    ) -> Result<CalendarAction, AgentFailure> {
        let admission = store.agent_action_admission(execution_id).await?;
        let action = admission.envelope.action.clone();
        if action.person_id != person_id
            || !matches!(
                action.state,
                CalendarActionState::Executing | CalendarActionState::Unknown { .. }
            )
        {
            return Err(AgentFailure::Conflict);
        }
        let state = match provider.lookup(&action).await {
            Ok(receipts) if receipts.len() == 1 && receipts[0].matches(&action) => {
                CalendarActionState::Succeeded {
                    external_id: receipts[0].external_id.clone(),
                }
            }
            Ok(_) => CalendarActionState::Unknown {
                reason: ActionFailure::UncertainResult,
            },
            Err(reason) => CalendarActionState::Unknown { reason },
        };
        let settled = store.settle_agent_action(&admission, state).await?;
        self.project_agent_action(person_id, settled).await
    }

    async fn project_agent_action(
        &self,
        person_id: PersonId,
        action: CalendarAction,
    ) -> Result<CalendarAction, AgentFailure> {
        if action.person_id != person_id {
            return Err(AgentFailure::NotFound);
        }
        let previous = match self.actions().calendar_action(person_id, action.id).await {
            Ok(previous) => Some(previous),
            Err(error) if error.code == ActionErrorCode::NotFound => None,
            Err(error) => return Err(agent_error(error)),
        };
        self.repository
            .save_calendar_action(&action, previous.as_ref())
            .await
            .map_err(agent_error)?;
        Ok(action)
    }

    async fn publish_expert_calendar_action(
        &self,
        store: &impl ExpertActionStore,
        request: &ExpertCalendarRequest,
        evidence: ExpertCalendarProposal,
        clock: &impl Fn() -> DateTime<Utc>,
    ) -> Result<CalendarAction, AgentFailure> {
        let destination = &request.destination;
        evidence.validate()?;
        if !matches!(
            (evidence.data_class, destination.provider),
            (DataClass::Synthetic, CalendarProvider::Fixture)
                | (DataClass::Personal, CalendarProvider::EventKit)
        ) {
            return Err(AgentFailure::PolicyDenied);
        }
        let proposal = &evidence.draft;
        let schedule = TimedSchedule::new(
            timestamp(proposal.starts_at_unix_ms)?,
            timestamp(proposal.ends_at_unix_ms)?,
            &destination.timezone,
        )
        .map_err(|_| AgentFailure::InvalidInput)?;
        let origin = AgentActionOrigin {
            schema_version: 1,
            instance_id: evidence.instance_id,
            session_id: request.reference.session_id,
            invocation_id: evidence.task_id,
            assignment_id: evidence.assignment_id,
            package: evidence.package.clone(),
            evidence_id: evidence.evidence_id,
            state_revision: evidence.state_revision,
            data_class: evidence.data_class,
            automatic: false,
        };
        match store.agent_calendar_action(evidence.task_id).await {
            Ok(existing) => return matching_action(existing, &origin, destination, &schedule),
            Err(AgentFailure::NotFound) => match self
                .actions()
                .calendar_action(evidence.person_id, evidence.task_id)
                .await
            {
                Ok(_) => return Err(AgentFailure::Conflict),
                Err(error) if error.code == ActionErrorCode::NotFound => {}
                Err(error) => return Err(agent_error(error)),
            },
            Err(failure) => return Err(failure),
        }
        let now = clock();
        let source_expiry = timestamp(evidence.expires_at_unix_ms)?;
        if source_expiry <= now {
            return Err(AgentFailure::StaleContext);
        }
        let mut action = self
            .actions()
            .draft_calendar_action(
                evidence.person_id,
                destination.calendar_id.clone(),
                "Focus time".into(),
                schedule.clone(),
                now,
            )
            .await
            .map_err(agent_error)?;
        let connection = self
            .repository
            .calendar_connection(evidence.person_id)
            .await
            .map_err(agent_error)?
            .ok_or(AgentFailure::CapabilityUnavailable)?;
        if action.provider != destination.provider
            || connection.disconnected
            || connection.error.is_some()
            || connection
                .source_statuses
                .get(&destination.calendar_id)
                .is_some_and(|status| status.error.is_some())
        {
            return Err(AgentFailure::StaleContext);
        }
        let context_dependency = store
            .expert_proposal_dependency(&request.reference, &evidence)
            .await?;
        self.fence.observation(&context_dependency)?;
        validate_context_calendar_source(
            &context_dependency,
            &connection,
            &destination.calendar_id,
        )?;
        action.connection_revision = destination.connection_revision;
        let authority = store.agent_action_policy().await?;
        action.id = evidence.task_id;
        action.execution_id = evidence.task_id;
        action.expires_at = action.expires_at.min(source_expiry);
        action.agent_origin = Some(AgentActionOrigin {
            automatic: authority == ActionAuthorityMode::Allow,
            ..origin.clone()
        });
        action.state = match authority {
            ActionAuthorityMode::Ask => CalendarActionState::Pending,
            ActionAuthorityMode::Allow => {
                action.approved_at = Some(now);
                CalendarActionState::Approved
            }
            ActionAuthorityMode::Deny => CalendarActionState::Blocked {
                reason: ActionBlockReason::PolicyDenied,
            },
        };
        let publish_time = clock();
        if publish_time < now || publish_time >= action.expires_at {
            return Err(AgentFailure::StaleContext);
        }
        if request.cancellation.is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        if Instant::now() >= request.deadline {
            return Err(AgentFailure::DeadlineExceeded);
        }
        store
            .store_agent_action_envelope(AgentActionEnvelope {
                action: action.clone(),
                dependency: context_dependency,
                write_approval: authority == ActionAuthorityMode::Allow,
            })
            .await?;
        match self.repository.save_calendar_action(&action, None).await {
            Ok(()) => Ok(action),
            Err(error) if error.code == ActionErrorCode::Conflict => {
                let existing = store.agent_calendar_action(evidence.task_id).await?;
                matching_action(existing, &origin, destination, &schedule)
            }
            Err(error) => Err(agent_error(error)),
        }
    }
}

fn validate_context_calendar_source(
    dependency: &ContextDependency,
    connection: &floe_day::CalendarConnection,
    calendar_id: &str,
) -> Result<(), AgentFailure> {
    if dependency.source().connection_id().as_str() != connection.connection_id
        || dependency.source().execution_owner().as_str() != connection.device_id
        || dependency.source().source_authority() != connection.source_authority
        || connection.disconnected
        || !dependency
            .resources()
            .iter()
            .any(|resource| resource.as_str() == calendar_id)
        || !connection
            .calendars
            .iter()
            .any(|calendar| calendar.calendar_id == calendar_id)
    {
        return Err(AgentFailure::StaleContext);
    }
    Ok(())
}

fn matching_action(
    action: CalendarAction,
    origin: &AgentActionOrigin,
    destination: &ExpertCalendarDestination,
    schedule: &TimedSchedule,
) -> Result<CalendarAction, AgentFailure> {
    let Some(stored_origin) = &action.agent_origin else {
        return Err(AgentFailure::Conflict);
    };
    let expected = AgentActionOrigin {
        automatic: stored_origin.automatic,
        ..origin.clone()
    };
    if stored_origin != &expected
        || action.id != origin.invocation_id
        || action.direct
        || action.mutation.is_some()
        || action.provider != destination.provider
        || action.calendar_id != destination.calendar_id
        || action.connection_revision != destination.connection_revision
        || action.title != "Focus time"
        || &action.schedule != schedule
    {
        return Err(AgentFailure::Conflict);
    }
    Ok(action)
}

fn timestamp(milliseconds: u64) -> Result<DateTime<Utc>, AgentFailure> {
    DateTime::from_timestamp_millis(
        i64::try_from(milliseconds).map_err(|_| AgentFailure::InvalidInput)?,
    )
    .ok_or(AgentFailure::InvalidInput)
}

fn agent_error(error: ActionError) -> AgentFailure {
    AgentFailure::from(error)
}
