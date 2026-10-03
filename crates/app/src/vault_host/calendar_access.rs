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
    async fn source_is_fenced(&self, person_id: PersonId, connection_id: &floe_context_contract::ConnectionId) -> Result<bool, AgentFailure> {
        floe_connections::SourceOperationRepository::source_is_fenced(self.core.store.as_ref(), person_id, connection_id).await.map_err(|_| AgentFailure::StorageUnavailable)
    }
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
        let admission = floe_access::current_native_calendar_grant(self.vault, person_id,
            connection.connection_id().as_str(), floe_context_contract::CalendarProvider::EventKit,
            connection.execution_owner_id().as_str(), &consumer).await?;
        Ok(floe_access::CalendarReadAccessAdmission::device_local(
            person_id,
            admission.id(),
            admission.authority(),
            admission.source().clone(),
            connection.source_authority(),
            admission.scope().clone(),
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
                        self.core.store.clone(),
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
                    deadline_unix_ms: start + i64::try_from(request.window.deadline.saturating_duration_since(tokio::time::Instant::now()).as_millis().min(30_000)).map_err(|_| AgentFailure::DeadlineExceeded)?,
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

