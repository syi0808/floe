use std::{future::Future, pin::Pin};

use chrono::{DateTime, Utc};
use floe_agent_contract::{AgentFailure, ModelPlacement};
use floe_conversation::{ModelRequest};
use floe_context::{
    is_remote_view, remote_view_connector_admissible, remote_view_data_category,
    remote_view_resource, validate_remote_view, validate_remote_view_query,
};
use floe_vault::{EncryptedAgentVault, GovernedDependencyLiveness, GovernedDependencyResolver, RemoteCalendarAuthorizationExpectation, RemoteProducerIdentity, RemoteViewSourceReference, VaultKeyProvider};
use floe_access::{DataAccessGrant, GrantState, active_resource_grant};
use floe_context_contract::{ContextDependency, GrantConsumer, GrantOperation, GrantPurpose, GrantScope, ProcessingRestriction, ResourceHandle};

const ASSISTANT_PURPOSE: GrantPurpose = GrantPurpose::Assistant;
use floe_provider_adapters::control::RemoteViewAuthorizationRequest;
use floe_provider_adapters::sources::ServerSourceClient;
use floe_protocol::AgentRemoteRouteDto;
use serde_json::Value;
use uuid::Uuid;

pub(crate) struct RemoteViewReader<'a, Keys: VaultKeyProvider> {
    pub(crate) vault: &'a EncryptedAgentVault<Keys>,
    pub(crate) source_client: &'a ServerSourceClient,
    pub(crate) person_id: floe_kernel::PersonId,
    pub(crate) client_id: &'a str,
    pub(crate) device_id: &'a str,
    pub(crate) route: &'a AgentRemoteRouteDto,
}

pub(crate) struct RemoteDependencyResolver<'a, Keys: VaultKeyProvider> {
    pub(crate) reader: &'a RemoteViewReader<'a, Keys>,
}

pub(crate) struct RemoteViewGrantPreview {
    pub(crate) reference: RemoteViewSourceReference,
    pub(crate) producer: floe_provider_adapters::control::authorization::ProducerIdentityResponse,
    pub(crate) connection_revision: u64,
    pub(crate) consumer: String,
}

pub(crate) async fn preview_remote_view_grant<Keys: VaultKeyProvider>(
    vault: &EncryptedAgentVault<Keys>,
    route: &AgentRemoteRouteDto,
    person_id: floe_kernel::PersonId,
    view_id: &str,
    connector_id: &str,
    connection_id: &str,
    resource: &str,
    consumer_name: &str,
    deadline: tokio::time::Instant,
    cancellation: &floe_execution::Cancellation,
) -> Result<RemoteViewGrantPreview, AgentFailure> {
    let pairing = route.pairing.as_ref().ok_or(AgentFailure::PolicyDenied)?;
    if pairing.person_id != person_id.to_string()
        || pairing.client_id.is_empty()
        || pairing.device_id.is_empty()
        || remote_view_resource(view_id, connection_id) != resource
        || !is_remote_view(view_id)
        || consumer_name.trim().is_empty()
    {
        return Err(AgentFailure::PolicyDenied);
    }
    let consumer = GrantConsumer::builtin(consumer_name).map_err(|_| AgentFailure::InvalidInput)?;
    let client = floe_provider_adapters::control::RemoteAuthorizationClient::new(route)?;
    let producer = client.producer_identity(deadline, cancellation).await?;
    let pinned = vault.remote_pinned_producer().await?;
    let observed = RemoteProducerIdentity {
        schema_version: producer.schema_version,
        instance_id: producer.instance_id.clone(),
        execution_owner: producer.execution_owner.clone(),
        audience: producer.audience.clone(),
        key_id: producer.key_id.clone(),
        public_key: producer.public_key.clone(),
        fingerprint: producer.fingerprint.clone(),
    };
    if pinned != observed {
        return Err(AgentFailure::PolicyDenied);
    }
    let preview = client
        .view_source_preview(
            view_id,
            connector_id,
            connection_id,
            resource,
            deadline,
            cancellation,
        )
        .await?;
    let reference = vault
        .verify_remote_view_source_preview(
            &preview.descriptor_b64url,
            &preview.producer_signature,
            &pairing.person_id,
            &pairing.client_id,
            &pairing.device_id,
            view_id,
            connector_id,
            connection_id,
            resource,
        )
        .await?;
    if reference.execution_owner != producer.execution_owner
        || reference.audience != producer.audience
        || reference.connection_revision != preview.connection_revision
    {
        return Err(AgentFailure::PolicyDenied);
    }
    let _ = consumer;
    Ok(RemoteViewGrantPreview {
        reference,
        producer,
        connection_revision: preview.connection_revision,
        consumer: consumer_name.to_owned(),
    })
}

