//! Real native/provider evidence adapters supplied to Connections. Source and
//! grant policy, immutable review construction and recovery belong to owners.
use crate::local_context::LocalContextHost;
use floe_access::{
    AuthorizationSigner, GrantSourceBinding, PersonalSubjectInspector, PersonalSubjectProbe,
    RemoteCallWindow, RemoteGrantTransport, RemotePairingIdentity, RemoteSourceQuery,
    SourceExpectation, SourcePreviewVerifier,
};
use floe_connections::{SourceConnection, SourceReviewEvidence};
use floe_context::NativeCalendarSubjectSource;
use floe_execution::{BoxFuture, ExecutionScope};
use floe_kernel::{AgentFailure, OwnerActor};
use floe_provider_adapters::gateway::GatewayCredentialStore;
use std::sync::Arc;

pub(crate) struct HostSourceEvidence {
    pub local_context: Arc<LocalContextHost>,
    pub credentials: Arc<GatewayCredentialStore>,
    pub signer: Arc<dyn AuthorizationSigner>,
    pub verifier: Arc<dyn SourcePreviewVerifier>,
}
impl SourceReviewEvidence for HostSourceEvidence {
    fn inspect_selection<'a>(
        &'a self,
        actor: &'a OwnerActor,
        source: &'a SourceConnection,
        selected_resources: &'a [floe_connections::ConnectionResource],
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<String, AgentFailure>> {
        Box::pin(async move {
            actor.validate()?;
            if source.person_id() != actor.person_id || selected_resources.is_empty() {
                return Err(AgentFailure::PolicyDenied);
            }
            let handles = selected_resources
                .iter()
                .map(|resource| resource.handle().as_str().to_owned())
                .collect::<Vec<_>>();
            if let Some(provider) =
                floe_access::native_calendar_provider(source.connector_id().as_str())
            {
                let observed = super::calendar_access::DeviceCalendarSubject {
                    local_context: &self.local_context,
                }
                .subject(floe_context::NativeSubjectRequest {
                    person_id: actor.person_id,
                    device_id: actor.device_id.clone(),
                    provider,
                    calendar_ids: handles,
                    connection_id: source.connection_id().as_str().to_owned(),
                    connection_revision: source.revision(),
                    window: RemoteCallWindow {
                        deadline: scope.deadline(),
                        cancellation: scope.cancellation().clone(),
                    },
                })
                .await?;
                if observed.after.as_deref() != Some(observed.before.as_str())
                    || !floe_access::valid_subject_fingerprint(&observed.before)
                {
                    return Err(AgentFailure::AccessReviewRequired);
                }
                return Ok(observed.before);
            }
            let probe = match source.connector_id().as_str() {
                "contacts.apple" | "contacts.android" => PersonalSubjectProbe::People {
                    selected_handles: handles,
                },
                "attention.macos" if handles == [floe_access::ATTENTION_RESOURCE] => {
                    PersonalSubjectProbe::Attention
                }
                "health.apple" if handles == [floe_access::WELLBEING_RESOURCE] => {
                    PersonalSubjectProbe::Wellbeing
                }
                _ => return Err(AgentFailure::CapabilityUnavailable),
            };
            let driver = super::personal_grants::native_driver(&self.local_context);
            let observed = driver
                .inspect(
                    actor.person_id,
                    &actor.device_id,
                    probe,
                    None,
                    Some(scope.deadline()),
                    scope.cancellation().clone(),
                )
                .await?;
            if observed.before != observed.after
                || !floe_access::valid_subject_fingerprint(&observed.before)
            {
                return Err(AgentFailure::AccessReviewRequired);
            }
            Ok(observed.before)
        })
    }
    fn observe<'a>(
        &'a self,
        actor: &'a OwnerActor,
        source: &'a SourceConnection,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<SourceExpectation, AgentFailure>> {
        Box::pin(async move {
            actor.validate()?;
            if source.person_id() != actor.person_id || scope.cancellation().is_cancelled() {
                return Err(AgentFailure::PolicyDenied);
            }
            let binding = GrantSourceBinding::try_new(
                source.person_id(),
                source.connection_id().clone(),
                source.connector_id().clone(),
                source.execution_owner_id().clone(),
            )
            .map_err(|_| AgentFailure::InvalidInput)?;
            let mut physical_resources = source
                .resources()
                .iter()
                .map(|resource| resource.handle().clone())
                .collect::<Vec<_>>();
            physical_resources.sort();
            let connector = source.connector_id().as_str();
            if let Some(provider) = floe_access::native_calendar_provider(connector) {
                let observation = super::calendar_access::DeviceCalendarSubject {
                    local_context: &self.local_context,
                }
                .subject(floe_context::NativeSubjectRequest {
                    person_id: actor.person_id,
                    device_id: actor.device_id.clone(),
                    provider,
                    calendar_ids: physical_resources
                        .iter()
                        .map(|resource| resource.as_str().to_owned())
                        .collect(),
                    connection_id: source.connection_id().as_str().to_owned(),
                    connection_revision: source.revision(),
                    window: RemoteCallWindow {
                        deadline: scope.deadline(),
                        cancellation: scope.cancellation().clone(),
                    },
                })
                .await?;
                let after = observation
                    .after
                    .as_deref()
                    .ok_or(AgentFailure::PolicyDenied)?;
                let expected = source
                    .native_subject_fingerprint()
                    .ok_or(AgentFailure::AccessReviewRequired)?;
                floe_access::subject_unchanged(expected, &observation.before, after)?;
                return Ok(SourceExpectation {
                    source: binding,
                    revision: Some(source.revision()),
                    provider_revision: None,
                    authority: source.source_authority(),
                    physical_resources,
                    subject_fingerprint: observation.before,
                    gateway: None,
                });
            }
            if matches!(
                connector,
                "contacts.apple" | "contacts.android" | "attention.macos" | "health.apple"
            ) {
                let spec = floe_connections::PersonalSourceSpec::for_connector(connector)?;
                spec.validate_connection(source, &actor.device_id)?;
                let probe = match connector {
                    "contacts.apple" | "contacts.android" => PersonalSubjectProbe::People {
                        selected_handles: physical_resources
                            .iter()
                            .map(|resource| resource.as_str().to_owned())
                            .collect(),
                    },
                    "attention.macos" => PersonalSubjectProbe::Attention,
                    _ => PersonalSubjectProbe::Wellbeing,
                };
                let expected = source
                    .native_subject_fingerprint()
                    .ok_or(AgentFailure::AccessReviewRequired)?;
                let driver = super::personal_grants::native_driver(&self.local_context);
                let observation = driver
                    .inspect(
                        actor.person_id,
                        &actor.device_id,
                        probe,
                        Some(expected.to_owned()),
                        Some(scope.deadline()),
                        scope.cancellation().clone(),
                    )
                    .await?;
                floe_access::subject_unchanged(expected, &observation.before, &observation.after)?;
                return Ok(SourceExpectation {
                    source: binding,
                    revision: Some(source.revision()),
                    provider_revision: None,
                    authority: source.source_authority(),
                    physical_resources,
                    subject_fingerprint: observation.before,
                    gateway: None,
                });
            }
            let client =
                floe_provider_adapters::sources::ServerSourceClient::from_current_connection(
                    &self.credentials,
                    &actor.person_id.to_string(),
                    &actor.device_id,
                )
                .await?
                .ok_or(AgentFailure::CapabilityUnavailable)?;
            let transport = floe_provider_adapters::sources::AuthorizedSourceClient::new(
                &client,
                self.signer.as_ref(),
            );
            let window = RemoteCallWindow {
                deadline: scope.deadline(),
                cancellation: scope.cancellation().clone(),
            };
            let person_text = actor.person_id.to_string();
            let pairing = RemotePairingIdentity {
                person_id: &person_text,
                client_id: client.source().client_id(),
                device_id: &actor.device_id,
            };
            let mut observed: Option<SourceExpectation> = None;
            for view_id in floe_access::source_view_ids(connector) {
                let resource = floe_context_contract::connection_view_resource(
                    view_id,
                    source.connection_id(),
                )
                .map_err(|_| AgentFailure::InvalidInput)?;
                let query = RemoteSourceQuery {
                    view_id,
                    connector_id: connector,
                    connection_id: source.connection_id().as_str(),
                    resource: resource.as_str(),
                };
                let preview = transport.view_source_preview(query, &window).await?;
                let reference = self.verifier.verify(&preview, pairing, query).await?;
                floe_access::admit_remote_view_source(&reference, &binding)?;
                if reference.source_authority != source.source_authority()
                    || reference.source_resources != physical_resources
                {
                    return Err(AgentFailure::Conflict);
                }
                let expected = SourceExpectation {
                    source: binding.clone(),
                    revision: Some(source.revision()),
                    provider_revision: Some(reference.connection_revision),
                    authority: reference.source_authority,
                    physical_resources: reference.source_resources,
                    subject_fingerprint: reference.provider_identity,
                    gateway: Some(client.source().binding().clone()),
                };
                if observed.as_ref().is_some_and(|prior| prior != &expected) {
                    return Err(AgentFailure::Conflict);
                }
                observed = Some(expected);
            }
            observed.ok_or(AgentFailure::CapabilityUnavailable)
        })
    }
}

