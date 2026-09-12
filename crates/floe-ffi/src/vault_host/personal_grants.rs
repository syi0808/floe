use std::{future::Future, pin::Pin};

use chrono::{DateTime, Utc};
use floe_agent::{
    AgentFailure, AttentionState, AttentionView, ModelPlacement, ModelRequest, PeopleView,
    validate_people_view,
};
use floe_core::{
    EncryptedAgentVault, GovernedDependencyLiveness, GovernedDependencyResolver, VaultKeyProvider,
};
use floe_domain::{
    ConnectionId, ConnectorId, ContextDependency, DataAccessGrant, ExecutionOwnerId, GrantConsumer,
    GrantDataCategory, GrantOperation, GrantPurpose, GrantScope, GrantSourceBinding, GrantState,
    PersonId, ProcessingRestriction, ResourceHandle, SourceAuthority,
};
use floe_protocol::{
    ContactsAccessChangeDto, ContactsAccessConfigurationDto,
    LocalContextAttentionAcquisitionModeDto, LocalContextAttentionAcquisitionRequestDto,
    LocalContextPersonalAcquisitionRequestDto, LocalContextPersonalAcquisitionResultDto,
    LocalContextPersonalDomainDto, PersonalAccessChangeDto, PersonalAccessConfigurationDto,
    PersonalAccessOverviewDto,
};
use sha2::Digest;
use uuid::Uuid;

pub(crate) const ATTENTION_CONNECTOR: &str = "attention.macos";
const ATTENTION_CONNECTION: &str = "attention.macos.local";
const ATTENTION_RESOURCE: &str = "attention.coarse";
const PEOPLE_RESOURCE: &str = "people.identity";
pub(crate) const ATTENTION_ASSISTANT_CONSUMER: &str = "assistant";
pub(crate) const ATTENTION_EXPERT_CONSUMER: &str = "attention.expert";