pub(crate) async fn review_and_activate_remote_view_grant<Keys: VaultKeyProvider>(
    vault: &EncryptedAgentVault<Keys>,
    route: &AgentRemoteRouteDto,
    person_id: floe_kernel::PersonId,
    view_id: &str,
    connector_id: &str,
    connection_id: &str,
    resource: &str,
    consumer_name: &str,
    expected_producer_fingerprint: &str,
    expected_source_authority: floe_context_contract::SourceAuthority,
    expected_connection_revision: u64,
    expected_provider_identity: &str,
    expected_recipient: &str,
    deadline: tokio::time::Instant,
    cancellation: &floe_execution::Cancellation,
) -> Result<DataAccessGrant, AgentFailure> {
    let preview = preview_remote_view_grant(
        vault,
        route,
        person_id,
        view_id,
        connector_id,
        connection_id,
        resource,
        consumer_name,
        deadline,
        cancellation,
    )
    .await?;
    let producer = &preview.producer;
    if producer.fingerprint != expected_producer_fingerprint {
        return Err(AgentFailure::PolicyDenied);
    }
    if preview.reference.source_authority != expected_source_authority
        || preview.connection_revision != expected_connection_revision
        || preview.reference.provider_identity != expected_provider_identity
        || preview.producer.audience != expected_recipient
    {
        return Err(AgentFailure::PolicyDenied);
    }
    let consumer = GrantConsumer::builtin(consumer_name).map_err(|_| AgentFailure::InvalidInput)?;
    let category = remote_view_data_category(view_id);
    let scope = GrantScope::try_new(
        vec![ResourceHandle::try_new(resource).map_err(|_| AgentFailure::InvalidInput)?],
        vec![category.clone()],
        vec![GrantOperation::Read],
        vec![ASSISTANT_PURPOSE],
        vec![consumer],
        ProcessingRestriction::ApprovedRecipient {
            recipient: preview.producer.audience.clone(),
            categories: vec![category],
        },
    )
    .map_err(|_| AgentFailure::InvalidInput)?;
    let source = reference_to_source(&preview.reference)?;
    let existing = vault
        .find_remote_view_grant(view_id, &source, consumer_name)
        .await?;
    if let Some(grant) = existing.as_ref() {
        if grant.state() == GrantState::Active {
            if grant.scope() == &scope {
                return Ok(grant.clone());
            }
            return Err(AgentFailure::PolicyDenied);
        }
    }
    let expected = existing.as_ref().map(|grant| grant.authority());
    let grant_id = existing
        .as_ref()
        .map(|grant| grant.id())
        .unwrap_or_else(floe_context_contract::GrantId::new);
    vault
        .review_and_activate_remote_view_grant(view_id, grant_id, expected, source, scope, None)
        .await
}

