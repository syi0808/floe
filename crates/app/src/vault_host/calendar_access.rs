//! The concrete device, connection store and registry a calendar access change
//! runs against.
//!
//! Every judgment here belongs to an owner: Experts decides what the change
//! means, Context orders the acquisition, Access admits the source. What this
//! file supplies is the wiring — a device probe, the Person's calendar
//! connection, and their encrypted registry.

use std::{future::Future, pin::Pin, time::Duration};

use floe_access::{
    ConnectionId, ExecutionOwnerId, GrantSourceBinding, RemoteCallWindow,
    native_calendar_connector, valid_subject_fingerprint, validate_grant_expectation,
};
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
use crate::local_access_services::{
    CalendarAccessChange, CalendarAccessOverview, CalendarAccessState,
};
use crate::local_context::LocalContextHost;

/// How long the device is given to answer for its own calendar subject.
const SUBJECT_DEADLINE: Duration = Duration::from_secs(30);

pub(super) fn calendar_first_party_consumers() -> Result<Vec<floe_access::GrantConsumer>, AgentFailure> {
    Ok(crate::first_party_observe::calendar_policy()?.consumers)
}

/// The window one device probe must finish inside.
pub(super) fn subject_window(cancellation: Cancellation) -> RemoteCallWindow {
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
    ) -> Result<Option<floe_day::CalendarConnection>, AgentFailure> {
        self.core
            .calendar_connection(self.person_id)
            .await
            .map_err(|_| AgentFailure::StorageUnavailable)
    }
}

pub(super) struct VaultNativeCalendarGrants<'a, Keys: VaultKeyProvider> {
    pub vault: &'a EncryptedAgentVault<Keys>,
}

