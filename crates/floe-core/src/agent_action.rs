use floe_agent::{DataClass, PackageRef};
use floe_domain::PersonId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentActionOrigin {
    pub schema_version: u32,
    pub instance_id: Uuid,
    pub session_id: Uuid,
    pub invocation_id: Uuid,
    pub assignment_id: Uuid,
    pub package: PackageRef,
    pub view_handle: Uuid,
    pub state_revision: u64,
    pub data_class: DataClass,
    pub automatic: bool,
}

impl AgentActionOrigin {
    pub(crate) fn valid_for(&self, action: &crate::CalendarAction) -> bool {
        self.schema_version == 1
            && self.package.kind == floe_agent::PackageKind::Expert
            && self.invocation_id == action.id
            && self.state_revision > 0
            && !action.direct
            && action.mutation.is_none()
            && matches!(
                (self.data_class, action.provider),
                (DataClass::Synthetic, floe_domain::CalendarProvider::Fixture)
                    | (
                        DataClass::Personal,
                        floe_domain::CalendarProvider::EventKit
                            | floe_domain::CalendarProvider::Android
                            | floe_domain::CalendarProvider::Google
                            | floe_domain::CalendarProvider::Microsoft,
                    )
            )
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertProposalReference {
    pub person_id: PersonId,
    pub session_id: Uuid,
    pub invocation_id: Uuid,
}

#[cfg(unix)]
mod manager {
    use chrono::{DateTime, Utc};
    use floe_agent::{AgentFailure, Cancellation, ExpertResult};
    use floe_domain::{CalendarProvider, TimedSchedule};
    use tokio::time::Instant;

    use super::*;
    use crate::{
        ActionAuthorityMode, ActionBlockReason, ActionFailure, AgentActionEnvelope, CalendarAction,
        CalendarActionPolicy, CalendarActionProvider, CalendarActionState, CoreError,
        EncryptedAgentVault, ErrorCode, FloeCore, VaultKeyProvider,
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

    impl FloeCore {
        pub async fn inspect_expert_calendar_action<Keys: VaultKeyProvider>(
            &self,
            vault: &EncryptedAgentVault<Keys>,
            request: ExpertCalendarInspection,
        ) -> Result<Option<CalendarAction>, AgentFailure> {
            let deadline = request
                .deadline
                .min(Instant::now() + std::time::Duration::from_secs(30));
            let session_id = request.reference.session_id;
            let result = tokio::select! {
                biased;
                _ = request.cancellation.cancelled() => Err(AgentFailure::Cancelled),
                _ = tokio::time::sleep_until(deadline) => Err(AgentFailure::DeadlineExceeded),
                result = vault.with_recorded_expert_proposal(&request.reference, |evidence| async move {
                    if !matches!(evidence.data_class, DataClass::Synthetic | DataClass::Personal) {
                        return Err(AgentFailure::PolicyDenied);
                    }
                    let Some(action) = self.store.bounded_expert_calendar_action(evidence.person_id, evidence.invocation_id).await? else {
                        return Ok(None);
                    };
                    let Some(origin) = action.agent_origin.as_ref() else {
                        return Err(AgentFailure::Conflict);
                    };
                    Self::validate_calendar_source_handle(
                        &evidence.source_handle,
                        evidence.view_handle,
                        action.connection_revision,
                    )?;
                    let proposal = &evidence.action_proposals[0];
                    if !origin.valid_for(&action)
                        || origin.instance_id != evidence.instance_id
                        || origin.session_id != session_id
                        || origin.invocation_id != evidence.invocation_id
                        || origin.assignment_id != evidence.assignment_id
                        || origin.package != evidence.package
                        || origin.view_handle != evidence.view_handle
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

        pub async fn prepare_expert_calendar_action<Keys: VaultKeyProvider>(
            &self,
            vault: &EncryptedAgentVault<Keys>,
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
                result = vault.with_expert_proposal(&request.reference, |evidence| async move {
                    Ok(evidence)
                }) => match result {
                    Ok(evidence) => {
                        self.publish_expert_calendar_action(vault, &request, evidence, &clock).await
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

        pub async fn execute_expert_calendar_action<Keys: VaultKeyProvider>(
            &self,
            vault: &EncryptedAgentVault<Keys>,
            person_id: PersonId,
            execution_id: Uuid,
            policy: &CalendarActionPolicy,
            provider: &impl CalendarActionProvider,
            clock: impl Fn() -> DateTime<Utc>,
        ) -> Result<CalendarAction, AgentFailure> {
            self.execute_expert_calendar_action_with_cancellation(
                vault,
                person_id,
                execution_id,
                policy,
                provider,
                clock,
                Cancellation::default(),
            )
            .await
        }

        pub async fn execute_expert_calendar_action_with_cancellation<Keys: VaultKeyProvider>(
            &self,
            vault: &EncryptedAgentVault<Keys>,
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
            let stored = vault.agent_action_admission(execution_id).await?;
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
                let (_, subject) = self
                    .lease_registry
                    .observation(&stored.envelope.dependency)?;
                provider
                    .validate_source(&action, &stored.envelope.dependency, &subject)
                    .await
                    .map_err(|_| AgentFailure::StaleContext)?;
                Some(subject)
            } else {
                None
            };
            let local_events = self
                .store
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
                let (_, subject) = self
                    .lease_registry
                    .observation(&stored.envelope.dependency)?;
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
            let admission = vault
                .admit_agent_action_dispatch_with_cancellation_and_fence(
                    execution_id,
                    &stored.digest,
                    clock(),
                    cancellation,
                    || {
                        if let Some(expected_subject) = &native_subject {
                            let (_, subject) = self
                                .lease_registry
                                .observation(&stored.envelope.dependency)?;
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
            self.store
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
            let settled = vault.settle_agent_action(&admission, state).await?;
            self.store
                .save_calendar_action(&settled, Some(&executing))
                .await
                .map_err(agent_error)?;
            Ok(settled)
        }

        pub async fn decide_expert_calendar_action<Keys: VaultKeyProvider>(
            &self,
            vault: &EncryptedAgentVault<Keys>,
            person_id: PersonId,
            execution_id: Uuid,
            approve: bool,
            now: DateTime<Utc>,
        ) -> Result<CalendarAction, AgentFailure> {
            let current = vault.agent_action_admission(execution_id).await?;
            if current.envelope.action.person_id != person_id {
                return Err(AgentFailure::NotFound);
            }
            let decided = vault
                .decide_agent_action(execution_id, &current.digest, approve, now)
                .await?;
            self.project_agent_action(person_id, decided.envelope.action)
                .await
        }

        pub async fn cancel_expert_calendar_action<Keys: VaultKeyProvider>(
            &self,
            vault: &EncryptedAgentVault<Keys>,
            person_id: PersonId,
            execution_id: Uuid,
        ) -> Result<CalendarAction, AgentFailure> {
            let current = vault.agent_action_admission(execution_id).await?;
            if current.envelope.action.person_id != person_id {
                return Err(AgentFailure::NotFound);
            }
            let cancelled = vault
                .cancel_agent_action(execution_id, &current.digest)
                .await?;
            self.project_agent_action(person_id, cancelled.envelope.action)
                .await
        }

        pub async fn recover_expert_calendar_action<Keys: VaultKeyProvider>(
            &self,
            vault: &EncryptedAgentVault<Keys>,
            person_id: PersonId,
            execution_id: Uuid,
            provider: &impl CalendarActionProvider,
        ) -> Result<CalendarAction, AgentFailure> {
            let admission = vault.agent_action_admission(execution_id).await?;
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
            let settled = vault.settle_agent_action(&admission, state).await?;
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
            let previous = match self.calendar_action(person_id, action.id).await {
                Ok(previous) => Some(previous),
                Err(error) if error.code == ErrorCode::NotFound => None,
                Err(error) => return Err(agent_error(error)),
            };
            self.store
                .save_calendar_action(&action, previous.as_ref())
                .await
                .map_err(agent_error)?;
            Ok(action)
        }

        async fn publish_expert_calendar_action<Keys: VaultKeyProvider>(
            &self,
            vault: &EncryptedAgentVault<Keys>,
            request: &ExpertCalendarRequest,
            evidence: ExpertResult,
            clock: &impl Fn() -> DateTime<Utc>,
        ) -> Result<CalendarAction, AgentFailure> {
            let destination = &request.destination;
            Self::validate_publish_calendar_source_handle(
                &evidence.source_handle,
                evidence.view_handle,
                destination.connection_revision,
            )?;
            if !matches!(
                (evidence.data_class, destination.provider),
                (DataClass::Synthetic, CalendarProvider::Fixture)
                    | (DataClass::Personal, CalendarProvider::EventKit)
            ) {
                return Err(AgentFailure::PolicyDenied);
            }
            let proposal = &evidence.action_proposals[0];
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
                invocation_id: evidence.invocation_id,
                assignment_id: evidence.assignment_id,
                package: evidence.package,
                view_handle: evidence.view_handle,
                state_revision: evidence.state_revision,
                data_class: evidence.data_class,
                automatic: false,
            };
            let governed_source = evidence.source_handle.starts_with("calendar.timeline:")
                || evidence.source_handle.starts_with("calendar.lease:");
            if governed_source {
                match vault.agent_calendar_action(evidence.invocation_id).await {
                    Ok(existing) => {
                        return matching_action(existing, &origin, destination, &schedule);
                    }
                    Err(AgentFailure::NotFound) => match self
                        .calendar_action(evidence.person_id, evidence.invocation_id)
                        .await
                    {
                        Ok(_) => return Err(AgentFailure::Conflict),
                        Err(error) if error.code == ErrorCode::NotFound => {}
                        Err(error) => return Err(agent_error(error)),
                    },
                    Err(failure) => return Err(failure),
                }
            } else {
                match self
                    .calendar_action(evidence.person_id, evidence.invocation_id)
                    .await
                {
                    Ok(existing) => {
                        return matching_action(existing, &origin, destination, &schedule);
                    }
                    Err(error) if error.code == ErrorCode::NotFound => {}
                    Err(error) => return Err(agent_error(error)),
                }
            }
            let now = clock();
            let source_expiry = timestamp(evidence.expires_at_unix_ms)?;
            if source_expiry <= now {
                return Err(AgentFailure::StaleContext);
            }
            let mut action = self
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
            action.connection_revision = destination.connection_revision;
            let authority = if evidence.source_handle.starts_with("calendar.timeline:")
                || evidence.source_handle.starts_with("calendar.lease:")
            {
                vault.agent_action_policy().await?
            } else {
                self.action_authority(evidence.person_id)
                    .await
                    .map_err(agent_error)?
                    .calendar_create
            };
            action.id = evidence.invocation_id;
            action.execution_id = evidence.invocation_id;
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
            if evidence.source_handle.starts_with("calendar.timeline:")
                || evidence.source_handle.starts_with("calendar.lease:")
            {
                let dependency = vault.expert_proposal_dependency(&request.reference).await?;
                vault
                    .store_agent_action_envelope(AgentActionEnvelope {
                        action: action.clone(),
                        dependency,
                        write_approval: authority == ActionAuthorityMode::Allow,
                    })
                    .await?;
            }
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
            match self.store.save_calendar_action(&action, None).await {
                Ok(()) => Ok(action),
                Err(error) if error.code == ErrorCode::Conflict => {
                    let existing = if governed_source {
                        vault.agent_calendar_action(evidence.invocation_id).await?
                    } else {
                        self.calendar_action(evidence.person_id, evidence.invocation_id)
                            .await
                            .map_err(agent_error)?
                    };
                    matching_action(existing, &origin, destination, &schedule)
                }
                Err(error) => Err(agent_error(error)),
            }
        }

        fn validate_calendar_source_handle(
            source_handle: &str,
            view_handle: Uuid,
            connection_revision: u64,
        ) -> Result<(), AgentFailure> {
            if let Some(revision) = source_handle.strip_prefix("calendar.timeline:") {
                if revision != format!("{}:{}", view_handle, connection_revision) {
                    return Err(AgentFailure::StaleContext);
                }
            } else if let Some(observation_id) = source_handle.strip_prefix("calendar.lease:") {
                Uuid::parse_str(observation_id).map_err(|_| AgentFailure::StaleContext)?;
            }
            Ok(())
        }

        fn validate_publish_calendar_source_handle(
            source_handle: &str,
            view_handle: Uuid,
            connection_revision: u64,
        ) -> Result<(), AgentFailure> {
            Self::validate_calendar_source_handle(source_handle, view_handle, connection_revision)
        }
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

    fn agent_error(error: CoreError) -> AgentFailure {
        match error.code {
            ErrorCode::NotFound => AgentFailure::NotFound,
            ErrorCode::Conflict => AgentFailure::Conflict,
            ErrorCode::Validation | ErrorCode::NoFocusSlot => AgentFailure::InvalidInput,
            ErrorCode::Storage => AgentFailure::StorageUnavailable,
        }
    }
}

#[cfg(unix)]
pub use manager::{ExpertCalendarDestination, ExpertCalendarInspection, ExpertCalendarRequest};

#[cfg(all(test, unix))]
mod tests;
