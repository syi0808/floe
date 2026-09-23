//! The concrete device, connection store and registry a calendar access change
//! runs against.
//!
//! Every judgment here belongs to an owner: Experts decides what the change
//! means, Context orders the acquisition, Access admits the source. What this
//! file supplies is the wiring — a device probe, the Person's calendar
//! connection, and their encrypted registry.

use std::{future::Future, pin::Pin, time::Duration};

use floe_access::RemoteCallWindow;
use floe_agent_contract::AgentFailure;
use floe_context::{
    CalendarConnectionReader, NativeCalendarGrantReader, NativeCalendarSourceRequest,
    NativeCalendarSubjectSource, NativeSubjectObservation, NativeSubjectRequest,
};
use floe_execution::Cancellation;
use floe_experts::{
    AdmittedCalendarSource, BoxFuture, CalendarAccessConfiguration, CalendarAccessSource,
    CalendarExpertOverview, CalendarExpertSetup, CalendarSetupStore, CalendarSourceAdmission,
    ExpertPackaging,
};
use floe_kernel::PersonId;
use floe_vault::{EncryptedAgentVault, VaultKeyProvider};

use crate::FloeCore;
use crate::local_context::LocalContextHost;

/// How long the device is given to answer for its own calendar subject.
const SUBJECT_DEADLINE: Duration = Duration::from_secs(30);

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

    /// The source a request names, stated the way Context asks about one.
    pub(super) fn source_request(
        &self,
        request: &CalendarExpertSetup,
    ) -> NativeCalendarSourceRequest {
        NativeCalendarSourceRequest {
            person_id: self.connections.person_id,
            provider: request.provider,
            device_id: request.device_id.clone(),
            calendar_ids: request.calendar_ids.clone(),
            connection_scope: request.connection_scope,
            source_authority: request.source_authority,
            reviewed_native_subject_fingerprint: request
                .reviewed_native_subject_fingerprint
                .clone(),
            connection_id: None,
        }
    }
}

impl CalendarSourceAdmission for DeviceCalendarAdmission<'_> {
    fn admit<'a>(
        &'a self,
        request: &'a CalendarExpertSetup,
    ) -> BoxFuture<'a, Result<Option<AdmittedCalendarSource>, AgentFailure>> {
        Box::pin(async move {
            let admitted = floe_context::admit_native_calendar_source(
                &self.connections,
                &self.device,
                &self.source_request(request),
                &self.window,
            )
            .await?;
            Ok(admitted.map(|admitted| AdmittedCalendarSource {
                connection_id: admitted.connection_id,
                source_authority: admitted.source_authority,
                native_subject_fingerprint: admitted.native_subject_fingerprint,
            }))
        })
    }
}

/// The Person's encrypted Expert registry, as a calendar change writes it.
pub(super) struct VaultCalendarSetups<'a, Keys> {
    pub vault: &'a EncryptedAgentVault<Keys>,
    pub packaging: ExpertPackaging,
    pub cancellation: Cancellation,
}

impl<Keys: VaultKeyProvider> CalendarSetupStore for VaultCalendarSetups<'_, Keys> {
    fn overview<'a>(&'a self) -> BoxFuture<'a, Result<CalendarExpertOverview, AgentFailure>> {
        Box::pin(self.vault.calendar_expert_overview())
    }

    fn granted_connection_id<'a>(
        &'a self,
        setup_id: uuid::Uuid,
    ) -> BoxFuture<'a, Result<String, AgentFailure>> {
        Box::pin(self.vault.calendar_grant_connection_id(setup_id))
    }

    fn install<'a>(
        &'a self,
        request: CalendarExpertSetup,
        connection_id: String,
    ) -> BoxFuture<'a, Result<(), AgentFailure>> {
        Box::pin(async move {
            self.vault
                .install_calendar_expert_with_connection(
                    request,
                    &self.packaging,
                    connection_id,
                    self.cancellation.clone(),
                )
                .await?;
            Ok(())
        })
    }

    fn configure<'a>(
        &'a self,
        configuration: CalendarAccessConfiguration,
        source: CalendarAccessSource,
    ) -> BoxFuture<'a, Result<CalendarExpertOverview, AgentFailure>> {
        Box::pin(async move {
            match source {
                CalendarAccessSource::Registry => {
                    self.vault
                        .configure_calendar_access(configuration, self.cancellation.clone())
                        .await
                }
                CalendarAccessSource::Native { connection_id } => {
                    self.vault
                        .configure_calendar_access_with_connection(
                            configuration,
                            connection_id,
                            self.cancellation.clone(),
                        )
                        .await
                }
            }
        })
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
