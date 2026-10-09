//! Reading one remote view, start to finish.
//!
//! Context owns the order this happens in: pick the grant that admits the read,
//! take a fresh signed preview of the source, check the preview against the
//! grant, check the binding against both, read, validate what came back, and
//! record what the read now depends on. Access judges each step; the transport
//! performs the I/O; neither of them owns the sequence.

use crate::application::remote_views::{
    is_remote_view, merge_remote_views, remote_view_data_categories, remote_view_dependency,
    validate_remote_view, validate_remote_view_query,
};
use chrono::Utc;
use floe_access::{
    DataAccessGrant, DependencyAuthorization, GrantRepository, GrantState, RemoteCallWindow,
    RemoteGrantTransport, RemotePairingIdentity, RemoteSourceQuery, SourcePreviewVerifier,
    admit_remote_view_binding, admit_remote_view_source, grant_unchanged, remote_dependency_live,
    remote_dependency_resource, remote_dependency_source_admits, remote_view_source,
    source_matches_producer,
};
use floe_agent_contract::{AgentFailure, BoxFuture, PersonId};
use floe_connections::{SourceOperationRepository, SourceRepository};
use floe_context_contract::{
    AuthorizedSourceBinding, ContextDependency, GrantConsumer, GrantOperation, GrantPurpose,
    MAX_SOURCE_ACCESS_BLOCKERS, ObservedGrant, ResourceHandle, SourceAccessBlockers,
    SourceAccessRequirement, SourceAccessRequirementKind, SourceReadOutcome,
    SourceSelectionReference, SourceUnavailable, connection_view_resource,
    source_access_id_for_capability, split_connection_view_resource,
};
use serde_json::Value;
use uuid::Uuid;

/// The read a remote transport performs once the grant has admitted it.
pub struct AdmittedRemoteRead<'a> {
    pub view_id: &'a str,
    pub grant: &'a DataAccessGrant,
    pub source_authority: floe_context_contract::SourceAuthority,
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

fn check_window(window: &RemoteCallWindow) -> Result<(), AgentFailure> {
    if window.cancellation.is_cancelled() {
        return Err(AgentFailure::Cancelled);
    }
    if tokio::time::Instant::now() >= window.deadline {
        return Err(AgentFailure::DeadlineExceeded);
    }
    Ok(())
}

/// Whether this grant admits reading the view on its own connection.
fn remote_grant_admits(
    grant: &DataAccessGrant,
    person_id: PersonId,
    consumer: &GrantConsumer,
    expected_resource: &str,
) -> bool {
    grant.source().person_id() == person_id
        && grant.state() == GrantState::Active
        && !grant.review_required()
        && grant.scope().operations() == [GrantOperation::Read]
        && grant.scope().purposes() == [GrantPurpose::Assistant]
        && grant.scope().consumers().contains(consumer)
        && grant.scope().resources().len() == 1
        && grant.scope().resources()[0].as_str() == expected_resource
}

/// One concrete blocker for a known source that does not admit this read.
///
/// Identity comes from the observed grant itself; nothing is invented.
fn remote_source_blocker(
    grant: &DataAccessGrant,
    view_id: &str,
    source_id: &str,
    consumer: &GrantConsumer,
    reason: SourceAccessRequirementKind,
) -> Result<SourceAccessRequirement, AgentFailure> {
    let source = grant.source();
    let resource = connection_view_resource(view_id, &source.connection_id())
        .map_err(|_| AgentFailure::InvalidInput)?;
    let observed = ObservedGrant::try_new(grant.id(), grant.authority())
        .map_err(|_| AgentFailure::InvalidInput)?;
    SourceAccessRequirement::try_new(
        source_id,
        Some(source.connector().clone()),
        Some(source.connection_id()),
        GrantOperation::Read,
        consumer.clone(),
        GrantPurpose::Assistant,
        vec![resource],
        None,
        reason,
        None,
        Some(observed),
        matches!(
            reason,
            SourceAccessRequirementKind::EnableObserve
                | SourceAccessRequirementKind::ReviewChangedSource
        ),
    )
    .map_err(|_| AgentFailure::InvalidInput)
}