fn subject_fingerprint(person_id: PersonId, device_id: &str, view: &AttentionView) -> String {
    let digest = sha2::Sha256::digest(
        format!(
            "attention.macos\0{}\0{}\0{}\0{}",
            person_id, device_id, view.source_handle, view.view_id,
        )
        .as_bytes(),
    );
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn query_fingerprint(
    person_id: PersonId,
    device_id: &str,
    view: &AttentionView,
    observation_id: Uuid,
    process_incarnation_id: Uuid,
) -> Vec<u8> {
    let subject = subject_fingerprint(person_id, device_id, view);
    sha2::Sha256::digest(
        format!(
            "attention.query\0{}\0{}\0{}\0{}\0{}\0{}",
            subject,
            observation_id,
            process_incarnation_id,
            view.observed_at_unix_ms,
            view.expires_at_unix_ms,
            view.state as u8,
        )
        .as_bytes(),
    )
    .to_vec()
}

fn check_read_window(
    deadline: tokio::time::Instant,
    cancellation: &floe_agent::Cancellation,
) -> Result<(), AgentFailure> {
    if cancellation.is_cancelled() {
        return Err(AgentFailure::Cancelled);
    }
    if tokio::time::Instant::now() >= deadline {
        return Err(AgentFailure::DeadlineExceeded);
    }
    Ok(())
}

pub(crate) async fn read_people<Keys: VaultKeyProvider>(
    vault: &EncryptedAgentVault<Keys>,
    local_context: &crate::local_context::LocalContextStore,
    person_id: PersonId,
    device_id: &str,
    source: GrantSourceBinding,
    selected_handles: &[String],
    expected_native_subject_fingerprint: &str,
    consumer_name: &str,
    deadline: tokio::time::Instant,
    cancellation: &floe_agent::Cancellation,
) -> Result<(PeopleView, ContextDependency), AgentFailure> {
    check_read_window(deadline, cancellation)?;
    if source.person_id() != person_id
        || !matches!(
            source.connector().as_str(),
            "contacts.apple" | "contacts.android"
        )
        || selected_handles.is_empty()
        || selected_handles.len() > 64
        || selected_handles.windows(2).any(|pair| pair[0] >= pair[1])
        || !valid_native_subject_fingerprint(expected_native_subject_fingerprint)
    {
        return Err(AgentFailure::InvalidInput);
    }
    let consumer = GrantConsumer::builtin(consumer_name).map_err(|_| AgentFailure::InvalidInput)?;
    let grant = active_people_grant(
        &vault.list_data_access_grants(128).await?,
        &source,
        &consumer,
    )?;
    let host_epoch = local_context.personal_acquisition_host_epoch(person_id)?;
    let request_id = Uuid::new_v4();
    let observation_id = Uuid::new_v4();
    let result = local_context
        .acquire_personal(
            LocalContextPersonalAcquisitionRequestDto {
                request_id: request_id.to_string(),
                host_epoch,
                person_id: person_id.to_string(),
                device_id: device_id.to_owned(),
                domain: LocalContextPersonalDomainDto::People,
                selected_handles: selected_handles.to_vec(),
                event_handle: None,
                evidence_handles: Vec::new(),
                destination_latitude: None,
                destination_longitude: None,
                event_start_unix_ms: None,
                event_end_unix_ms: None,
                travel_mode: None,
                deadline_unix_ms: chrono::Utc::now().timestamp_millis().saturating_add(
                    i64::try_from(
                        deadline
                            .saturating_duration_since(tokio::time::Instant::now())
                            .as_millis(),
                    )
                    .map_err(|_| AgentFailure::DeadlineExceeded)?,
                ),
                expected_native_subject_fingerprint: Some(
                    expected_native_subject_fingerprint.to_owned(),
                ),
            },
            cancellation.clone(),
        )
        .await?;
    check_read_window(deadline, cancellation)?;
    if result.native_subject_fingerprint_before != expected_native_subject_fingerprint
        || result.native_subject_fingerprint_after != expected_native_subject_fingerprint
    {
        return Err(AgentFailure::AccessReviewRequired);
    }
    let view: PeopleView =
        serde_json::from_value(result.view.ok_or(AgentFailure::CapabilityUnavailable)?)
            .map_err(|_| AgentFailure::CapabilityUnavailable)?;
    validate_people_view(&view, Utc::now().timestamp_millis())
        .map_err(|_| AgentFailure::CapabilityUnavailable)?;
    let current = active_people_grant(
        &vault.list_data_access_grants(128).await?,
        &source,
        &consumer,
    )?;
    if current.id() != grant.id()
        || current.authority() != grant.authority()
        || current.source() != grant.source()
    {
        return Err(AgentFailure::PolicyDenied);
    }
    let policy = vault.personal_grant_consumer_policy(current.id()).await?;
    let process = local_context.process_incarnation();
    let observed = DateTime::<Utc>::from_timestamp_millis(view.observed_at_unix_ms)
        .ok_or(AgentFailure::StaleContext)?;
    let expires = DateTime::<Utc>::from_timestamp_millis(view.expires_at_unix_ms)
        .ok_or(AgentFailure::StaleContext)?;
    let dependency_fingerprint = people_query_fingerprint(
        &view,
        selected_handles,
        expected_native_subject_fingerprint,
        observation_id,
        process,
    );
    let dependency = ContextDependency::try_new(
        person_id,
        current.id(),
        current.authority(),
        current.source().clone(),
        vec![ResourceHandle::try_new(PEOPLE_RESOURCE).map_err(|_| AgentFailure::InvalidInput)?],
        vec![GrantDataCategory::Derived],
        GrantOperation::Read,
        GrantPurpose::Assistant,
        consumer,
        ProcessingRestriction::LocalOnly,
        policy,
        observation_id,
        dependency_fingerprint.clone(),
        request_id,
        process,
        observed,
        expires,
    )
    .map_err(|_| AgentFailure::InvalidInput)?;
    local_context.commit_trusted_personal_observation(
        person_id,
        device_id,
        observation_id,
        process,
        expected_native_subject_fingerprint,
        view.observed_at_unix_ms,
        view.expires_at_unix_ms,
        dependency_fingerprint,
    )?;
    Ok((view, dependency))
}

fn active_people_grant(
    grants: &[DataAccessGrant],
    source: &GrantSourceBinding,
    consumer: &GrantConsumer,
) -> Result<DataAccessGrant, AgentFailure> {
    let grant = grants
        .iter()
        .find(|grant| grant.source() == source && grant.state() != GrantState::Revoked)
        .ok_or(AgentFailure::AccessReviewRequired)?;
    if grant.state() != GrantState::Active
        || grant.review_required()
        || !grant
            .scope()
            .resources()
            .iter()
            .any(|r| r.as_str() == PEOPLE_RESOURCE)
        || !grant
            .scope()
            .categories()
            .contains(&GrantDataCategory::Derived)
        || !grant.scope().operations().contains(&GrantOperation::Read)
        || !grant.scope().purposes().contains(&GrantPurpose::Assistant)
        || !grant.scope().consumers().contains(consumer)
        || grant.scope().processing() != &ProcessingRestriction::LocalOnly
    {
        return Err(AgentFailure::AccessReviewRequired);
    }
    Ok(grant.clone())
}

fn people_query_fingerprint(
    view: &PeopleView,
    selected_handles: &[String],
    native_subject_fingerprint: &str,
    observation: Uuid,
    process: Uuid,
) -> Vec<u8> {
    sha2::Sha256::digest(
        format!(
            "people.query\0{}\0{}\0{}\0{}\0{}\0{}\0{}",
            view.source_handle,
            selected_handles.join("\0"),
            native_subject_fingerprint,
            observation,
            process,
            view.observed_at_unix_ms,
            view.expires_at_unix_ms,
        )
        .as_bytes(),
    )
    .to_vec()
}

fn valid_native_subject_fingerprint(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn attention_consumer(value: &str) -> Result<GrantConsumer, AgentFailure> {
    if !matches!(
        value,
        ATTENTION_ASSISTANT_CONSUMER | ATTENTION_EXPERT_CONSUMER
    ) {
        return Err(AgentFailure::PolicyDenied);
    }
    GrantConsumer::builtin(value).map_err(|_| AgentFailure::InvalidInput)
}

fn active_attention_grant(
    grants: &[DataAccessGrant],
    person_id: PersonId,
    device_id: &str,
    consumer: &GrantConsumer,
) -> Result<DataAccessGrant, AgentFailure> {
    let grant = grants
        .iter()
        .find(|grant| {
            matches_source(grant, person_id, device_id) && grant.state() != GrantState::Revoked
        })
        .ok_or(AgentFailure::AccessReviewRequired)?;
    if grant.state() != GrantState::Active
        || grant.review_required()
        || !grant
            .scope()
            .resources()
            .iter()
            .any(|resource| resource.as_str() == ATTENTION_RESOURCE)
        || !grant
            .scope()
            .categories()
            .contains(&GrantDataCategory::Derived)
        || !grant.scope().operations().contains(&GrantOperation::Read)
        || !grant.scope().purposes().contains(&GrantPurpose::Assistant)
        || !grant.scope().consumers().contains(consumer)
        || grant.scope().processing() != &ProcessingRestriction::LocalOnly
    {
        return Err(AgentFailure::AccessReviewRequired);
    }
    Ok(grant.clone())
}

pub(crate) async fn admit_attention<Keys: VaultKeyProvider>(
    vault: &EncryptedAgentVault<Keys>,
    local_context: &crate::local_context::LocalContextStore,
    person_id: PersonId,
    device_id: &str,
    consumer: &str,
    lease_invocation_id: Uuid,
    deadline: tokio::time::Instant,
    cancellation: &floe_agent::Cancellation,
) -> Result<(AttentionView, ContextDependency), AgentFailure> {
    check_read_window(deadline, cancellation)?;
    let consumer = attention_consumer(consumer)?;
    let grants = vault.list_data_access_grants(128).await?;
    let grant = active_attention_grant(&grants, person_id, device_id, &consumer)?;
    let reviewed_subject = vault.personal_grant_subject_fingerprint(grant.id()).await?;
    let host_epoch = local_context.attention_acquisition_host_epoch(person_id)?;
    let result = local_context
        .acquire_attention(
            attention_request(
                person_id,
                device_id,
                host_epoch.clone(),
                LocalContextAttentionAcquisitionModeDto::ReadProjection,
                Some(reviewed_subject.clone()),
                Some(deadline),
            )?,
            cancellation.clone(),
        )
        .await?;
    if result.native_subject_fingerprint_before != reviewed_subject
        || result.native_subject_fingerprint_after != reviewed_subject
    {
        return Err(AgentFailure::AccessReviewRequired);
    }
    let native_subject = result.native_subject_fingerprint_before.clone();
    let view: AttentionView =
        serde_json::from_value(result.view.ok_or(AgentFailure::CapabilityUnavailable)?)
            .map_err(|_| AgentFailure::CapabilityUnavailable)?;
    if view.state == AttentionState::Unknown || view.evidence_handles.is_empty() {
        return Err(AgentFailure::CapabilityUnavailable);
    }
    let observed_at = DateTime::<Utc>::from_timestamp_millis(view.observed_at_unix_ms)
        .ok_or(AgentFailure::StaleContext)?;
    let expires_at = DateTime::<Utc>::from_timestamp_millis(view.expires_at_unix_ms)
        .ok_or(AgentFailure::StaleContext)?;
    check_read_window(deadline, cancellation)?;
    let current_grant = active_attention_grant(
        &vault.list_data_access_grants(128).await?,
        person_id,
        device_id,
        &consumer,
    )?;
    if current_grant.id() != grant.id()
        || current_grant.authority() != grant.authority()
        || current_grant.source() != grant.source()
    {
        return Err(AgentFailure::PolicyDenied);
    }
    let current_subject = vault
        .personal_grant_subject_fingerprint(current_grant.id())
        .await?;
    if current_subject != reviewed_subject || current_subject != native_subject {
        return Err(AgentFailure::AccessReviewRequired);
    }
    let policy = vault
        .personal_grant_consumer_policy(current_grant.id())
        .await?;
    let (observation_id, process_incarnation_id) = local_context
        .commit_trusted_attention_projection_for_host(
            person_id,
            &host_epoch,
            device_id,
            &view,
            &native_subject,
        )?;
    let source = current_grant.source().clone();
    let fingerprint = query_fingerprint(
        person_id,
        device_id,
        &view,
        observation_id,
        process_incarnation_id,
    );
    let dependency = ContextDependency::try_new(
        person_id,
        current_grant.id(),
        current_grant.authority(),
        source,
        vec![ResourceHandle::try_new(ATTENTION_RESOURCE).map_err(|_| AgentFailure::InvalidInput)?],
        vec![GrantDataCategory::Derived],
        GrantOperation::Read,
        GrantPurpose::Assistant,
        consumer,
        ProcessingRestriction::LocalOnly,
        policy,
        observation_id,
        fingerprint,
        lease_invocation_id,
        process_incarnation_id,
        observed_at,
        expires_at,
    )
    .map_err(|_| AgentFailure::InvalidInput)?;
    check_read_window(deadline, cancellation)?;
    Ok((view, dependency))
}

pub(crate) struct PersonalDependencyResolver<'a, Keys: VaultKeyProvider> {
    pub(crate) vault: &'a EncryptedAgentVault<Keys>,
    pub(crate) local_context: &'a crate::local_context::LocalContextStore,
    pub(crate) person_id: PersonId,
    pub(crate) device_id: &'a str,
}

pub(crate) struct PersonalDependencyLiveness<'a> {
    pub(crate) local_context: &'a crate::local_context::LocalContextStore,
    pub(crate) person_id: PersonId,
    pub(crate) device_id: &'a str,
}

