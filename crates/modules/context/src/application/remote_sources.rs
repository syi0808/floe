//! Reading one remote view, start to finish.
//!
//! Context owns the order this happens in: pick the grant that admits the read,
//! take a fresh signed preview of the source, check the preview against the
//! grant, check the binding against both, read, validate what came back, and
//! record what the read now depends on. Access judges each step; the transport
//! performs the I/O; neither of them owns the sequence.

use chrono::Utc;
use floe_access::{
    DataAccessGrant, DependencyAuthorization, GrantState, RemoteCallWindow, RemoteGrantStore,
    RemoteGrantTransport, RemotePairingIdentity, RemoteSourceQuery,
    admit_remote_view_binding, admit_remote_view_source, grant_unchanged,
    remote_dependency_binding_matches, remote_dependency_live, remote_dependency_resource,
    remote_dependency_source_admits, remote_view_source, source_matches_producer,
};
use floe_agent_contract::{AgentFailure, BoxFuture, PersonId};
use floe_context_contract::{
    AuthorizedSourceBinding, ContextDependency, GrantConsumer, GrantOperation, GrantPurpose,
    MAX_SOURCE_ACCESS_BLOCKERS, ObservedGrant, ProcessingRestriction,
    ResourceHandle, SourceAccessBlockers, SourceAccessRequirement, SourceAccessRequirementKind,
    SourceReadOutcome, SourceSelectionReference, SourceUnavailable, connection_view_resource,
    source_access_id_for_capability, split_connection_view_resource,
};
use serde_json::Value;
use uuid::Uuid;

#[cfg(test)]
use crate::application::remote_views::{LOGISTICS_VIEW, MAIL_VIEW, WORK_VIEW};
use crate::application::remote_views::{
    is_remote_view, merge_remote_views, remote_view_data_categories, remote_view_dependency,
    validate_remote_view, validate_remote_view_query,
};

/// The read a remote transport performs once the grant has admitted it.
pub struct AdmittedRemoteRead<'a> {
    pub view_id: &'a str,
    pub binding: &'a floe_access::RemoteGrantBinding,
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
        && grant.scope().processing() == &ProcessingRestriction::LocalOnly
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

