//! The concrete device, connection store and registry a calendar access change
//! runs against.
//!
//! Every judgment here belongs to an owner: Experts decides what the change
//! means, Context orders the acquisition, Access admits the source. What this
//! file supplies is the wiring — a device probe, the Person's calendar
//! connection, and their encrypted registry.

use std::{future::Future, pin::Pin, time::Duration};

use floe_access::{ExecutionOwnerId, GrantSourceBinding, RemoteCallWindow};
use floe_agent_contract::AgentFailure;
use floe_context::{
    CalendarConnectionReader, NativeCalendarGrantReader, NativeCalendarSourceRequest,
    NativeCalendarSubjectSource, NativeSubjectObservation, NativeSubjectRequest,
    preview_native_calendar_subject,
};
use floe_execution::Cancellation;
use floe_kernel::PersonId;
use floe_vault::{EncryptedAgentVault, VaultKeyProvider};

use crate::FloeCore;
use crate::local_context::LocalContextHost;
use crate::{
    ConnectionObserveExpectation, ConnectionObserveMember, ConnectionObserveOperation,
    ConnectionObserveOverview, ConnectionObserveReviewedMember, ConnectionObserveStatus,
};

/// How long the device is given to answer for its own calendar subject.
const SUBJECT_DEADLINE: Duration = Duration::from_secs(30);

/// The window one device probe must finish inside.
pub(crate) fn subject_window(cancellation: Cancellation) -> RemoteCallWindow {
    RemoteCallWindow {
        deadline: tokio::time::Instant::now() + SUBJECT_DEADLINE,
        cancellation,
    }
}

/// The Person's calendar connection, as this process stores it.
pub(super) struct CoreCalendarConnections<'a> {
    pub core: &'a FloeCore,
    pub person_id: PersonId,
}

impl CalendarConnectionReader for CoreCalendarConnections<'_> {
    async fn calendar_connection(
        &self,
    ) -> Result<Option<floe_connections::SourceConnection>, AgentFailure> {
        let connector = floe_context_contract::ConnectorId::try_new("calendar.event_kit")
            .map_err(|_| AgentFailure::InvalidInput)?;
        let mut sources = self
            .core
            .source_service()
            .list_current(self.person_id, &connector)
            .await
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        if sources.len() > 1 {
            return Err(AgentFailure::Conflict);
        }
        Ok(sources.pop())
    }
}

pub(super) struct VaultNativeCalendarGrants<'a, Keys: VaultKeyProvider> {
    pub vault: &'a EncryptedAgentVault<Keys>,
}

impl<Keys: VaultKeyProvider> NativeCalendarGrantReader for VaultNativeCalendarGrants<'_, Keys> {
    async fn admit(
        &self,
        connection: &floe_connections::SourceConnection,
        person_id: PersonId,
        consumer: &str,
    ) -> Result<floe_access::CalendarReadAccessAdmission, AgentFailure> {
        if person_id != self.vault.person_id() {
            return Err(AgentFailure::CapabilityDenied);
        }
        let consumer = floe_access::GrantConsumer::builtin(consumer)
            .map_err(|_| AgentFailure::CapabilityDenied)?;
        let admission = self
            .vault
            .authorize_current_native_calendar_grant(
                connection.connection_id().as_str(),
                floe_context_contract::CalendarProvider::EventKit,
                connection.execution_owner_id().as_str(),
                floe_access::GrantOperation::Read,
                floe_access::GrantPurpose::Assistant,
                consumer.clone(),
                floe_access::ProcessingRestriction::LocalOnly,
            )
            .await?;
        Ok(floe_access::CalendarReadAccessAdmission::device_local(
            person_id,
            admission.grant_id,
            admission.authority,
            admission.source,
            connection.source_authority(),
            admission.scope,
            consumer,
        ))
    }
}

pub(super) struct NativeCalendarDependencyResolver<'a, Keys: VaultKeyProvider> {
    pub core: &'a FloeCore,
    pub vault: &'a EncryptedAgentVault<Keys>,
    pub person_id: PersonId,
    pub device_id: &'a str,
}

