//! Metadata-only native subject/catalog probes and explicitly admitted setup.
//! This adapter never creates a source, grant, review or source projection.
use std::sync::Arc;

use floe_connections::{
    ConnectionResource, ConnectionResourceGroup, NativeSetupObservation, NativeSetupRequest,
    NativeSetupState, NativeSourceSetupPort, SourceCatalogObservation, SourceCatalogPort,
    SourceConnection,
};
use floe_context::{NativeSubjectObservation, SourceMetadataTransport};
use floe_context_contract::{CalendarProvider, ResourceHandle};
use floe_execution::{BoxFuture, ExecutionScope};
use floe_kernel::{AgentFailure, OwnerActor};
use floe_native::{
    AttentionAcquisitionMode, AttentionAcquisitionRequest, AttentionBroker,
    CalendarAcquisitionMode, CalendarAcquisitionRequest, CalendarAcquisitionResult, CalendarBroker,
    NativeSourceResource, PersonalAcquisitionMode, PersonalAcquisitionRequest,
    PersonalAcquisitionResult, PersonalBroker, PersonalDomain,
};
use uuid::Uuid;

pub struct NativeSourceMetadataAdapter {
    calendar: Arc<CalendarBroker>,
    personal: Arc<PersonalBroker>,
    attention: Arc<AttentionBroker>,
}

impl NativeSourceMetadataAdapter {
    pub fn new(
        calendar: Arc<CalendarBroker>,
        personal: Arc<PersonalBroker>,
        attention: Arc<AttentionBroker>,
    ) -> Self {
        Self {
            calendar,
            personal,
            attention,
        }
    }

    async fn calendar_metadata(
        &self,
        actor: &OwnerActor,
        source: &SourceConnection,
        mode: CalendarAcquisitionMode,
        calendar_ids: Vec<String>,
        scope: &ExecutionScope,
    ) -> Result<CalendarAcquisitionResult, AgentFailure> {
        validate_source(actor, source)?;
        if source.connector_id().as_str() != "calendar.event_kit"
            || !matches!(
                mode,
                CalendarAcquisitionMode::InspectSubject | CalendarAcquisitionMode::InspectCatalog
            )
            || (mode == CalendarAcquisitionMode::InspectCatalog) != calendar_ids.is_empty()
            || calendar_ids.len() > 256
            || calendar_ids.windows(2).any(|pair| pair[0] >= pair[1])
            || calendar_ids
                .iter()
                .any(|id| ResourceHandle::try_new(id.clone()).is_err())
        {
            return Err(AgentFailure::InvalidInput);
        }
        let (now, deadline) = deadline(scope)?;
        let request = CalendarAcquisitionRequest {
            request_id: Uuid::new_v4(),
            host_epoch: self.calendar.host_epoch(actor.person_id)?,
            person_id: actor.person_id,
            device_id: actor.device_id.clone(),
            connection_id: source.connection_id().as_str().to_owned(),
            connection_revision: source.revision(),
            provider: CalendarProvider::EventKit,
            mode,
            calendar_ids,
            range_start_unix_ms: now,
            range_end_unix_ms: now.checked_add(1).ok_or(AgentFailure::InvalidInput)?,
            deadline_unix_ms: deadline,
            expected_native_subject_fingerprint: None,
        };
        let response = self.submit_calendar(request, scope).await?;
        if !matches!(response.permission_class.as_str(), "full" | "authorized") {
            return Err(AgentFailure::AccessReviewRequired);
        }
        stable_subject(
            &response.native_subject_fingerprint_before,
            &response.native_subject_fingerprint_after,
        )?;
        if !response.batches.is_empty() {
            return Err(AgentFailure::PolicyDenied);
        }
        let resources = validate_resources(response.available_calendars.clone())?;
        if resources
            .iter()
            .map(|resource| resource.handle().as_str())
            .ne(response.available_calendar_ids.iter().map(String::as_str))
            || response
                .calendar_ids
                .iter()
                .any(|id| !response.available_calendar_ids.contains(id))
        {
            return Err(AgentFailure::StaleContext);
        }
        Ok(response)
    }