impl<Keys: VaultKeyProvider> floe_context::SourceReader for RemoteViewReader<'_, Keys> {
    fn read<'a>(
        &'a self,
        request: &'a floe_context::SourceReadRequest,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<floe_context::SourceRead, AgentFailure>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            let (payload, dependency, scope) = self
                .read_view(
                    request.source().as_str(),
                    request.consumer().identifier(),
                    request.query().clone(),
                    request.deadline(),
                    request.cancellation(),
                    request.process_incarnation_id(),
                    request.query_fingerprint(),
                )
                .await?;
            Ok(floe_context::SourceRead::new(
                request.source().clone(),
                payload,
                dependency,
                scope,
            ))
        })
    }
}

impl<Keys: VaultKeyProvider> RemoteViewReader<'_, Keys> {
    async fn read_view(
        &self,
        view_id: &str,
        consumer_name: &str,
        query: Value,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
        process_incarnation_id: Uuid,
        query_fingerprint: &[u8],
    ) -> Result<(Value, ContextDependency, GrantScope), AgentFailure> {
        check_window(deadline, cancellation)?;
        let consumer =
            GrantConsumer::builtin(consumer_name).map_err(|_| AgentFailure::InvalidInput)?;
        let (max_items, max_bytes) = validate_remote_view_query(view_id, &query)?;
        let grants = self.vault.list_data_access_grants(128).await?;
        let grant = active_resource_grant(&grants, self.person_id, &consumer, |source| {
            remote_view_grant_resource(view_id, source)
        })?;
        let source = grant.source();
        let connection_id = source.connection_id();
        let connection_id_text = connection_id.as_str();
        let resource = remote_view_resource(view_id, connection_id_text);
        let client = self.source_client.authorization_client()?;
        let preview = client
            .view_source_preview(
                view_id,
                source.connector().as_str(),
                connection_id_text,
                &resource,
                deadline,
                cancellation,
            )
            .await?;
        let reference = self
            .vault
            .verify_remote_view_source_preview(
                &preview.descriptor_b64url,
                &preview.producer_signature,
                &self.person_id.to_string(),
                self.client_id,
                self.device_id,
                view_id,
                source.connector().as_str(),
                connection_id_text,
                &resource,
            )
            .await?;
        if reference.source_authority != source.source_authority()
            || reference.connection_revision == 0
            || reference.provider_identity.is_empty()
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let binding = self
            .vault
            .remote_view_grant_binding(
                view_id,
                source.connector().as_str(),
                connection_id_text,
                reference.source_authority,
            )
            .await?;
        if binding.grant.id() != grant.id()
            || binding.grant.authority() != grant.authority()
            || binding.grant.source() != source
            || !binding
                .grant
                .scope()
                .resources()
                .iter()
                .any(|item| item.as_str() == resource)
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let policy_incarnation = binding.consumer_policy.incarnation().to_string();
        let grant_id = grant.id().as_uuid().to_string();
        let grant_incarnation = grant.authority().incarnation().to_string();
        let path = format!("/v1/views/{view_id}/admit");
        let expected = RemoteCalendarAuthorizationExpectation {
            operation: "".into(),
            client_id: self.client_id.into(),
            device_id: self.device_id.into(),
            challenge_id: String::new(),
            admission_id: String::new(),
            query_sha256: String::new(),
            result_sha256: String::new(),
            grant_id: grant_id.clone(),
            grant_incarnation: grant_incarnation.clone(),
            grant_epoch: grant.authority().access_epoch().get(),
            source_connector: source.connector().as_str().into(),
            source_connection: connection_id_text.into(),
            source_execution_owner: source.execution_owner().as_str().into(),
            source_incarnation: source.source_authority().incarnation().to_string(),
            source_epoch: source.source_authority().epoch().get(),
            resources: vec![resource.clone()],
            max_items: u32::try_from(max_items).map_err(|_| AgentFailure::BudgetExceeded)?,
            max_bytes: u32::try_from(max_bytes).map_err(|_| AgentFailure::BudgetExceeded)?,
        };
        let request = RemoteViewAuthorizationRequest {
            path: &path,
            connector_id: source.connector().as_str(),
            connection_id: connection_id_text,
            connection_revision: reference.connection_revision,
            resource: &resource,
            policy_incarnation: &policy_incarnation,
            policy_epoch: binding.consumer_policy.epoch().get(),
            grant_id: &grant_id,
            grant_incarnation: &grant_incarnation,
            grant_epoch: grant.authority().access_epoch().get(),
            purpose: "assistant",
            consumer: consumer_name,
            max_items: u32::try_from(max_items).map_err(|_| AgentFailure::BudgetExceeded)?,
            max_bytes: u32::try_from(max_bytes).map_err(|_| AgentFailure::BudgetExceeded)?,
            query,
        };
        let value = self
            .source_client
            .read_authorized_view(self.vault, request, expected, deadline, cancellation)
            .await?;
        let now = Utc::now().timestamp_millis();
        let (value, observed, expires) = validate_remote_view(view_id, value, now, max_items, max_bytes)?;
        let observed =
            DateTime::<Utc>::from_timestamp_millis(observed).ok_or(AgentFailure::StaleContext)?;
        let expires =
            DateTime::<Utc>::from_timestamp_millis(expires).ok_or(AgentFailure::StaleContext)?;
        let process = Uuid::new_v4();
        let dependency = ContextDependency::try_new(
            self.person_id,
            binding.grant.id(),
            binding.grant.authority(),
            reference_to_source(&reference)?,
            vec![ResourceHandle::try_new(resource).map_err(|_| AgentFailure::InvalidInput)?],
            binding.grant.scope().categories().to_vec(),
            GrantOperation::Read,
            ASSISTANT_PURPOSE,
            consumer,
            binding.grant.scope().processing().clone(),
            binding.consumer_policy,
            Uuid::new_v4(),
            query_fingerprint.to_vec(),
            process_incarnation_id,
            process,
            observed,
            expires,
        )
        .map_err(|_| AgentFailure::InvalidInput)?;
        Ok((value, dependency, binding.grant.scope().clone()))
    }
}