impl GovernedDependencyLiveness for PersonalDependencyLiveness<'_> {
    fn validate(&self, dependency: &ContextDependency) -> Result<(), AgentFailure> {
        if dependency.person_id() != self.person_id
            || dependency.source().person_id() != self.person_id
        {
            return Err(AgentFailure::PolicyDenied);
        }
        if dependency.source().connector().as_str() == ATTENTION_CONNECTOR {
            if dependency.source().connection_id().as_str() != ATTENTION_CONNECTION
                || dependency.source().execution_owner().as_str() != execution_owner(self.device_id)
                || !matches!(
                    dependency.consumer().identifier(),
                    ATTENTION_ASSISTANT_CONSUMER | ATTENTION_EXPERT_CONSUMER
                )
            {
                return Err(AgentFailure::PolicyDenied);
            }
            let (view, subject) = self.local_context.trusted_attention_observation(
                self.person_id,
                self.device_id,
                dependency.observation_id(),
                dependency.process_incarnation_id(),
            )?;
            if dependency.observed_at().timestamp_millis() != view.observed_at_unix_ms
                || dependency.expires_at().timestamp_millis() != view.expires_at_unix_ms
                || dependency.query_fingerprint()
                    != query_fingerprint(
                        self.person_id,
                        self.device_id,
                        &view,
                        dependency.observation_id(),
                        dependency.process_incarnation_id(),
                    )
                || subject.is_empty()
            {
                return Err(AgentFailure::PolicyDenied);
            }
            return Ok(());
        }
        if !matches!(
            dependency.source().connector().as_str(),
            "contacts.apple" | "contacts.android"
        ) || dependency.source().connection_id().as_str()
            != contacts_connection(dependency.source().connector().as_str())
            || dependency.source().execution_owner().as_str()
                != contacts_execution_owner(
                    dependency.source().connector().as_str(),
                    self.device_id,
                )
            || !matches!(
                dependency.consumer().identifier(),
                "assistant" | "contacts.expert"
            )
            || dependency.operation() != GrantOperation::Read
            || dependency.purpose() != GrantPurpose::Assistant
            || dependency.processing() != &ProcessingRestriction::LocalOnly
            || dependency.resources()
                != [ResourceHandle::try_new(PEOPLE_RESOURCE)
                    .map_err(|_| AgentFailure::PolicyDenied)?]
            || dependency.categories() != [GrantDataCategory::Derived]
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let observation = self.local_context.trusted_personal_observation(
            self.person_id,
            self.device_id,
            dependency.observation_id(),
            dependency.process_incarnation_id(),
        )?;
        if dependency.observed_at().timestamp_millis() != observation.observed_at_unix_ms
            || dependency.expires_at().timestamp_millis() != observation.expires_at_unix_ms
            || dependency.query_fingerprint() != observation.query_fingerprint
            || observation.native_subject_fingerprint.is_empty()
        {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(())
    }
}