impl<Keys: VaultKeyProvider> floe_access::DependencyResolver
    for NativeCalendarDependencyResolver<'_, Keys>
{
    fn authorize<'a>(
        &'a self,
        dependency: &'a floe_context_contract::ContextDependency,
        request: &'a floe_access::DependencyAuthorization,
    ) -> Pin<Box<dyn Future<Output = Result<(), AgentFailure>> + Send + 'a>> {
        Box::pin(async move {
            if dependency.person_id() != self.person_id
                || dependency.source().execution_owner().as_str() != self.device_id
            {
                return Err(AgentFailure::CapabilityDenied);
            }
            #[cfg(target_os = "macos")]
            {
                let connections = CoreCalendarConnections {
                    core: self.core,
                    person_id: self.person_id,
                };
                let connection =
                    floe_context::CalendarConnectionReader::calendar_connection(&connections)
                        .await?
                        .ok_or(AgentFailure::AccessReviewRequired)?;
                if connection.connector_id().as_str() != "calendar.event_kit" {
                    return Err(AgentFailure::StaleContext);
                }
                let source =
                    floe_provider_adapters::sources::native_calendar::NativeCalendarReadAccess::new(
                        self.person_id,
                        self.device_id.to_owned(),
                        floe_context_contract::CalendarProvider::EventKit,
                        dependency
                            .source_resources()
                            .iter()
                            .map(|resource| resource.as_str().to_owned())
                            .collect(),
                        connection.connection_id().as_str().to_owned(),
                        connection.revision(),
                    );
                let grants = VaultNativeCalendarGrants { vault: self.vault };
                return floe_context::authorize_native_calendar_dependency(
                    &connections,
                    &source,
                    &grants,
                    &self.core.lease_registry,
                    dependency,
                    &RemoteCallWindow {
                        deadline: request.deadline,
                        cancellation: request.cancellation.clone(),
                    },
                )
                .await;
            }
            #[cfg(not(target_os = "macos"))]
            {
                let _ = request;
                Err(AgentFailure::CapabilityUnavailable)
            }
        })
    }
}

/// The device this host runs on, asked what calendar subject it would answer for.
pub(crate) struct DeviceCalendarSubject<'a> {
    pub local_context: &'a LocalContextHost,
}

impl NativeCalendarSubjectSource for DeviceCalendarSubject<'_> {
    #[cfg(target_os = "macos")]
    async fn subject(
        &self,
        request: NativeSubjectRequest,
    ) -> Result<NativeSubjectObservation, AgentFailure> {
        let _ = self.local_context;
        if request.provider != floe_context_contract::CalendarProvider::EventKit {
            return Err(AgentFailure::CapabilityUnavailable);
        }
        let access =
            floe_provider_adapters::sources::native_calendar::NativeCalendarReadAccess::new(
                request.person_id,
                request.device_id.clone(),
                request.provider,
                request.calendar_ids.clone(),
                request.connection_id.clone(),
                request.connection_revision,
            );
        let stamp = floe_context::CalendarSource::check(
            &access,
            floe_access::CalendarReadAccessRequest {
                person_id: request.person_id,
                device_id: request.device_id.clone(),
                provider: request.provider,
                calendar_ids: request.calendar_ids.clone(),
                expected_native_subject_fingerprint: None,
                deadline: request.window.deadline,
                cancellation: request.window.cancellation.clone(),
            },
        )
        .await?;
        Ok(NativeSubjectObservation {
            before: stamp.native_subject_fingerprint,
            after: None,
        })
    }

    #[cfg(not(target_os = "macos"))]
    async fn subject(
        &self,
        request: NativeSubjectRequest,
    ) -> Result<NativeSubjectObservation, AgentFailure> {
        use floe_provider_adapters::sources::native_acquisition::{
            CalendarAcquisitionMode, CalendarAcquisitionRequest,
        };

        let host_epoch = self
            .local_context
            .calendar()
            .host_epoch(request.person_id)?;
        let start = chrono::Utc::now().timestamp_millis();
        let result = self
            .local_context
            .calendar()
            .submit(
                CalendarAcquisitionRequest {
                    request_id: uuid::Uuid::new_v4(),
                    host_epoch,
                    person_id: request.person_id,
                    device_id: request.device_id.clone(),
                    connection_id: request.connection_id.clone(),
                    connection_revision: request.connection_revision,
                    provider: request.provider,
                    mode: CalendarAcquisitionMode::InspectSubject,
                    calendar_ids: request.calendar_ids.clone(),
                    range_start_unix_ms: start,
                    range_end_unix_ms: start + 86_400_000,
                    deadline_unix_ms: start + 30_000,
                    expected_native_subject_fingerprint: None,
                },
                start,
                request.window.cancellation.clone(),
            )
            .await?;
        Ok(NativeSubjectObservation {
            before: result.native_subject_fingerprint_before,
            after: Some(result.native_subject_fingerprint_after),
        })
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn calendar_template_grants_trusted_shipped_consumers() {
        let consumers = crate::first_party_observe::calendar_policy()
            .unwrap()
            .consumers;
        assert_eq!(consumers.len(), 4);
        assert!(
            !consumers
                .iter()
                .any(|consumer| consumer.identifier() == "assistant")
        );
    }
}

