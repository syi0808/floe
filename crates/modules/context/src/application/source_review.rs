//! Current source evidence for Connections reviews. Transport reports facts;
//! Context binds them to the exact source, selected resources and subject.
use std::sync::Arc;

use floe_access::{
    GrantSourceBinding, PersonalSubjectInspector, PersonalSubjectProbe, RemoteCallWindow,
    RemotePairingIdentity, RemoteSourceQuery, SourceExpectation,
};
use floe_connections::{ConnectionResource, PersonalSourceSpec, SourceConnection, SourceReviewEvidence, SourceState};
use floe_execution::{BoxFuture, ExecutionScope};
use floe_kernel::{AgentFailure, OwnerActor};

use crate::{ExpertSourceTransport, SourceMetadataTransport};

pub struct ContextSourceReview {
    native: Arc<dyn SourceMetadataTransport>,
    personal: Arc<dyn PersonalSubjectInspector + Send>,
    remote: Arc<dyn ExpertSourceTransport>,
}

impl ContextSourceReview {
    pub fn new(
        native: Arc<dyn SourceMetadataTransport>,
        personal: Arc<dyn PersonalSubjectInspector + Send>,
        remote: Arc<dyn ExpertSourceTransport>,
    ) -> Self {
        Self { native, personal, remote }
    }

    async fn native_subject(
        &self,
        actor: &OwnerActor,
        source: &SourceConnection,
        handles: Vec<String>,
        expected: Option<&str>,
        scope: &ExecutionScope,
    ) -> Result<String, AgentFailure> {
        let (before, after) = if floe_access::native_calendar_provider(source.connector_id().as_str()).is_some() {
            if source.execution_owner_id().as_str() != actor.device_id {
                return Err(AgentFailure::PolicyDenied);
            }
            let observed = self.native.calendar_subject(actor, source, &handles, scope).await?;
            (observed.before, observed.after.ok_or(AgentFailure::PolicyDenied)?)
        } else {
            let spec = PersonalSourceSpec::for_connector(source.connector_id().as_str())?;
            if source.execution_owner_id().as_str() != spec.execution_owner(&actor.device_id)?
                || source.connection_id().as_str() != spec.connection
                || source.resource_mode() != spec.mode
            {
                return Err(AgentFailure::PolicyDenied);
            }
            let probe = match source.connector_id().as_str() {
                "contacts.apple" | "contacts.android" if handles.len() <= 64 =>
                    PersonalSubjectProbe::People { selected_handles: handles },
                "attention.macos" if handles == [floe_access::ATTENTION_RESOURCE] => PersonalSubjectProbe::Attention,
                "health.apple" if handles == [floe_access::WELLBEING_RESOURCE] => PersonalSubjectProbe::Wellbeing,
                _ => return Err(AgentFailure::CapabilityUnavailable),
            };
            let observed = self.personal.inspect(actor.person_id, &actor.device_id, probe,
                expected.map(str::to_owned), Some(scope.deadline()), scope.cancellation().clone()).await?;
            (observed.before, observed.after)
        };
        if !floe_access::valid_subject_fingerprint(&before) || before != after {
            return Err(AgentFailure::AccessReviewRequired);
        }
        if let Some(expected) = expected { floe_access::subject_unchanged(expected, &before, &after)?; }
        Ok(before)
    }
}

impl SourceReviewEvidence for ContextSourceReview {
    fn inspect_selection<'a>(&'a self, actor: &'a OwnerActor, source: &'a SourceConnection,
        selected_resources: &'a [ConnectionResource], scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<String, AgentFailure>>
    {
        Box::pin(scope.run(async move {
            validate_source(actor, source)?;
            let handles = resource_handles(selected_resources)?;
            self.native_subject(actor, source, handles, None, scope).await
        }))
    }