impl<Keys: VaultKeyProvider> GovernedDependencyResolver for PersonalDependencyResolver<'_, Keys> {
    fn authorize<'a>(
        &'a self,
        dependency: &'a ContextDependency,
        request: &'a ModelRequest,
    ) -> Pin<Box<dyn Future<Output = Result<(), AgentFailure>> + Send + 'a>> {
        Box::pin(async move {
            if matches!(
                dependency.source().connector().as_str(),
                "contacts.apple" | "contacts.android"
            ) {
                if request.policy.allowed_placements != [ModelPlacement::DeviceLocal]
                    || dependency.source().connection_id().as_str()
                        != contacts_connection(dependency.source().connector().as_str())
                    || dependency.source().execution_owner().as_str()
                        != contacts_execution_owner(
                            dependency.source().connector().as_str(),
                            self.device_id,
                        )
                    || dependency.operation() != GrantOperation::Read
                    || dependency.purpose() != GrantPurpose::Assistant
                    || dependency.processing() != &ProcessingRestriction::LocalOnly
                    || dependency.resources()
                        != [ResourceHandle::try_new(PEOPLE_RESOURCE)
                            .map_err(|_| AgentFailure::PolicyDenied)?]
                    || dependency.categories() != [GrantDataCategory::Derived]
                    || !matches!(
                        dependency.consumer().identifier(),
                        "assistant" | "contacts.expert"
                    )
                    || dependency.lease_invocation_id().is_nil()
                {
                    return Err(AgentFailure::PolicyDenied);
                }
                let grants = self.vault.list_data_access_grants(128).await?;
                let grant =
                    active_people_grant(&grants, dependency.source(), dependency.consumer())?;
                if dependency.grant_id() != grant.id()
                    || dependency.grant_authority() != grant.authority()
                    || dependency.source() != grant.source()
                {
                    return Err(AgentFailure::PolicyDenied);
                }
                let observation = self
                    .local_context
                    .trusted_personal_observation(
                        self.person_id,
                        self.device_id,
                        dependency.observation_id(),
                        dependency.process_incarnation_id(),
                    )
                    .map_err(|_| AgentFailure::PolicyDenied)?;
                let reviewed_subject = self
                    .vault
                    .personal_grant_subject_fingerprint(grant.id())
                    .await?;
                if observation.native_subject_fingerprint != reviewed_subject
                    || dependency.observed_at().timestamp_millis()
                        != observation.observed_at_unix_ms
                    || dependency.expires_at().timestamp_millis() != observation.expires_at_unix_ms
                    || dependency.query_fingerprint() != observation.query_fingerprint
                {
                    return Err(AgentFailure::PolicyDenied);
                }
                let policy = self
                    .vault
                    .personal_grant_consumer_policy(grant.id())
                    .await?;
                if dependency.consumer_policy() != policy {
                    return Err(AgentFailure::PolicyDenied);
                }
                return Ok(());
            }
            if request.policy.allowed_placements != [ModelPlacement::DeviceLocal]
                || dependency.person_id() != self.person_id
                || dependency.source().person_id() != self.person_id
                || dependency.source().connector().as_str() != ATTENTION_CONNECTOR
                || dependency.source().connection_id().as_str() != ATTENTION_CONNECTION
                || dependency.source().execution_owner().as_str() != execution_owner(self.device_id)
                || dependency.operation() != GrantOperation::Read
                || dependency.purpose() != GrantPurpose::Assistant
                || dependency.processing() != &ProcessingRestriction::LocalOnly
                || dependency.resources().len() != 1
                || dependency.resources()[0].as_str() != ATTENTION_RESOURCE
                || dependency.categories() != [GrantDataCategory::Derived]
                || dependency.consumer().identifier() != ATTENTION_ASSISTANT_CONSUMER
                    && dependency.consumer().identifier() != ATTENTION_EXPERT_CONSUMER
            {
                return Err(AgentFailure::PolicyDenied);
            }
            let grants = self.vault.list_data_access_grants(128).await?;
            let grant = active_attention_grant(
                &grants,
                self.person_id,
                self.device_id,
                dependency.consumer(),
            )?;
            if dependency.grant_id() != grant.id()
                || dependency.grant_authority() != grant.authority()
                || dependency.source() != grant.source()
                || dependency.operation() != GrantOperation::Read
                || dependency.purpose() != GrantPurpose::Assistant
                || dependency.processing() != &ProcessingRestriction::LocalOnly
                || dependency
                    .resources()
                    .iter()
                    .any(|item| !grant.scope().resources().contains(item))
                || dependency
                    .categories()
                    .iter()
                    .any(|item| !grant.scope().categories().contains(item))
            {
                return Err(AgentFailure::PolicyDenied);
            }
            let (current_view, trusted_observation_subject) = self
                .local_context
                .trusted_attention_observation(
                    self.person_id,
                    self.device_id,
                    dependency.observation_id(),
                    dependency.process_incarnation_id(),
                )
                .map_err(|_| AgentFailure::PolicyDenied)?;
            let reviewed_subject = self
                .vault
                .personal_grant_subject_fingerprint(grant.id())
                .await?;
            if trusted_observation_subject != reviewed_subject
                || dependency.lease_invocation_id().is_nil()
                || dependency.observed_at().timestamp_millis() != current_view.observed_at_unix_ms
                || dependency.expires_at().timestamp_millis() != current_view.expires_at_unix_ms
                || dependency.query_fingerprint()
                    != query_fingerprint(
                        self.person_id,
                        self.device_id,
                        &current_view,
                        dependency.observation_id(),
                        dependency.process_incarnation_id(),
                    )
            {
                return Err(AgentFailure::PolicyDenied);
            }
            let policy = self
                .vault
                .personal_grant_consumer_policy(grant.id())
                .await?;
            if dependency.consumer_policy() != policy {
                return Err(AgentFailure::PolicyDenied);
            }
            let probe = self
                .local_context
                .acquire_attention(
                    attention_request(
                        self.person_id,
                        self.device_id,
                        self.local_context
                            .attention_acquisition_host_epoch(self.person_id)
                            .map_err(|_| AgentFailure::PolicyDenied)?,
                        LocalContextAttentionAcquisitionModeDto::InspectSubject,
                        None,
                        Some(request.deadline),
                    )
                    .map_err(|_| AgentFailure::PolicyDenied)?,
                    request.cancellation.clone(),
                )
                .await
                .map_err(|_| AgentFailure::PolicyDenied)?;
            if probe.native_subject_fingerprint_before != reviewed_subject
                || probe.native_subject_fingerprint_after != reviewed_subject
            {
                return Err(AgentFailure::PolicyDenied);
            }
            Ok(())
        })
    }
}