impl<Keys: VaultKeyProvider> NativeCalendarGrantReader for VaultNativeCalendarGrants<'_, Keys> {
    async fn admit(
        &self,
        connection: &floe_day::CalendarConnection,
        person_id: PersonId,
        calendar_ids: &[String],
        consumer: &str,
        native_subject_fingerprint: &str,
    ) -> Result<floe_access::CalendarReadAccessAdmission, AgentFailure> {
        if person_id != self.vault.person_id() {
            return Err(AgentFailure::CapabilityDenied);
        }
        let consumer = floe_access::GrantConsumer::builtin(consumer)
            .map_err(|_| AgentFailure::CapabilityDenied)?;
        let admission = self
            .vault
            .authorize_current_native_calendar_grant(
                &connection.connection_id,
                connection.provider,
                &connection.device_id,
                calendar_ids,
                connection.source_authority,
                floe_access::GrantOperation::Read,
                floe_access::GrantPurpose::Assistant,
                consumer.clone(),
                floe_access::ProcessingRestriction::LocalOnly,
                Some(native_subject_fingerprint),
            )
            .await?;
        Ok(floe_access::CalendarReadAccessAdmission::device_local(
            person_id,
            admission.grant_id,
            admission.authority,
            admission.source,
            admission.scope,
            admission.consumer_policy,
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
                if connection.provider != floe_context_contract::CalendarProvider::EventKit {
                    return Err(AgentFailure::StaleContext);
                }
                let source =
                    floe_provider_adapters::sources::native_calendar::NativeCalendarReadAccess::new(
                        self.person_id,
                        self.device_id.to_owned(),
                        connection.provider,
                        connection
                            .calendars
                            .iter()
                            .map(|calendar| calendar.calendar_id.clone())
                            .collect(),
                        connection.connection_id,
                        connection.revision,
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
pub(super) struct DeviceCalendarSubject<'a> {
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

/// The device admission an Expert calendar change asks for, run through Context.
pub(super) struct DeviceCalendarAdmission<'a> {
    pub connections: CoreCalendarConnections<'a>,
    pub device: DeviceCalendarSubject<'a>,
    pub window: RemoteCallWindow,
}

impl<'host> DeviceCalendarAdmission<'host> {
    pub(super) fn new(
        core: &'host FloeCore,
        local_context: &'host LocalContextHost,
        person_id: PersonId,
        cancellation: Cancellation,
    ) -> Self {
        Self {
            connections: CoreCalendarConnections { core, person_id },
            device: DeviceCalendarSubject { local_context },
            window: subject_window(cancellation),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use floe_experts_builtin::{BuiltinContextSource, BuiltinExpertKind};

    #[test]
    fn calendar_consumers_are_derived_from_builtin_declarations() {
        let consumers = calendar_first_party_consumers().unwrap();
        let expected = BuiltinExpertKind::ALL
            .into_iter()
            .filter(|kind| {
                kind.declaration()
                    .required_sources
                    .contains(&BuiltinContextSource::Calendar.source_id())
            })
            .map(|kind| kind.package_id())
            .collect::<Vec<_>>();

        for package_id in expected {
            assert!(
                consumers
                    .iter()
                    .any(|consumer| consumer.identifier() == package_id),
                "missing {package_id}"
            );
        }
        assert_eq!(consumers.len(), 4);
        assert!(
            !consumers
                .iter()
                .any(|consumer| consumer.identifier() == "floe.builtin.communication")
        );
        assert!(
            consumers
                .iter()
                .all(|consumer| matches!(consumer, floe_access::GrantConsumer::Builtin(_)))
        );
    }

    #[test]
    fn remote_calendar_scope_uses_exact_canonical_consumers() {
        use floe_access::{
            ConnectionId, ConnectorId, DataAccessGrant, ExecutionOwnerId, GrantId,
            GrantSourceBinding, SourceAuthority,
        };

        let person_id = PersonId::new();
        let source = GrantSourceBinding::try_new(
            person_id,
            ConnectionId::try_new("remote-calendar").unwrap(),
            ConnectorId::try_new("calendar.google").unwrap(),
            ExecutionOwnerId::try_new("server-owner").unwrap(),
            SourceAuthority::new(),
        )
        .unwrap();
        let consumers = calendar_first_party_consumers().unwrap();
        let scope = floe_access::remote_calendar_scope("primary", &consumers).unwrap();
        let mut grant = DataAccessGrant::new(
            GrantId::new(),
            uuid::Uuid::new_v4(),
            source.clone(),
            scope.clone(),
        )
        .unwrap();
        grant
            .activate_review(grant.authority(), source, scope)
            .unwrap();

        for consumer in &consumers {
            floe_access::admit_remote_calendar_read(&grant, consumer, "primary").unwrap();
        }
        assert!(
            consumers.iter().any(|consumer| {
                consumer.identifier() == BuiltinExpertKind::Schedule.package_id()
            })
        );
        assert_eq!(
            floe_access::admit_remote_calendar_read(
                &grant,
                &floe_access::GrantConsumer::extension("third-party.schedule").unwrap(),
                "primary",
            ),
            Err(AgentFailure::PolicyDenied)
        );
    }

    #[test]
    fn legacy_calendar_expert_scope_does_not_admit_schedule() {
        use floe_access::{
            ConnectionId, ConnectorId, DataAccessGrant, ExecutionOwnerId, GrantId,
            GrantSourceBinding, SourceAuthority,
        };

        let person_id = PersonId::new();
        let source = GrantSourceBinding::try_new(
            person_id,
            ConnectionId::try_new("legacy-calendar").unwrap(),
            ConnectorId::try_new("calendar.google").unwrap(),
            ExecutionOwnerId::try_new("server-owner").unwrap(),
            SourceAuthority::new(),
        )
        .unwrap();
        let scope = floe_access::remote_calendar_scope(
            "primary",
            &[floe_access::GrantConsumer::builtin("calendar.expert").unwrap()],
        )
        .unwrap();
        let mut grant = DataAccessGrant::new(
            GrantId::new(),
            uuid::Uuid::new_v4(),
            source.clone(),
            scope.clone(),
        )
        .unwrap();
        grant
            .activate_review(grant.authority(), source, scope)
            .unwrap();

        assert_eq!(
            floe_access::admit_remote_calendar_read(
                &grant,
                &floe_access::GrantConsumer::builtin(BuiltinExpertKind::Schedule.package_id())
                    .unwrap(),
                "primary",
            ),
            Err(AgentFailure::PolicyDenied)
        );
    }
}

/// What this device records about the calendar connection a remote grant would
/// name, with nothing judged about it.
pub(super) struct RemoteCalendarEvidence {
    provider: floe_context_contract::CalendarProvider,
    connection_id: String,
    calendar_ids: Vec<String>,
    disconnected: bool,
}

impl RemoteCalendarEvidence {
    /// The same evidence, stated the way Access reads one.
    pub(super) fn as_access(&self) -> floe_access::RemoteCalendarConnection<'_> {
        floe_access::RemoteCalendarConnection {
            provider: self.provider,
            connection_id: &self.connection_id,
            calendar_ids: &self.calendar_ids,
            disconnected: self.disconnected,
        }
    }
}

/// Read what this device currently records about the Person's calendar.
///
/// Having no connection at all is a review they owe rather than a storage
/// failure: nothing on this device says which calendar a grant would name.
pub(super) async fn remote_calendar_evidence(
    core: &FloeCore,
    person_id: PersonId,
) -> Result<RemoteCalendarEvidence, AgentFailure> {
    let connection = core
        .calendar_connection(person_id)
        .await
        .map_err(|_| AgentFailure::StorageUnavailable)?
        .ok_or(AgentFailure::AccessReviewRequired)?;
    Ok(RemoteCalendarEvidence {
        provider: connection.provider,
        connection_id: connection.connection_id,
        calendar_ids: connection
            .calendars
            .into_iter()
            .map(|calendar| calendar.calendar_id)
            .collect(),
        disconnected: connection.disconnected,
    })
}

/// Run one native Calendar Observe change: inspect, review, pause or remove.
///
/// The backend is authoritative for the current connection, the admitted
/// consumers, and the grant expectation comparison. A review observes the
/// fresh native subject outside any Vault transaction and compares it with
/// the reviewed subject before the E1 mutation runs; pause and remove only
/// touch the grant, never the connection, credentials or Day data.
pub(super) async fn apply_calendar_access<Keys, Subject>(
    core: &FloeCore,
    vault: &EncryptedAgentVault<Keys>,
    subject: &Subject,
    person_id: PersonId,
    device_id: String,
    change: CalendarAccessChange,
    cancellation: Cancellation,
) -> Result<CalendarAccessOverview, AgentFailure>
where
    Keys: VaultKeyProvider,
    Subject: NativeCalendarSubjectSource,
{
    match change {
        CalendarAccessChange::Inspect => {
            let connection = usable_native_connection(core, person_id).await?;
            let source = native_grant_source(person_id, &device_id, &connection)?;
            match vault
                .data_access_grants_for_source(&source, 2)
                .await?
                .as_slice()
            {
                [] => overview(person_id, &connection, None, vault).await,
                [grant] => overview(person_id, &connection, Some(grant), vault).await,
                _ => Err(AgentFailure::VaultUnavailable),
            }
        }
        CalendarAccessChange::Review {
            connection_id,
            calendar_ids,
            expected_source_authority,
            expected_native_subject_fingerprint,
            expected_grant_id,
            expected_grant_authority,
        } => {
            if connection_id.trim().is_empty()
                || calendar_ids.is_empty()
                || calendar_ids.len() > 4
                || calendar_ids.iter().any(|id| {
                    id.is_empty()
                        || id.len() > 256
                        || id.trim() != id
                        || id.chars().any(char::is_control)
                })
                || !expected_source_authority.is_valid()
                || !valid_subject_fingerprint(&expected_native_subject_fingerprint)
            {
                return Err(AgentFailure::InvalidInput);
            }
            let expected_grant =
                validate_grant_expectation(expected_grant_id, expected_grant_authority)?;
            let connection = usable_native_connection(core, person_id).await?;
            if connection.connection_id != connection_id
                || connection.source_authority != expected_source_authority
            {
                return Err(AgentFailure::StaleContext);
            }
            let selected = connection
                .calendars
                .iter()
                .map(|calendar| calendar.calendar_id.clone())
                .collect::<Vec<_>>();
            if !calendar_ids.iter().all(|id| selected.contains(id)) {
                return Err(AgentFailure::StaleContext);
            }
            let fresh = preview_native_calendar_subject(
                &CoreCalendarConnections { core, person_id },
                subject,
                &NativeCalendarSourceRequest {
                    person_id,
                    provider: connection.provider,
                    device_id: device_id.clone(),
                    calendar_ids: calendar_ids.clone(),
                    connection_scope: connection.scope,
                    source_authority: Some(connection.source_authority),
                    reviewed_native_subject_fingerprint: None,
                    connection_id: Some(connection.connection_id.clone()),
                },
                &subject_window(cancellation),
            )
            .await?;
            if fresh.native_subject_fingerprint != expected_native_subject_fingerprint {
                return Err(AgentFailure::AccessReviewRequired);
            }
            let current = usable_native_connection(core, person_id).await?;
            if current.connection_id != connection.connection_id
                || current.source_authority != connection.source_authority
                || current.revision != connection.revision
            {
                return Err(AgentFailure::StaleContext);
            }
            let grant = vault
                .review_native_calendar_grant(
                    &current.connection_id,
                    current.provider,
                    &device_id,
                    &calendar_ids,
                    current.source_authority,
                    &calendar_first_party_consumers()?,
                    &expected_native_subject_fingerprint,
                    expected_grant,
                )
                .await?;
            overview(person_id, &current, Some(&grant), vault).await
        }
        CalendarAccessChange::Pause {
            grant_id,
            expected_grant_authority,
        } => {
            if !grant_id.is_valid() || !expected_grant_authority.is_valid() {
                return Err(AgentFailure::InvalidInput);
            }
            let current = usable_native_connection(core, person_id).await?;
            let grant = vault
                .pause_native_calendar_grant(
                    grant_id,
                    expected_grant_authority,
                    &current.connection_id,
                    current.provider,
                    &device_id,
                    current.source_authority,
                )
                .await?;
            overview(person_id, &current, Some(&grant), vault).await
        }
        CalendarAccessChange::Remove {
            grant_id,
            expected_grant_authority,
        } => {
            if !grant_id.is_valid() || !expected_grant_authority.is_valid() {
                return Err(AgentFailure::InvalidInput);
            }
            let current = usable_native_connection(core, person_id).await?;
            let grant = vault
                .revoke_native_calendar_grant(
                    grant_id,
                    expected_grant_authority,
                    &current.connection_id,
                    current.provider,
                    &device_id,
                    current.source_authority,
                )
                .await?;
            overview(person_id, &current, Some(&grant), vault).await
        }
    }
}

async fn usable_native_connection(
    core: &FloeCore,
    person_id: PersonId,
) -> Result<floe_day::CalendarConnection, AgentFailure> {
    let connection = core
        .calendar_connection(person_id)
        .await
        .map_err(|_| AgentFailure::StorageUnavailable)?
        .ok_or(AgentFailure::AccessReviewRequired)?;
    if !matches!(
        connection.provider,
        floe_context_contract::CalendarProvider::EventKit
            | floe_context_contract::CalendarProvider::Android
    ) {
        return Err(AgentFailure::CapabilityUnavailable);
    }
    if connection.disconnected {
        return Err(AgentFailure::AccessReviewRequired);
    }
    Ok(connection)
}

fn native_grant_source(
    person_id: PersonId,
    device_id: &str,
    connection: &floe_day::CalendarConnection,
) -> Result<GrantSourceBinding, AgentFailure> {
    let connector = native_calendar_connector(connection.provider)
        .ok_or(AgentFailure::CapabilityUnavailable)?;
    GrantSourceBinding::try_new(
        person_id,
        ConnectionId::try_new(&connection.connection_id).map_err(|_| AgentFailure::InvalidInput)?,
        floe_access::ConnectorId::try_new(connector).map_err(|_| AgentFailure::InvalidInput)?,
        ExecutionOwnerId::try_new(device_id).map_err(|_| AgentFailure::InvalidInput)?,
        connection.source_authority,
    )
    .map_err(|_| AgentFailure::InvalidInput)
}

async fn overview<Keys: VaultKeyProvider>(
    person_id: PersonId,
    connection: &floe_day::CalendarConnection,
    grant: Option<&floe_access::DataAccessGrant>,
    vault: &EncryptedAgentVault<Keys>,
) -> Result<CalendarAccessOverview, AgentFailure> {
    let (state, review_required, grant_id, grant_authority, consumer_policy, granted) = match grant
    {
            None => (
                CalendarAccessState::NeedsReview,
                true,
                None,
                None,
                None,
                Vec::new(),
            ),
            Some(grant) => {
                let policy = vault.calendar_grant_policy_authority(grant.id()).await?;
                let state = match grant.state() {
                    floe_access::GrantState::Active => CalendarAccessState::Active,
                    floe_access::GrantState::Paused => CalendarAccessState::Paused,
                    floe_access::GrantState::Revoked => CalendarAccessState::Revoked,
                };
                (
                    state,
                    grant.review_required(),
                    Some(grant.id()),
                    Some(grant.authority()),
                    Some(policy),
                    grant
                        .scope()
                        .resources()
                        .iter()
                        .map(|resource| resource.as_str().to_owned())
                        .collect(),
                )
            }
        };
    Ok(CalendarAccessOverview {
        person_id,
        provider: connection.provider,
        connection_id: connection.connection_id.clone(),
        selected_resources: connection
            .calendars
            .iter()
            .map(|calendar| calendar.calendar_id.clone())
            .collect(),
        granted_resources: granted,
        source_authority: connection.source_authority,
        grant_id,
        grant_authority,
        consumer_policy,
        state,
        review_required,
    })
}