    fn observe<'a>(&'a self, actor: &'a OwnerActor, source: &'a SourceConnection, scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<SourceExpectation, AgentFailure>>
    {
        Box::pin(scope.run(async move {
            validate_source(actor, source)?;
            let handles = resource_handles(source.resources())?;
            let binding = GrantSourceBinding::try_new(source.person_id(), source.connection_id().clone(),
                source.connector_id().clone(), source.execution_owner_id().clone())
                .map_err(|_| AgentFailure::InvalidInput)?;
            let physical_resources = source.resources().iter().map(|resource| resource.handle().clone()).collect::<Vec<_>>();
            let connector = source.connector_id().as_str();
            if floe_access::native_calendar_provider(connector).is_some()
                || matches!(connector, "contacts.apple" | "contacts.android" | "attention.macos" | "health.apple")
            {
                if !source.is_serving() { return Err(AgentFailure::AccessReviewRequired); }
                if floe_access::native_calendar_provider(connector).is_none() {
                    PersonalSourceSpec::for_connector(connector)?.validate_connection(source, &actor.device_id)?;
                }
                let expected = source.native_subject_fingerprint().ok_or(AgentFailure::AccessReviewRequired)?;
                let subject = self.native_subject(actor, source, handles, Some(expected), scope).await?;
                return Ok(SourceExpectation {
                    source: binding, revision: Some(source.revision()), provider_revision: None,
                    authority: source.source_authority(), physical_resources,
                    subject_fingerprint: subject, gateway: None,
                });
            }
            let remote = self.remote.remote(actor, scope).await?.ok_or(AgentFailure::CapabilityUnavailable)?;
            let gateway = remote.transport.gateway_binding();
            gateway.validate()?;
            if gateway.person_id != actor.person_id.to_string() || gateway.device_id != actor.device_id
                || gateway.client_id != remote.transport.client_id()
                || gateway.producer_instance != remote.transport.producer().instance_id
                || gateway.producer_key_fingerprint != remote.transport.producer().fingerprint
                || gateway.producer_audience != remote.transport.producer().audience
                || source.execution_owner_id().as_str() != remote.transport.producer().execution_owner
            {
                return Err(AgentFailure::PolicyDenied);
            }
            let window = RemoteCallWindow { deadline: scope.deadline(), cancellation: scope.cancellation().clone() };
            let person = actor.person_id.to_string();
            let pairing = RemotePairingIdentity { person_id: &person,
                client_id: remote.transport.client_id(), device_id: &actor.device_id };
            let observed_producer = remote.transport.producer_identity(&window).await?;
            floe_access::producer_is_pinned(remote.transport.producer(), &observed_producer)?;
            let mut observed = None;
            for view_id in floe_access::source_view_ids(connector) {
                let resource = floe_context_contract::connection_view_resource(view_id, source.connection_id())
                    .map_err(|_| AgentFailure::InvalidInput)?;
                let query = RemoteSourceQuery { view_id, connector_id: connector,
                    connection_id: source.connection_id().as_str(), resource: resource.as_str() };
                let preview = remote.transport.view_source_preview(query, &window).await?;
                let reference = remote.verifier.verify(&preview, pairing, query).await?;
                floe_access::source_matches_producer(&reference, remote.transport.producer(), preview.connection_revision)?;
                floe_access::admit_remote_view_source(&reference, &binding)?;
                if reference.source_authority != source.source_authority()
                    || reference.source_resources != physical_resources
                {
                    return Err(AgentFailure::Conflict);
                }
                let expectation = SourceExpectation {
                    source: binding.clone(), revision: Some(source.revision()),
                    provider_revision: Some(reference.connection_revision), authority: reference.source_authority,
                    physical_resources: reference.source_resources, subject_fingerprint: reference.provider_identity,
                    gateway: Some(gateway.clone()),
                };
                if observed.as_ref().is_some_and(|prior| prior != &expectation) { return Err(AgentFailure::Conflict); }
                observed = Some(expectation);
            }
            observed.ok_or(AgentFailure::CapabilityUnavailable)
        }))
    }
}

fn validate_source(actor: &OwnerActor, source: &SourceConnection) -> Result<(), AgentFailure> {
    actor.validate()?;
    source.validate().map_err(|_| AgentFailure::InvalidInput)?;
    if source.person_id() != actor.person_id || source.state() == SourceState::Disconnected {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(())
}

fn resource_handles(resources: &[ConnectionResource]) -> Result<Vec<String>, AgentFailure> {
    if resources.is_empty() || resources.len() > 256 { return Err(AgentFailure::InvalidInput); }
    let mut handles = Vec::with_capacity(resources.len());
    for resource in resources {
        ConnectionResource::new(resource.handle().clone(), resource.label().to_owned())
            .map_err(|_| AgentFailure::InvalidInput)?;
        handles.push(resource.handle().as_str().to_owned());
    }
    if handles.windows(2).any(|pair| pair[0] >= pair[1]) { return Err(AgentFailure::InvalidInput); }
    Ok(handles)
}