fn classify_selected_remote_sources(
    grants: &[DataAccessGrant],
    person_id: PersonId,
    consumer: &GrantConsumer,
    view_id: &str,
    source_id: &str,
    selected: &[SourceSelectionReference],
) -> Result<(Vec<DataAccessGrant>, Vec<SourceAccessRequirement>), AgentFailure> {
    if selected.is_empty()
        || selected.len() > 16
        || selected.windows(2).any(|pair| pair[0] >= pair[1])
    {
        return Err(AgentFailure::InvalidInput);
    }
    let mut admitted = Vec::new();
    let mut blocked = Vec::new();
    for target in selected {
        target.validate().map_err(|_| AgentFailure::InvalidInput)?;
        if target.capability_id != view_id
            || target.contract_version != 1
            || !connection_view_resource(view_id, &target.connection_id)
                .is_ok_and(|expected| expected == target.resource)
        {
            return Err(AgentFailure::InvalidInput);
        }
        let matching = grants
            .iter()
            .filter(|grant| {
                grant.state() != GrantState::Revoked
                    && grant.source().person_id() == person_id
                    && grant.source().connector() == &target.connector_id
                    && grant.source().connection_id() == target.connection_id
                    && grant.source().execution_owner() == &target.execution_owner_id
                    && grant.scope().resources().contains(&target.resource)
            })
            .collect::<Vec<_>>();
        let grant = match matching.as_slice() {
            [] => {
                blocked.push(
                    SourceAccessRequirement::try_new(
                        source_id,
                        Some(target.connector_id.clone()),
                        Some(target.connection_id.clone()),
                        GrantOperation::Read,
                        consumer.clone(),
                        GrantPurpose::Assistant,
                        vec![target.resource.clone()],
                        None,
                        SourceAccessRequirementKind::EnableObserve,
                        None,
                        None,
                        false,
                    )
                    .map_err(|_| AgentFailure::InvalidInput)?,
                );
                continue;
            }
            [grant] => *grant,
            _ => return Err(AgentFailure::Conflict),
        };
        if remote_grant_admits(grant, person_id, consumer, target.resource.as_str())
            && grant.scope().categories() == remote_view_data_categories(view_id)
        {
            admitted.push(grant.clone());
        } else {
            blocked.push(remote_source_blocker(
                grant,
                view_id,
                source_id,
                consumer,
                if grant.state() == GrantState::Paused {
                    SourceAccessRequirementKind::EnableObserve
                } else {
                    SourceAccessRequirementKind::ReviewChangedSource
                },
            )?);
        }
    }
    Ok((admitted, blocked))
}

/// Read one source of an admitted multi-source view.
#[allow(clippy::too_many_arguments)]
async fn read_one_remote_source(
    store: &(impl GrantRepository + ?Sized),
    verifier: &(impl SourcePreviewVerifier + ?Sized),
    sources: &(impl SourceRepository + ?Sized),
    operations: &(impl SourceOperationRepository + ?Sized),
    transport: &(impl RemoteViewTransport + ?Sized),
    person_id: PersonId,
    pairing: RemotePairingIdentity<'_>,
    view_id: &str,
    consumer: &GrantConsumer,
    consumer_name: &str,
    grant: &DataAccessGrant,
    query: &Value,
    window: &RemoteCallWindow,
    process_incarnation_id: Uuid,
    query_fingerprint: &[u8],
    max_items: usize,
    max_bytes: usize,
    now: i64,
) -> Result<(Value, AuthorizedSourceBinding), AgentFailure> {
    let source = grant.source();
    require_unfenced(operations, source).await?;
    let connection_id = source.connection_id();
    let connection_id_text = connection_id.as_str();
    let resource = connection_view_resource(view_id, &connection_id)
        .map_err(|_| AgentFailure::InvalidInput)?
        .as_str()
        .to_owned();
    let source_query = RemoteSourceQuery {
        view_id,
        connector_id: source.connector().as_str(),
        connection_id: connection_id_text,
        resource: &resource,
    };
    let preview = transport.view_source_preview(source_query, window).await?;
    let reference = verifier.verify(&preview, pairing, source_query).await?;
    source_matches_producer(&reference, &preview.producer, preview.connection_revision)?;
    admit_remote_view_source(&reference, source)?;
    let local_source = current_remote_source(sources, source, &reference).await?;
    let current_grant = exact_view_grant(store, view_id, source)
        .await?
        .ok_or(AgentFailure::AccessReviewRequired)?;
    admit_remote_view_binding(&current_grant, grant, source, &resource)?;
    let raw = transport
        .read_admitted_view(
            AdmittedRemoteRead {
                view_id,
                grant: &current_grant,
                source_authority: reference.source_authority,
                consumer: consumer_name,
                resource: &resource,
                connection_revision: reference.connection_revision,
                max_items,
                max_bytes,
                query: query.clone(),
                pairing,
            },
            window,
        )
        .await?;
    check_window(window)?;
    let current_grant = exact_view_grant(store, view_id, source)
        .await?
        .ok_or(AgentFailure::AccessReviewRequired)?;
    grant_unchanged(grant, &current_grant)?;
    require_unfenced(operations, source).await?;
    let current_preview = transport.view_source_preview(source_query, window).await?;
    let current_reference = verifier
        .verify(&current_preview, pairing, source_query)
        .await?;
    source_matches_producer(
        &current_reference,
        &current_preview.producer,
        current_preview.connection_revision,
    )?;
    if current_remote_source(sources, source, &current_reference).await? != local_source
        || current_reference != reference
    {
        return Err(AgentFailure::StaleContext);
    }
    admit_remote_view_binding(&current_grant, grant, source, &resource)?;
    check_window(window)?;
    let (value, observed, expires) =
        validate_remote_view(view_id, raw, query, now, max_items, max_bytes)?;
    let dependency = remote_view_dependency(
        person_id,
        &current_grant,
        remote_view_source(&reference)?,
        &resource,
        reference.source_authority,
        &reference.source_resources,
        consumer.clone(),
        query_fingerprint.to_vec(),
        Uuid::new_v4(),
        process_incarnation_id,
        observed,
        expires,
    )?;
    Ok((
        value,
        AuthorizedSourceBinding {
            dependency,
            scope: current_grant.scope().clone(),
        },
    ))
}

