//! Reading one remote view, start to finish.
//!
//! Context owns the order this happens in: pick the grant that admits the read,
//! take a fresh signed preview of the source, check the preview against the
//! grant, check the binding against both, read, validate what came back, and
//! record what the read now depends on. Access judges each step; the transport
//! performs the I/O; neither of them owns the sequence.

use chrono::Utc;
use floe_access::{
    DependencyAuthorization, RemoteCallWindow, RemoteGrantStore, RemoteGrantTransport,
    RemotePairingIdentity, RemoteSourceQuery, active_resource_grant, admit_remote_view_binding,
    admit_remote_view_source, remote_dependency_binding_matches, remote_dependency_live,
    remote_dependency_resource, remote_dependency_source_admits, remote_view_source,
};
use floe_agent_contract::{AgentFailure, BoxFuture, PersonId};
use floe_context_contract::{ContextDependency, GrantConsumer, GrantScope, GrantSourceBinding};
use serde_json::Value;
use uuid::Uuid;

use crate::application::remote_views::{
    is_remote_view, remote_view_connector_admissible, remote_view_dependency, remote_view_resource,
    split_remote_view_resource, validate_remote_view, validate_remote_view_query,
};

/// The read a remote transport performs once the grant has admitted it.
pub struct AdmittedRemoteRead<'a> {
    pub view_id: &'a str,
    pub binding: &'a floe_access::RemoteGrantBinding,
    pub consumer: &'a str,
    pub resource: &'a str,
    pub connection_revision: u64,
    pub max_items: usize,
    pub max_bytes: usize,
    pub query: Value,
    pub pairing: RemotePairingIdentity<'a>,
}

/// The transport that actually reads a remote view under an admitted grant.
pub trait RemoteViewTransport: RemoteGrantTransport {
    fn read_admitted_view<'a>(
        &'a self,
        read: AdmittedRemoteRead<'a>,
        window: &'a RemoteCallWindow,
    ) -> BoxFuture<'a, Result<Value, AgentFailure>>;
}

/// The resource handle a grant must name to admit this view from this source.
pub fn remote_view_grant_resource(view_id: &str, source: &GrantSourceBinding) -> Option<String> {
    (is_remote_view(view_id)
        && remote_view_connector_admissible(view_id, source.connector().as_str()))
    .then(|| remote_view_resource(view_id, source.connection_id().as_str()))
}

fn check_window(window: &RemoteCallWindow) -> Result<(), AgentFailure> {
    if window.cancellation.is_cancelled() {
        return Err(AgentFailure::Cancelled);
    }
    if tokio::time::Instant::now() >= window.deadline {
        return Err(AgentFailure::DeadlineExceeded);
    }
    Ok(())
}

/// Read one remote view and state what the result depends on.
#[allow(clippy::too_many_arguments)]
pub async fn read_remote_view(
    store: &impl RemoteGrantStore,
    transport: &impl RemoteViewTransport,
    person_id: PersonId,
    pairing: RemotePairingIdentity<'_>,
    view_id: &str,
    consumer_name: &str,
    query: Value,
    window: &RemoteCallWindow,
    process_incarnation_id: Uuid,
    query_fingerprint: &[u8],
) -> Result<(Value, ContextDependency, GrantScope), AgentFailure> {
    check_window(window)?;
    let consumer = GrantConsumer::builtin(consumer_name).map_err(|_| AgentFailure::InvalidInput)?;
    let (max_items, max_bytes) = validate_remote_view_query(view_id, &query)?;
    let grants = store.grants(128).await?;
    let grant = active_resource_grant(&grants, person_id, &consumer, |source| {
        remote_view_grant_resource(view_id, source)
    })?;
    let source = grant.source();
    let connection_id = source.connection_id();
    let connection_id_text = connection_id.as_str();
    let resource = remote_view_resource(view_id, connection_id_text);
    let source_query = RemoteSourceQuery {
        view_id,
        connector_id: source.connector().as_str(),
        connection_id: connection_id_text,
        resource: &resource,
    };
    let preview = transport.view_source_preview(source_query, window).await?;
    let reference = store
        .verify_view_source_preview(&preview, pairing, source_query)
        .await?;
    admit_remote_view_source(&reference, source)?;
    let binding = store
        .view_grant_binding(
            view_id,
            source.connector().as_str(),
            connection_id_text,
            reference.source_authority,
        )
        .await?;
    admit_remote_view_binding(&binding.grant, &grant, source, &resource)?;
    let value = transport
        .read_admitted_view(
            AdmittedRemoteRead {
                view_id,
                binding: &binding,
                consumer: consumer_name,
                resource: &resource,
                connection_revision: reference.connection_revision,
                max_items,
                max_bytes,
                query,
                pairing,
            },
            window,
        )
        .await?;
    let now = Utc::now().timestamp_millis();
    let (value, observed, expires) =
        validate_remote_view(view_id, value, now, max_items, max_bytes)?;
    let dependency = remote_view_dependency(
        person_id,
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

/// Whether a recorded remote dependency may still be relied on.
///
/// The whole re-admission runs here: the observation is still live, the grant
/// still names the resource, the producer's current descriptor still admits the
/// source, and the binding still matches.
///
/// Route-neutral: the model route is never consulted here. Whether a
/// reauthorized dependency may reach a Device or External model target is
/// decided by Access model dispatch.
pub async fn authorize_remote_dependency(
    store: &impl RemoteGrantStore,
    transport: &impl RemoteGrantTransport,
    person_id: PersonId,
    pairing: RemotePairingIdentity<'_>,
    dependency: &ContextDependency,
    authorization: &DependencyAuthorization,
) -> Result<(), AgentFailure> {
    remote_dependency_live(dependency, person_id, Utc::now())?;
    if pairing.person_id != person_id.to_string() {
        return Err(AgentFailure::PolicyDenied);
    }
    let grants = store.grants(128).await?;
    let grant = grants
        .iter()
        .find(|grant| grant.id() == dependency.grant_id())
        .ok_or(AgentFailure::PolicyDenied)?;
    let resource = remote_dependency_resource(grant, dependency)?;
    let source_connection = dependency.source().connection_id();
    let connection_id = source_connection.as_str();
    let view_id = split_remote_view_resource(resource, connection_id)?;
    let window = RemoteCallWindow {
        deadline: authorization.deadline,
        cancellation: authorization.cancellation.clone(),
    };
    let source_query = RemoteSourceQuery {
        view_id,
        connector_id: dependency.source().connector().as_str(),
        connection_id,
        resource,
    };
    let preview = transport.view_source_preview(source_query, &window).await?;
    let reference = store
        .verify_view_source_preview(&preview, pairing, source_query)
        .await?;
    remote_dependency_source_admits(
        dependency,
        &reference,
        preview.connection_revision,
        &preview.producer.audience,
    )?;
    let binding = store
        .view_grant_binding(
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
}