pub(crate) async fn apply<Keys: VaultKeyProvider>(
    vault: &EncryptedAgentVault<Keys>,
    local_context: &crate::local_context::LocalContextStore,
    person_id: PersonId,
    request: PersonalAccessConfigurationDto,
    cancellation: floe_agent::Cancellation,
) -> Result<PersonalAccessOverviewDto, AgentFailure> {
    validate_request(&request)?;
    if request.connector != ATTENTION_CONNECTOR {
        return Err(AgentFailure::CapabilityUnavailable);
    }
    let presence = local_context
        .attention_observation(person_id, &request.device_id)
        .ok();
    let mut grants = vault.list_data_access_grants(128).await?;
    let matching: Vec<_> = grants
        .drain(..)
        .filter(|grant| matches_source(grant, person_id, &request.device_id))
        .collect();
    let existing = matching
        .iter()
        .find(|grant| grant.state() != GrantState::Revoked)
        .or_else(|| {
            matching
                .iter()
                .find(|grant| grant.state() == GrantState::Revoked)
        })
        .cloned();
    match request.change {
        PersonalAccessChangeDto::Inspect {} => {
            let result = local_context
                .acquire_attention(
                    attention_request(
                        person_id,
                        &request.device_id,
                        local_context.attention_acquisition_host_epoch(person_id)?,
                        LocalContextAttentionAcquisitionModeDto::InspectSubject,
                        None,
                        None,
                    )?,
                    cancellation.clone(),
                )
                .await?;
            Ok(overview(
                person_id,
                &request.device_id,
                existing.as_ref(),
                presence,
                Some(result.native_subject_fingerprint_before),
            ))
        }
        PersonalAccessChangeDto::Review {
            expected_native_subject_fingerprint,
            consumers,
            expected_grant_id,
            expected_grant_authority,
        } => {
            let inspected = local_context
                .acquire_attention(
                    attention_request(
                        person_id,
                        &request.device_id,
                        local_context.attention_acquisition_host_epoch(person_id)?,
                        LocalContextAttentionAcquisitionModeDto::InspectSubject,
                        None,
                        None,
                    )?,
                    cancellation.clone(),
                )
                .await?;
            if inspected.native_subject_fingerprint_before != expected_native_subject_fingerprint {
                return Err(AgentFailure::AccessReviewRequired);
            }
            let source_authority = match existing.as_ref() {
                Some(grant)
                    if vault.personal_grant_subject_fingerprint(grant.id()).await?
                        == expected_native_subject_fingerprint =>
                {
                    grant.source().source_authority()
                }
                Some(_) | None => SourceAuthority::new(),
            };
            let (source, scope) = source_and_scope(
                person_id,
                &request.device_id,
                source_authority,
                reviewed_consumers(&consumers)?,
            )?;
            let expected = match (expected_grant_id, expected_grant_authority) {
                (Some(id), Some(authority)) => Some((id, authority)),
                (None, None) => None,
                _ => return Err(AgentFailure::InvalidInput),
            };
            let grant = match existing {
                Some(_grant) => {
                    vault
                        .review_personal_grant(
                            source,
                            scope,
                            &expected_native_subject_fingerprint,
                            expected,
                        )
                        .await?
                }
                None => {
                    vault
                        .review_personal_grant(
                            source,
                            scope,
                            &expected_native_subject_fingerprint,
                            expected,
                        )
                        .await?
                }
            };
            Ok(overview(
                person_id,
                &request.device_id,
                Some(&grant),
                presence,
                Some(expected_native_subject_fingerprint),
            ))
        }
        PersonalAccessChangeDto::SetEnabled { enabled } => {
            let grant = existing.ok_or(AgentFailure::AccessReviewRequired)?;
            if enabled {
                let inspected = local_context
                    .acquire_attention(
                        attention_request(
                            person_id,
                            &request.device_id,
                            local_context.attention_acquisition_host_epoch(person_id)?,
                            LocalContextAttentionAcquisitionModeDto::InspectSubject,
                            None,
                            None,
                        )?,
                        cancellation.clone(),
                    )
                    .await?;
                let fingerprint = vault.personal_grant_subject_fingerprint(grant.id()).await?;
                if inspected.native_subject_fingerprint_before != fingerprint {
                    return Err(AgentFailure::AccessReviewRequired);
                }
                let (source, scope) = source_and_scope(
                    person_id,
                    &request.device_id,
                    grant.source().source_authority(),
                    grant
                        .scope()
                        .consumers()
                        .iter()
                        .map(|consumer| consumer.identifier().to_owned())
                        .collect(),
                )?;
                let grant = vault
                    .review_personal_grant(
                        source,
                        scope,
                        &fingerprint,
                        Some((grant.id(), grant.authority())),
                    )
                    .await?;
                Ok(overview(
                    person_id,
                    &request.device_id,
                    Some(&grant),
                    presence,
                    Some(fingerprint),
                ))
            } else {
                let grant = vault
                    .pause_personal_grant(grant.id(), grant.authority())
                    .await?;
                Ok(overview(
                    person_id,
                    &request.device_id,
                    Some(&grant),
                    presence,
                    None,
                ))
            }
        }
    }
}