/// Partition the grants that name this view into admitted reads and concrete
/// per-source blockers, before any read runs.
///
/// A paused grant is re-enableable, an active-but-mismatched grant needs drift
/// review, and a revoked or foreign grant is not this read's. Two live grants
/// naming the same connection and requested resource are ambiguous authority;
/// distinct view resources on one connection are independent targets.
fn classify_remote_sources(
    grants: &[DataAccessGrant],
    person_id: PersonId,
    consumer: &GrantConsumer,
    view_id: &str,
    source_id: &str,
) -> Result<(Vec<DataAccessGrant>, Vec<SourceAccessRequirement>), AgentFailure> {
    let mut known: Vec<&DataAccessGrant> = grants
        .iter()
        .filter(|grant| {
            grant.state() != GrantState::Revoked
                && grant.source().person_id() == person_id
                && grant.scope().resources().iter().any(|resource| {
                    connection_view_resource(view_id, &grant.source().connection_id())
                        .is_ok_and(|expected| &expected == resource)
                })
        })
        .collect();
    known.sort_by(|left, right| {
        left.source()
            .connector()
            .cmp(right.source().connector())
            .then_with(|| {
                left.source()
                    .connection_id()
                    .cmp(&right.source().connection_id())
            })
            .then_with(|| left.id().cmp(&right.id()))
    });
    if known.windows(2).any(|pair| {
        pair[0].source().person_id() == pair[1].source().person_id()
            && pair[0].source().connector() == pair[1].source().connector()
            && pair[0].source().connection_id() == pair[1].source().connection_id()
    }) {
        return Err(AgentFailure::Conflict);
    }
    let mut admitted = Vec::with_capacity(known.len());
    let mut blocked = Vec::with_capacity(known.len());
    for grant in known {
        let expected = connection_view_resource(view_id, &grant.source().connection_id())
            .map_err(|_| AgentFailure::InvalidInput)?;
        if remote_grant_admits(grant, person_id, consumer, expected.as_str())
            && grant.scope().categories() == remote_view_data_categories(view_id)
        {
            admitted.push((*grant).clone());
            continue;
        }
        let reason = if grant.state() == GrantState::Paused {
            SourceAccessRequirementKind::EnableObserve
        } else {
            SourceAccessRequirementKind::ReviewChangedSource
        };
        blocked.push(remote_source_blocker(
            grant, view_id, source_id, consumer, reason,
        )?);
    }
    Ok((admitted, blocked))
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
    store: &impl RemoteGrantStore,
    transport: &impl RemoteViewTransport,
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
    let reference = store
        .verify_view_source_preview(&preview, pairing, source_query)
        .await?;
    source_matches_producer(&reference, &preview.producer, preview.connection_revision)?;
    admit_remote_view_source(&reference, source)?;
    let binding = store
        .view_grant_binding(view_id, source.connector().as_str(), connection_id_text)
        .await?;
    admit_remote_view_binding(&binding.grant, grant, source, &resource)?;
    let raw = transport
        .read_admitted_view(
            AdmittedRemoteRead {
                view_id,
                binding: &binding,
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
    let current_grant = store
        .find_view_grant(view_id, source, consumer_name)
        .await?
        .ok_or(AgentFailure::AccessReviewRequired)?;
    grant_unchanged(grant, &current_grant)?;
    let current_preview = transport.view_source_preview(source_query, window).await?;
    let current_reference = store
        .verify_view_source_preview(&current_preview, pairing, source_query)
        .await?;
    source_matches_producer(
        &current_reference,
        &current_preview.producer,
        current_preview.connection_revision,
    )?;
    if current_reference != reference {
        return Err(AgentFailure::StaleContext);
    }
    let current_binding = store
        .view_grant_binding(view_id, source.connector().as_str(), connection_id_text)
        .await?;
    admit_remote_view_binding(&current_binding.grant, grant, source, &resource)?;
    if current_binding.consumer_policy != binding.consumer_policy {
        return Err(AgentFailure::PolicyDenied);
    }
    check_window(window)?;
    let (value, observed, expires) =
        validate_remote_view(view_id, raw, query, now, max_items, max_bytes)?;
    let dependency = remote_view_dependency(
        person_id,
        &binding.grant,
        binding.consumer_policy,
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
            scope: binding.grant.scope().clone(),
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

/// Read one remote view and state what the result depends on.
///
/// Admission classifies every known source before any read runs. A read that
/// is blocked on any source returns only concrete per-source blockers: never a
/// truncated aggregate presented as complete Ready, and never payload or
/// dependency from a failed acquisition leaking alongside the blockers.
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
) -> Result<SourceReadOutcome<(Value, Vec<AuthorizedSourceBinding>)>, AgentFailure> {
    check_window(window)?;
    let consumer = GrantConsumer::builtin(consumer_name).map_err(|_| AgentFailure::InvalidInput)?;
    let source_id = source_access_id_for_capability(view_id).ok_or(AgentFailure::InvalidInput)?;
    let (max_items, max_bytes) = validate_remote_view_query(view_id, &query)?;
    let grants = store.grants(128).await?;
    let (admitted, blocked) =
        classify_remote_sources(&grants, person_id, &consumer, view_id, source_id)?;
    if !blocked.is_empty() {
        return blocked_outcome(blocked);
    }
    if admitted.is_empty() {
        // No known source names this view: navigation-only selection for the
        // source category, inventing no connection, resource or fingerprint.
        let requirement = SourceAccessRequirement::try_new(
            source_id,
            None,
            None,
            GrantOperation::Read,
            consumer,
            GrantPurpose::Assistant,
            vec![],
            None,
            SourceAccessRequirementKind::SelectResource,
            None,
            None,
            false,
        )
        .map_err(|_| AgentFailure::InvalidInput)?;
        return blocked_outcome(vec![requirement]);
    }
    read_classified_remote_view(
        store,
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
pub async fn read_selected_remote_view(
    store: &impl RemoteGrantStore,
    transport: &impl RemoteViewTransport,
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
    let grants = store.grants(128).await?;
    let (admitted, blocked) = classify_selected_remote_sources(
        &grants, person_id, &consumer, view_id, source_id, selected,
    )?;
    if !blocked.is_empty() {
        return blocked_outcome(blocked);
    }
    read_classified_remote_view(
        store,
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
    store: &impl RemoteGrantStore,
    transport: &impl RemoteViewTransport,
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
    let reference = store
        .verify_view_source_preview(&preview, pairing, source_query)
        .await?;
    source_matches_producer(&reference, &preview.producer, preview.connection_revision)?;
    admit_remote_view_source(&reference, dependency.source())?;
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
        )
        .await?;
    remote_dependency_binding_matches(
        binding.consumer_policy,
        binding.grant.authority(),
        dependency,
    )
}

#[cfg(test)]
mod remote_view_tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use floe_access::{
        ConsumerPolicyAuthority, DataAccessGrant, GrantAuthority, GrantDataCategory, GrantId,
        GrantOperation, GrantPurpose, GrantScope, GrantSourceBinding, RemoteGrantBinding,
        RemoteProducerIdentity, RemoteViewSourceReference, SignedSourcePreview,
    };
    use floe_context_contract::{
        ConnectionId, ConnectorId, ExecutionOwnerId, GrantConsumer, ProcessingRestriction,
        ResourceHandle, SourceAuthority,
    };
    use floe_execution::Cancellation;
    use tokio::time::{Duration, Instant};

    use super::*;

    fn fixture_view_resource(view_id: &str, connection_id: &str) -> String {
        connection_view_resource(
            view_id,
            &floe_context_contract::ConnectionId::try_new(connection_id).unwrap(),
        )
        .unwrap()
        .as_str()
        .to_owned()
    }

    struct SourceFixture {
        grant: DataAccessGrant,
        source_authority: SourceAuthority,
        source_resources: Vec<ResourceHandle>,
        policy: ConsumerPolicyAuthority,
        view: Value,
        expired_credential: bool,
    }

    struct ViewFixture {
        person_id: PersonId,
        sources: Vec<SourceFixture>,
        reads: AtomicUsize,
        rotated_source_authority: bool,
    }

    impl ViewFixture {
        fn new(person_id: PersonId) -> Self {
            Self {
                person_id,
                sources: Vec::new(),
                reads: AtomicUsize::new(0),
                rotated_source_authority: false,
            }
        }

        fn add_source(&mut self, connection_id: &str, active: bool) {
            self.add_view_source(connection_id, MAIL_VIEW, active);
        }

        fn add_view_source(&mut self, connection_id: &str, view_id: &str, active: bool) {
            let authority = SourceAuthority::new();
            let connector = if view_id == floe_context_contract::CALENDAR_CONTEXT_VIEW_ID {
                "calendar.google"
            } else {
                "gmail"
            };
            let source = GrantSourceBinding::try_new(
                self.person_id,
                ConnectionId::try_new(connection_id).unwrap(),
                ConnectorId::try_new(connector).unwrap(),
                ExecutionOwnerId::try_new("server-owner").unwrap(),
            )
            .unwrap();
            let resource = fixture_view_resource(view_id, connection_id);
            let scope = GrantScope::try_new(
                vec![ResourceHandle::try_new(&resource).unwrap()],
                remote_view_data_categories(view_id).to_vec(),
                vec![GrantOperation::Read],
                vec![GrantPurpose::Assistant],
                vec![GrantConsumer::builtin("assistant").unwrap()],
                ProcessingRestriction::LocalOnly,
            )
            .unwrap();
            let mut grant = DataAccessGrant::new(
                GrantId::new(),
                Uuid::new_v4(),
                source.clone(),
                scope.clone(),
            )
            .unwrap();
            if active {
                grant.activate_review(grant.authority(), scope).unwrap();
            }
            let now = Utc::now().timestamp_millis();
            let view = match view_id {
                MAIL_VIEW => serde_json::json!({
                    "schema_version": floe_agent_contract::AGENT_VERSION,
                    "view_id": MAIL_VIEW,
                    "source_handle": format!("mail:{connection_id}"),
                    "observed_at_unix_ms": now - 1,
                    "expires_at_unix_ms": now + 60_000,
                    "coverage_complete": true,
                    "next_cursor": null,
                    "items": [],
                }),
                LOGISTICS_VIEW => serde_json::json!({
                    "schema_version": floe_agent_contract::AGENT_VERSION,
                    "view_id": LOGISTICS_VIEW,
                    "source_handle": format!("logistics:{connection_id}"),
                    "observed_at_unix_ms": now - 1,
                    "expires_at_unix_ms": now + 60_000,
                    "coverage_complete": true,
                    "items": [],
                }),
                floe_context_contract::CALENDAR_CONTEXT_VIEW_ID => serde_json::json!({
                    "schema_version": floe_agent_contract::AGENT_VERSION,
                    "view_id": floe_context_contract::CALENDAR_CONTEXT_VIEW_ID,
                    "source_handle": format!("calendar:{connection_id}"),
                    "observed_at_unix_ms": now - 1,
                    "expires_at_unix_ms": now + 60_000,
                    "range_start_unix_ms": 1_000,
                    "range_end_unix_ms": 2_000,
                    "coverage_complete": true,
                    "items": [],
                }),
                _ => unreachable!(),
            };
            self.sources.push(SourceFixture {
                grant,
                source_authority: authority,
                source_resources: vec![ResourceHandle::try_new(resource).unwrap()],
                policy: ConsumerPolicyAuthority::new(),
                view,
                expired_credential: false,
            });
        }

        fn reference(&self, view_id: &str, connection_id: &str) -> RemoteViewSourceReference {
            let source = self
                .sources
                .iter()
                .find(|source| {
                    source.grant.scope().resources().iter().any(|resource| {
                        resource.as_str() == fixture_view_resource(view_id, connection_id)
                    })
                })
                .unwrap();
            RemoteViewSourceReference {
                view_id: view_id.into(),
                person_id: self.person_id.to_string(),
                client_id: "client".into(),
                device_id: "device".into(),
                audience: "server-audience".into(),
                connector_id: source.grant.source().connector().as_str().into(),
                connection_id: connection_id.into(),
                connection_revision: 7,
                execution_owner: "server-owner".into(),
                source_authority: if self.rotated_source_authority {
                    SourceAuthority::new()
                } else {
                    source.source_authority
                },
                resource: fixture_view_resource(view_id, connection_id),
                source_resources: source.source_resources.clone(),
                provider_identity: "account".into(),
            }
        }
    }

    impl RemoteGrantTransport for ViewFixture {
        fn producer_identity<'a>(
            &'a self,
            _: &'a RemoteCallWindow,
        ) -> BoxFuture<'a, Result<RemoteProducerIdentity, AgentFailure>> {
            Box::pin(async { Err(AgentFailure::CapabilityUnavailable) })
        }

        fn view_source_preview<'a>(
            &'a self,
            _: RemoteSourceQuery<'a>,
            _: &'a RemoteCallWindow,
        ) -> BoxFuture<'a, Result<SignedSourcePreview, AgentFailure>> {
            Box::pin(async {
                Ok(SignedSourcePreview {
                    descriptor_b64url: "signed".into(),
                    producer_signature: "signature".into(),
                    connection_revision: 7,
                    producer: RemoteProducerIdentity {
                        schema_version: 1,
                        instance_id: "server".into(),
                        execution_owner: "server-owner".into(),
                        audience: "server-audience".into(),
                        key_id: "key".into(),
                        public_key: "key".into(),
                        fingerprint: "fingerprint".into(),
                    },
                })
            })
        }

        fn calendar_source_preview<'a>(
            &'a self,
            _: floe_access::RemoteCalendarQuery<'a>,
            _: &'a RemoteCallWindow,
        ) -> BoxFuture<'a, Result<floe_access::SignedCalendarPreview, AgentFailure>> {
            Box::pin(async { Err(AgentFailure::CapabilityUnavailable) })
        }
    }

    impl RemoteViewTransport for ViewFixture {
        fn read_admitted_view<'a>(
            &'a self,
            read: AdmittedRemoteRead<'a>,
            _: &'a RemoteCallWindow,
        ) -> BoxFuture<'a, Result<Value, AgentFailure>> {
            Box::pin(async move {
                self.reads.fetch_add(1, Ordering::SeqCst);
                let source = self
                    .sources
                    .iter()
                    .find(|source| {
                        source
                            .grant
                            .scope()
                            .resources()
                            .iter()
                            .any(|resource| resource.as_str() == read.resource)
                    })
                    .unwrap();
                if source.expired_credential {
                    return Err(AgentFailure::CredentialExpired);
                }
                Ok(source.view.clone())
            })
        }
    }

    impl RemoteGrantStore for ViewFixture {
        fn pinned_producer<'a>(
            &'a self,
        ) -> BoxFuture<'a, Result<RemoteProducerIdentity, AgentFailure>> {
            Box::pin(async { Err(AgentFailure::CapabilityUnavailable) })
        }

        fn verify_view_source_preview<'a>(
            &'a self,
            _: &'a SignedSourcePreview,
            _: RemotePairingIdentity<'a>,
            query: RemoteSourceQuery<'a>,
        ) -> BoxFuture<'a, Result<RemoteViewSourceReference, AgentFailure>> {
            let reference = self.reference(query.view_id, query.connection_id);
            Box::pin(async move { Ok(reference) })
        }

        fn grants<'a>(
            &'a self,
            _: usize,
        ) -> BoxFuture<'a, Result<Vec<DataAccessGrant>, AgentFailure>> {
            let grants = self
                .sources
                .iter()
                .map(|source| source.grant.clone())
                .collect();
            Box::pin(async move { Ok(grants) })
        }

        fn find_view_grant<'a>(
            &'a self,
            view_id: &'a str,
            source: &'a GrantSourceBinding,
            consumer: &'a str,
        ) -> BoxFuture<'a, Result<Option<DataAccessGrant>, AgentFailure>> {
            let grant = self.sources.iter().find(|candidate| {
                candidate.grant.source() == source
                    && candidate.grant.scope().resources().iter().any(|resource| {
                        resource.as_str() == fixture_view_resource(view_id, source.connection_id().as_str())
                    })
                    && candidate.grant.scope().consumers().iter().any(|allowed| allowed.identifier() == consumer)
            }).map(|candidate| candidate.grant.clone());
            Box::pin(async move { Ok(grant) })
        }

        fn activate_view_grant<'a>(
            &'a self,
            _: &'a str,
            _: GrantId,
            _: Option<GrantAuthority>,
            _: GrantSourceBinding,
            _: GrantScope,
        ) -> BoxFuture<'a, Result<DataAccessGrant, AgentFailure>> {
            Box::pin(async { Err(AgentFailure::CapabilityUnavailable) })
        }

        fn verify_calendar_source_preview<'a>(
            &'a self,
            _: &'a floe_access::SignedCalendarPreview,
            _: RemotePairingIdentity<'a>,
            _: floe_access::RemoteCalendarQuery<'a>,
        ) -> BoxFuture<'a, Result<floe_access::RemoteCalendarSourceReference, AgentFailure>>
        {
            Box::pin(async { Err(AgentFailure::CapabilityUnavailable) })
        }

        fn activate_calendar_grant<'a>(
            &'a self,
            _: GrantId,
            _: Option<GrantAuthority>,
            _: GrantSourceBinding,
            _: GrantScope,
            _: Option<ConsumerPolicyAuthority>,
        ) -> BoxFuture<'a, Result<DataAccessGrant, AgentFailure>> {
            Box::pin(async { Err(AgentFailure::CapabilityUnavailable) })
        }

        fn find_calendar_grant<'a>(
            &'a self,
            _: &'a GrantSourceBinding,
            _: &'a str,
        ) -> BoxFuture<'a, Result<Option<DataAccessGrant>, AgentFailure>> {
            Box::pin(async { Err(AgentFailure::CapabilityUnavailable) })
        }

        fn calendar_grant_policy<'a>(
            &'a self,
            _: GrantId,
        ) -> BoxFuture<'a, Result<ConsumerPolicyAuthority, AgentFailure>> {
            Box::pin(async { Err(AgentFailure::CapabilityUnavailable) })
        }

        fn calendar_grant<'a>(
            &'a self,
            _: GrantId,
        ) -> BoxFuture<'a, Result<DataAccessGrant, AgentFailure>> {
            Box::pin(async { Err(AgentFailure::CapabilityUnavailable) })
        }

        fn pause_calendar_grant<'a>(
            &'a self,
            _: GrantId,
            _: GrantAuthority,
        ) -> BoxFuture<'a, Result<DataAccessGrant, AgentFailure>> {
            Box::pin(async { Err(AgentFailure::CapabilityUnavailable) })
        }

        fn view_grant_binding<'a>(
            &'a self,
            view_id: &'a str,
            connector: &'a str,
            connection_id: &'a str,
        ) -> BoxFuture<'a, Result<RemoteGrantBinding, AgentFailure>> {
            let found = self
                .sources
                .iter()
                .map(|source| (source.grant.clone(), source.policy))
                .find(|(grant, _)| {
                    let source = grant.source();
                    source.connector().as_str() == connector
                        && source.connection_id().as_str() == connection_id
                        && grant.scope().resources().iter().any(|resource| {
                            resource.as_str() == fixture_view_resource(view_id, connection_id)
                        })
                });
            Box::pin(async move {
                found
                    .map(|(grant, consumer_policy)| RemoteGrantBinding {
                        grant,
                        consumer_policy,
                    })
                    .ok_or(AgentFailure::AccessReviewRequired)
            })
        }

        fn calendar_grant_binding<'a>(
            &'a self,
            _: &'a str,
            _: &'a str,
            _: &'a str,
        ) -> BoxFuture<'a, Result<RemoteGrantBinding, AgentFailure>> {
            Box::pin(async { Err(AgentFailure::CapabilityUnavailable) })
        }
    }

    async fn read_mail(
        fixture: &ViewFixture,
    ) -> Result<SourceReadOutcome<(Value, Vec<AuthorizedSourceBinding>)>, AgentFailure> {
        read_view(fixture, MAIL_VIEW).await
    }

    async fn read_view(
        fixture: &ViewFixture,
        view_id: &str,
    ) -> Result<SourceReadOutcome<(Value, Vec<AuthorizedSourceBinding>)>, AgentFailure> {
        let person_text = fixture.person_id.to_string();
        let window = RemoteCallWindow {
            deadline: Instant::now() + Duration::from_secs(5),
            cancellation: Cancellation::default(),
        };
        read_remote_view(
            fixture,
            fixture,
            fixture.person_id,
            RemotePairingIdentity {
                person_id: &person_text,
                client_id: "client",
                device_id: "device",
            },
            view_id,
            "assistant",
            if view_id == MAIL_VIEW {
                serde_json::json!({
                    "schema_version": floe_agent_contract::AGENT_VERSION,
                    "query": "",
                    "cursor": 0,
                    "limit": 25,
                })
            } else if view_id == floe_context_contract::CALENDAR_CONTEXT_VIEW_ID {
                serde_json::json!({
                    "range_start_unix_ms": 1_000,
                    "range_end_unix_ms": 2_000,
                    "cursor": null,
                    "limit": 25,
                })
            } else {
                serde_json::json!({"schema_version": floe_agent_contract::AGENT_VERSION})
            },
            &window,
            Uuid::new_v4(),
            &[7; 32],
        )
        .await
    }

    fn selected_mail(connection_id: &str) -> SourceSelectionReference {
        SourceSelectionReference {
            connector_id: ConnectorId::try_new("gmail").unwrap(),
            connection_id: ConnectionId::try_new(connection_id).unwrap(),
            execution_owner_id: ExecutionOwnerId::try_new("server-owner").unwrap(),
            capability_id: MAIL_VIEW.into(),
            resource: ResourceHandle::try_new(fixture_view_resource(MAIL_VIEW, connection_id))
                .unwrap(),
            contract_version: 1,
        }
    }

    async fn read_selected_mail(
        fixture: &ViewFixture,
        selected: &[SourceSelectionReference],
    ) -> Result<SourceReadOutcome<(Value, Vec<AuthorizedSourceBinding>)>, AgentFailure> {
        let person_text = fixture.person_id.to_string();
        let window = RemoteCallWindow {
            deadline: Instant::now() + Duration::from_secs(5),
            cancellation: Cancellation::default(),
        };
        read_selected_remote_view(
            fixture,
            fixture,
            fixture.person_id,
            RemotePairingIdentity {
                person_id: &person_text,
                client_id: "client",
                device_id: "device",
            },
            MAIL_VIEW,
            "assistant",
            selected,
            serde_json::json!({
                "schema_version": floe_agent_contract::AGENT_VERSION,
                "query": "",
                "cursor": 0,
                "limit": 25,
            }),
            &window,
            Uuid::new_v4(),
            &[7; 32],
        )
        .await
    }

    #[tokio::test]
    async fn hosted_calendar_selected_view_records_exact_signed_leaves_and_stales_on_change() {
        let mut fixture = ViewFixture::new(PersonId::new());
        fixture.add_view_source("calendar-account", floe_context_contract::CALENDAR_CONTEXT_VIEW_ID, true);
        fixture.sources[0].source_resources = ["A", "B"]
            .into_iter()
            .map(|resource| ResourceHandle::try_new(resource).unwrap())
            .collect();
        let grant_id = fixture.sources[0].grant.id();
        let grant_authority = fixture.sources[0].grant.authority();
        let policy = fixture.sources[0].policy;
        let selected = SourceSelectionReference {
            connector_id: ConnectorId::try_new("calendar.google").unwrap(),
            connection_id: ConnectionId::try_new("calendar-account").unwrap(),
            execution_owner_id: ExecutionOwnerId::try_new("server-owner").unwrap(),
            capability_id: floe_context_contract::CALENDAR_CONTEXT_VIEW_ID.into(),
            resource: ResourceHandle::try_new("calendar.timeline:calendar-account").unwrap(),
            contract_version: 1,
        };
        let person_text = fixture.person_id.to_string();
        let window = RemoteCallWindow {
            deadline: Instant::now() + Duration::from_secs(5),
            cancellation: Cancellation::default(),
        };
        let pairing = RemotePairingIdentity {
            person_id: &person_text,
            client_id: "client",
            device_id: "device",
        };
        let query = serde_json::json!({
            "range_start_unix_ms": 1_000,
            "range_end_unix_ms": 2_000,
            "cursor": null,
            "limit": 25,
        });
        let outcome = read_selected_remote_view(
            &fixture,
            &fixture,
            fixture.person_id,
            pairing,
            floe_context_contract::CALENDAR_CONTEXT_VIEW_ID,
            "assistant",
            &[selected],
            query,
            &window,
            Uuid::new_v4(),
            &[7; 32],
        )
        .await
        .unwrap();
        let SourceReadOutcome::Ready((view, bindings)) = outcome else {
            panic!("logical Calendar source must read");
        };
        assert_eq!(view["view_id"], floe_context_contract::CALENDAR_CONTEXT_VIEW_ID);
        assert_eq!(bindings.len(), 1);
        let dependency = &bindings[0].dependency;
        assert_eq!(dependency.resources()[0].as_str(), "calendar.timeline:calendar-account");
        assert_eq!(dependency.source_resources().iter().map(ResourceHandle::as_str).collect::<Vec<_>>(), vec!["A", "B"]);
        assert_eq!(dependency.grant_id(), grant_id);
        assert_eq!(dependency.grant_authority(), grant_authority);
        assert_eq!(dependency.consumer_policy(), policy);
        fixture.sources[0].source_resources.push(ResourceHandle::try_new("C").unwrap());
        assert_eq!(
            authorize_remote_dependency(
                &fixture,
                &fixture,
                fixture.person_id,
                pairing,
                dependency,
                &DependencyAuthorization {
                    deadline: window.deadline,
                    cancellation: window.cancellation.clone(),
                },
            )
            .await,
            Err(AgentFailure::PolicyDenied)
        );
        assert_eq!(fixture.sources[0].grant.id(), grant_id);
        assert_eq!(fixture.sources[0].grant.authority(), grant_authority);
        assert_eq!(fixture.sources[0].policy, policy);
    }

    #[tokio::test]
    async fn selected_a_ignores_unselected_b_even_if_b_is_blocked() {
        let mut fixture = ViewFixture::new(PersonId::new());
        fixture.add_source("a-connection", true);
        fixture.add_source("b-connection", false);
        let outcome = read_selected_mail(&fixture, &[selected_mail("a-connection")])
            .await
            .unwrap();
        let SourceReadOutcome::Ready((_, bindings)) = outcome else {
            panic!("selected A must read")
        };
        assert_eq!(bindings.len(), 1);
        assert_eq!(
            bindings[0].dependency.source().connection_id().as_str(),
            "a-connection"
        );
        assert_eq!(fixture.reads.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn selected_missing_a_does_not_adopt_live_b() {
        let mut fixture = ViewFixture::new(PersonId::new());
        fixture.add_source("b-connection", true);
        let outcome = read_selected_mail(&fixture, &[selected_mail("a-connection")])
            .await
            .unwrap();
        let SourceReadOutcome::NeedsUserAction(blockers) = outcome else {
            panic!("missing A must block")
        };
        assert_eq!(blockers.blockers().len(), 1);
        assert_eq!(
            blockers.blockers()[0].connection_id().unwrap().as_str(),
            "a-connection"
        );
        assert_eq!(fixture.reads.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn same_connection_mail_and_logistics_grants_are_not_duplicate_authority() {
        let mut fixture = ViewFixture::new(PersonId::new());
        fixture.add_view_source("shared", MAIL_VIEW, true);
        fixture.add_view_source("shared", LOGISTICS_VIEW, true);
        assert!(matches!(
            read_mail(&fixture).await.unwrap(),
            SourceReadOutcome::Ready(_)
        ));
        assert!(matches!(
            read_view(&fixture, LOGISTICS_VIEW).await.unwrap(),
            SourceReadOutcome::Ready(_)
        ));
        assert_eq!(fixture.reads.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn work_view_ignores_unrelated_mail_grants() {
        let mut fixture = ViewFixture::new(PersonId::new());
        fixture.add_source("shared", true);
        let outcome = read_view(&fixture, WORK_VIEW).await.unwrap();
        let SourceReadOutcome::NeedsUserAction(blockers) = outcome else {
            panic!("no work target");
        };
        assert_eq!(
            blockers.blockers()[0].reason(),
            SourceAccessRequirementKind::SelectResource
        );
        assert_eq!(fixture.reads.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn duplicate_exact_target_fails_before_payload_io() {
        let mut fixture = ViewFixture::new(PersonId::new());
        fixture.add_source("shared", true);
        fixture.add_source("shared", true);
        assert!(matches!(
            read_mail(&fixture).await,
            Err(AgentFailure::Conflict)
        ));
        assert_eq!(fixture.reads.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn exact_target_with_wrong_consumer_is_a_review_blocker() {
        let mut fixture = ViewFixture::new(PersonId::new());
        fixture.add_source("shared", true);
        let source = &mut fixture.sources[0];
        let scope = GrantScope::try_new(
            source.grant.scope().resources().to_vec(),
            source.grant.scope().categories().to_vec(),
            vec![GrantOperation::Read],
            vec![GrantPurpose::Assistant],
            vec![GrantConsumer::builtin("schedule").unwrap()],
            ProcessingRestriction::LocalOnly,
        )
        .unwrap();
        source
            .grant
            .activate_review(source.grant.authority(), scope)
            .unwrap();
        let outcome = read_mail(&fixture).await.unwrap();
        let SourceReadOutcome::NeedsUserAction(blockers) = outcome else {
            panic!("wrong consumer");
        };
        assert_eq!(
            blockers.blockers()[0].reason(),
            SourceAccessRequirementKind::ReviewChangedSource
        );
        assert_eq!(fixture.reads.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn exact_target_with_wrong_scope_is_never_ready() {
        for (categories, operations, purpose, processing) in [
            (
                vec![GrantDataCategory::Derived],
                vec![GrantOperation::Read],
                GrantPurpose::Assistant,
                ProcessingRestriction::LocalOnly,
            ),
            (
                vec![GrantDataCategory::Content],
                vec![GrantOperation::Read],
                GrantPurpose::Scheduling,
                ProcessingRestriction::LocalOnly,
            ),
            (
                vec![GrantDataCategory::Content],
                vec![GrantOperation::Read],
                GrantPurpose::Assistant,
                ProcessingRestriction::approved_recipient(
                    "model",
                    vec![GrantDataCategory::Content],
                )
                .unwrap(),
            ),
            (
                vec![GrantDataCategory::Content],
                vec![GrantOperation::Suggestion],
                GrantPurpose::Assistant,
                ProcessingRestriction::LocalOnly,
            ),
        ] {
            let mut fixture = ViewFixture::new(PersonId::new());
            fixture.add_source("shared", true);
            let source = &mut fixture.sources[0];
            let scope = GrantScope::try_new(
                source.grant.scope().resources().to_vec(),
                categories,
                operations,
                vec![purpose],
                vec![GrantConsumer::builtin("assistant").unwrap()],
                processing,
            )
            .unwrap();
            source
                .grant
                .activate_review(source.grant.authority(), scope)
                .unwrap();
            let outcome = read_mail(&fixture).await.unwrap();
            let SourceReadOutcome::NeedsUserAction(blockers) = outcome else {
                panic!("mismatched scope must block");
            };
            assert_eq!(
                blockers.blockers()[0].reason(),
                SourceAccessRequirementKind::ReviewChangedSource
            );
            assert_eq!(fixture.reads.load(Ordering::SeqCst), 0);
        }
    }

    #[tokio::test]
    async fn changed_source_incarnation_stales_existing_dependency_without_changing_grant() {
        let mut fixture = ViewFixture::new(PersonId::new());
        fixture.add_source("shared", true);
        let outcome = read_mail(&fixture).await.unwrap();
        let SourceReadOutcome::Ready((_, bindings)) = outcome else {
            panic!("initial source must be admitted");
        };
        let dependency = &bindings[0].dependency;
        let grant_authority = fixture.sources[0].grant.authority();
        fixture.rotated_source_authority = true;
        let person_text = fixture.person_id.to_string();
        let window = RemoteCallWindow {
            deadline: Instant::now() + Duration::from_secs(5),
            cancellation: Cancellation::default(),
        };
        assert_eq!(
            authorize_remote_dependency(
                &fixture,
                &fixture,
                fixture.person_id,
                RemotePairingIdentity {
                    person_id: &person_text,
                    client_id: "client",
                    device_id: "device",
                },
                dependency,
                &DependencyAuthorization {
                    deadline: window.deadline,
                    cancellation: window.cancellation.clone(),
                },
            )
            .await,
            Err(AgentFailure::PolicyDenied)
        );
        assert_eq!(fixture.sources[0].grant.authority(), grant_authority);
        assert_eq!(fixture.reads.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn foreign_person_grant_is_never_adopted() {
        let mut fixture = ViewFixture::new(PersonId::new());
        fixture.add_source("shared", true);
        fixture.person_id = PersonId::new();
        let outcome = read_mail(&fixture).await.unwrap();
        let SourceReadOutcome::NeedsUserAction(blockers) = outcome else {
            panic!("foreign grant must not be ready");
        };
        assert_eq!(
            blockers.blockers()[0].reason(),
            SourceAccessRequirementKind::SelectResource
        );
        assert_eq!(fixture.reads.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn paused_source_blocks_without_a_truncated_aggregate() {
        let person_id = PersonId::new();
        let mut fixture = ViewFixture::new(person_id);
        fixture.add_source("a-connection", true);
        fixture.add_source("b-connection", false);
        let outcome = read_mail(&fixture).await.unwrap();
        let SourceReadOutcome::NeedsUserAction(blockers) = outcome else {
            panic!("a paused source must block, not truncate");
        };
        assert_eq!(blockers.blockers().len(), 1);
        let blocker = &blockers.blockers()[0];
        assert_eq!(blocker.source_id(), "floe.source.mail");
        assert_eq!(blocker.connection_id().unwrap().as_str(), "b-connection");
        assert_eq!(blocker.reason(), SourceAccessRequirementKind::EnableObserve);
        assert!(blocker.inline_resolution());
        // Classification runs before any read: no payload was fetched, so no
        // partial aggregate and no failed-acquisition dependency can leak.
        assert_eq!(fixture.reads.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn unknown_target_is_navigation_only_selection() {
        let fixture = ViewFixture::new(PersonId::new());
        let outcome = read_mail(&fixture).await.unwrap();
        let SourceReadOutcome::NeedsUserAction(blockers) = outcome else {
            panic!("an unknown target must request selection");
        };
        assert_eq!(blockers.blockers().len(), 1);
        let blocker = &blockers.blockers()[0];
        assert_eq!(
            blocker.reason(),
            SourceAccessRequirementKind::SelectResource
        );
        assert!(blocker.connection_id().is_none());
        assert!(!blocker.inline_resolution());
        assert_eq!(fixture.reads.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn admitted_sources_keep_their_own_bindings() {
        let person_id = PersonId::new();
        let mut fixture = ViewFixture::new(person_id);
        fixture.add_source("a-connection", true);
        fixture.add_source("b-connection", true);
        let outcome = read_mail(&fixture).await.unwrap();
        let SourceReadOutcome::Ready((_, bindings)) = outcome else {
            panic!("admitted sources must read");
        };
        assert_eq!(bindings.len(), 2);
        let mut connections: Vec<_> = bindings
            .iter()
            .map(|binding| {
                binding
                    .dependency
                    .source()
                    .connection_id()
                    .as_str()
                    .to_owned()
            })
            .collect();
        connections.sort();
        assert_eq!(connections, vec!["a-connection", "b-connection"]);
        assert_eq!(fixture.reads.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn expired_credential_mid_read_becomes_a_reconnect_blocker() {
        let person_id = PersonId::new();
        let mut fixture = ViewFixture::new(person_id);
        fixture.add_source("a-connection", true);
        fixture.add_source("b-connection", true);
        fixture
            .sources
            .iter_mut()
            .find(|source| source.grant.source().connection_id().as_str() == "b-connection")
            .unwrap()
            .expired_credential = true;
        let outcome = read_mail(&fixture).await.unwrap();
        let SourceReadOutcome::NeedsUserAction(blockers) = outcome else {
            panic!("an expired credential must block, not truncate");
        };
        assert_eq!(blockers.blockers().len(), 1);
        let blocker = &blockers.blockers()[0];
        assert_eq!(blocker.connection_id().unwrap().as_str(), "b-connection");
        assert_eq!(blocker.reason(), SourceAccessRequirementKind::Reconnect);
    }
}