impl<Keys: VaultKeyProvider> GovernedDependencyLiveness for RemoteDependencyResolver<'_, Keys> {
    fn validate(&self, dependency: &ContextDependency) -> Result<(), AgentFailure> {
        dependency
            .validate()
            .map_err(|_| AgentFailure::PolicyDenied)?;
        if dependency.person_id() != self.reader.person_id
            || dependency.source().person_id() != self.reader.person_id
            || dependency.operation() != GrantOperation::Read
            || dependency.purpose() != ASSISTANT_PURPOSE
            || dependency.source().execution_owner().as_str().is_empty()
            || Utc::now() >= dependency.expires_at()
        {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(())
    }
}

impl<Keys: VaultKeyProvider> GovernedDependencyResolver for RemoteDependencyResolver<'_, Keys> {
    fn authorize<'a>(
        &'a self,
        dependency: &'a ContextDependency,
        request: &'a ModelRequest,
    ) -> Pin<Box<dyn Future<Output = Result<(), AgentFailure>> + Send + 'a>> {
        Box::pin(async move {
            self.validate(dependency)?;
            if request.policy.allowed_placements != [ModelPlacement::Remote] {
                return Err(AgentFailure::PolicyDenied);
            }
            let grant = self
                .reader
                .vault
                .get_remote_view_grant(dependency.grant_id())
                .await?;
            if grant.authority() != dependency.grant_authority()
                || grant.source() != dependency.source()
                || grant.state() != GrantState::Active
                || grant.review_required()
                || !grant.scope().operations().contains(&GrantOperation::Read)
                || !grant.scope().purposes().contains(&ASSISTANT_PURPOSE)
                || !grant.scope().consumers().contains(dependency.consumer())
                || grant.scope().resources().len() != 1
            {
                return Err(AgentFailure::PolicyDenied);
            }
            let resource = grant.scope().resources()[0].as_str();
            let (view_id, connection_id) =
                resource.split_once(':').ok_or(AgentFailure::PolicyDenied)?;
            if remote_view_resource(view_id, dependency.source().connection_id().as_str()) != resource
                || connection_id != dependency.source().connection_id().as_str()
            {
                return Err(AgentFailure::PolicyDenied);
            }
            let client = floe_provider_adapters::control::RemoteAuthorizationClient::new(self.reader.route)?;
            let preview = client
                .view_source_preview(
                    view_id,
                    dependency.source().connector().as_str(),
                    connection_id,
                    resource,
                    request.deadline,
                    &request.cancellation,
                )
                .await?;
            let pairing = self
                .reader
                .route
                .pairing
                .as_ref()
                .ok_or(AgentFailure::PolicyDenied)?;
            let reference = self
                .reader
                .vault
                .verify_remote_view_source_preview(
                    &preview.descriptor_b64url,
                    &preview.producer_signature,
                    &self.reader.person_id.to_string(),
                    self.reader.client_id,
                    self.reader.device_id,
                    view_id,
                    dependency.source().connector().as_str(),
                    connection_id,
                    resource,
                )
                .await?;
            if pairing.person_id != self.reader.person_id.to_string()
                || reference.source_authority != dependency.source().source_authority()
                || reference.execution_owner != dependency.source().execution_owner().as_str()
                || reference.connection_revision != preview.connection_revision
            {
                return Err(AgentFailure::PolicyDenied);
            }
            match dependency.processing() {
                ProcessingRestriction::ApprovedRecipient { recipient, .. }
                    if recipient == &preview.producer.audience => {}
                _ => return Err(AgentFailure::PolicyDenied),
            }
            let binding = self
                .reader
                .vault
                .remote_view_grant_binding(
                    view_id,
                    dependency.source().connector().as_str(),
                    connection_id,
                    reference.source_authority,
                )
                .await?;
            if binding.consumer_policy != dependency.consumer_policy()
                || binding.grant.authority() != dependency.grant_authority()
            {
                return Err(AgentFailure::PolicyDenied);
            }
            Ok(())
        })
    }
}