fn blocked_outcome(
    blockers: Vec<SourceAccessRequirement>,
) -> Result<SourceReadOutcome<(Value, Vec<AuthorizedSourceBinding>)>, AgentFailure> {
    if blockers.len() > MAX_SOURCE_ACCESS_BLOCKERS {
        return Err(AgentFailure::BudgetExceeded);
    }
    let blockers =
        SourceAccessBlockers::try_new(blockers).map_err(|_| AgentFailure::InvalidInput)?;
    Ok(SourceReadOutcome::NeedsUserAction(blockers))
}

#[allow(clippy::too_many_arguments)]
pub async fn read_selected_remote_view(
    store: &(impl GrantRepository + ?Sized),
    verifier: &(impl SourcePreviewVerifier + ?Sized),
    sources: &(impl SourceRepository + ?Sized),
    operations: &(impl SourceOperationRepository + ?Sized),
    transport: &(impl RemoteViewTransport + ?Sized),
    person_id: PersonId,
    pairing: RemotePairingIdentity<'_>,
    view_id: &str,
    consumer_name: &str,
    selected: &[SourceSelectionReference],
    query: Value,
    window: &RemoteCallWindow,
    process_incarnation_id: Uuid,
    query_fingerprint: &[u8],
) -> Result<SourceReadOutcome<(Value, Vec<AuthorizedSourceBinding>)>, AgentFailure> {
    check_window(window)?;
    let consumer = GrantConsumer::builtin(consumer_name).map_err(|_| AgentFailure::InvalidInput)?;
    let source_id = source_access_id_for_capability(view_id).ok_or(AgentFailure::InvalidInput)?;
    let (max_items, max_bytes) = validate_remote_view_query(view_id, &query)?;
    let mut grants = Vec::new();
    for selection in selected {
        let source = floe_context_contract::GrantSourceBinding::try_new(
            person_id,
            selection.connection_id.clone(),
            selection.connector_id.clone(),
            selection.execution_owner_id.clone(),
        )
        .map_err(|_| AgentFailure::InvalidInput)?;
        require_unfenced(operations, &source).await?;
        for grant in store.snapshot(source).await?.grants {
            if !grants
                .iter()
                .any(|known: &DataAccessGrant| known.id() == grant.id())
            {
                grants.push(grant);
            }
        }
    }
    let (admitted, blocked) = classify_selected_remote_sources(
        &grants, person_id, &consumer, view_id, source_id, selected,
    )?;
    if !blocked.is_empty() {
        return blocked_outcome(blocked);
    }
    read_classified_remote_view(
        store,
        verifier,
        sources,
        operations,
        transport,
        person_id,
        pairing,
        view_id,
        &consumer,
        source_id,
        query,
        window,
        process_incarnation_id,
        query_fingerprint,
        max_items,
        max_bytes,
        admitted,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn read_classified_remote_view(
    store: &(impl GrantRepository + ?Sized),
    verifier: &(impl SourcePreviewVerifier + ?Sized),
    sources: &(impl SourceRepository + ?Sized),
    operations: &(impl SourceOperationRepository + ?Sized),
    transport: &(impl RemoteViewTransport + ?Sized),
    person_id: PersonId,
    pairing: RemotePairingIdentity<'_>,
    view_id: &str,
    consumer: &GrantConsumer,
    source_id: &str,
    query: Value,
    window: &RemoteCallWindow,
    process_incarnation_id: Uuid,
    query_fingerprint: &[u8],
    max_items: usize,
    max_bytes: usize,
    admitted: Vec<DataAccessGrant>,
) -> Result<SourceReadOutcome<(Value, Vec<AuthorizedSourceBinding>)>, AgentFailure> {
    if admitted.is_empty() {
        return Err(AgentFailure::CapabilityUnavailable);
    }
    let now = Utc::now().timestamp_millis();
    let mut values = Vec::with_capacity(admitted.len());
    let mut bindings = Vec::with_capacity(admitted.len());
    let mut reconnect = Vec::new();
    for grant in &admitted {
        check_window(window)?;
        match read_one_remote_source(
            store,
            verifier,
            sources,
            operations,
            transport,
            person_id,
            pairing,
            view_id,
            consumer,
            consumer.identifier(),
            grant,
            &query,
            window,
            process_incarnation_id,
            query_fingerprint,
            max_items,
            max_bytes,
            now,
        )
        .await
        {
            Ok((value, binding)) => {
                values.push(value);
                bindings.push(binding);
            }
            Err(AgentFailure::CredentialExpired) => reconnect.push(remote_source_blocker(
                grant,
                view_id,
                source_id,
                consumer,
                SourceAccessRequirementKind::Reconnect,
            )?),
            Err(AgentFailure::CapabilityUnavailable) => {
                return Ok(SourceReadOutcome::Unavailable(
                    SourceUnavailable::TemporarilyUnavailable,
                ));
            }
            Err(error) => return Err(error),
        }
    }
    if !reconnect.is_empty() {
        return blocked_outcome(reconnect);
    }
    Ok(SourceReadOutcome::Ready((
        merge_remote_views(view_id, values, &query, now, max_items, max_bytes)?,
        bindings,
    )))
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
    store: &(impl GrantRepository + ?Sized),
    verifier: &(impl SourcePreviewVerifier + ?Sized),
    sources: &(impl SourceRepository + ?Sized),
    operations: &(impl SourceOperationRepository + ?Sized),
    transport: &(impl RemoteGrantTransport + ?Sized),
    person_id: PersonId,
    pairing: RemotePairingIdentity<'_>,
    dependency: &ContextDependency,
    authorization: &DependencyAuthorization,
) -> Result<(), AgentFailure> {
    remote_dependency_live(dependency, person_id, Utc::now())?;
    if pairing.person_id != person_id.to_string() {
        return Err(AgentFailure::PolicyDenied);
    }
    require_unfenced(operations, dependency.source()).await?;
    let grants = store.snapshot(dependency.source().clone()).await?.grants;
    let grant = grants
        .iter()
        .find(|grant| grant.id() == dependency.grant_id())
        .ok_or(AgentFailure::AccessReviewRequired)?;
    let resource = remote_dependency_resource(grant, dependency)?;
    let source_connection = dependency.source().connection_id();
    let connection_id = source_connection.as_str();
    let resource_handle =
        ResourceHandle::try_new(resource).map_err(|_| AgentFailure::PolicyDenied)?;
    let view_id = split_connection_view_resource(&resource_handle, &source_connection)
        .map_err(|_| AgentFailure::PolicyDenied)?;
    if !is_remote_view(view_id) {
        return Err(AgentFailure::PolicyDenied);
    }
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
    let reference = verifier.verify(&preview, pairing, source_query).await?;
    source_matches_producer(&reference, &preview.producer, preview.connection_revision)?;
    admit_remote_view_source(&reference, dependency.source())?;
    let _current_source = current_remote_source(sources, dependency.source(), &reference).await?;
    remote_dependency_source_admits(
        dependency,
        &reference,
        preview.connection_revision,
        &preview.producer.audience,
    )?;
    let current = exact_view_grant(store, view_id, dependency.source())
        .await?
        .ok_or(AgentFailure::AccessReviewRequired)?;
    require_unfenced(operations, dependency.source()).await?;
    admit_remote_view_binding(&current, grant, dependency.source(), resource)
}

async fn exact_view_grant(
    store: &(impl GrantRepository + ?Sized),
    view_id: &str,
    source: &floe_context_contract::GrantSourceBinding,
) -> Result<Option<DataAccessGrant>, AgentFailure> {
    let resource = connection_view_resource(view_id, &source.connection_id())
        .map_err(|_| AgentFailure::InvalidInput)?;
    let mut matching = store
        .snapshot(source.clone())
        .await?
        .grants
        .into_iter()
        .filter(|grant| {
            grant.state() != GrantState::Revoked && grant.scope().resources().contains(&resource)
        });
    let grant = matching.next();
    if matching.next().is_some() {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(grant)
}
async fn require_unfenced(
    operations: &(impl SourceOperationRepository + ?Sized),
    source: &floe_context_contract::GrantSourceBinding,
) -> Result<(), AgentFailure> {
    if operations
        .source_is_fenced(source.person_id(), &source.connection_id())
        .await
        .map_err(|_| AgentFailure::StorageUnavailable)?
    {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(())
}

async fn current_remote_source(
    sources: &(impl SourceRepository + ?Sized),
    binding: &floe_context_contract::GrantSourceBinding,
    reference: &floe_access::RemoteViewSourceReference,
) -> Result<floe_connections::SourceConnection, AgentFailure> {
    let source = sources
        .load(binding.person_id(), &binding.connection_id())
        .await
        .map_err(|_| AgentFailure::StorageUnavailable)?
        .ok_or(AgentFailure::StaleContext)?;
    if source.person_id() != binding.person_id() {
        return Err(AgentFailure::PolicyDenied);
    }
    if !source.is_serving() {
        return Err(AgentFailure::StaleContext);
    }
    if source.connector_id() != binding.connector()
        || source.execution_owner_id() != binding.execution_owner()
    {
        return Err(AgentFailure::PolicyDenied);
    }
    if source.source_authority() != reference.source_authority
        || source
            .resources()
            .iter()
            .map(|resource| resource.handle())
            .ne(reference.source_resources.iter())
    {
        return Err(AgentFailure::StaleContext);
    }
    Ok(source)
}

/// Resolve configured source identities through Connections, independently of
/// grant presence. A grant list is never a source discovery catalog.
pub async fn configured_remote_source_selections(
    sources: &(impl SourceRepository + ?Sized),
    person_id: PersonId,
    view_id: &str,
) -> Result<Vec<SourceSelectionReference>, AgentFailure> {
    if !is_remote_view(view_id) {
        return Err(AgentFailure::InvalidInput);
    }
    let mut selected = Vec::new();
    for connector in floe_access::remote_connector_ids_for_view(view_id) {
        let connector = floe_context_contract::ConnectorId::try_new(*connector)
            .map_err(|_| AgentFailure::InvalidInput)?;
        for source in sources
            .list_current(person_id, &connector)
            .await
            .map_err(|_| AgentFailure::StorageUnavailable)?
        {
            selected.push(SourceSelectionReference {
                connector_id: source.connector_id().clone(),
                connection_id: source.connection_id().clone(),
                execution_owner_id: source.execution_owner_id().clone(),
                capability_id: view_id.to_owned(),
                resource: connection_view_resource(view_id, source.connection_id())
                    .map_err(|_| AgentFailure::InvalidInput)?,
                contract_version: 1,
            });
        }
    }
    selected.sort();
    selected.dedup();
    if selected.len() > 16 {
        return Err(AgentFailure::BudgetExceeded);
    }
    Ok(selected)
}

#[allow(clippy::too_many_arguments)]
pub async fn read_configured_remote_view(
    store: &(impl GrantRepository + ?Sized),
    verifier: &(impl SourcePreviewVerifier + ?Sized),
    sources: &(impl SourceRepository + ?Sized),
    operations: &(impl SourceOperationRepository + ?Sized),
    transport: &(impl RemoteViewTransport + ?Sized),
    person_id: PersonId,
    pairing: RemotePairingIdentity<'_>,
    view_id: &str,
    consumer_name: &str,
    query: Value,
    window: &RemoteCallWindow,
    process_incarnation_id: Uuid,
    query_fingerprint: &[u8],
) -> Result<SourceReadOutcome<(Value, Vec<AuthorizedSourceBinding>)>, AgentFailure> {
    let selected = configured_remote_source_selections(sources, person_id, view_id).await?;
    if selected.is_empty() {
        let source_id =
            source_access_id_for_capability(view_id).ok_or(AgentFailure::InvalidInput)?;
        let consumer =
            GrantConsumer::builtin(consumer_name).map_err(|_| AgentFailure::InvalidInput)?;
        let requirement = SourceAccessRequirement::try_new(
            source_id,
            None,
            None,
            GrantOperation::Read,
            consumer,
            GrantPurpose::Assistant,
            Vec::new(),
            None,
            SourceAccessRequirementKind::SelectResource,
            None,
            None,
            false,
        )
        .map_err(|_| AgentFailure::InvalidInput)?;
        return blocked_outcome(vec![requirement]);
    }
    read_selected_remote_view(
        store,
        verifier,
        sources,
        operations,
        transport,
        person_id,
        pairing,
        view_id,
        consumer_name,
        &selected,
        query,
        window,
        process_incarnation_id,
        query_fingerprint,
    )
    .await
}