    async fn submit_calendar(
        &self,
        request: CalendarAcquisitionRequest,
        scope: &ExecutionScope,
    ) -> Result<CalendarAcquisitionResult, AgentFailure> {
        let response = scope
            .run(self.calendar.submit(
                request.clone(),
                chrono::Utc::now().timestamp_millis(),
                scope.cancellation().clone(),
            ))
            .await?;
        if self.calendar.host_epoch(request.person_id).as_deref() != Ok(request.host_epoch.as_str())
            || response.request_id != request.request_id
            || response.host_epoch != request.host_epoch
            || response.person_id != request.person_id
            || response.device_id != request.device_id
            || response.connection_id != request.connection_id
            || response.connection_revision != request.connection_revision
            || response.provider != request.provider
            || response.mode != request.mode
            || response.calendar_ids != request.calendar_ids
            || response.range_start_unix_ms != request.range_start_unix_ms
            || response.range_end_unix_ms != request.range_end_unix_ms
        {
            return Err(AgentFailure::StaleContext);
        }
        validate_subject(&response.native_subject_fingerprint_before)?;
        validate_subject(&response.native_subject_fingerprint_after)?;
        Ok(response)
    }

    async fn submit_personal(
        &self,
        request: PersonalAcquisitionRequest,
        scope: &ExecutionScope,
    ) -> Result<PersonalAcquisitionResult, AgentFailure> {
        let response = scope
            .run(self.personal.submit(
                request.clone(),
                chrono::Utc::now().timestamp_millis(),
                scope.cancellation().clone(),
            ))
            .await?;
        let provider = match request.domain {
            PersonalDomain::People => "apple_contacts",
            PersonalDomain::Wellbeing => "apple_health",
        };
        if self.personal.host_epoch(request.person_id).as_deref() != Ok(request.host_epoch.as_str())
            || response.request_id != request.request_id
            || response.host_epoch != request.host_epoch
            || response.person_id != request.person_id
            || response.device_id != request.device_id
            || response.domain != request.domain
            || response.mode != request.mode
            || response.provider != provider
            || response.view.is_some()
            || response.transform_operation_id.is_some()
        {
            return Err(AgentFailure::PolicyDenied);
        }
        validate_subject(&response.native_subject_fingerprint_before)?;
        validate_subject(&response.native_subject_fingerprint_after)?;
        if request.mode != PersonalAcquisitionMode::RequestPermission {
            stable_subject(
                &response.native_subject_fingerprint_before,
                &response.native_subject_fingerprint_after,
            )?;
        }
        Ok(response)
    }

    async fn personal_catalog(
        &self,
        actor: &OwnerActor,
        domain: PersonalDomain,
        scope: &ExecutionScope,
    ) -> Result<(Vec<ConnectionResource>, bool), AgentFailure> {
        let (_, deadline) = deadline(scope)?;
        let response = self
            .submit_personal(
                PersonalAcquisitionRequest {
                    request_id: Uuid::new_v4(),
                    host_epoch: self.personal.host_epoch(actor.person_id)?,
                    person_id: actor.person_id,
                    device_id: actor.device_id.clone(),
                    domain,
                    mode: PersonalAcquisitionMode::InspectCatalog,
                    selected_handles: Vec::new(),
                    deadline_unix_ms: deadline,
                    expected_native_subject_fingerprint: None,
                },
                scope,
            )
            .await?;
        match domain {
            PersonalDomain::People
                if !matches!(response.permission_class.as_str(), "authorized" | "limited") =>
            {
                return Err(AgentFailure::AccessReviewRequired);
            }
            PersonalDomain::Wellbeing => match response.permission_class.as_str() {
                // Health metadata cannot prove the OS granted read access.
                "permission_required"
                | "no_data_or_read_access_limited"
                | "pending"
                | "stale"
                | "ready" => {}
                "unsupported" | "unavailable" => return Err(AgentFailure::CapabilityUnavailable),
                _ => return Err(AgentFailure::InvalidInput),
            },
            _ => {}
        }
        Ok((
            validate_resources(response.resources)?,
            response.catalog_complete,
        ))
    }

    async fn attention_catalog(
        &self,
        actor: &OwnerActor,
        scope: &ExecutionScope,
    ) -> Result<Vec<ConnectionResource>, AgentFailure> {
        let (_, deadline) = deadline(scope)?;
        let host_epoch = self.attention.host_epoch(actor.person_id)?;
        let request_id = Uuid::new_v4();
        let response = scope
            .run(self.attention.submit(
                AttentionAcquisitionRequest {
                    request_id,
                    host_epoch: host_epoch.clone(),
                    person_id: actor.person_id,
                    device_id: actor.device_id.clone(),
                    mode: AttentionAcquisitionMode::InspectSubject,
                    deadline_unix_ms: deadline,
                    expected_native_subject_fingerprint: None,
                },
                chrono::Utc::now().timestamp_millis(),
                scope.cancellation().clone(),
            ))
            .await?;
        if self.attention.host_epoch(actor.person_id).as_deref() != Ok(host_epoch.as_str())
            || response.request_id != request_id
            || response.host_epoch != host_epoch
            || response.person_id != actor.person_id
            || response.device_id != actor.device_id
            || response.mode != AttentionAcquisitionMode::InspectSubject
            || response.view.is_some()
            || response.permission_class != "session_observation"
        {
            return Err(AgentFailure::PolicyDenied);
        }
        stable_subject(
            &response.native_subject_fingerprint_before,
            &response.native_subject_fingerprint_after,
        )?;
        validate_resources(vec![NativeSourceResource {
            handle: floe_access::ATTENTION_RESOURCE.into(),
            label: "Attention".into(),
            group: None,
        }])
    }
}