pub(crate) async fn apply_contacts<Keys: VaultKeyProvider>(
    vault: &EncryptedAgentVault<Keys>,
    local_context: &crate::local_context::LocalContextStore,
    person_id: PersonId,
    request: ContactsAccessConfigurationDto,
    cancellation: floe_agent::Cancellation,
) -> Result<PersonalAccessOverviewDto, AgentFailure> {
    validate_contacts_request(&request)?;
    let source = contacts_source(
        person_id,
        &request.device_id,
        &request.connector,
        SourceAuthority::new(),
    )?;
    let grants = vault.list_data_access_grants(128).await?;
    let existing = grants
        .iter()
        .filter(|grant| {
            grant.source().person_id() == person_id
                && grant.source().connector() == source.connector()
        })
        .find(|grant| {
            grant.source().connection_id() == source.connection_id()
                && grant.source().execution_owner() == source.execution_owner()
                && grant.state() != GrantState::Revoked
        })
        .cloned();
    match request.change {
        ContactsAccessChangeDto::Inspect { selected_handles } => {
            let result = acquire_people_subject(
                local_context,
                person_id,
                &request.device_id,
                selected_handles.clone(),
                None,
                cancellation,
            )
            .await?;
            Ok(contacts_overview(
                person_id,
                &request.device_id,
                &request.connector,
                existing.as_ref(),
                Some(result.native_subject_fingerprint_before),
            ))
        }
        ContactsAccessChangeDto::Review {
            mut selected_handles,
            expected_native_subject_fingerprint,
            consumers,
            expected_grant_id,
            expected_grant_authority,
        } => {
            selected_handles.sort();
            let result = acquire_people_subject(
                local_context,
                person_id,
                &request.device_id,
                selected_handles.clone(),
                Some(expected_native_subject_fingerprint.clone()),
                cancellation,
            )
            .await?;
            if result.native_subject_fingerprint_before != expected_native_subject_fingerprint
                || result.native_subject_fingerprint_after != expected_native_subject_fingerprint
            {
                return Err(AgentFailure::AccessReviewRequired);
            }
            let expected = match (expected_grant_id, expected_grant_authority) {
                (Some(id), Some(authority)) => Some((id, authority)),
                (None, None) => None,
                _ => return Err(AgentFailure::InvalidInput),
            };
            let consumers = reviewed_contacts_consumers(&consumers)?;
            let authority = existing
                .as_ref()
                .map(|grant| grant.source().source_authority())
                .unwrap_or_else(SourceAuthority::new);
            let source =
                contacts_source(person_id, &request.device_id, &request.connector, authority)?;
            let scope = contacts_scope(consumers)?;
            let grant = vault
                .review_personal_grant_with_selection(
                    source,
                    scope,
                    &expected_native_subject_fingerprint,
                    expected,
                    &selected_handles,
                )
                .await?;
            Ok(contacts_overview(
                person_id,
                &request.device_id,
                &request.connector,
                Some(&grant),
                Some(expected_native_subject_fingerprint),
            ))
        }
        ContactsAccessChangeDto::SetEnabled { enabled } => {
            let grant = existing.ok_or(AgentFailure::AccessReviewRequired)?;
            if !enabled {
                let grant = vault
                    .pause_personal_grant(grant.id(), grant.authority())
                    .await?;
                Ok(contacts_overview(
                    person_id,
                    &request.device_id,
                    &request.connector,
                    Some(&grant),
                    None,
                ))
            } else {
                let selected_handles = vault.personal_grant_selected_handles(grant.id()).await?;
                if selected_handles.is_empty() {
                    return Err(AgentFailure::AccessReviewRequired);
                }
                let fingerprint = vault.personal_grant_subject_fingerprint(grant.id()).await?;
                let inspected = acquire_people_subject(
                    local_context,
                    person_id,
                    &request.device_id,
                    selected_handles.clone(),
                    Some(fingerprint.clone()),
                    cancellation,
                )
                .await?;
                if inspected.native_subject_fingerprint_before != fingerprint
                    || inspected.native_subject_fingerprint_after != fingerprint
                {
                    return Err(AgentFailure::AccessReviewRequired);
                }
                let source = contacts_source(
                    person_id,
                    &request.device_id,
                    &request.connector,
                    grant.source().source_authority(),
                )?;
                let consumers = grant
                    .scope()
                    .consumers()
                    .iter()
                    .map(|consumer| consumer.identifier().to_owned())
                    .collect();
                let grant = vault
                    .review_personal_grant_with_selection(
                        source,
                        contacts_scope(consumers)?,
                        &fingerprint,
                        Some((grant.id(), grant.authority())),
                        &selected_handles,
                    )
                    .await?;
                Ok(contacts_overview(
                    person_id,
                    &request.device_id,
                    &request.connector,
                    Some(&grant),
                    Some(fingerprint),
                ))
            }
        }
    }
}

