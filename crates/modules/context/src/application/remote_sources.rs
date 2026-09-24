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
    RemoteGrantTransport, RemotePairingIdentity, RemoteSourceQuery, active_resource_grant,
    admit_remote_view_binding, admit_remote_view_source, remote_calendar_dependency_source_admits,
    remote_dependency_binding_matches, remote_dependency_live, remote_dependency_resource,
    remote_dependency_source_admits, remote_view_source, source_matches_producer,
};
use floe_agent_contract::{AgentFailure, BoxFuture, PersonId};
use floe_context_contract::{
    AuthorizedSourceBinding, CALENDAR_CONTEXT_VIEW_ID, CalendarContextView, CalendarViewQuery,
    ContextDependency, GrantConsumer, GrantOperation, GrantPurpose, GrantScope, GrantSourceBinding,
    MAX_CALENDAR_CONTEXT_BYTES, MAX_SOURCE_ACCESS_BLOCKERS, ObservedGrant, ProcessingRestriction,
    ResourceHandle, SourceAccessBlockers, SourceAccessRequirement, SourceAccessRequirementKind,
    SourceReadOutcome, SourceUnavailable, validate_calendar_context_view_for_query,
};
use serde_json::Value;
use uuid::Uuid;

use crate::application::remote_views::{
    LOGISTICS_VIEW, MAIL_VIEW, WORK_VIEW, is_remote_view, merge_remote_views,
    remote_view_connector_admissible, remote_view_dependency, remote_view_resource,
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

pub struct RemoteCalendarViewRead<'a> {
    pub person_id: PersonId,
    pub pairing: RemotePairingIdentity<'a>,
    pub connector_id: &'a str,
    pub connection_id: &'a str,
    pub connection_revision: u64,
    pub resource: &'a str,
    pub consumer_name: &'a str,
    pub query: &'a CalendarViewQuery,
    pub window: &'a RemoteCallWindow,
    pub process_incarnation_id: Uuid,
}