impl SourceMetadataTransport for NativeSourceMetadataAdapter {
    fn calendar_subject<'a>(
        &'a self,
        actor: &'a OwnerActor,
        source: &'a SourceConnection,
        calendar_ids: &'a [String],
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<NativeSubjectObservation, AgentFailure>> {
        Box::pin(async move {
            let result = self
                .calendar_metadata(
                    actor,
                    source,
                    CalendarAcquisitionMode::InspectSubject,
                    calendar_ids.to_vec(),
                    scope,
                )
                .await?;
            Ok(NativeSubjectObservation {
                before: result.native_subject_fingerprint_before,
                after: Some(result.native_subject_fingerprint_after),
            })
        })
    }
}

impl SourceCatalogPort for NativeSourceMetadataAdapter {
    fn inspect<'a>(
        &'a self,
        actor: &'a OwnerActor,
        source: &'a SourceConnection,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<SourceCatalogObservation, AgentFailure>> {
        Box::pin(scope.run(async move {
            validate_source(actor, source)?;
            let (resources, complete) = match source.connector_id().as_str() {
                "calendar.event_kit" => {
                    let response = self
                        .calendar_metadata(
                            actor,
                            source,
                            CalendarAcquisitionMode::InspectCatalog,
                            Vec::new(),
                            scope,
                        )
                        .await?;
                    (validate_resources(response.available_calendars)?, true)
                }
                "contacts.apple" => {
                    self.personal_catalog(actor, PersonalDomain::People, scope)
                        .await?
                }
                "health.apple" => {
                    self.personal_catalog(actor, PersonalDomain::Wellbeing, scope)
                        .await?
                }
                "attention.macos" => (self.attention_catalog(actor, scope).await?, true),
                _ => return Err(AgentFailure::CapabilityUnavailable),
            };
            let catalog_digest = floe_access::digest(&(source, &resources, complete))?;
            Ok(SourceCatalogObservation {
                source: source.clone(),
                resources,
                catalog_digest,
                catalog_complete: complete,
            })
        }))
    }
}

impl NativeSourceSetupPort for NativeSourceMetadataAdapter {
    fn request_permission<'a>(
        &'a self,
        actor: &'a OwnerActor,
        request: NativeSetupRequest,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<NativeSetupObservation, AgentFailure>> {
        Box::pin(scope.run(async move {
            actor.validate()?;
            if request.operation_id.is_nil() || request.source_revision == 0 {
                return Err(AgentFailure::InvalidInput);
            }
            let (now, deadline) = deadline(scope)?;
            let permission = match request.connector_id.as_str() {
                "calendar.event_kit" => {
                    let response = self
                        .submit_calendar(
                            CalendarAcquisitionRequest {
                                request_id: request.operation_id,
                                host_epoch: self.calendar.host_epoch(actor.person_id)?,
                                person_id: actor.person_id,
                                device_id: actor.device_id.clone(),
                                connection_id: request.connection_id.as_str().to_owned(),
                                connection_revision: request.source_revision,
                                provider: CalendarProvider::EventKit,
                                mode: CalendarAcquisitionMode::RequestPermission,
                                calendar_ids: Vec::new(),
                                range_start_unix_ms: now,
                                range_end_unix_ms: now
                                    .checked_add(1)
                                    .ok_or(AgentFailure::InvalidInput)?,
                                deadline_unix_ms: deadline,
                                expected_native_subject_fingerprint: None,
                            },
                            scope,
                        )
                        .await?;
                    if !response.available_calendar_ids.is_empty()
                        || !response.available_calendars.is_empty()
                        || !response.batches.is_empty()
                    {
                        return Err(AgentFailure::PolicyDenied);
                    }
                    response.permission_class
                }
                "contacts.apple" | "health.apple" => {
                    let domain = if request.connector_id.as_str() == "contacts.apple" {
                        PersonalDomain::People
                    } else {
                        PersonalDomain::Wellbeing
                    };
                    let response = self
                        .submit_personal(
                            PersonalAcquisitionRequest {
                                request_id: request.operation_id,
                                host_epoch: self.personal.host_epoch(actor.person_id)?,
                                person_id: actor.person_id,
                                device_id: actor.device_id.clone(),
                                domain,
                                mode: PersonalAcquisitionMode::RequestPermission,
                                selected_handles: Vec::new(),
                                deadline_unix_ms: deadline,
                                expected_native_subject_fingerprint: None,
                            },
                            scope,
                        )
                        .await?;
                    if !response.resources.is_empty() || response.catalog_complete {
                        return Err(AgentFailure::PolicyDenied);
                    }
                    response.permission_class
                }
                // There is no shipped permission-request route for Attention.
                "attention.macos" => "unavailable".to_owned(),
                _ => return Err(AgentFailure::CapabilityUnavailable),
            };
            let state = match permission.as_str() {
                "request_completed" => NativeSetupState::Completed,
                "denied" => NativeSetupState::Denied,
                "unavailable" => NativeSetupState::Unavailable,
                _ => return Err(AgentFailure::InvalidInput),
            };
            Ok(NativeSetupObservation {
                operation_id: request.operation_id,
                connector_id: request.connector_id,
                state,
            })
        }))
    }
}