async fn usable_native_connection(
    core: &FloeCore,
    person_id: PersonId,
) -> Result<floe_connections::SourceConnection, AgentFailure> {
    let connection =
        floe_context::CalendarConnectionReader::calendar_connection(&CoreCalendarConnections {
            core,
            person_id,
        })
        .await?
        .ok_or(AgentFailure::AccessReviewRequired)?;
    if connection.connector_id().as_str() != "calendar.event_kit" {
        return Err(AgentFailure::CapabilityUnavailable);
    }
    if connection.state() == floe_connections::SourceState::Disconnected {
        return Err(AgentFailure::AccessReviewRequired);
    }
    Ok(connection)
}

async fn observe_calendar_grant<Keys: VaultKeyProvider>(
    vault: &EncryptedAgentVault<Keys>,
    person_id: PersonId,
    device_id: &str,
    connection: &floe_connections::SourceConnection,
) -> Result<Option<floe_access::DataAccessGrant>, AgentFailure> {
    vault
        .data_access_grant_for_source_resource(
            &native_grant_source(person_id, device_id, connection)?,
            &floe_access::native_calendar_resource(connection.connection_id().as_str())?,
        )
        .await
}

fn observe_calendar_overview(
    connection_id: &str,
    connection: Option<&floe_connections::SourceConnection>,
    grant: Option<&floe_access::DataAccessGrant>,
) -> ConnectionObserveOverview {
    let members = grant
        .map(|grant| {
            vec![ConnectionObserveMember {
                view_id: "calendar.timeline".into(),
                state: grant.state(),
                review_required: grant.review_required(),
            }]
        })
        .unwrap_or_default();
    let mut overview = ConnectionObserveOverview::from_members(
        "calendar.event_kit",
        connection_id,
        connection
            .map(|connection| {
                connection
                    .resources()
                    .iter()
                    .map(|resource| resource.handle().as_str().to_owned())
                    .collect()
            })
            .unwrap_or_default(),
        &["calendar.timeline"],
        members,
    );
    if connection.is_none() {
        overview.status = ConnectionObserveStatus::NeedsSystemAccess;
    } else if connection.is_some_and(|connection| !connection.is_serving()) {
        overview.status = ConnectionObserveStatus::Unavailable;
        overview.enabled = false;
    }
    overview
}