fn check_window(
    deadline: tokio::time::Instant,
    cancellation: &floe_execution::Cancellation,
) -> Result<(), AgentFailure> {
    if cancellation.is_cancelled() {
        return Err(AgentFailure::Cancelled);
    }
    if tokio::time::Instant::now() >= deadline {
        return Err(AgentFailure::DeadlineExceeded);
    }
    Ok(())
}

fn reference_to_source(
    reference: &RemoteViewSourceReference,
) -> Result<floe_context_contract::GrantSourceBinding, AgentFailure> {
    floe_context_contract::GrantSourceBinding::try_new(
        floe_kernel::PersonId(
            reference
                .person_id
                .parse()
                .map_err(|_| AgentFailure::InvalidInput)?,
        ),
        floe_context_contract::ConnectionId::try_new(reference.connection_id.clone())
            .map_err(|_| AgentFailure::InvalidInput)?,
        floe_context_contract::ConnectorId::try_new(reference.connector_id.clone())
            .map_err(|_| AgentFailure::InvalidInput)?,
        floe_context_contract::ExecutionOwnerId::try_new(reference.execution_owner.clone())
            .map_err(|_| AgentFailure::InvalidInput)?,
        reference.source_authority,
    )
    .map_err(|_| AgentFailure::InvalidInput)
}

/// The resource handle a grant must name to admit this view from this source.
fn remote_view_grant_resource(
    view_id: &str,
    source: &floe_context_contract::GrantSourceBinding,
) -> Option<String> {
    (is_remote_view(view_id)
        && remote_view_connector_admissible(view_id, source.connector().as_str()))
    .then(|| remote_view_resource(view_id, source.connection_id().as_str()))
}
