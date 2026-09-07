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
                    | (DataClass::Personal, floe_domain::CalendarProvider::EventKit)
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
        ActionAuthorityMode, ActionBlockReason, CalendarAction, CalendarActionState, CoreError,
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
                        || (evidence.source_handle.starts_with("calendar.timeline:")
                            && evidence.source_handle != format!("calendar.timeline:{}:{}", evidence.view_handle, action.connection_revision))
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
                result = vault.with_expert_proposal(&request.reference, |evidence| async {
                    self.publish_expert_calendar_action(&request, evidence, &clock).await
                }) => result,
            };
            if request.cancellation.is_cancelled() {
                return Err(AgentFailure::Cancelled);
            }
            if Instant::now() >= request.deadline {
                return Err(AgentFailure::DeadlineExceeded);
            }
            result
        }

        async fn publish_expert_calendar_action(
            &self,
            request: &ExpertCalendarRequest,
            evidence: ExpertResult,
            clock: &impl Fn() -> DateTime<Utc>,
        ) -> Result<CalendarAction, AgentFailure> {
            let destination = &request.destination;
            if evidence.source_handle.starts_with("calendar.timeline:")
                && evidence.source_handle
                    != format!(
                        "calendar.timeline:{}:{}",
                        evidence.view_handle, destination.connection_revision
                    )
            {
                return Err(AgentFailure::StaleContext);
            }
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
            match self
                .calendar_action(evidence.person_id, evidence.invocation_id)
                .await
            {
                Ok(existing) => return matching_action(existing, &origin, destination, &schedule),
                Err(error) if error.code == ErrorCode::NotFound => {}
                Err(error) => return Err(agent_error(error)),
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
                || action.connection_revision != destination.connection_revision
                || connection.revision != destination.connection_revision
                || connection.disconnected
                || connection.error.is_some()
                || connection
                    .source_statuses
                    .get(&destination.calendar_id)
                    .is_some_and(|status| status.error.is_some())
            {
                return Err(AgentFailure::StaleContext);
            }
            let authority = self
                .action_authority(evidence.person_id)
                .await
                .map_err(agent_error)?
                .calendar_create;
            action.id = evidence.invocation_id;
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
            match self.store.save_calendar_action(&action, None).await {
                Ok(()) => Ok(action),
                Err(error) if error.code == ErrorCode::Conflict => {
                    let existing = self
                        .calendar_action(evidence.person_id, evidence.invocation_id)
                        .await
                        .map_err(agent_error)?;
                    matching_action(existing, &origin, destination, &schedule)
                }
                Err(error) => Err(agent_error(error)),
            }
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