async fn review_calendar_observe<Keys, Subject>(
    core: &FloeCore,
    vault: &EncryptedAgentVault<Keys>,
    subject: &Subject,
    person_id: PersonId,
    device_id: &str,
    connection_id: &str,
    update_subject: bool,
    cancellation: Cancellation,
) -> Result<ConnectionObserveExpectation, AgentFailure>
where
    Keys: VaultKeyProvider,
    Subject: NativeCalendarSubjectSource,
{
    let source = usable_native_connection(core, person_id).await?;
    if source.connection_id().as_str() != connection_id || !source.is_serving() {
        return Err(AgentFailure::AccessReviewRequired);
    }
    native_grant_source(person_id, device_id, &source)?;
    let calendar_ids = source
        .resources()
        .iter()
        .map(|resource| resource.handle().as_str().to_owned())
        .collect();
    let observed = preview_native_calendar_subject(
        &CoreCalendarConnections { core, person_id },
        subject,
        &NativeCalendarSourceRequest {
            person_id,
            provider: floe_context_contract::CalendarProvider::EventKit,
            device_id: device_id.to_owned(),
            calendar_ids,
            connection_scope: match source.resource_mode() {
                floe_connections::ResourceMode::Selected => {
                    floe_context_contract::CalendarScope::Selected
                }
                floe_connections::ResourceMode::AllAvailable => {
                    floe_context_contract::CalendarScope::All
                }
            },
            source_authority: Some(source.source_authority()),
            reviewed_native_subject_fingerprint: None,
            connection_id: Some(connection_id.to_owned()),
        },
        &subject_window(cancellation),
    )
    .await?;
    let reloaded = usable_native_connection(core, person_id).await?;
    if reloaded != source {
        return Err(AgentFailure::StaleContext);
    }
    let current = if update_subject {
        core.source_service()
            .update_native_subject(
                person_id,
                source.connection_id(),
                source.revision(),
                observed.native_subject_fingerprint.clone(),
            )
            .await
            .map_err(|_| AgentFailure::StaleContext)?
    } else {
        if source.native_subject_fingerprint() != Some(observed.native_subject_fingerprint.as_str())
        {
            return Err(AgentFailure::AccessReviewRequired);
        }
        source
    };
    let grant = observe_calendar_grant(vault, person_id, device_id, &current).await?;
    let policy = crate::first_party_observe::calendar_policy()?;
    let expectation = ConnectionObserveExpectation {
        connector_id: "calendar.event_kit".into(),
        connection_id: connection_id.to_owned(),
        source_authority: current.source_authority(),
        connection_revision: Some(current.revision()),
        native_subject: Some(observed.native_subject_fingerprint),
        producer_fingerprint: None,
        members: vec![ConnectionObserveReviewedMember {
            view_id: policy.view_id.into(),
            policy_digest: crate::first_party_observe::policy_digest(&policy)?,
            resource: floe_access::native_calendar_resource(connection_id)?
                .as_str()
                .to_owned(),
            expected_grant_id: grant.as_ref().map(floe_access::DataAccessGrant::id),
            expected_grant_authority: grant.as_ref().map(floe_access::DataAccessGrant::authority),
        }],
    };
    expectation.validate()?;
    Ok(expectation)
}

pub(super) async fn apply_connection_observe<Keys, Subject>(
    core: &FloeCore,
    vault: &EncryptedAgentVault<Keys>,
    subject: &Subject,
    person_id: PersonId,
    device_id: &str,
    operation: &ConnectionObserveOperation,
    cancellation: Cancellation,
) -> Result<
    (
        Option<ConnectionObserveOverview>,
        Option<ConnectionObserveExpectation>,
    ),
    AgentFailure,