async fn acquire_people_subject(
    local_context: &crate::local_context::LocalContextStore,
    person_id: PersonId,
    device_id: &str,
    mut selected_handles: Vec<String>,
    expected_native_subject_fingerprint: Option<String>,
    cancellation: floe_agent::Cancellation,
) -> Result<LocalContextPersonalAcquisitionResultDto, AgentFailure> {
    if selected_handles.is_empty() || selected_handles.len() > 64 {
        return Err(AgentFailure::InvalidInput);
    }
    selected_handles.sort();
    if selected_handles.windows(2).any(|pair| pair[0] >= pair[1])
        || selected_handles
            .iter()
            .any(|value| value.is_empty() || value.chars().any(char::is_whitespace))
    {
        return Err(AgentFailure::InvalidInput);
    }
    let host_epoch = local_context.personal_acquisition_host_epoch(person_id)?;
    local_context
        .acquire_personal(
            LocalContextPersonalAcquisitionRequestDto {
                request_id: Uuid::new_v4().to_string(),
                host_epoch,
                person_id: person_id.to_string(),
                device_id: device_id.to_owned(),
                domain: LocalContextPersonalDomainDto::People,
                selected_handles,
                event_handle: None,
                evidence_handles: Vec::new(),
                destination_latitude: None,
                destination_longitude: None,
                event_start_unix_ms: None,
                event_end_unix_ms: None,
                travel_mode: None,
                deadline_unix_ms: Utc::now().timestamp_millis().saturating_add(30_000),
                expected_native_subject_fingerprint,
            },
            cancellation,
        )
        .await
}

fn validate_contacts_request(request: &ContactsAccessConfigurationDto) -> Result<(), AgentFailure> {
    if !matches!(
        request.connector.as_str(),
        "contacts.apple" | "contacts.android"
    ) || request.device_id.is_empty()
        || request.device_id.len() > 128
        || request.device_id.chars().any(char::is_whitespace)
    {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(())
}

fn contacts_source(
    person_id: PersonId,
    device_id: &str,
    connector: &str,
    authority: SourceAuthority,
) -> Result<GrantSourceBinding, AgentFailure> {
    if !connector.starts_with("contacts.") {
        return Err(AgentFailure::InvalidInput);
    }
    GrantSourceBinding::try_new(
        person_id,
        ConnectionId::try_new(contacts_connection(connector))
            .map_err(|_| AgentFailure::InvalidInput)?,
        ConnectorId::try_new(connector).map_err(|_| AgentFailure::InvalidInput)?,
        ExecutionOwnerId::try_new(contacts_execution_owner(connector, device_id))
            .map_err(|_| AgentFailure::InvalidInput)?,
        authority,
    )
    .map_err(|_| AgentFailure::InvalidInput)
}

fn contacts_connection(connector: &str) -> String {
    format!("{connector}.local")
}

fn contacts_execution_owner(connector: &str, device_id: &str) -> String {
    let platform = connector.strip_prefix("contacts.").unwrap_or("unknown");
    format!("{platform}:{device_id}")
}

fn contacts_scope(consumer_names: Vec<String>) -> Result<GrantScope, AgentFailure> {
    let consumers = consumer_names
        .into_iter()
        .map(GrantConsumer::builtin)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AgentFailure::InvalidInput)?;
    GrantScope::try_new(
        vec![ResourceHandle::try_new(PEOPLE_RESOURCE).map_err(|_| AgentFailure::InvalidInput)?],
        vec![GrantDataCategory::Derived],
        vec![GrantOperation::Read],
        vec![GrantPurpose::Assistant],
        consumers,
        ProcessingRestriction::LocalOnly,
    )
    .map_err(|_| AgentFailure::InvalidInput)
}

fn reviewed_contacts_consumers(consumers: &[String]) -> Result<Vec<String>, AgentFailure> {
    if consumers.is_empty()
        || consumers.len() > 2
        || consumers
            .iter()
            .any(|value| !matches!(value.as_str(), "assistant" | "contacts.expert"))
    {
        return Err(AgentFailure::InvalidInput);
    }
    let mut values = consumers.to_vec();
    values.sort();
    values.dedup();
    if values.len() == consumers.len() {
        Ok(values)
    } else {
        Err(AgentFailure::InvalidInput)
    }
}

fn contacts_overview(
    person_id: PersonId,
    device_id: &str,
    connector: &str,
    grant: Option<&DataAccessGrant>,
    native_subject_fingerprint: Option<String>,
) -> PersonalAccessOverviewDto {
    PersonalAccessOverviewDto {
        schema_version: 1,
        person_id: person_id.to_string(),
        connector: connector.to_owned(),
        device_id: device_id.to_owned(),
        connection_id: contacts_connection(connector),
        source_authority: grant.map(|value| value.source().source_authority()),
        grant_id: grant.map(DataAccessGrant::id),
        grant_authority: grant.map(DataAccessGrant::authority),
        state: grant.map_or_else(
            || "needs_review".to_owned(),
            |value| match value.state() {
                GrantState::Paused => "paused".to_owned(),
                GrantState::Active => "active".to_owned(),
                GrantState::Revoked => "revoked".to_owned(),
            },
        ),
        review_required: grant.is_none_or(DataAccessGrant::review_required),
        presence_available: false,
        consumers: grant
            .map(|value| {
                value
                    .scope()
                    .consumers()
                    .iter()
                    .map(|consumer| consumer.identifier().to_owned())
                    .collect()
            })
            .unwrap_or_default(),
        native_subject_fingerprint,
        process_incarnation: None,
    }
}