pub async fn read_remote_calendar_view(
    store: &impl RemoteGrantStore,
    transport: &impl RemoteViewTransport,
    read: RemoteCalendarViewRead<'_>,
) -> Result<(CalendarContextView, ContextDependency, GrantScope), AgentFailure> {
    check_window(read.window)?;
    read.query.validate()?;
    if read.pairing.person_id != read.person_id.to_string()
        || !matches!(read.connector_id, "calendar.google" | "calendar.microsoft")
        || read.connection_revision == 0
        || read.resource.is_empty()
        || read.process_incarnation_id.is_nil()
        || read.pairing.client_id.is_empty()
        || read.pairing.device_id.is_empty()
    {
        return Err(AgentFailure::PolicyDenied);
    }
    let consumer =
        GrantConsumer::builtin(read.consumer_name).map_err(|_| AgentFailure::InvalidInput)?;
    let grants = store.grants(128).await?;
    let grant = active_resource_grant(&grants, read.person_id, &consumer, |source| {
        (source.connector().as_str() == read.connector_id
            && source.connection_id().as_str() == read.connection_id)
            .then(|| read.resource.to_owned())
    })?;
    floe_access::admit_remote_calendar_read(&grant, &consumer, read.resource)?;
    let source_query = RemoteSourceQuery {
        view_id: CALENDAR_CONTEXT_VIEW_ID,
        connector_id: read.connector_id,
        connection_id: read.connection_id,
        resource: read.resource,
    };
    let preview = transport
        .view_source_preview(source_query, read.window)
        .await?;
    let reference = store
        .verify_view_source_preview(&preview, read.pairing, source_query)
        .await?;
    source_matches_producer(&reference, &preview.producer, preview.connection_revision)?;
    admit_remote_view_source(&reference, grant.source())?;
    if reference.connection_revision != read.connection_revision {
        return Err(AgentFailure::StaleContext);
    }
    let binding = store
        .calendar_grant_binding(
            read.connector_id,
            read.connection_id,
            reference.source_authority,
            read.resource,
        )
        .await?;
    admit_remote_view_binding(&binding.grant, &grant, grant.source(), read.resource)?;
    let query = serde_json::json!({
        "range_start_unix_ms": read.query.range_start_unix_ms(),
        "range_end_unix_ms": read.query.range_end_unix_ms(),
        "cursor": read.query.cursor().unwrap_or(""),
        "limit": read.query.limit(),
    });
    let value = transport
        .read_admitted_view(
            AdmittedRemoteRead {
                view_id: CALENDAR_CONTEXT_VIEW_ID,
                binding: &binding,
                consumer: read.consumer_name,
                resource: read.resource,
                connection_revision: reference.connection_revision,
                max_items: read.query.limit(),
                max_bytes: MAX_CALENDAR_CONTEXT_BYTES,
                query,
                pairing: read.pairing,
            },
            read.window,
        )
        .await?;
    check_window(read.window)?;
    let current_grants = store.grants(128).await?;
    let current_grant =
        active_resource_grant(&current_grants, read.person_id, &consumer, |source| {
            (source.connector().as_str() == read.connector_id
                && source.connection_id().as_str() == read.connection_id)
                .then(|| read.resource.to_owned())
        })?;
    floe_access::grant_unchanged(&grant, &current_grant)?;
    let current_preview = transport
        .view_source_preview(source_query, read.window)
        .await?;
    let current_reference = store
        .verify_view_source_preview(&current_preview, read.pairing, source_query)
        .await?;
    source_matches_producer(
        &current_reference,
        &current_preview.producer,
        current_preview.connection_revision,
    )?;
    if current_reference != reference
        || current_preview.connection_revision != read.connection_revision
    {
        return Err(AgentFailure::StaleContext);
    }
    let current_binding = store
        .calendar_grant_binding(
            read.connector_id,
            read.connection_id,
            current_reference.source_authority,
            read.resource,
        )
        .await?;
    admit_remote_view_binding(
        &current_binding.grant,
        &grant,
        grant.source(),
        read.resource,
    )?;
    if current_binding.consumer_policy != binding.consumer_policy {
        return Err(AgentFailure::PolicyDenied);
    }
    check_window(read.window)?;
    let view: CalendarContextView =
        serde_json::from_value(value).map_err(|_| AgentFailure::CapabilityUnavailable)?;
    validate_calendar_context_view_for_query(&view, read.query, Utc::now().timestamp_millis())?;
    let dependency = remote_view_dependency(
        read.person_id,
        &binding.grant,
        binding.consumer_policy,
        remote_view_source(&reference)?,
        read.resource,
        consumer,
        serde_json::to_vec(read.query).map_err(|_| AgentFailure::InvalidInput)?,
        Uuid::new_v4(),
        read.process_incarnation_id,
        view.observed_at_unix_ms,
        view.expires_at_unix_ms,
    )?;
    if dependency.processing() != &ProcessingRestriction::LocalOnly {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok((view, dependency, binding.grant.scope().clone()))
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

/// The source identity one remote view reports blockers under.
fn remote_view_source_id(view_id: &str) -> Option<&'static str> {
    match view_id {
        MAIL_VIEW => Some("floe.source.mail"),
        WORK_VIEW => Some("floe.source.work-context"),
        LOGISTICS_VIEW => Some("floe.source.logistics"),
        _ => None,
    }
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
        && grant.scope().operations().contains(&GrantOperation::Read)
        && grant.scope().purposes().contains(&GrantPurpose::Assistant)
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
    let resource = ResourceHandle::try_new(remote_view_resource(
        view_id,
        source.connection_id().as_str(),
    ))
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
        Some(source.source_authority()),
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
/// over the same source are a duplicate authority the review cannot pick
/// between, so the read fails closed with no card.
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
                && remote_view_connector_admissible(view_id, grant.source().connector().as_str())
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
    if known
        .windows(2)
        .any(|pair| pair[0].source() == pair[1].source())
    {
        return Err(AgentFailure::Conflict);
    }
    let mut admitted = Vec::with_capacity(known.len());
    let mut blocked = Vec::with_capacity(known.len());
    for grant in known {
        let expected =
            remote_view_resource(view_id, grant.source().connection_id().as_str());
        if remote_grant_admits(grant, person_id, consumer, &expected) {
            admitted.push((*grant).clone());
            continue;
        }
        let reason = if grant.state() == GrantState::Paused {
            SourceAccessRequirementKind::EnableObserve
        } else {
            SourceAccessRequirementKind::ReviewChangedSource
        };
        blocked.push(remote_source_blocker(grant, view_id, source_id, consumer, reason)?);
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
    admit_remote_view_binding(&binding.grant, grant, source, &resource)?;
    let raw = transport
        .read_admitted_view(
            AdmittedRemoteRead {
                view_id,
                binding: &binding,
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
    let (value, observed, expires) =
        validate_remote_view(view_id, raw, now, max_items, max_bytes)?;
    let dependency = remote_view_dependency(
        person_id,
        &binding.grant,
        binding.consumer_policy,
        remote_view_source(&reference)?,
        &resource,
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
    let source_id = remote_view_source_id(view_id).ok_or(AgentFailure::InvalidInput)?;
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
            &consumer,
            consumer_name,
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
                &consumer,
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
        merge_remote_views(view_id, values, now, max_items, max_bytes)?,
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
    let calendar = matches!(
        dependency.source().connector().as_str(),
        "calendar.google" | "calendar.microsoft"
    );
    let view_id = if calendar {
        CALENDAR_CONTEXT_VIEW_ID
    } else {
        split_remote_view_resource(resource, connection_id)?
    };
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
    let binding = if calendar {
        remote_calendar_dependency_source_admits(
            dependency,
            &reference,
            preview.connection_revision,
        )?;
        store
            .calendar_grant_binding(
                dependency.source().connector().as_str(),
                connection_id,
                reference.source_authority,
                resource,
            )
            .await?
    } else {
        remote_dependency_source_admits(
            dependency,
            &reference,
            preview.connection_revision,
            &preview.producer.audience,
        )?;
        store
            .view_grant_binding(
                view_id,
                dependency.source().connector().as_str(),
                connection_id,
                reference.source_authority,
            )
            .await?
    };
    remote_dependency_binding_matches(
        binding.consumer_policy,
        binding.grant.authority(),
        dependency,
    )
}

#[cfg(test)]
mod calendar_tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use floe_access::{
        ConnectionId, ConnectorId, ConsumerPolicyAuthority, DataAccessGrant, ExecutionOwnerId,
        GrantAuthority, GrantId, GrantScope, GrantSourceBinding, RemoteCalendarQuery,
        RemoteGrantBinding, RemoteProducerIdentity, RemoteViewSourceReference,
        SignedCalendarPreview, SignedSourcePreview, SourceAuthority,
    };
    use floe_context_contract::{CALENDAR_CONTEXT_VIEW_ID, CalendarContextItem};
    use floe_execution::Cancellation;
    use tokio::time::{Duration, Instant};

    use super::*;

    struct CalendarFixture {
        grant: DataAccessGrant,
        reference: RemoteViewSourceReference,
        view: CalendarContextView,
        policy: ConsumerPolicyAuthority,
        reads: AtomicUsize,
        read_resources: std::sync::Mutex<Vec<String>>,
        source_changes_after_read: bool,
        sibling: Option<(
            DataAccessGrant,
            ConsumerPolicyAuthority,
            CalendarContextView,
        )>,
    }

    impl CalendarFixture {
        fn new(person_id: PersonId, connection_id: &str, query: &CalendarViewQuery) -> Self {
            Self::new_with_consumers(person_id, connection_id, query, &["floe.builtin.schedule"])
        }

        fn new_with_consumers(
            person_id: PersonId,
            connection_id: &str,
            query: &CalendarViewQuery,
            consumers: &[&str],
        ) -> Self {
            let source_authority = SourceAuthority::new();
            let source = GrantSourceBinding::try_new(
                person_id,
                ConnectionId::try_new(connection_id).unwrap(),
                ConnectorId::try_new("calendar.google").unwrap(),
                ExecutionOwnerId::try_new("server-owner").unwrap(),
                source_authority,
            )
            .unwrap();
            let scope = floe_access::remote_calendar_scope(
                "primary",
                &consumers
                    .iter()
                    .map(|consumer| GrantConsumer::builtin(*consumer).unwrap())
                    .collect::<Vec<_>>(),
            )
            .unwrap();
            let mut grant = DataAccessGrant::new(
                GrantId::new(),
                Uuid::new_v4(),
                source.clone(),
                scope.clone(),
            )
            .unwrap();
            grant
                .activate_review(grant.authority(), source, scope)
                .unwrap();
            let now = Utc::now().timestamp_millis();
            Self {
                grant,
                reference: RemoteViewSourceReference {
                    view_id: CALENDAR_CONTEXT_VIEW_ID.into(),
                    person_id: person_id.to_string(),
                    client_id: "client".into(),
                    device_id: "device".into(),
                    audience: "server-audience".into(),
                    connector_id: "calendar.google".into(),
                    connection_id: connection_id.into(),
                    connection_revision: 7,
                    execution_owner: "server-owner".into(),
                    source_authority,
                    resource: "primary".into(),
                    provider_identity: "account".into(),
                },
                view: CalendarContextView {
                    schema_version: 1,
                    view_id: CALENDAR_CONTEXT_VIEW_ID.into(),
                    source_handle: "calendar:test".into(),
                    observed_at_unix_ms: now - 1,
                    expires_at_unix_ms: now + 60_000,
                    range_start_unix_ms: query.range_start_unix_ms(),
                    range_end_unix_ms: query.range_end_unix_ms(),
                    coverage_complete: false,
                    next_cursor: Some("page-two".into()),
                    items: vec![CalendarContextItem {
                        evidence_handle: "event:one".into(),
                        untrusted_title: "Review".into(),
                        starts_at_unix_ms: now,
                        ends_at_unix_ms: now + 1_000,
                        all_day: false,
                    }],
                },
                policy: ConsumerPolicyAuthority::new(),
                reads: AtomicUsize::new(0),
                read_resources: std::sync::Mutex::new(Vec::new()),
                source_changes_after_read: false,
                sibling: None,
            }
        }

        fn with_sibling(mut self, resource: &str, consumers: &[&str]) -> Self {
            let source = GrantSourceBinding::try_new(
                self.grant.source().person_id(),
                ConnectionId::try_new(self.grant.source().connection_id().as_str()).unwrap(),
                ConnectorId::try_new(self.grant.source().connector().as_str()).unwrap(),
                ExecutionOwnerId::try_new(self.grant.source().execution_owner().as_str()).unwrap(),
                self.grant.source().source_authority(),
            )
            .unwrap();
            let scope = floe_access::remote_calendar_scope(
                resource,
                &consumers
                    .iter()
                    .map(|consumer| GrantConsumer::builtin(*consumer).unwrap())
                    .collect::<Vec<_>>(),
            )
            .unwrap();
            let mut grant = DataAccessGrant::new(
                GrantId::new(),
                Uuid::new_v4(),
                source.clone(),
                scope.clone(),
            )
            .unwrap();
            grant
                .activate_review(grant.authority(), source, scope)
                .unwrap();
            let mut view = self.view.clone();
            view.source_handle = format!("calendar:{resource}");
            view.items = vec![CalendarContextItem {
                evidence_handle: format!("event:{resource}"),
                untrusted_title: format!("Review {resource}"),
                starts_at_unix_ms: view.items[0].starts_at_unix_ms,
                ends_at_unix_ms: view.items[0].ends_at_unix_ms,
                all_day: false,
            }];
            self.sibling = Some((grant, ConsumerPolicyAuthority::new(), view));
            self
        }

        fn resources(&self) -> Vec<String> {
            let mut resources = vec!["primary".to_owned()];
            if let Some((grant, _, _)) = &self.sibling {
                let resource = grant.scope().resources()[0].as_str().to_owned();
                if resource != "primary" {
                    resources.push(resource);
                }
            }
            resources
        }
    }

    impl RemoteGrantTransport for CalendarFixture {
        fn producer_identity<'a>(
            &'a self,
            _: &'a RemoteCallWindow,
        ) -> BoxFuture<'a, Result<RemoteProducerIdentity, AgentFailure>> {
            Box::pin(async { Err(AgentFailure::CapabilityUnavailable) })
        }

        fn view_source_preview<'a>(
            &'a self,
            query: RemoteSourceQuery<'a>,
            _: &'a RemoteCallWindow,
        ) -> BoxFuture<'a, Result<SignedSourcePreview, AgentFailure>> {
            let resources = self.resources();
            Box::pin(async move {
                assert_eq!(query.view_id, CALENDAR_CONTEXT_VIEW_ID);
                assert!(resources.iter().any(|resource| resource == query.resource));
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
            _: RemoteCalendarQuery<'a>,
            _: &'a RemoteCallWindow,
        ) -> BoxFuture<'a, Result<SignedCalendarPreview, AgentFailure>> {
            Box::pin(async { Err(AgentFailure::CapabilityUnavailable) })
        }
    }

    impl RemoteViewTransport for CalendarFixture {
        fn read_admitted_view<'a>(
            &'a self,
            read: AdmittedRemoteRead<'a>,
            _: &'a RemoteCallWindow,
        ) -> BoxFuture<'a, Result<Value, AgentFailure>> {
            let resources = self.resources();
            Box::pin(async move {
                self.reads.fetch_add(1, Ordering::SeqCst);
                self.read_resources
                    .lock()
                    .unwrap()
                    .push(read.resource.to_owned());
                assert_eq!(read.view_id, CALENDAR_CONTEXT_VIEW_ID);
                assert!(matches!(
                    read.consumer,
                    "floe.builtin.schedule" | "floe.builtin.focus-attention"
                ));
                assert!(resources.iter().any(|resource| resource == read.resource));
                assert_eq!(read.connection_revision, 7);
                let view = match &self.sibling {
                    Some((grant, _, sibling_view))
                        if grant.scope().resources()[0].as_str() == read.resource =>
                    {
                        sibling_view
                    }
                    _ => &self.view,
                };
                serde_json::to_value(view).map_err(|_| AgentFailure::InvalidInput)
            })
        }
    }

    impl RemoteGrantStore for CalendarFixture {
        fn pinned_producer<'a>(
            &'a self,
        ) -> BoxFuture<'a, Result<RemoteProducerIdentity, AgentFailure>> {
            Box::pin(async { Err(AgentFailure::CapabilityUnavailable) })
        }

        fn verify_view_source_preview<'a>(
            &'a self,
            _: &'a SignedSourcePreview,
            _: RemotePairingIdentity<'a>,
            _: RemoteSourceQuery<'a>,
        ) -> BoxFuture<'a, Result<RemoteViewSourceReference, AgentFailure>> {
            Box::pin(async move {
                let mut reference = self.reference.clone();
                if self.source_changes_after_read && self.reads.load(Ordering::SeqCst) > 0 {
                    reference.source_authority = SourceAuthority::new();
                }
                Ok(reference)
            })
        }

        fn grants<'a>(
            &'a self,
            _: usize,
        ) -> BoxFuture<'a, Result<Vec<DataAccessGrant>, AgentFailure>> {
            let mut grants = vec![self.grant.clone()];
            if let Some((sibling, _, _)) = &self.sibling {
                grants.push(sibling.clone());
            }
            Box::pin(async move { Ok(grants) })
        }

        fn find_view_grant<'a>(
            &'a self,
            _: &'a str,
            _: &'a GrantSourceBinding,
            _: &'a str,
        ) -> BoxFuture<'a, Result<Option<DataAccessGrant>, AgentFailure>> {
            Box::pin(async { Err(AgentFailure::CapabilityUnavailable) })
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
            _: &'a SignedCalendarPreview,
            _: RemotePairingIdentity<'a>,
            _: RemoteCalendarQuery<'a>,
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
            _: &'a str,
            _: &'a str,
            _: &'a str,
            _: SourceAuthority,
        ) -> BoxFuture<'a, Result<RemoteGrantBinding, AgentFailure>> {
            Box::pin(async { Err(AgentFailure::CapabilityUnavailable) })
        }

        fn calendar_grant_binding<'a>(
            &'a self,
            connector: &'a str,
            connection_id: &'a str,
            source_authority: SourceAuthority,
            resource: &'a str,
        ) -> BoxFuture<'a, Result<RemoteGrantBinding, AgentFailure>> {
            let mut candidates = vec![(self.grant.clone(), self.policy)];
            if let Some((sibling, policy, _)) = &self.sibling {
                candidates.push((sibling.clone(), *policy));
            }
            Box::pin(async move {
                let mut found = None;
                for (grant, policy) in &candidates {
                    let source = grant.source();
                    if source.connector().as_str() == connector
                        && source.connection_id().as_str() == connection_id
                        && source.source_authority() == source_authority
                        && grant.scope().resources().len() == 1
                        && grant.scope().resources()[0].as_str() == resource
                    {
                        if found.is_some() {
                            return Err(AgentFailure::Conflict);
                        }
                        found = Some(RemoteGrantBinding {
                            grant: grant.clone(),
                            consumer_policy: *policy,
                        });
                    }
                }
                found.ok_or(AgentFailure::AccessReviewRequired)
            })
        }
    }

    #[tokio::test]
    async fn calendar_read_and_dependency_recheck_use_the_same_grant_and_resource() {
        let person_id = PersonId::new();
        let connection_id = Uuid::new_v4().to_string();
        let now = Utc::now().timestamp_millis();
        let query = CalendarViewQuery::try_new(now - 60_000, now + 60_000, None, 1).unwrap();
        let fixture = CalendarFixture::new(person_id, &connection_id, &query);
        let window = RemoteCallWindow {
            deadline: Instant::now() + Duration::from_secs(5),
            cancellation: Cancellation::default(),
        };
        let person_text = person_id.to_string();
        let pairing = RemotePairingIdentity {
            person_id: &person_text,
            client_id: "client",
            device_id: "device",
        };
        let process_incarnation_id = Uuid::new_v4();
        let (view, dependency, scope) = read_remote_calendar_view(
            &fixture,
            &fixture,
            RemoteCalendarViewRead {
                person_id,
                pairing,
                connector_id: "calendar.google",
                connection_id: &connection_id,
                connection_revision: 7,
                resource: "primary",
                consumer_name: "floe.builtin.schedule",
                query: &query,
                window: &window,
                process_incarnation_id,
            },
        )
        .await
        .unwrap();
        assert_eq!(view.next_cursor.as_deref(), Some("page-two"));
        assert_eq!(dependency.consumer().identifier(), "floe.builtin.schedule");
        assert_eq!(dependency.resources()[0].as_str(), "primary");
        assert_eq!(dependency.process_incarnation_id(), process_incarnation_id);
        assert_eq!(scope.processing(), &ProcessingRestriction::LocalOnly);
        assert_eq!(fixture.reads.load(Ordering::SeqCst), 1);
        authorize_remote_dependency(
            &fixture,
            &fixture,
            person_id,
            pairing,
            &dependency,
            &DependencyAuthorization {
                deadline: window.deadline,
                cancellation: window.cancellation.clone(),
            },
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn calendar_read_rejects_ungranted_consumer_and_changed_source() {
        let person_id = PersonId::new();
        let connection_id = Uuid::new_v4().to_string();
        let now = Utc::now().timestamp_millis();
        let query = CalendarViewQuery::try_new(now - 60_000, now + 60_000, None, 1).unwrap();
        let mut fixture = CalendarFixture::new(person_id, &connection_id, &query);
        let window = RemoteCallWindow {
            deadline: Instant::now() + Duration::from_secs(5),
            cancellation: Cancellation::default(),
        };
        let person_text = person_id.to_string();
        let pairing = RemotePairingIdentity {
            person_id: &person_text,
            client_id: "client",
            device_id: "device",
        };
        let read = |consumer_name| RemoteCalendarViewRead {
            person_id,
            pairing,
            connector_id: "calendar.google",
            connection_id: &connection_id,
            connection_revision: 7,
            resource: "primary",
            consumer_name,
            query: &query,
            window: &window,
            process_incarnation_id: Uuid::new_v4(),
        };
        assert!(matches!(
            read_remote_calendar_view(&fixture, &fixture, read("assistant")).await,
            Err(AgentFailure::AccessReviewRequired)
        ));
        assert!(matches!(
            read_remote_calendar_view(&fixture, &fixture, read("calendar.expert")).await,
            Err(AgentFailure::AccessReviewRequired)
        ));
        assert_eq!(fixture.reads.load(Ordering::SeqCst), 0);
        fixture.reference.connection_revision = 8;
        assert!(matches!(
            read_remote_calendar_view(&fixture, &fixture, read("floe.builtin.schedule")).await,
            Err(AgentFailure::PolicyDenied)
        ));
        assert_eq!(fixture.reads.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn sibling_resources_complete_independent_reads_and_rechecks() {
        let person_id = PersonId::new();
        let connection_id = Uuid::new_v4().to_string();
        let now = Utc::now().timestamp_millis();
        let query = CalendarViewQuery::try_new(now - 60_000, now + 60_000, None, 1).unwrap();
        let fixture = CalendarFixture::new(person_id, &connection_id, &query)
            .with_sibling("secondary", &["floe.builtin.schedule"]);
        let window = RemoteCallWindow {
            deadline: Instant::now() + Duration::from_secs(5),
            cancellation: Cancellation::default(),
        };
        let person_text = person_id.to_string();
        let pairing = RemotePairingIdentity {
            person_id: &person_text,
            client_id: "client",
            device_id: "device",
        };
        let read = |resource| RemoteCalendarViewRead {
            person_id,
            pairing,
            connector_id: "calendar.google",
            connection_id: &connection_id,
            connection_revision: 7,
            resource,
            consumer_name: "floe.builtin.schedule",
            query: &query,
            window: &window,
            process_incarnation_id: Uuid::new_v4(),
        };
        let (primary_view, primary_dependency, _) =
            read_remote_calendar_view(&fixture, &fixture, read("primary"))
                .await
                .unwrap();
        let (secondary_view, secondary_dependency, _) =
            read_remote_calendar_view(&fixture, &fixture, read("secondary"))
                .await
                .unwrap();
        assert_eq!(primary_view.items[0].evidence_handle, "event:one");
        assert_eq!(secondary_view.items[0].evidence_handle, "event:secondary");
        assert_eq!(primary_dependency.resources()[0].as_str(), "primary");
        assert_eq!(secondary_dependency.resources()[0].as_str(), "secondary");
        assert_ne!(
            primary_dependency.grant_id(),
            secondary_dependency.grant_id()
        );
        assert_ne!(
            primary_dependency.consumer_policy(),
            secondary_dependency.consumer_policy()
        );
        assert_eq!(
            fixture.read_resources.lock().unwrap().as_slice(),
            &["primary".to_owned(), "secondary".to_owned()]
        );
        for dependency in [&primary_dependency, &secondary_dependency] {
            authorize_remote_dependency(
                &fixture,
                &fixture,
                person_id,
                pairing,
                dependency,
                &DependencyAuthorization {
                    deadline: window.deadline,
                    cancellation: window.cancellation.clone(),
                },
            )
            .await
            .unwrap();
        }
    }

    #[tokio::test]
    async fn duplicate_exact_resource_grants_conflict_before_provider_io() {
        let person_id = PersonId::new();
        let connection_id = Uuid::new_v4().to_string();
        let now = Utc::now().timestamp_millis();
        let query = CalendarViewQuery::try_new(now - 60_000, now + 60_000, None, 1).unwrap();
        let fixture = CalendarFixture::new(person_id, &connection_id, &query)
            .with_sibling("primary", &["floe.builtin.schedule"]);
        let window = RemoteCallWindow {
            deadline: Instant::now() + Duration::from_secs(5),
            cancellation: Cancellation::default(),
        };
        let person_text = person_id.to_string();
        let result = read_remote_calendar_view(
            &fixture,
            &fixture,
            RemoteCalendarViewRead {
                person_id,
                pairing: RemotePairingIdentity {
                    person_id: &person_text,
                    client_id: "client",
                    device_id: "device",
                },
                connector_id: "calendar.google",
                connection_id: &connection_id,
                connection_revision: 7,
                resource: "primary",
                consumer_name: "floe.builtin.schedule",
                query: &query,
                window: &window,
                process_incarnation_id: Uuid::new_v4(),
            },
        )
        .await;
        assert_eq!(result, Err(AgentFailure::Conflict));
        assert_eq!(fixture.reads.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn two_canonical_consumers_read_under_one_remote_grant() {
        let person_id = PersonId::new();
        let connection_id = Uuid::new_v4().to_string();
        let now = Utc::now().timestamp_millis();
        let query = CalendarViewQuery::try_new(now - 60_000, now + 60_000, None, 1).unwrap();
        let fixture = CalendarFixture::new_with_consumers(
            person_id,
            &connection_id,
            &query,
            &["floe.builtin.schedule", "floe.builtin.focus-attention"],
        );
        let window = RemoteCallWindow {
            deadline: Instant::now() + Duration::from_secs(5),
            cancellation: Cancellation::default(),
        };
        let person_text = person_id.to_string();
        let pairing = RemotePairingIdentity {
            person_id: &person_text,
            client_id: "client",
            device_id: "device",
        };
        let read = |consumer_name| RemoteCalendarViewRead {
            person_id,
            pairing,
            connector_id: "calendar.google",
            connection_id: &connection_id,
            connection_revision: 7,
            resource: "primary",
            consumer_name,
            query: &query,
            window: &window,
            process_incarnation_id: Uuid::new_v4(),
        };
        let (_, schedule_dependency, _) =
            read_remote_calendar_view(&fixture, &fixture, read("floe.builtin.schedule"))
                .await
                .unwrap();
        let (_, focus_dependency, _) =
            read_remote_calendar_view(&fixture, &fixture, read("floe.builtin.focus-attention"))
                .await
                .unwrap();
        assert_eq!(
            schedule_dependency.consumer().identifier(),
            "floe.builtin.schedule"
        );
        assert_eq!(
            focus_dependency.consumer().identifier(),
            "floe.builtin.focus-attention"
        );
        assert_eq!(schedule_dependency.grant_id(), focus_dependency.grant_id());
        assert_eq!(fixture.reads.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn calendar_read_rechecks_source_after_provider_io() {
        let person_id = PersonId::new();
        let connection_id = Uuid::new_v4().to_string();
        let now = Utc::now().timestamp_millis();
        let query = CalendarViewQuery::try_new(now - 60_000, now + 60_000, None, 1).unwrap();
        let mut fixture = CalendarFixture::new(person_id, &connection_id, &query);
        fixture.source_changes_after_read = true;
        let window = RemoteCallWindow {
            deadline: Instant::now() + Duration::from_secs(5),
            cancellation: Cancellation::default(),
        };
        let person_text = person_id.to_string();
        let result = read_remote_calendar_view(
            &fixture,
            &fixture,
            RemoteCalendarViewRead {
                person_id,
                pairing: RemotePairingIdentity {
                    person_id: &person_text,
                    client_id: "client",
                    device_id: "device",
                },
                connector_id: "calendar.google",
                connection_id: &connection_id,
                connection_revision: 7,
                resource: "primary",
                consumer_name: "floe.builtin.schedule",
                query: &query,
                window: &window,
                process_incarnation_id: Uuid::new_v4(),
            },
        )
        .await;
        assert!(matches!(result, Err(AgentFailure::StaleContext)));
        assert_eq!(fixture.reads.load(Ordering::SeqCst), 1);
    }
}

#[cfg(test)]
mod remote_view_tests {
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use floe_access::{
        ConsumerPolicyAuthority, DataAccessGrant, GrantAuthority, GrantDataCategory, GrantId,
        GrantOperation, GrantPurpose, GrantScope, GrantSourceBinding, RemoteGrantBinding,
        RemoteProducerIdentity, RemoteViewSourceReference, SignedSourcePreview,
    };
    use floe_context_contract::{
        CommunicationView, ConnectionId, ConnectorId, ExecutionOwnerId, GrantConsumer,
        ProcessingRestriction, ResourceHandle, SourceAuthority,
    };
    use floe_execution::Cancellation;
    use tokio::time::{Duration, Instant};

    use super::*;

    struct SourceFixture {
        grant: DataAccessGrant,
        policy: ConsumerPolicyAuthority,
        view: CommunicationView,
        expired_credential: bool,
    }

    struct ViewFixture {
        person_id: PersonId,
        sources: HashMap<String, SourceFixture>,
        reads: AtomicUsize,
    }

    impl ViewFixture {
        fn new(person_id: PersonId) -> Self {
            Self {
                person_id,
                sources: HashMap::new(),
                reads: AtomicUsize::new(0),
            }
        }

        fn add_source(&mut self, connection_id: &str, active: bool) {
            let authority = SourceAuthority::new();
            let source = GrantSourceBinding::try_new(
                self.person_id,
                ConnectionId::try_new(connection_id).unwrap(),
                ConnectorId::try_new("gmail").unwrap(),
                ExecutionOwnerId::try_new("server-owner").unwrap(),
                authority,
            )
            .unwrap();
            let resource = remote_view_resource(MAIL_VIEW, connection_id);
            let scope = GrantScope::try_new(
                vec![ResourceHandle::try_new(&resource).unwrap()],
                vec![GrantDataCategory::Derived],
                vec![GrantOperation::Read],
                vec![GrantPurpose::Assistant],
                vec![GrantConsumer::builtin("assistant").unwrap()],
                ProcessingRestriction::LocalOnly,
            )
            .unwrap();
            let mut grant =
                DataAccessGrant::new(GrantId::new(), Uuid::new_v4(), source.clone(), scope.clone())
                    .unwrap();
            if active {
                grant
                    .activate_review(grant.authority(), source, scope)
                    .unwrap();
            }
            let now = Utc::now().timestamp_millis();
            self.sources.insert(
                connection_id.to_owned(),
                SourceFixture {
                    grant,
                    policy: ConsumerPolicyAuthority::new(),
                    view: CommunicationView {
                        schema_version: floe_agent_contract::AGENT_VERSION,
                        view_id: MAIL_VIEW.into(),
                        source_handle: format!("mail:{connection_id}"),
                        observed_at_unix_ms: now - 1,
                        expires_at_unix_ms: now + 60_000,
                        coverage_complete: true,
                        next_cursor: None,
                        items: vec![],
                    },
                    expired_credential: false,
                },
            );
        }

        fn reference(&self, connection_id: &str) -> RemoteViewSourceReference {
            let grant = &self.sources[connection_id].grant;
            RemoteViewSourceReference {
                view_id: MAIL_VIEW.into(),
                person_id: self.person_id.to_string(),
                client_id: "client".into(),
                device_id: "device".into(),
                audience: "server-audience".into(),
                connector_id: "gmail".into(),
                connection_id: connection_id.into(),
                connection_revision: 7,
                execution_owner: "server-owner".into(),
                source_authority: grant.source().source_authority(),
                resource: remote_view_resource(MAIL_VIEW, connection_id),
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
                assert_eq!(read.view_id, MAIL_VIEW);
                let connection = read.resource.strip_prefix("mail.communication:").unwrap();
                let source = &self.sources[connection];
                if source.expired_credential {
                    return Err(AgentFailure::CredentialExpired);
                }
                serde_json::to_value(&source.view).map_err(|_| AgentFailure::InvalidInput)
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
            let reference = self.reference(query.connection_id);
            Box::pin(async move { Ok(reference) })
        }

        fn grants<'a>(
            &'a self,
            _: usize,
        ) -> BoxFuture<'a, Result<Vec<DataAccessGrant>, AgentFailure>> {
            let grants = self.sources.values().map(|source| source.grant.clone()).collect();
            Box::pin(async move { Ok(grants) })
        }

        fn find_view_grant<'a>(
            &'a self,
            _: &'a str,
            _: &'a GrantSourceBinding,
            _: &'a str,
        ) -> BoxFuture<'a, Result<Option<DataAccessGrant>, AgentFailure>> {
            Box::pin(async { Err(AgentFailure::CapabilityUnavailable) })
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
            _: &'a str,
            connector: &'a str,
            connection_id: &'a str,
            source_authority: SourceAuthority,
        ) -> BoxFuture<'a, Result<RemoteGrantBinding, AgentFailure>> {
            let found = self
                .sources
                .values()
                .map(|source| (source.grant.clone(), source.policy))
                .find(|(grant, _)| {
                    let source = grant.source();
                    source.connector().as_str() == connector
                        && source.connection_id().as_str() == connection_id
                        && source.source_authority() == source_authority
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
            _: SourceAuthority,
            _: &'a str,
        ) -> BoxFuture<'a, Result<RemoteGrantBinding, AgentFailure>> {
            Box::pin(async { Err(AgentFailure::CapabilityUnavailable) })
        }
    }

    async fn read_mail(
        fixture: &ViewFixture,
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
            MAIL_VIEW,
            "assistant",
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
        assert_eq!(
            blocker.reason(),
            SourceAccessRequirementKind::EnableObserve
        );
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
        fixture.sources.get_mut("b-connection").unwrap().expired_credential = true;
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