>
where
    Keys: VaultKeyProvider,
    Subject: NativeCalendarSubjectSource,
{
    operation.validate()?;
    if vault.person_id() != person_id {
        return Err(AgentFailure::CapabilityDenied);
    }
    let (connector_id, connection_id) = operation.identity();
    if connector_id != "calendar.event_kit" {
        return Err(AgentFailure::InvalidInput);
    }
    match operation {
        ConnectionObserveOperation::Inspect { .. } => {
            let source = floe_context::CalendarConnectionReader::calendar_connection(
                &CoreCalendarConnections { core, person_id },
            )
            .await?;
            if source
                .as_ref()
                .is_some_and(|source| source.connection_id().as_str() != connection_id)
            {
                return Err(AgentFailure::AccessReviewRequired);
            }
            let grant = match source.as_ref() {
                Some(source) => observe_calendar_grant(vault, person_id, device_id, source).await?,
                None => None,
            };
            Ok((
                Some(observe_calendar_overview(
                    connection_id,
                    source.as_ref(),
                    grant.as_ref(),
                )),
                None,
            ))
        }
        ConnectionObserveOperation::Review { .. } => Ok((
            None,
            Some(
                review_calendar_observe(
                    core,
                    vault,
                    subject,
                    person_id,
                    device_id,
                    connection_id,
                    true,
                    cancellation,
                )
                .await?,
            ),
        )),
        ConnectionObserveOperation::SetEnabled {
            enabled,
            disconnecting,
            expected,
            ..
        } => {
            let source = usable_native_connection(core, person_id).await?;
            if source.connection_id().as_str() != connection_id {
                return Err(AgentFailure::AccessReviewRequired);
            }
            let grant = observe_calendar_grant(vault, person_id, device_id, &source).await?;
            if *enabled {
                let reviewed = review_calendar_observe(
                    core,
                    vault,
                    subject,
                    person_id,
                    device_id,
                    connection_id,
                    false,
                    cancellation,
                )
                .await?;
                if Some(&reviewed) != expected.as_ref()
                    || reviewed.source_authority != source.source_authority()
                    || reviewed.connection_revision != Some(source.revision())
                {
                    return Err(AgentFailure::AccessReviewRequired);
                }
                let current = usable_native_connection(core, person_id).await?;
                if current != source {
                    return Err(AgentFailure::StaleContext);
                }
                let policy = crate::first_party_observe::calendar_policy()?;
                let expected_grant = grant.as_ref().map(|grant| (grant.id(), grant.authority()));
                if expected_grant
                    != reviewed.members[0]
                        .expected_grant_id
                        .zip(reviewed.members[0].expected_grant_authority)
                {
                    return Err(AgentFailure::AccessReviewRequired);
                }
                let active = vault
                    .review_native_calendar_grant(
                        connection_id,
                        floe_context_contract::CalendarProvider::EventKit,
                        device_id,
                        &policy.consumers,
                        expected_grant,
                    )
                    .await?;
                Ok((
                    Some(observe_calendar_overview(
                        connection_id,
                        Some(&current),
                        Some(&active),
                    )),
                    None,
                ))
            } else {
                let grant = grant.ok_or(AgentFailure::AccessReviewRequired)?;
                let changed = if *disconnecting {
                    vault
                        .revoke_native_calendar_grant(
                            grant.id(),
                            grant.authority(),
                            connection_id,
                            floe_context_contract::CalendarProvider::EventKit,
                            device_id,
                            source.source_authority(),
                        )
                        .await?
                } else {
                    vault
                        .pause_native_calendar_grant(
                            grant.id(),
                            grant.authority(),
                            connection_id,
                            floe_context_contract::CalendarProvider::EventKit,
                            device_id,
                            source.source_authority(),
                        )
                        .await?
                };
                Ok((
                    Some(observe_calendar_overview(
                        connection_id,
                        Some(&source),
                        Some(&changed),
                    )),
                    None,
                ))
            }
        }
    }
}

fn native_grant_source(
    person_id: PersonId,
    device_id: &str,
    connection: &floe_connections::SourceConnection,
) -> Result<GrantSourceBinding, AgentFailure> {
    if connection.execution_owner_id().as_str() != device_id || connection.person_id() != person_id
    {
        return Err(AgentFailure::CapabilityDenied);
    }
    GrantSourceBinding::try_new(
        person_id,
        connection.connection_id().clone(),
        connection.connector_id().clone(),
        ExecutionOwnerId::try_new(device_id).map_err(|_| AgentFailure::InvalidInput)?,
    )
    .map_err(|_| AgentFailure::InvalidInput)
}
