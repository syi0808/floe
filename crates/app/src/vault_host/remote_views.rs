use std::{future::Future, pin::Pin};

use chrono::Utc;
use floe_agent_contract::{AgentFailure, ModelPlacement};
use floe_conversation::{ModelRequest};
use floe_context::{
    is_remote_view, remote_view_connector_admissible, remote_view_data_category,
    remote_view_resource, validate_remote_view, validate_remote_view_query,
};
use floe_vault::{EncryptedAgentVault, GovernedDependencyLiveness, GovernedDependencyResolver, RemoteCalendarAuthorizationExpectation, RemoteProducerIdentity, RemoteViewSourceReference, VaultKeyProvider};
use floe_access::{
    DataAccessGrant, RemoteViewApproval, RemoteViewGrantReview, active_resource_grant,
    admit_remote_view_binding, admit_remote_view_source, matches_review, producer_is_pinned,
    remote_dependency_binding_matches, remote_dependency_live, remote_dependency_resource,
    remote_dependency_source_admits, remote_view_scope, remote_view_source,
    review_remote_view_grant, source_matches_producer,
};
use floe_context_contract::{ContextDependency, GrantConsumer, GrantScope};

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
    producer_is_pinned(
        &vault.remote_pinned_producer().await?,
        &RemoteProducerIdentity {
            schema_version: producer.schema_version,
            instance_id: producer.instance_id.clone(),
            execution_owner: producer.execution_owner.clone(),
            audience: producer.audience.clone(),
            key_id: producer.key_id.clone(),
            public_key: producer.public_key.clone(),
            fingerprint: producer.fingerprint.clone(),
        },
    )?;
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
    source_matches_producer(&reference, &observed_producer(&producer), preview.connection_revision)?;
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
    matches_review(
        &RemoteViewApproval {
            producer_fingerprint: expected_producer_fingerprint,
            source_authority: expected_source_authority,
            connection_revision: expected_connection_revision,
            provider_identity: expected_provider_identity,
            recipient: expected_recipient,
        },
        &preview.reference,
        &observed_producer(&preview.producer),
        preview.connection_revision,
    )?;
    let scope = remote_view_scope(
        resource,
        remote_view_data_category(view_id),
        GrantConsumer::builtin(consumer_name).map_err(|_| AgentFailure::InvalidInput)?,
        preview.producer.audience.clone(),
    )?;
    let source = remote_view_source(&preview.reference)?;
    let existing = vault
        .find_remote_view_grant(view_id, &source, consumer_name)
        .await?;
    match review_remote_view_grant(existing.as_ref(), &scope)? {
        RemoteViewGrantReview::AlreadyGranted => {
            existing.ok_or(AgentFailure::PolicyDenied)
        }
        RemoteViewGrantReview::Activate { grant_id, expected } => {
            vault
                .review_and_activate_remote_view_grant(
                    view_id, grant_id, expected, source, scope, None,
                )
                .await
        }
    }
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
        admit_remote_view_source(&reference, source)?;
        let binding = self
            .vault
            .remote_view_grant_binding(
                view_id,
                source.connector().as_str(),
                connection_id_text,
                reference.source_authority,
            )
            .await?;
        admit_remote_view_binding(&binding.grant, &grant, source, &resource)?;
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
        let (value, observed, expires) =
            validate_remote_view(view_id, value, now, max_items, max_bytes)?;
        let dependency = floe_context::remote_view_dependency(
            self.person_id,
            &binding.grant,
            binding.consumer_policy,
            remote_view_source(&reference)?,
            &resource,
            consumer,
            query_fingerprint.to_vec(),
            process_incarnation_id,
            Uuid::new_v4(),
            observed,
            expires,
        )?;
        Ok((value, dependency, binding.grant.scope().clone()))
    }
}

impl<Keys: VaultKeyProvider> GovernedDependencyLiveness for RemoteDependencyResolver<'_, Keys> {
    fn validate(&self, dependency: &ContextDependency) -> Result<(), AgentFailure> {
        remote_dependency_live(dependency, self.reader.person_id, Utc::now())
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
            let resource = remote_dependency_resource(&grant, dependency)?;
            let source_connection = dependency.source().connection_id();
            let connection_id = source_connection.as_str();
            let view_id = floe_context::split_remote_view_resource(resource, connection_id)?;
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
            if pairing.person_id != self.reader.person_id.to_string() {
                return Err(AgentFailure::PolicyDenied);
            }
            remote_dependency_source_admits(
                dependency,
                &reference,
                preview.connection_revision,
                &preview.producer.audience,
            )?;
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
            remote_dependency_binding_matches(
                binding.consumer_policy,
                binding.grant.authority(),
                dependency,
            )
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

/// The resource handle a grant must name to admit this view from this source.
fn remote_view_grant_resource(
    view_id: &str,
    source: &floe_context_contract::GrantSourceBinding,
) -> Option<String> {
    (is_remote_view(view_id)
        && remote_view_connector_admissible(view_id, source.connector().as_str()))
    .then(|| remote_view_resource(view_id, source.connection_id().as_str()))
}

/// The producer identity the control client observed, as Access states it.
fn observed_producer(
    producer: &floe_provider_adapters::control::authorization::ProducerIdentityResponse,
) -> RemoteProducerIdentity {
    RemoteProducerIdentity {
        schema_version: producer.schema_version,
        instance_id: producer.instance_id.clone(),
        execution_owner: producer.execution_owner.clone(),
        audience: producer.audience.clone(),
        key_id: producer.key_id.clone(),
        public_key: producer.public_key.clone(),
        fingerprint: producer.fingerprint.clone(),
    }
}