pub(crate) fn validate_request(
    request: &PersonalAccessConfigurationDto,
) -> Result<(), AgentFailure> {
    if request.connector != ATTENTION_CONNECTOR
        || request.device_id.is_empty()
        || request.device_id.len() > 128
        || request.device_id.chars().any(char::is_whitespace)
    {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(())
}

fn attention_request(
    person_id: PersonId,
    device_id: &str,
    host_epoch: String,
    mode: LocalContextAttentionAcquisitionModeDto,
    expected_native_subject_fingerprint: Option<String>,
    caller_deadline: Option<tokio::time::Instant>,
) -> Result<LocalContextAttentionAcquisitionRequestDto, AgentFailure> {
    let remaining = caller_deadline
        .map(|deadline| deadline.saturating_duration_since(tokio::time::Instant::now()))
        .unwrap_or_else(|| std::time::Duration::from_secs(30))
        .min(std::time::Duration::from_secs(30));
    let deadline_unix_ms = chrono::Utc::now()
        .timestamp_millis()
        .checked_add(
            i64::try_from(remaining.as_millis()).map_err(|_| AgentFailure::DeadlineExceeded)?,
        )
        .ok_or(AgentFailure::DeadlineExceeded)?;
    Ok(LocalContextAttentionAcquisitionRequestDto {
        request_id: Uuid::new_v4().to_string(),
        host_epoch,
        person_id: person_id.to_string(),
        device_id: device_id.to_owned(),
        mode,
        deadline_unix_ms,
        expected_native_subject_fingerprint,
    })
}

pub(crate) fn matches_source(
    grant: &DataAccessGrant,
    person_id: PersonId,
    device_id: &str,
) -> bool {
    grant.source().person_id() == person_id
        && grant.source().connector().as_str() == ATTENTION_CONNECTOR
        && grant.source().connection_id().as_str() == ATTENTION_CONNECTION
        && grant.source().execution_owner().as_str() == execution_owner(device_id)
}

pub(crate) fn source_and_scope(
    person_id: PersonId,
    device_id: &str,
    authority: SourceAuthority,
    consumer_names: Vec<String>,
) -> Result<(GrantSourceBinding, GrantScope), AgentFailure> {
    let source = GrantSourceBinding::try_new(
        person_id,
        ConnectionId::try_new(ATTENTION_CONNECTION).map_err(|_| AgentFailure::InvalidInput)?,
        ConnectorId::try_new(ATTENTION_CONNECTOR).map_err(|_| AgentFailure::InvalidInput)?,
        ExecutionOwnerId::try_new(execution_owner(device_id))
            .map_err(|_| AgentFailure::InvalidInput)?,
        authority,
    )
    .map_err(|_| AgentFailure::InvalidInput)?;
    let consumers = consumer_names
        .into_iter()
        .map(GrantConsumer::builtin)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AgentFailure::InvalidInput)?;
    let scope = GrantScope::try_new(
        vec![ResourceHandle::try_new(ATTENTION_RESOURCE).map_err(|_| AgentFailure::InvalidInput)?],
        vec![GrantDataCategory::Derived],
        vec![GrantOperation::Read],
        vec![GrantPurpose::Assistant],
        consumers,
        ProcessingRestriction::LocalOnly,
    )
    .map_err(|_| AgentFailure::InvalidInput)?;
    Ok((source, scope))
}

fn reviewed_consumers(consumers: &[String]) -> Result<Vec<String>, AgentFailure> {
    if consumers.is_empty()
        || consumers.len() > 2
        || consumers.iter().any(|consumer| {
            !matches!(
                consumer.as_str(),
                ATTENTION_ASSISTANT_CONSUMER | ATTENTION_EXPERT_CONSUMER
            )
        })
    {
        return Err(AgentFailure::InvalidInput);
    }
    let mut values = consumers.to_vec();
    values.sort();
    values.dedup();
    if values.len() == consumers.len() {
        Ok(values)
    } else {
        Err(AgentFailure::InvalidInput)
    }
}

pub(crate) fn overview(
    person_id: PersonId,
    device_id: &str,
    grant: Option<&DataAccessGrant>,
    presence: Option<(floe_agent::AttentionView, Uuid, Uuid)>,
    native_subject_fingerprint: Option<String>,
) -> PersonalAccessOverviewDto {
    PersonalAccessOverviewDto {
        schema_version: 1,
        person_id: person_id.to_string(),
        connector: ATTENTION_CONNECTOR.into(),
        device_id: device_id.into(),
        connection_id: ATTENTION_CONNECTION.into(),
        source_authority: grant.map(|grant| grant.source().source_authority()),
        grant_id: grant.map(|grant| grant.id()),
        grant_authority: grant.map(|grant| grant.authority()),
        state: grant.map_or_else(
            || "needs_review".into(),
            |grant| match grant.state() {
                GrantState::Paused => "paused".into(),
                GrantState::Active => "active".into(),
                GrantState::Revoked => "revoked".into(),
            },
        ),
        review_required: grant.is_none_or(DataAccessGrant::review_required),
        presence_available: presence.is_some(),
        consumers: grant
            .map(|value| {
                value
                    .scope()
                    .consumers()
                    .iter()
                    .map(|consumer| consumer.identifier().to_owned())
                    .collect()
            })
            .unwrap_or_default(),
        native_subject_fingerprint,
        process_incarnation: presence.map(|(_, _, process)| process),
    }
}

fn execution_owner(device_id: &str) -> String {
    format!("macos:{device_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attention() -> AttentionView {
        AttentionView {
            schema_version: 1,
            view_id: "attention.coarse".into(),
            source_handle: "attention.macos:session_idle".into(),
            observed_at_unix_ms: 1_000,
            expires_at_unix_ms: 2_000,
            state: AttentionState::Available,
            confidence_millis: 900,
            evidence_handles: vec!["attention:aggregate".into()],
        }
    }

    #[test]
    fn review_consumers_are_finite_and_canonical() {
        assert_eq!(
            reviewed_consumers(&[
                ATTENTION_EXPERT_CONSUMER.into(),
                ATTENTION_ASSISTANT_CONSUMER.into()
            ])
            .unwrap(),
            vec![ATTENTION_ASSISTANT_CONSUMER, ATTENTION_EXPERT_CONSUMER]
        );
        assert_eq!(
            reviewed_consumers(&[
                ATTENTION_ASSISTANT_CONSUMER.into(),
                ATTENTION_ASSISTANT_CONSUMER.into()
            ]),
            Err(AgentFailure::InvalidInput)
        );
        assert_eq!(
            reviewed_consumers(&["unknown".into()]),
            Err(AgentFailure::InvalidInput)
        );
    }

    #[test]
    fn subject_fingerprint_is_stable_across_observations() {
        let person = PersonId::new();
        let view = attention();
        let first = subject_fingerprint(person, "device", &view);
        let second = subject_fingerprint(person, "device", &view);
        assert_eq!(first, second);
        assert_ne!(
            query_fingerprint(person, "device", &view, Uuid::new_v4(), Uuid::new_v4()),
            query_fingerprint(person, "device", &view, Uuid::new_v4(), Uuid::new_v4())
        );
    }
}