pub(crate) struct HostSourceCatalog {
    pub local_context: Arc<LocalContextHost>,
}

pub(crate) struct HostNativeSourceSetup {
    pub local_context: Arc<LocalContextHost>,
}
impl floe_connections::NativeSourceSetupPort for HostNativeSourceSetup {
    fn request_permission<'a>(
        &'a self,
        actor: &'a OwnerActor,
        request: floe_connections::NativeSetupRequest,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<floe_connections::NativeSetupObservation, AgentFailure>> {
        Box::pin(async move {
            use floe_provider_adapters::sources::native_acquisition::{
                CalendarAcquisitionMode, CalendarAcquisitionRequest, PersonalAcquisitionMode,
                PersonalAcquisitionRequest, PersonalDomain,
            };
            actor.validate()?;
            if request.operation_id.is_nil() || request.source_revision == 0 {
                return Err(AgentFailure::InvalidInput);
            }
            if scope.cancellation().is_cancelled() {
                return Err(AgentFailure::Cancelled);
            }
            let now = chrono::Utc::now().timestamp_millis();
            let remaining = scope
                .deadline()
                .saturating_duration_since(tokio::time::Instant::now())
                .as_millis()
                .min(30_000);
            if remaining == 0 {
                return Err(AgentFailure::DeadlineExceeded);
            }
            let deadline = now
                .checked_add(i64::try_from(remaining).map_err(|_| AgentFailure::DeadlineExceeded)?)
                .ok_or(AgentFailure::DeadlineExceeded)?;
            let permission = if let Some(provider) =
                floe_access::native_calendar_provider(request.connector_id.as_str())
            {
                self.local_context
                    .calendar()
                    .submit(
                        CalendarAcquisitionRequest {
                            request_id: request.operation_id,
                            host_epoch: self
                                .local_context
                                .calendar()
                                .host_epoch(actor.person_id)?,
                            person_id: actor.person_id,
                            device_id: actor.device_id.clone(),
                            connection_id: request.connection_id.as_str().to_owned(),
                            connection_revision: request.source_revision,
                            provider,
                            mode: CalendarAcquisitionMode::RequestPermission,
                            calendar_ids: Vec::new(),
                            range_start_unix_ms: now,
                            range_end_unix_ms: now
                                .checked_add(86_400_000)
                                .ok_or(AgentFailure::InvalidInput)?,
                            deadline_unix_ms: deadline,
                            expected_native_subject_fingerprint: None,
                        },
                        now,
                        scope.cancellation().clone(),
                    )
                    .await?
                    .permission_class
            } else {
                let domain = match request.connector_id.as_str() {
                    "contacts.apple" => PersonalDomain::People,
                    "health.apple" => PersonalDomain::Wellbeing,
                    _ => return Err(AgentFailure::CapabilityUnavailable),
                };
                self.local_context
                    .personal()
                    .submit(
                        PersonalAcquisitionRequest {
                            request_id: request.operation_id,
                            host_epoch: self
                                .local_context
                                .personal()
                                .host_epoch(actor.person_id)?,
                            person_id: actor.person_id,
                            device_id: actor.device_id.clone(),
                            domain,
                            mode: PersonalAcquisitionMode::RequestPermission,
                            selected_handles: Vec::new(),
                            deadline_unix_ms: deadline,
                            expected_native_subject_fingerprint: None,
                        },
                        now,
                        scope.cancellation().clone(),
                    )
                    .await?
                    .permission_class
            };
            let state = match permission.as_str() {
                "request_completed" => floe_connections::NativeSetupState::Completed,
                "denied" => floe_connections::NativeSetupState::Denied,
                "unavailable" => floe_connections::NativeSetupState::Unavailable,
                _ => return Err(AgentFailure::InvalidInput),
            };
            Ok(floe_connections::NativeSetupObservation {
                operation_id: request.operation_id,
                connector_id: request.connector_id,
                state,
            })
        })
    }
}
impl floe_connections::SourceCatalogPort for HostSourceCatalog {
    fn inspect<'a>(
        &'a self,
        actor: &'a OwnerActor,
        source: &'a SourceConnection,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<floe_connections::SourceCatalogObservation, AgentFailure>> {
        Box::pin(async move {
            use floe_provider_adapters::sources::native_acquisition::{
                CalendarAcquisitionMode, CalendarAcquisitionRequest, PersonalAcquisitionMode,
                PersonalAcquisitionRequest, PersonalDomain,
            };
            actor.validate()?;
            if source.person_id() != actor.person_id {
                return Err(AgentFailure::PolicyDenied);
            }
            let now = chrono::Utc::now().timestamp_millis();
            let remaining = scope
                .deadline()
                .saturating_duration_since(tokio::time::Instant::now())
                .as_millis()
                .min(30_000);
            if remaining == 0 {
                return Err(AgentFailure::DeadlineExceeded);
            }
            let deadline = now
                .checked_add(i64::try_from(remaining).map_err(|_| AgentFailure::DeadlineExceeded)?)
                .ok_or(AgentFailure::DeadlineExceeded)?;
            let (mut resources, complete) = if let Some(provider) =
                floe_access::native_calendar_provider(source.connector_id().as_str())
            {
                let host_epoch = self.local_context.calendar().host_epoch(actor.person_id)?;
                let result = self
                    .local_context
                    .calendar()
                    .submit(
                        CalendarAcquisitionRequest {
                            request_id: uuid::Uuid::new_v4(),
                            host_epoch,
                            person_id: actor.person_id,
                            device_id: actor.device_id.clone(),
                            connection_id: source.connection_id().as_str().to_owned(),
                            connection_revision: source.revision(),
                            provider,
                            mode: CalendarAcquisitionMode::InspectCatalog,
                            calendar_ids: Vec::new(),
                            range_start_unix_ms: now,
                            range_end_unix_ms: now
                                .checked_add(86_400_000)
                                .ok_or(AgentFailure::InvalidInput)?,
                            deadline_unix_ms: deadline,
                            expected_native_subject_fingerprint: None,
                        },
                        now,
                        scope.cancellation().clone(),
                    )
                    .await?;
                let mut resources = Vec::new();
                for resource in result.available_calendars {
                    resources.push(
                        floe_connections::ConnectionResource::new(
                            floe_context_contract::ResourceHandle::try_new(resource.handle)
                                .map_err(|_| AgentFailure::InvalidInput)?,
                            resource.label,
                        )
                        .map_err(|_| AgentFailure::InvalidInput)?,
                    );
                }
                (resources, true)
            } else if matches!(
                source.connector_id().as_str(),
                "contacts.apple" | "contacts.android" | "health.apple"
            ) {
                let domain = if source.connector_id().as_str() == "health.apple" {
                    PersonalDomain::Wellbeing
                } else {
                    PersonalDomain::People
                };
                let result = self
                    .local_context
                    .personal()
                    .submit(
                        PersonalAcquisitionRequest {
                            request_id: uuid::Uuid::new_v4(),
                            host_epoch: self
                                .local_context
                                .personal()
                                .host_epoch(actor.person_id)?,
                            person_id: actor.person_id,
                            device_id: actor.device_id.clone(),
                            domain,
                            mode: PersonalAcquisitionMode::InspectCatalog,
                            selected_handles: Vec::new(),
                            deadline_unix_ms: deadline,
                            expected_native_subject_fingerprint: None,
                        },
                        now,
                        scope.cancellation().clone(),
                    )
                    .await?;
                let resources = result
                    .resources
                    .into_iter()
                    .map(|resource| {
                        floe_connections::ConnectionResource::new(
                            floe_context_contract::ResourceHandle::try_new(resource.handle)
                                .map_err(|_| AgentFailure::InvalidInput)?,
                            resource.label,
                        )
                        .map_err(|_| AgentFailure::InvalidInput)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                (resources, result.catalog_complete)
            } else if source.connector_id().as_str() == "attention.macos" {
                let driver = super::personal_grants::native_driver(&self.local_context);
                let expected = source
                    .native_subject_fingerprint()
                    .ok_or(AgentFailure::AccessReviewRequired)?;
                let observed = driver
                    .inspect(
                        actor.person_id,
                        &actor.device_id,
                        PersonalSubjectProbe::Attention,
                        Some(expected.to_owned()),
                        Some(scope.deadline()),
                        scope.cancellation().clone(),
                    )
                    .await?;
                floe_access::subject_unchanged(expected, &observed.before, &observed.after)?;
                (
                    vec![
                        floe_connections::ConnectionResource::new(
                            floe_context_contract::ResourceHandle::try_new(
                                floe_access::ATTENTION_RESOURCE,
                            )
                            .map_err(|_| AgentFailure::InvalidInput)?,
                            "Attention".to_owned(),
                        )
                        .map_err(|_| AgentFailure::InvalidInput)?,
                    ],
                    true,
                )
            } else {
                return Err(AgentFailure::CapabilityUnavailable);
            };
            resources.sort_by(|left, right| left.handle().cmp(right.handle()));
            if resources.len() > 256
                || resources
                    .windows(2)
                    .any(|pair| pair[0].handle() == pair[1].handle())
            {
                return Err(AgentFailure::BudgetExceeded);
            }
            let catalog_digest = floe_access::digest(&(source, &resources, complete))?;
            Ok(floe_connections::SourceCatalogObservation {
                source: source.clone(),
                resources,
                catalog_digest,
                catalog_complete: complete,
            })
        })
    }
}