fn validate_source(actor: &OwnerActor, source: &SourceConnection) -> Result<(), AgentFailure> {
    actor.validate()?;
    source.validate().map_err(|_| AgentFailure::InvalidInput)?;
    let expected_owner = match source.connector_id().as_str() {
        "calendar.event_kit" => floe_access::apple_execution_owner(&actor.device_id),
        "contacts.apple" | "health.apple" => format!("apple:{}", actor.device_id),
        "attention.macos" => format!("macos:{}", actor.device_id),
        _ => return Err(AgentFailure::CapabilityUnavailable),
    };
    if source.person_id() != actor.person_id
        || source.execution_owner_id().as_str() != expected_owner
    {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(())
}

fn deadline(scope: &ExecutionScope) -> Result<(i64, i64), AgentFailure> {
    if scope.cancellation().is_cancelled() {
        return Err(AgentFailure::Cancelled);
    }
    let remaining = scope
        .deadline()
        .saturating_duration_since(tokio::time::Instant::now())
        .as_millis()
        .min(30_000);
    if remaining == 0 {
        return Err(AgentFailure::DeadlineExceeded);
    }
    let now = chrono::Utc::now().timestamp_millis();
    let deadline = now
        .checked_add(i64::try_from(remaining).map_err(|_| AgentFailure::DeadlineExceeded)?)
        .ok_or(AgentFailure::DeadlineExceeded)?;
    Ok((now, deadline))
}

fn validate_subject(subject: &str) -> Result<(), AgentFailure> {
    if !floe_access::valid_subject_fingerprint(subject) {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(())
}

fn stable_subject(before: &str, after: &str) -> Result<(), AgentFailure> {
    validate_subject(before)?;
    validate_subject(after)?;
    if before != after {
        return Err(AgentFailure::AccessReviewRequired);
    }
    Ok(())
}

fn validate_resources(
    resources: Vec<NativeSourceResource>,
) -> Result<Vec<ConnectionResource>, AgentFailure> {
    if resources.len() > 256 {
        return Err(AgentFailure::BudgetExceeded);
    }
    let mut resources = resources
        .into_iter()
        .map(|resource| {
            ConnectionResource::new(
                ResourceHandle::try_new(resource.handle).map_err(|_| AgentFailure::InvalidInput)?,
                resource.label,
                resource
                    .group
                    .map(|group| {
                        Ok::<_, AgentFailure>(ConnectionResourceGroup {
                            handle: ResourceHandle::try_new(group.handle)
                                .map_err(|_| AgentFailure::InvalidInput)?,
                            label: group.label,
                        })
                    })
                    .transpose()?,
            )
            .map_err(|_| AgentFailure::InvalidInput)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut groups = std::collections::BTreeMap::new();
    for resource in &resources {
        if let Some(group) = resource.group() {
            if groups
                .insert(&group.handle, &group.label)
                .is_some_and(|label| label != &group.label)
            {
                return Err(AgentFailure::InvalidInput);
            }
        }
    }
    resources.sort_by(|left, right| left.handle().cmp(right.handle()));
    if resources
        .windows(2)
        .any(|pair| pair[0].handle() == pair[1].handle())
    {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(resources)
}
