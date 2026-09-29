//! Reading one of the Person's own device sources under the grant they left.
//!
//! Every such read is the same shape: find the one grant that admits it, ask
//! the device, check that the device subject and the grant are still the ones
//! it started under, validate the view, and record what the answer owes its
//! provenance to. Access decides what a grant admits; the driver asks the
//! device; this is the read itself, and the catalog of what there is to read.

use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use uuid::Uuid;

use floe_access::{
    ContextDependency, GrantConsumer, GrantDataCategory, GrantOperation, GrantPurpose,
    GrantSourceBinding, PersonalReadRequirement, ProcessingRestriction, ResourceHandle,
    SourceAuthority, active_read_grant, grant_unchanged, subject_unchanged,
};
use floe_agent_contract::{AgentFailure, PersonId};
use floe_connections::SourceConnection;
use floe_context_contract::connection_view_resource;
use floe_execution::Cancellation;
use tokio::time::Instant;

use crate::application::personal_lineage::{
    FeasibilityQueryLineage, attention_query_fingerprint, feasibility_query_fingerprint,
    people_query_fingerprint, wellbeing_query_fingerprint,
};
use crate::ports::personal_source::{
    AttentionAcquisition, AttentionAcquisitionMode, PersonalAcquisition, PersonalConnectionReader,
    PersonalDomain, PersonalGrantRecords, PersonalSourceDriver,
};
use crate::{
    AttentionView, FeasibilityView, PeopleView, WellbeingView, validate_feasibility_view,
    validate_people_view, validate_wellbeing_view,
};

/// The names a grant binds a personal source under belong to Access; a read
/// only quotes them.
pub use floe_access::{
    ATTENTION_CONNECTION, ATTENTION_CONNECTOR, ATTENTION_RESOURCE, FEASIBILITY_CONNECTION,
    FEASIBILITY_CONNECTOR, FEASIBILITY_RESOURCE, PEOPLE_RESOURCE, WELLBEING_CONNECTION,
    WELLBEING_CONNECTOR, WELLBEING_RESOURCE, apple_execution_owner, attention_execution_owner,
    attention_source, contacts_connection, contacts_execution_owner, feasibility_source,
    wellbeing_source,
};

/// A read that has not run out of time and has not been cancelled.
fn within_read_window(deadline: Instant, cancellation: &Cancellation) -> Result<(), AgentFailure> {
    if cancellation.is_cancelled() {
        return Err(AgentFailure::Cancelled);
    }
    if Instant::now() >= deadline {
        return Err(AgentFailure::DeadlineExceeded);
    }
    Ok(())
}

fn freshness(observed: i64, expires: i64) -> Result<(DateTime<Utc>, DateTime<Utc>), AgentFailure> {
    Ok((
        DateTime::<Utc>::from_timestamp_millis(observed).ok_or(AgentFailure::StaleContext)?,
        DateTime::<Utc>::from_timestamp_millis(expires).ok_or(AgentFailure::StaleContext)?,
    ))
}

/// One read of a Person's own device source, and what distinguishes it.
struct PersonalRead<'a> {
    person_id: PersonId,
    device_id: &'a str,
    source: GrantSourceBinding,
    resource: &'static str,
    domain: PersonalDomain,
    consumer: GrantConsumer,
    reject_ambiguous: bool,
    selected_handles: Vec<String>,
    /// The device subject this read insists answered it: the one the caller was
    /// handed, or the one the grant was reviewed against.
    expected_subject: Option<String>,
    /// What the dependency this read produces is charged to.
    lease_invocation_id: Uuid,
    deadline: Instant,
}

/// What a completed read yields, before its own view is decoded.
struct CompletedRead {
    value: serde_json::Value,
    subject: String,
    observation_id: Uuid,
    process: Uuid,
    grant: floe_access::DataAccessGrant,
    source_authority: SourceAuthority,
}

fn read_requirement<'a>(read: &'a PersonalRead<'a>) -> PersonalReadRequirement<'a> {
    PersonalReadRequirement {
        source: &read.source,
        resource: read.resource,
        consumer: &read.consumer,
        reject_ambiguous: read.reject_ambiguous,
    }
}

/// The one grant this read runs under, chosen once.
///
/// Every later question — what the grant admits reading, which device subject
/// it was reviewed against, whether it is still the same grant afterwards — is
/// asked of this grant and no other.
async fn admit_personal_read(
    records: &impl PersonalGrantRecords,
    read: &PersonalRead<'_>,
) -> Result<floe_access::DataAccessGrant, AgentFailure> {
    active_read_grant(&records.grants().await?, &read_requirement(read))
}

async fn acquire_personal_source(
    records: &impl PersonalGrantRecords,
    driver: &impl PersonalSourceDriver,
    read: &PersonalRead<'_>,
    grant: &floe_access::DataAccessGrant,
    feasibility: Option<&floe_access::FeasibilityGrantQuery>,
    cancellation: &Cancellation,
) -> Result<CompletedRead, AgentFailure> {
    within_read_window(read.deadline, cancellation)?;
    let requirement = read_requirement(read);
    let source_authority = records.current_source_authority(grant.id()).await?;
    let subject = match &read.expected_subject {
        Some(subject) => subject.clone(),
        None => records.reviewed_subject(grant.id()).await?,
    };
    let acquired = driver
        .acquire(
            PersonalAcquisition {
                person_id: read.person_id,
                device_id: read.device_id,
                host_epoch: driver.personal_host_epoch(read.person_id)?,
                domain: read.domain,
                selected_handles: read.selected_handles.clone(),
                feasibility,
                expected_subject: subject.clone(),
                deadline: read.deadline,
            },
            cancellation.clone(),
        )
        .await?;
    within_read_window(read.deadline, cancellation)?;
    subject_unchanged(&subject, &acquired.subject_before, &acquired.subject_after)?;
    let value = acquired.view.ok_or(AgentFailure::CapabilityUnavailable)?;
    // The Person can revoke or re-review while the device is answering, so the
    // grant the read started under has to be the one it finished under.
    let current = active_read_grant(&records.grants().await?, &requirement)?;
    grant_unchanged(grant, &current)?;
    if records.current_source_authority(current.id()).await? != source_authority {
        return Err(AgentFailure::StaleContext);
    }
    Ok(CompletedRead {
        value,
        subject,
        observation_id: Uuid::new_v4(),
        process: driver.process_incarnation(),
        grant: current,
        source_authority,
    })
}

#[allow(clippy::too_many_arguments)]
fn personal_dependency(
    read: &PersonalRead<'_>,
    completed: &CompletedRead,
    query_fingerprint: Vec<u8>,
    observed_at_unix_ms: i64,
    expires_at_unix_ms: i64,
) -> Result<ContextDependency, AgentFailure> {
    let (observed, expires) = freshness(observed_at_unix_ms, expires_at_unix_ms)?;
    let source_resources = if read.domain == PersonalDomain::People {
        read.selected_handles
            .iter()
            .map(|handle| ResourceHandle::try_new(handle.clone()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| AgentFailure::InvalidInput)?
    } else {
        vec![ResourceHandle::try_new(read.resource).map_err(|_| AgentFailure::InvalidInput)?]
    };
    ContextDependency::try_new(
        read.person_id,
        completed.grant.id(),
        completed.grant.authority(),
        completed.grant.source().clone(),
        vec![ResourceHandle::try_new(read.resource).map_err(|_| AgentFailure::InvalidInput)?],
        completed.source_authority,
        source_resources,
        vec![GrantDataCategory::Derived],
        GrantOperation::Read,
        GrantPurpose::Assistant,
        read.consumer.clone(),
        ProcessingRestriction::LocalOnly,
        completed.observation_id,
        query_fingerprint,
        read.lease_invocation_id,
        completed.process,
        observed,
        expires,
    )
    .map_err(|_| AgentFailure::InvalidInput)
}

fn decode<View: DeserializeOwned>(value: serde_json::Value) -> Result<View, AgentFailure> {
    serde_json::from_value(value).map_err(|_| AgentFailure::CapabilityUnavailable)
}

/// Read whether the Person can still make the event they asked about.
pub async fn read_feasibility(
    records: &impl PersonalGrantRecords,
    driver: &impl PersonalSourceDriver,
    person_id: PersonId,
    device_id: &str,
    consumer_name: &str,
    lease_invocation_id: Uuid,
    deadline: Instant,
    cancellation: &Cancellation,
) -> Result<(FeasibilityView, ContextDependency), AgentFailure> {
    within_read_window(deadline, cancellation)?;
    let consumer = GrantConsumer::builtin(consumer_name).map_err(|_| AgentFailure::InvalidInput)?;
    let source = feasibility_source(person_id, device_id)?;
    let read = PersonalRead {
        person_id,
        device_id,
        source,
        resource: FEASIBILITY_RESOURCE,
        domain: PersonalDomain::Feasibility,
        consumer,
        reject_ambiguous: true,
        selected_handles: vec![],
        expected_subject: None,
        lease_invocation_id,
        deadline,
    };
    // The query is part of the grant, so it is read from the very grant this
    // read runs under. Asking for it separately would let the Person's grant
    // change in between and leave one grant's query authorized by another's.
    let grant = admit_personal_read(records, &read).await?;
    let query = records.feasibility_query(grant.id()).await?;
    let completed =
        acquire_personal_source(records, driver, &read, &grant, Some(&query), cancellation).await?;
    let view: FeasibilityView = decode(completed.value.clone())?;
    validate_feasibility_view(&view, Utc::now().timestamp_millis())
        .map_err(|_| AgentFailure::CapabilityUnavailable)?;
    let query_fingerprint = feasibility_query_fingerprint(
        &view,
        &FeasibilityQueryLineage {
            event_handle: &query.event_handle,
            evidence_handles: &query.evidence_handles,
            destination_latitude: query.destination_latitude,
            destination_longitude: query.destination_longitude,
            event_start_unix_ms: query.event_start_unix_ms,
            event_end_unix_ms: query.event_end_unix_ms,
            travel_mode: &query.travel_mode,
        },
        &completed.subject,
        completed.observation_id,
        completed.process,
    );
    let dependency = personal_dependency(
        &read,
        &completed,
        query_fingerprint.clone(),
        view.observed_at_unix_ms,
        view.expires_at_unix_ms,
    )?;
    driver.commit_personal_observation(
        person_id,
        device_id,
        completed.observation_id,
        completed.process,
        &completed.subject,
        view.observed_at_unix_ms,
        view.expires_at_unix_ms,
        query_fingerprint,
    )?;
    Ok((view, dependency))
}

/// Whether a stored personal dependency still describes a read this host made.
///
/// A dependency names the source it came from, the shape it was read under and
/// the observation behind it. All three have to still agree, or what it points
/// at is not what a later turn would be shown.
pub fn personal_dependency_holds(
    driver: &impl PersonalSourceDriver,
    person_id: PersonId,
    device_id: &str,
    dependency: &ContextDependency,
) -> Result<(), AgentFailure> {
    if dependency.person_id() != person_id || dependency.source().person_id() != person_id {
        return Err(AgentFailure::PolicyDenied);
    }
    if dependency.source().connector().as_str() == ATTENTION_CONNECTOR {
        if dependency.source().connection_id().as_str() != ATTENTION_CONNECTION
            || dependency.source().execution_owner().as_str()
                != attention_execution_owner(device_id)
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let (view, subject) = driver.trusted_attention_observation(
            person_id,
            device_id,
            dependency.observation_id(),
            dependency.process_incarnation_id(),
        )?;
        if dependency.observed_at().timestamp_millis() != view.observed_at_unix_ms
            || dependency.expires_at().timestamp_millis() != view.expires_at_unix_ms
            || dependency.query_fingerprint()
                != attention_query_fingerprint(
                    person_id,
                    device_id,
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
    if dependency.source().connector().as_str() == FEASIBILITY_CONNECTOR {
        if dependency.source().connection_id().as_str() != FEASIBILITY_CONNECTION
            || dependency.source().execution_owner().as_str() != apple_execution_owner(device_id)
            || dependency.consumer().identifier() != crate::ASSISTANT_CONSUMER
            || dependency.operation() != GrantOperation::Read
            || dependency.purpose() != GrantPurpose::Assistant
            || dependency.processing() != &ProcessingRestriction::LocalOnly
            || dependency.resources()
                != [ResourceHandle::try_new(FEASIBILITY_RESOURCE)
                    .map_err(|_| AgentFailure::PolicyDenied)?]
            || dependency.categories() != [GrantDataCategory::Derived]
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let observation = driver.trusted_personal_observation(
            person_id,
            device_id,
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
        return Ok(());
    }
    if dependency.source().connector().as_str() == WELLBEING_CONNECTOR {
        if dependency.source().connection_id().as_str() != WELLBEING_CONNECTION
            || dependency.source().execution_owner().as_str() != apple_execution_owner(device_id)
            || dependency.operation() != GrantOperation::Read
            || dependency.purpose() != GrantPurpose::Assistant
            || dependency.processing() != &ProcessingRestriction::LocalOnly
            || dependency.resources()
                != [ResourceHandle::try_new(WELLBEING_RESOURCE)
                    .map_err(|_| AgentFailure::PolicyDenied)?]
            || dependency.categories() != [GrantDataCategory::Derived]
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let observation = driver.trusted_personal_observation(
            person_id,
            device_id,
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
        return Ok(());
    }
    if !matches!(
        dependency.source().connector().as_str(),
        "contacts.apple" | "contacts.android"
    ) || dependency.source().connection_id().as_str()
        != contacts_connection(dependency.source().connector().as_str())
        || dependency.source().execution_owner().as_str()
            != contacts_execution_owner(dependency.source().connector().as_str(), device_id)
        || dependency.operation() != GrantOperation::Read
        || dependency.purpose() != GrantPurpose::Assistant
        || dependency.processing() != &ProcessingRestriction::LocalOnly
        || dependency.resources()
            != [ResourceHandle::try_new(PEOPLE_RESOURCE).map_err(|_| AgentFailure::PolicyDenied)?]
        || dependency.categories() != [GrantDataCategory::Derived]
    {
        return Err(AgentFailure::PolicyDenied);
    }
    let observation = driver.trusted_personal_observation(
        person_id,
        device_id,
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

/// Whether the Person's grant still admits a stored personal dependency.
///
/// Holding is not the same as being allowed: the grant behind the dependency
/// has to still be the grant it was recorded under, reviewed against the same
/// device subject, with the same consumer policy — and for attention, against
/// the device that would answer right now.
///
/// Route-neutral: model placement is never consulted here. Whether a
/// reauthorized dependency may reach a Device or External model target is
/// decided by Access model dispatch.
#[allow(clippy::too_many_arguments)]
async fn authorize_standing_dependency(
    connections: &impl PersonalConnectionReader,
    records: &impl PersonalGrantRecords,
    driver: &impl PersonalSourceDriver,
    person_id: PersonId,
    device_id: &str,
    dependency: &ContextDependency,
    deadline: Instant,
    cancellation: &Cancellation,
) -> Result<(), AgentFailure> {
    if dependency.person_id() != person_id
        || dependency.source().person_id() != person_id
        || dependency.operation() != GrantOperation::Read
        || dependency.purpose() != GrantPurpose::Assistant
        || dependency.processing() != &ProcessingRestriction::LocalOnly
        || dependency.categories() != [GrantDataCategory::Derived]
        || dependency.lease_invocation_id().is_nil()
    {
        return Err(AgentFailure::PolicyDenied);
    }
    let view = match dependency.source().connector().as_str() {
        "contacts.apple" | "contacts.android" => floe_context_contract::PEOPLE_VIEW_ID,
        ATTENTION_CONNECTOR => floe_context_contract::ATTENTION_VIEW_ID,
        WELLBEING_CONNECTOR => floe_context_contract::WELLBEING_VIEW_ID,
        _ => return Err(AgentFailure::PolicyDenied),
    };
    let logical = connection_view_resource(view, &dependency.source().connection_id())
        .map_err(|_| AgentFailure::PolicyDenied)?;
    if dependency.resources() != [logical.clone()] {
        return Err(AgentFailure::PolicyDenied);
    }
    let selected = floe_context_contract::SourceSelectionReference {
        connector_id: dependency.source().connector().clone(),
        connection_id: dependency.source().connection_id(),
        execution_owner_id: dependency.source().execution_owner().clone(),
        capability_id: view.to_owned(),
        resource: logical.clone(),
        contract_version: 1,
    };
    let connection = connections
        .load(person_id, &selected.connection_id)
        .await?
        .ok_or(AgentFailure::PolicyDenied)?;
    crate::validate_personal_source_selection(&selected, &connection, person_id, device_id)
        .map_err(|_| AgentFailure::PolicyDenied)?;
    let source = GrantSourceBinding::try_new(
        person_id,
        connection.connection_id().clone(),
        connection.connector_id().clone(),
        connection.execution_owner_id().clone(),
    )
    .map_err(|_| AgentFailure::PolicyDenied)?;
    if dependency.source() != &source
        || dependency.source_authority() != connection.source_authority()
        || dependency.source_resources()
            != connection
                .resources()
                .iter()
                .map(|resource| resource.handle().clone())
                .collect::<Vec<_>>()
    {
        return Err(AgentFailure::PolicyDenied);
    }
    let requirement = PersonalReadRequirement {
        source: &source,
        resource: logical.as_str(),
        consumer: dependency.consumer(),
        reject_ambiguous: view != floe_context_contract::ATTENTION_VIEW_ID,
    };
    let grant = active_read_grant(&records.grants().await?, &requirement)
        .map_err(|_| AgentFailure::PolicyDenied)?;
    if dependency.grant_id() != grant.id() || dependency.grant_authority() != grant.authority() {
        return Err(AgentFailure::PolicyDenied);
    }
    let subject = connection
        .native_subject_fingerprint()
        .ok_or(AgentFailure::PolicyDenied)?;
    if view == floe_context_contract::ATTENTION_VIEW_ID {
        let (current_view, observation_subject) = driver
            .trusted_attention_observation(
                person_id,
                device_id,
                dependency.observation_id(),
                dependency.process_incarnation_id(),
            )
            .map_err(|_| AgentFailure::PolicyDenied)?;
        if observation_subject != subject
            || dependency.observed_at().timestamp_millis() != current_view.observed_at_unix_ms
            || dependency.expires_at().timestamp_millis() != current_view.expires_at_unix_ms
            || dependency.query_fingerprint()
                != attention_query_fingerprint(
                    person_id,
                    device_id,
                    &current_view,
                    dependency.observation_id(),
                    dependency.process_incarnation_id(),
                )
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let probe = driver
            .acquire_attention(
                AttentionAcquisition {
                    person_id,
                    device_id,
                    host_epoch: driver.attention_host_epoch(person_id)?,
                    mode: AttentionAcquisitionMode::InspectSubject,
                    expected_subject: None,
                    deadline,
                },
                cancellation.clone(),
            )
            .await
            .map_err(|_| AgentFailure::PolicyDenied)?;
        if probe.subject_before != subject || probe.subject_after != subject {
            return Err(AgentFailure::PolicyDenied);
        }
        let current = connections
            .load(person_id, connection.connection_id())
            .await?
            .ok_or(AgentFailure::PolicyDenied)?;
        standing_source_unchanged(&connection, &current).map_err(|_| AgentFailure::PolicyDenied)?;
        let current_grant = active_read_grant(&records.grants().await?, &requirement)
            .map_err(|_| AgentFailure::PolicyDenied)?;
        grant_unchanged(&grant, &current_grant).map_err(|_| AgentFailure::PolicyDenied)?;
    } else {
        let observation = driver
            .trusted_personal_observation(
                person_id,
                device_id,
                dependency.observation_id(),
                dependency.process_incarnation_id(),
            )
            .map_err(|_| AgentFailure::PolicyDenied)?;
        if observation.native_subject_fingerprint != subject
            || dependency.observed_at().timestamp_millis() != observation.observed_at_unix_ms
            || dependency.expires_at().timestamp_millis() != observation.expires_at_unix_ms
            || dependency.query_fingerprint() != observation.query_fingerprint
        {
            return Err(AgentFailure::PolicyDenied);
        }
    }
    Ok(())
}

pub async fn authorize_personal_dependency(
    connections: &impl PersonalConnectionReader,
    records: &impl PersonalGrantRecords,
    driver: &impl PersonalSourceDriver,
    person_id: PersonId,
    device_id: &str,
    dependency: &ContextDependency,
    deadline: Instant,
    cancellation: &Cancellation,
) -> Result<(), AgentFailure> {
    if matches!(
        dependency.source().connector().as_str(),
        "contacts.apple" | "contacts.android" | ATTENTION_CONNECTOR | WELLBEING_CONNECTOR
    ) {
        return authorize_standing_dependency(
            connections,
            records,
            driver,
            person_id,
            device_id,
            dependency,
            deadline,
            cancellation,
        )
        .await;
    }
    if dependency.person_id() != person_id
        || dependency.source().person_id() != person_id
        || dependency.source().connector().as_str() != FEASIBILITY_CONNECTOR
        || dependency.source().connection_id().as_str() != FEASIBILITY_CONNECTION
        || dependency.source().execution_owner().as_str() != apple_execution_owner(device_id)
        || dependency.consumer().identifier() != crate::ASSISTANT_CONSUMER
        || dependency.operation() != GrantOperation::Read
        || dependency.purpose() != GrantPurpose::Assistant
        || dependency.processing() != &ProcessingRestriction::LocalOnly
        || dependency.resources()
            != [ResourceHandle::try_new(FEASIBILITY_RESOURCE)
                .map_err(|_| AgentFailure::PolicyDenied)?]
        || dependency.source_resources()
            != [ResourceHandle::try_new(FEASIBILITY_RESOURCE)
                .map_err(|_| AgentFailure::PolicyDenied)?]
        || dependency.categories() != [GrantDataCategory::Derived]
        || dependency.lease_invocation_id().is_nil()
    {
        return Err(AgentFailure::PolicyDenied);
    }
    if dependency.source_authority()
        != records
            .current_source_authority(dependency.grant_id())
            .await?
    {
        return Err(AgentFailure::PolicyDenied);
    }
    let grant = active_read_grant(
        &records.grants().await?,
        &PersonalReadRequirement {
            source: dependency.source(),
            resource: FEASIBILITY_RESOURCE,
            consumer: dependency.consumer(),
            reject_ambiguous: true,
        },
    )?;
    if dependency.grant_id() != grant.id()
        || dependency.grant_authority() != grant.authority()
        || dependency.source() != grant.source()
    {
        return Err(AgentFailure::PolicyDenied);
    }
    let observation = driver
        .trusted_personal_observation(
            person_id,
            device_id,
            dependency.observation_id(),
            dependency.process_incarnation_id(),
        )
        .map_err(|_| AgentFailure::PolicyDenied)?;
    if dependency.observed_at().timestamp_millis() != observation.observed_at_unix_ms
        || dependency.expires_at().timestamp_millis() != observation.expires_at_unix_ms
        || dependency.query_fingerprint() != observation.query_fingerprint
        || observation.native_subject_fingerprint != records.reviewed_subject(grant.id()).await?
    {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(())
}

/// The owner-known identity of one personal source: the source it reports
/// under, the resource it needs, the grant bindings that count as this
/// source, and — when the platform identity is fixed — the
/// connector/connection a missing grant still permits naming.
struct PersonalSourceIdentity {
    source_id: &'static str,
    resource: &'static str,
    expected: Vec<GrantSourceBinding>,
    known_identity: Option<(&'static str, &'static str)>,
}

fn feasibility_identity(
    person_id: PersonId,
    device_id: &str,
) -> Result<PersonalSourceIdentity, AgentFailure> {
    Ok(PersonalSourceIdentity {
        source_id: "floe.source.feasibility",
        resource: FEASIBILITY_RESOURCE,
        expected: vec![feasibility_source(person_id, device_id)?],
        known_identity: Some((FEASIBILITY_CONNECTOR, FEASIBILITY_CONNECTION)),
    })
}

fn personal_requirement(
    identity: &PersonalSourceIdentity,
    connector: Option<floe_context_contract::ConnectorId>,
    connection: Option<floe_context_contract::ConnectionId>,
    consumer: GrantConsumer,
    reason: floe_context_contract::SourceAccessRequirementKind,
    authority: Option<SourceAuthority>,
    observed: Option<floe_context_contract::ObservedGrant>,
) -> Result<floe_context_contract::SourceAccessRequirement, AgentFailure> {
    let resources = if connector.is_some() && connection.is_some() {
        vec![ResourceHandle::try_new(identity.resource).map_err(|_| AgentFailure::InvalidInput)?]
    } else {
        vec![]
    };
    let inline = !resources.is_empty()
        && matches!(
            reason,
            floe_context_contract::SourceAccessRequirementKind::EnableObserve
                | floe_context_contract::SourceAccessRequirementKind::ReviewChangedSource
        );
    floe_context_contract::SourceAccessRequirement::try_new(
        identity.source_id,
        connector,
        connection,
        GrantOperation::Read,
        consumer,
        GrantPurpose::Assistant,
        resources,
        None,
        reason,
        authority,
        observed,
        inline,
    )
    .map_err(|_| AgentFailure::InvalidInput)
}

/// The live grants binding this personal source, if the review can name
/// exactly one. Duplicates fail closed; absence is proven absence.
fn observe_personal_binding(
    grants: &[floe_access::DataAccessGrant],
    identity: &PersonalSourceIdentity,
) -> Result<Option<floe_access::DataAccessGrant>, AgentFailure> {
    let mut binding = grants.iter().filter(|grant| {
        grant.state() != floe_access::GrantState::Revoked
            && identity
                .expected
                .iter()
                .any(|expected| grant.source() == expected)
    });
    let Some(grant) = binding.next() else {
        return Ok(None);
    };
    if binding.next().is_some() {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(Some(grant.clone()))
}

fn observed_grant(
    grant: &floe_access::DataAccessGrant,
) -> Result<floe_context_contract::ObservedGrant, AgentFailure> {
    floe_context_contract::ObservedGrant::try_new(grant.id(), grant.authority())
        .map_err(|_| AgentFailure::InvalidInput)
}

/// A classified personal blocker: transiently unavailable, or blocked on a
/// concrete reviewable requirement.
enum PersonalBlock {
    Unavailable(floe_context_contract::SourceUnavailable),
    Blocked(floe_context_contract::SourceAccessBlockers),
}

impl PersonalBlock {
    fn blocked(
        requirement: floe_context_contract::SourceAccessRequirement,
    ) -> Result<Self, AgentFailure> {
        let blockers = floe_context_contract::SourceAccessBlockers::try_new(vec![requirement])
            .map_err(|_| AgentFailure::InvalidInput)?;
        Ok(Self::Blocked(blockers))
    }

    fn into_outcome<Value>(self) -> floe_context_contract::SourceReadOutcome<Value> {
        match self {
            Self::Unavailable(reason) => {
                floe_context_contract::SourceReadOutcome::Unavailable(reason)
            }
            Self::Blocked(blockers) => {
                floe_context_contract::SourceReadOutcome::NeedsUserAction(blockers)
            }
        }
    }
}

/// Classify a failed personal read against current grant facts.
///
/// Missing and paused grants are re-enableable, drift and vanished grants
/// need review, OS denial names system permission, and duplicate authority
/// fails closed. Budget, cancellation, deadline, invalid input and corrupt
/// storage stay hard failures with no card.
async fn classify_personal_blocker(
    records: &impl PersonalGrantRecords,
    identity: &PersonalSourceIdentity,
    consumer: GrantConsumer,
    error: AgentFailure,
) -> Result<PersonalBlock, AgentFailure> {
    use floe_context_contract::{SourceAccessRequirementKind, SourceUnavailable};

    match error {
        AgentFailure::CapabilityUnavailable => {
            return Ok(PersonalBlock::Unavailable(
                SourceUnavailable::TemporarilyUnavailable,
            ));
        }
        AgentFailure::AccessReviewRequired
        | AgentFailure::CredentialExpired
        | AgentFailure::CapabilityDenied
        | AgentFailure::PolicyDenied => {}
        _ => return Err(error),
    }
    let grants = records.grants().await?;
    let binding = observe_personal_binding(&grants, identity)?;
    let source_authority = match &binding {
        Some(grant) => Some(records.current_source_authority(grant.id()).await?),
        None => None,
    };
    let known = |identity: &PersonalSourceIdentity| {
        identity.known_identity.and_then(|(connector, connection)| {
            Some((
                floe_context_contract::ConnectorId::try_new(connector).ok()?,
                floe_context_contract::ConnectionId::try_new(connection).ok()?,
            ))
        })
    };
    let requirement = match error {
        AgentFailure::AccessReviewRequired => match &binding {
            None => {
                let (connector, connection) = known(identity).unzip();
                personal_requirement(
                    identity,
                    connector,
                    connection,
                    consumer,
                    SourceAccessRequirementKind::EnableObserve,
                    None,
                    None,
                )?
            }
            Some(grant) => {
                let reason = if grant.state() == floe_access::GrantState::Paused {
                    SourceAccessRequirementKind::EnableObserve
                } else {
                    SourceAccessRequirementKind::ReviewChangedSource
                };
                personal_requirement(
                    identity,
                    Some(grant.source().connector().clone()),
                    Some(grant.source().connection_id()),
                    consumer,
                    reason,
                    source_authority,
                    Some(observed_grant(grant)?),
                )?
            }
        },
        AgentFailure::CredentialExpired => {
            let (connector, connection, authority, observed) = match &binding {
                Some(grant) => (
                    Some(grant.source().connector().clone()),
                    Some(grant.source().connection_id()),
                    source_authority,
                    Some(observed_grant(grant)?),
                ),
                None => {
                    let (connector, connection) = known(identity).unzip();
                    (connector, connection, None, None)
                }
            };
            personal_requirement(
                identity,
                connector,
                connection,
                consumer,
                SourceAccessRequirementKind::Reconnect,
                authority,
                observed,
            )?
        }
        AgentFailure::CapabilityDenied => {
            // The driver is the OS boundary: a denial past admission means
            // the system refused, so the review names system permission
            // rather than the grant. A vanished grant is drift.
            let (connector, connection, authority, observed) = match &binding {
                Some(grant) => (
                    Some(grant.source().connector().clone()),
                    Some(grant.source().connection_id()),
                    source_authority,
                    Some(observed_grant(grant)?),
                ),
                None => {
                    let (connector, connection) = known(identity).unzip();
                    (connector, connection, None, None)
                }
            };
            let reason = if binding.is_some() {
                SourceAccessRequirementKind::RequestSystemPermission
            } else {
                SourceAccessRequirementKind::ReviewChangedSource
            };
            personal_requirement(
                identity, connector, connection, consumer, reason, authority, observed,
            )?
        }
        // Mid-read drift: the grant the read started under is no longer the
        // one current authority names.
        _ => {
            let (connector, connection, authority, observed) = match &binding {
                Some(grant) => (
                    Some(grant.source().connector().clone()),
                    Some(grant.source().connection_id()),
                    source_authority,
                    Some(observed_grant(grant)?),
                ),
                None => {
                    let (connector, connection) = known(identity).unzip();
                    (connector, connection, None, None)
                }
            };
            personal_requirement(
                identity,
                connector,
                connection,
                consumer,
                SourceAccessRequirementKind::ReviewChangedSource,
                authority,
                observed,
            )?
        }
    };
    PersonalBlock::blocked(requirement)
}

async fn selected_standing_grant(
    records: &impl PersonalGrantRecords,
    connection: &SourceConnection,
    source: &GrantSourceBinding,
    logical: &ResourceHandle,
    consumer: &GrantConsumer,
    source_id: &str,
    inline: bool,
) -> Result<floe_context_contract::SourceReadOutcome<floe_access::DataAccessGrant>, AgentFailure> {
    let grants = records.grants().await?;
    let matching = grants
        .iter()
        .filter(|grant| {
            grant.source() == source && grant.state() != floe_access::GrantState::Revoked
        })
        .collect::<Vec<_>>();
    if matching.len() > 1 {
        return Err(AgentFailure::PolicyDenied);
    }
    let requirement = PersonalReadRequirement {
        source,
        resource: logical.as_str(),
        consumer,
        reject_ambiguous: true,
    };
    match active_read_grant(&grants, &requirement) {
        Ok(grant) => Ok(floe_context_contract::SourceReadOutcome::Ready(grant)),
        Err(AgentFailure::AccessReviewRequired) => {
            let observed = matching
                .first()
                .map(|grant| observed_grant(grant))
                .transpose()?;
            let reason = match matching.first() {
                None => floe_context_contract::SourceAccessRequirementKind::EnableObserve,
                Some(grant) if grant.state() == floe_access::GrantState::Paused => {
                    floe_context_contract::SourceAccessRequirementKind::EnableObserve
                }
                Some(_) => floe_context_contract::SourceAccessRequirementKind::ReviewChangedSource,
            };
            let requirement = floe_context_contract::SourceAccessRequirement::try_new(
                source_id,
                Some(connection.connector_id().clone()),
                Some(connection.connection_id().clone()),
                GrantOperation::Read,
                consumer.clone(),
                GrantPurpose::Assistant,
                vec![logical.clone()],
                None,
                reason,
                Some(connection.source_authority()),
                observed,
                inline,
            )
            .map_err(|_| AgentFailure::InvalidInput)?;
            Ok(PersonalBlock::blocked(requirement)?.into_outcome())
        }
        Err(error) => Err(error),
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn read_selected_people_outcome(
    connections: &impl PersonalConnectionReader,
    records: &impl PersonalGrantRecords,
    driver: &impl PersonalSourceDriver,
    person_id: PersonId,
    device_id: &str,
    selected: &floe_context_contract::SourceSelectionReference,
    consumer_name: &str,
    deadline: Instant,
    cancellation: &Cancellation,
) -> Result<floe_context_contract::SourceReadOutcome<(PeopleView, ContextDependency)>, AgentFailure>
{
    within_read_window(deadline, cancellation)?;
    let connection = connections
        .load(person_id, &selected.connection_id)
        .await?
        .ok_or(AgentFailure::StaleContext)?;
    crate::validate_personal_source_selection(selected, &connection, person_id, device_id)?;
    if selected.capability_id != floe_context_contract::PEOPLE_VIEW_ID {
        return Err(AgentFailure::CapabilityDenied);
    }
    let source = GrantSourceBinding::try_new(
        person_id,
        connection.connection_id().clone(),
        connection.connector_id().clone(),
        connection.execution_owner_id().clone(),
    )
    .map_err(|_| AgentFailure::InvalidInput)?;
    let logical = connection_view_resource(
        floe_context_contract::PEOPLE_VIEW_ID,
        connection.connection_id(),
    )
    .map_err(|_| AgentFailure::InvalidInput)?;
    let consumer = GrantConsumer::builtin(consumer_name).map_err(|_| AgentFailure::InvalidInput)?;
    let grant = match selected_standing_grant(
        records,
        &connection,
        &source,
        &logical,
        &consumer,
        "floe.source.contacts",
        false,
    )
    .await?
    {
        floe_context_contract::SourceReadOutcome::Ready(grant) => grant,
        floe_context_contract::SourceReadOutcome::NeedsUserAction(blockers) => {
            return Ok(floe_context_contract::SourceReadOutcome::NeedsUserAction(
                blockers,
            ));
        }
        floe_context_contract::SourceReadOutcome::Unavailable(reason) => {
            return Ok(floe_context_contract::SourceReadOutcome::Unavailable(
                reason,
            ));
        }
    };
    let handles = connection
        .resources()
        .iter()
        .map(|resource| resource.handle().as_str().to_owned())
        .collect::<Vec<_>>();
    let subject = connection
        .native_subject_fingerprint()
        .ok_or(AgentFailure::StaleContext)?
        .to_owned();
    let acquired = driver
        .acquire(
            PersonalAcquisition {
                person_id,
                device_id,
                host_epoch: driver.personal_host_epoch(person_id)?,
                domain: PersonalDomain::People,
                selected_handles: handles.clone(),
                feasibility: None,
                expected_subject: subject.clone(),
                deadline,
            },
            cancellation.clone(),
        )
        .await?;
    within_read_window(deadline, cancellation)?;
    subject_unchanged(&subject, &acquired.subject_before, &acquired.subject_after)?;
    let current = connections
        .load(person_id, connection.connection_id())
        .await?
        .ok_or(AgentFailure::StaleContext)?;
    standing_source_unchanged(&connection, &current)?;
    let current_grant = active_read_grant(
        &records.grants().await?,
        &PersonalReadRequirement {
            source: &source,
            resource: logical.as_str(),
            consumer: &consumer,
            reject_ambiguous: true,
        },
    )?;
    grant_unchanged(&grant, &current_grant)?;
    let view: PeopleView = decode(acquired.view.ok_or(AgentFailure::CapabilityUnavailable)?)?;
    validate_people_view(&view, Utc::now().timestamp_millis())
        .map_err(|_| AgentFailure::CapabilityUnavailable)?;
    let observation_id = Uuid::new_v4();
    let process = driver.process_incarnation();
    let query_fingerprint =
        people_query_fingerprint(&view, &handles, &subject, observation_id, process);
    let (observed, expires) = freshness(view.observed_at_unix_ms, view.expires_at_unix_ms)?;
    let dependency = ContextDependency::try_new(
        person_id,
        current_grant.id(),
        current_grant.authority(),
        source,
        vec![logical],
        connection.source_authority(),
        connection
            .resources()
            .iter()
            .map(|resource| resource.handle().clone())
            .collect(),
        vec![GrantDataCategory::Derived],
        GrantOperation::Read,
        GrantPurpose::Assistant,
        consumer,
        ProcessingRestriction::LocalOnly,
        observation_id,
        query_fingerprint.clone(),
        Uuid::new_v4(),
        process,
        observed,
        expires,
    )
    .map_err(|_| AgentFailure::InvalidInput)?;
    driver.commit_personal_observation(
        person_id,
        device_id,
        observation_id,
        process,
        &subject,
        view.observed_at_unix_ms,
        view.expires_at_unix_ms,
        query_fingerprint,
    )?;
    Ok(floe_context_contract::SourceReadOutcome::Ready((
        view, dependency,
    )))
}

fn standing_source_unchanged(
    before: &SourceConnection,
    after: &SourceConnection,
) -> Result<(), AgentFailure> {
    if !after.is_serving()
        || before.person_id() != after.person_id()
        || before.connector_id() != after.connector_id()
        || before.connection_id() != after.connection_id()
        || before.execution_owner_id() != after.execution_owner_id()
        || before.resource_mode() != after.resource_mode()
        || before.source_authority() != after.source_authority()
        || before.native_subject_fingerprint() != after.native_subject_fingerprint()
        || before
            .resources()
            .iter()
            .map(|resource| resource.handle())
            .ne(after.resources().iter().map(|resource| resource.handle()))
    {
        return Err(AgentFailure::StaleContext);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn read_selected_wellbeing_outcome(
    connections: &impl PersonalConnectionReader,
    records: &impl PersonalGrantRecords,
    driver: &impl PersonalSourceDriver,
    person_id: PersonId,
    device_id: &str,
    selected: &floe_context_contract::SourceSelectionReference,
    consumer_name: &str,
    lease_invocation_id: Uuid,
    deadline: Instant,
    cancellation: &Cancellation,
) -> Result<
    floe_context_contract::SourceReadOutcome<(WellbeingView, ContextDependency)>,
    AgentFailure,
> {
    within_read_window(deadline, cancellation)?;
    let connection = connections
        .load(person_id, &selected.connection_id)
        .await?
        .ok_or(AgentFailure::StaleContext)?;
    crate::validate_personal_source_selection(selected, &connection, person_id, device_id)?;
    if selected.capability_id != floe_context_contract::WELLBEING_VIEW_ID {
        return Err(AgentFailure::CapabilityDenied);
    }
    let source = GrantSourceBinding::try_new(
        person_id,
        connection.connection_id().clone(),
        connection.connector_id().clone(),
        connection.execution_owner_id().clone(),
    )
    .map_err(|_| AgentFailure::InvalidInput)?;
    let logical = connection_view_resource(
        floe_context_contract::WELLBEING_VIEW_ID,
        connection.connection_id(),
    )
    .map_err(|_| AgentFailure::InvalidInput)?;
    let consumer = GrantConsumer::builtin(consumer_name).map_err(|_| AgentFailure::InvalidInput)?;
    let requirement = PersonalReadRequirement {
        source: &source,
        resource: logical.as_str(),
        consumer: &consumer,
        reject_ambiguous: true,
    };
    let grant = match selected_standing_grant(
        records,
        &connection,
        &source,
        &logical,
        &consumer,
        "floe.source.wellbeing",
        true,
    )
    .await?
    {
        floe_context_contract::SourceReadOutcome::Ready(grant) => grant,
        floe_context_contract::SourceReadOutcome::NeedsUserAction(blockers) => {
            return Ok(floe_context_contract::SourceReadOutcome::NeedsUserAction(
                blockers,
            ));
        }
        floe_context_contract::SourceReadOutcome::Unavailable(reason) => {
            return Ok(floe_context_contract::SourceReadOutcome::Unavailable(
                reason,
            ));
        }
    };
    let subject = connection
        .native_subject_fingerprint()
        .ok_or(AgentFailure::StaleContext)?
        .to_owned();
    let acquired = driver
        .acquire(
            PersonalAcquisition {
                person_id,
                device_id,
                host_epoch: driver.personal_host_epoch(person_id)?,
                domain: PersonalDomain::Wellbeing,
                selected_handles: Vec::new(),
                feasibility: None,
                expected_subject: subject.clone(),
                deadline,
            },
            cancellation.clone(),
        )
        .await?;
    within_read_window(deadline, cancellation)?;
    subject_unchanged(&subject, &acquired.subject_before, &acquired.subject_after)?;
    let current = connections
        .load(person_id, connection.connection_id())
        .await?
        .ok_or(AgentFailure::StaleContext)?;
    standing_source_unchanged(&connection, &current)?;
    let current_grant = active_read_grant(&records.grants().await?, &requirement)?;
    grant_unchanged(&grant, &current_grant)?;
    let view: WellbeingView = decode(acquired.view.ok_or(AgentFailure::CapabilityUnavailable)?)?;
    validate_wellbeing_view(&view, Utc::now().timestamp_millis())
        .map_err(|_| AgentFailure::CapabilityUnavailable)?;
    let observation_id = Uuid::new_v4();
    let process = driver.process_incarnation();
    let query_fingerprint = wellbeing_query_fingerprint(&view, &subject, observation_id, process);
    let (observed, expires) = freshness(view.observed_at_unix_ms, view.expires_at_unix_ms)?;
    let dependency = ContextDependency::try_new(
        person_id,
        current_grant.id(),
        current_grant.authority(),
        source,
        vec![logical],
        connection.source_authority(),
        connection
            .resources()
            .iter()
            .map(|resource| resource.handle().clone())
            .collect(),
        vec![GrantDataCategory::Derived],
        GrantOperation::Read,
        GrantPurpose::Assistant,
        consumer,
        ProcessingRestriction::LocalOnly,
        observation_id,
        query_fingerprint.clone(),
        lease_invocation_id,
        process,
        observed,
        expires,
    )
    .map_err(|_| AgentFailure::InvalidInput)?;
    driver.commit_personal_observation(
        person_id,
        device_id,
        observation_id,
        process,
        &subject,
        view.observed_at_unix_ms,
        view.expires_at_unix_ms,
        query_fingerprint,
    )?;
    Ok(floe_context_contract::SourceReadOutcome::Ready((
        view, dependency,
    )))
}

#[allow(clippy::too_many_arguments)]
pub async fn admit_selected_attention_outcome(
    connections: &impl PersonalConnectionReader,
    records: &impl PersonalGrantRecords,
    driver: &impl PersonalSourceDriver,
    person_id: PersonId,
    device_id: &str,
    selected: &floe_context_contract::SourceSelectionReference,
    consumer: GrantConsumer,
    lease_invocation_id: Uuid,
    deadline: Instant,
    cancellation: &Cancellation,
) -> Result<
    floe_context_contract::SourceReadOutcome<(AttentionView, ContextDependency)>,
    AgentFailure,
> {
    within_read_window(deadline, cancellation)?;
    let connection = connections
        .load(person_id, &selected.connection_id)
        .await?
        .ok_or(AgentFailure::StaleContext)?;
    crate::validate_personal_source_selection(selected, &connection, person_id, device_id)?;
    if selected.capability_id != floe_context_contract::ATTENTION_VIEW_ID {
        return Err(AgentFailure::CapabilityDenied);
    }
    let source = GrantSourceBinding::try_new(
        person_id,
        connection.connection_id().clone(),
        connection.connector_id().clone(),
        connection.execution_owner_id().clone(),
    )
    .map_err(|_| AgentFailure::InvalidInput)?;
    let logical = connection_view_resource(
        floe_context_contract::ATTENTION_VIEW_ID,
        connection.connection_id(),
    )
    .map_err(|_| AgentFailure::InvalidInput)?;
    let requirement = PersonalReadRequirement {
        source: &source,
        resource: logical.as_str(),
        consumer: &consumer,
        reject_ambiguous: false,
    };
    let grant = match selected_standing_grant(
        records,
        &connection,
        &source,
        &logical,
        &consumer,
        "floe.source.attention",
        true,
    )
    .await?
    {
        floe_context_contract::SourceReadOutcome::Ready(grant) => grant,
        floe_context_contract::SourceReadOutcome::NeedsUserAction(blockers) => {
            return Ok(floe_context_contract::SourceReadOutcome::NeedsUserAction(
                blockers,
            ));
        }
        floe_context_contract::SourceReadOutcome::Unavailable(reason) => {
            return Ok(floe_context_contract::SourceReadOutcome::Unavailable(
                reason,
            ));
        }
    };
    let subject = connection
        .native_subject_fingerprint()
        .ok_or(AgentFailure::StaleContext)?
        .to_owned();
    let host_epoch = driver.attention_host_epoch(person_id)?;
    let acquired = driver
        .acquire_attention(
            AttentionAcquisition {
                person_id,
                device_id,
                host_epoch: host_epoch.clone(),
                mode: AttentionAcquisitionMode::ReadProjection,
                expected_subject: Some(subject.clone()),
                deadline,
            },
            cancellation.clone(),
        )
        .await?;
    within_read_window(deadline, cancellation)?;
    subject_unchanged(&subject, &acquired.subject_before, &acquired.subject_after)?;
    let view: AttentionView = decode(acquired.view.ok_or(AgentFailure::CapabilityUnavailable)?)?;
    if view.state == crate::AttentionState::Unknown || view.evidence_handles.is_empty() {
        return Ok(floe_context_contract::SourceReadOutcome::Unavailable(
            floe_context_contract::SourceUnavailable::TemporarilyUnavailable,
        ));
    }
    let (observed, expires) = freshness(view.observed_at_unix_ms, view.expires_at_unix_ms)?;
    let current = connections
        .load(person_id, connection.connection_id())
        .await?
        .ok_or(AgentFailure::StaleContext)?;
    standing_source_unchanged(&connection, &current)?;
    let current_grant = active_read_grant(&records.grants().await?, &requirement)?;
    grant_unchanged(&grant, &current_grant)?;
    let (observation_id, process) =
        driver.commit_attention_projection(person_id, &host_epoch, device_id, &view, &subject)?;
    let dependency = ContextDependency::try_new(
        person_id,
        current_grant.id(),
        current_grant.authority(),
        source,
        vec![logical],
        connection.source_authority(),
        connection
            .resources()
            .iter()
            .map(|resource| resource.handle().clone())
            .collect(),
        vec![GrantDataCategory::Derived],
        GrantOperation::Read,
        GrantPurpose::Assistant,
        consumer,
        ProcessingRestriction::LocalOnly,
        observation_id,
        attention_query_fingerprint(person_id, device_id, &view, observation_id, process),
        lease_invocation_id,
        process,
        observed,
        expires,
    )
    .map_err(|_| AgentFailure::InvalidInput)?;
    within_read_window(deadline, cancellation)?;
    Ok(floe_context_contract::SourceReadOutcome::Ready((
        view, dependency,
    )))
}

/// Read the feasibility view, preserving a recoverable blocker as a typed
/// outcome instead of raising it. Only hard failures raise.
#[allow(clippy::too_many_arguments)]
pub async fn read_feasibility_outcome(
    records: &impl PersonalGrantRecords,
    driver: &impl PersonalSourceDriver,
    person_id: PersonId,
    device_id: &str,
    consumer_name: &str,
    consumer: GrantConsumer,
    lease_invocation_id: Uuid,
    deadline: Instant,
    cancellation: &Cancellation,
) -> Result<
    floe_context_contract::SourceReadOutcome<(FeasibilityView, ContextDependency)>,
    AgentFailure,
> {
    let identity = feasibility_identity(person_id, device_id)?;
    match read_feasibility(
        records,
        driver,
        person_id,
        device_id,
        consumer_name,
        lease_invocation_id,
        deadline,
        cancellation,
    )
    .await
    {
        Ok(read) => Ok(floe_context_contract::SourceReadOutcome::Ready(read)),
        Err(error) => Ok(
            classify_personal_blocker(records, &identity, consumer, error)
                .await?
                .into_outcome(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use floe_access::{
        DataAccessGrant, FeasibilityGrantQuery, GrantDataCategory, GrantId, GrantScope,
    };
    use floe_agent_contract::BoxFuture;

    use super::*;
    use crate::ports::personal_source::{AcquiredSource, AttentionAcquisition};

    const SUBJECT_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const SUBJECT_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    struct FixtureConnections(SourceConnection);

    impl PersonalConnectionReader for FixtureConnections {
        fn load<'a>(
            &'a self,
            person_id: PersonId,
            connection_id: &'a floe_context_contract::ConnectionId,
        ) -> BoxFuture<'a, Result<Option<SourceConnection>, AgentFailure>> {
            Box::pin(async move {
                Ok(
                    (self.0.person_id() == person_id && self.0.connection_id() == connection_id)
                        .then(|| self.0.clone()),
                )
            })
        }
    }

    fn scope() -> GrantScope {
        GrantScope::try_new(
            vec![ResourceHandle::try_new(FEASIBILITY_RESOURCE).unwrap()],
            vec![GrantDataCategory::Derived],
            vec![GrantOperation::Read],
            vec![GrantPurpose::Assistant],
            vec![GrantConsumer::builtin("assistant").unwrap()],
            ProcessingRestriction::LocalOnly,
        )
        .unwrap()
    }

    fn active_grant(person_id: PersonId, device_id: &str) -> DataAccessGrant {
        let source = feasibility_source(person_id, device_id).unwrap();
        let mut grant =
            DataAccessGrant::new(GrantId::new(), Uuid::new_v4(), source.clone(), scope()).unwrap();
        grant.activate_review(grant.authority(), scope()).unwrap();
        grant
    }

    fn query(event_handle: &str) -> FeasibilityGrantQuery {
        FeasibilityGrantQuery {
            event_handle: event_handle.into(),
            evidence_handles: vec!["calendar:one".into()],
            destination_latitude: 37.5,
            destination_longitude: 127.0,
            event_start_unix_ms: 1_000,
            event_end_unix_ms: 2_000,
            travel_mode: "transit".into(),
        }
    }

    /// Records whose grant is replaced between the first read and the next,
    /// exactly as a Person re-reviewing mid-read would do.
    struct SwappingRecords {
        grants: Vec<DataAccessGrant>,
        reads: Mutex<usize>,
        queries: Vec<FeasibilityGrantQuery>,
        subjects: Vec<String>,
    }

    impl PersonalGrantRecords for SwappingRecords {
        fn grants<'a>(&'a self) -> BoxFuture<'a, Result<Vec<DataAccessGrant>, AgentFailure>> {
            let index = {
                let mut reads = self.reads.lock().unwrap();
                let index = (*reads).min(self.grants.len() - 1);
                *reads += 1;
                index
            };
            let grant = self.grants[index].clone();
            Box::pin(async move { Ok(vec![grant]) })
        }

        fn current_source_authority<'a>(
            &'a self,
            _: GrantId,
        ) -> BoxFuture<'a, Result<SourceAuthority, AgentFailure>> {
            Box::pin(async {
                Ok(SourceAuthority::from_parts(
                    Uuid::from_u128(1),
                    std::num::NonZeroU64::new(1).unwrap(),
                )
                .unwrap())
            })
        }

        fn reviewed_subject<'a>(
            &'a self,
            grant: GrantId,
        ) -> BoxFuture<'a, Result<String, AgentFailure>> {
            let subject = self
                .grants
                .iter()
                .position(|held| held.id() == grant)
                .map(|index| self.subjects[index].clone());
            Box::pin(async move { subject.ok_or(AgentFailure::NotFound) })
        }

        fn feasibility_query<'a>(
            &'a self,
            grant: GrantId,
        ) -> BoxFuture<'a, Result<FeasibilityGrantQuery, AgentFailure>> {
            let query = self
                .grants
                .iter()
                .position(|held| held.id() == grant)
                .map(|index| self.queries[index].clone());
            Box::pin(async move { query.ok_or(AgentFailure::NotFound) })
        }

        fn selected_handles<'a>(
            &'a self,
            _: GrantId,
        ) -> BoxFuture<'a, Result<Vec<String>, AgentFailure>> {
            Box::pin(async { Ok(vec![]) })
        }
    }

    /// A device that answers whatever subject it was asked for.
    struct EchoingDriver;

    impl PersonalSourceDriver for EchoingDriver {
        fn personal_host_epoch(&self, _: PersonId) -> Result<String, AgentFailure> {
            Ok("host".into())
        }

        fn attention_host_epoch(&self, _: PersonId) -> Result<String, AgentFailure> {
            Ok("host".into())
        }

        fn process_incarnation(&self) -> Uuid {
            Uuid::new_v4()
        }

        fn acquire<'a>(
            &'a self,
            request: PersonalAcquisition<'a>,
            _: Cancellation,
        ) -> BoxFuture<'a, Result<AcquiredSource, AgentFailure>> {
            let subject = request.expected_subject.clone();
            Box::pin(async move {
                Ok(AcquiredSource {
                    view: Some(serde_json::json!({})),
                    subject_before: subject.clone(),
                    subject_after: subject,
                })
            })
        }

        fn acquire_attention<'a>(
            &'a self,
            _: AttentionAcquisition<'a>,
            _: Cancellation,
        ) -> BoxFuture<'a, Result<AcquiredSource, AgentFailure>> {
            Box::pin(async { Err(AgentFailure::CapabilityUnavailable) })
        }

        #[allow(clippy::too_many_arguments)]
        fn commit_personal_observation(
            &self,
            _: PersonId,
            _: &str,
            _: Uuid,
            _: Uuid,
            _: &str,
            _: i64,
            _: i64,
            _: Vec<u8>,
        ) -> Result<(), AgentFailure> {
            Ok(())
        }

        fn trusted_personal_observation(
            &self,
            _: PersonId,
            _: &str,
            _: Uuid,
            _: Uuid,
        ) -> Result<crate::TrustedObservation, AgentFailure> {
            Err(AgentFailure::NotFound)
        }

        fn trusted_attention_observation(
            &self,
            _: PersonId,
            _: &str,
            _: Uuid,
            _: Uuid,
        ) -> Result<(AttentionView, String), AgentFailure> {
            Err(AgentFailure::NotFound)
        }

        fn commit_attention_projection(
            &self,
            _: PersonId,
            _: &str,
            _: &str,
            _: &AttentionView,
            _: &str,
        ) -> Result<(Uuid, Uuid), AgentFailure> {
            Err(AgentFailure::CapabilityUnavailable)
        }
    }

    #[derive(Default)]
    struct CapturingPeopleDriver {
        selected: Mutex<Vec<String>>,
        subject_after: Option<String>,
    }

    impl PersonalSourceDriver for CapturingPeopleDriver {
        fn personal_host_epoch(&self, _: PersonId) -> Result<String, AgentFailure> {
            Ok("host".into())
        }

        fn attention_host_epoch(&self, _: PersonId) -> Result<String, AgentFailure> {
            Err(AgentFailure::CapabilityUnavailable)
        }

        fn process_incarnation(&self) -> Uuid {
            Uuid::new_v4()
        }

        fn acquire<'a>(
            &'a self,
            request: PersonalAcquisition<'a>,
            _: Cancellation,
        ) -> BoxFuture<'a, Result<AcquiredSource, AgentFailure>> {
            *self.selected.lock().unwrap() = request.selected_handles;
            let now = Utc::now().timestamp_millis();
            let subject_after = self
                .subject_after
                .clone()
                .unwrap_or_else(|| request.expected_subject.clone());
            Box::pin(async move {
                Ok(AcquiredSource {
                    view: Some(serde_json::json!({
                        "schema_version": floe_kernel::AGENT_VERSION,
                        "view_id": "people.identity",
                        "source_handle": "contacts.apple",
                        "observed_at_unix_ms": now,
                        "expires_at_unix_ms": now + 60_000,
                        "coverage_complete": true,
                        "identities": []
                    })),
                    subject_before: request.expected_subject.clone(),
                    subject_after,
                })
            })
        }

        fn acquire_attention<'a>(
            &'a self,
            _: AttentionAcquisition<'a>,
            _: Cancellation,
        ) -> BoxFuture<'a, Result<AcquiredSource, AgentFailure>> {
            Box::pin(async { Err(AgentFailure::CapabilityUnavailable) })
        }

        fn commit_personal_observation(
            &self,
            _: PersonId,
            _: &str,
            _: Uuid,
            _: Uuid,
            _: &str,
            _: i64,
            _: i64,
            _: Vec<u8>,
        ) -> Result<(), AgentFailure> {
            Ok(())
        }

        fn trusted_personal_observation(
            &self,
            _: PersonId,
            _: &str,
            _: Uuid,
            _: Uuid,
        ) -> Result<crate::TrustedObservation, AgentFailure> {
            Err(AgentFailure::NotFound)
        }

        fn trusted_attention_observation(
            &self,
            _: PersonId,
            _: &str,
            _: Uuid,
            _: Uuid,
        ) -> Result<(AttentionView, String), AgentFailure> {
            Err(AgentFailure::NotFound)
        }

        fn commit_attention_projection(
            &self,
            _: PersonId,
            _: &str,
            _: &str,
            _: &AttentionView,
            _: &str,
        ) -> Result<(Uuid, Uuid), AgentFailure> {
            Err(AgentFailure::CapabilityUnavailable)
        }
    }

    #[tokio::test]
    async fn a_feasibility_read_never_carries_one_grants_query_under_another() {
        let person_id = PersonId::new();
        let first = active_grant(person_id, "device");
        let second = active_grant(person_id, "device");
        assert_ne!(first.id(), second.id());
        let records = SwappingRecords {
            grants: vec![first, second],
            reads: Mutex::new(0),
            queries: vec![query("event:reviewed"), query("event:other")],
            subjects: vec![SUBJECT_A.into(), SUBJECT_B.into()],
        };
        assert_eq!(
            read_feasibility(
                &records,
                &EchoingDriver,
                person_id,
                "device",
                "assistant",
                Uuid::new_v4(),
                Instant::now() + std::time::Duration::from_secs(5),
                &Cancellation::default(),
            )
            .await
            .unwrap_err(),
            AgentFailure::PolicyDenied
        );
    }

    #[tokio::test]
    async fn selected_contacts_read_uses_current_connection_handles_and_logical_grant() {
        let person_id = PersonId::new();
        let mut connection = SourceConnection::establish_reviewed_native(
            person_id,
            floe_context_contract::ConnectorId::try_new("contacts.apple").unwrap(),
            floe_context_contract::ConnectionId::try_new("contacts.apple.local").unwrap(),
            floe_context_contract::ExecutionOwnerId::try_new("apple:device").unwrap(),
            floe_connections::ResourceMode::Selected,
            vec![
                floe_connections::ConnectionResource::new(
                    ResourceHandle::try_new("person.identity:a").unwrap(),
                    "A".into(),
                )
                .unwrap(),
            ],
            SUBJECT_A.into(),
        )
        .unwrap();
        let original_authority = connection.source_authority();
        connection
            .configure_reviewed_native(
                1,
                floe_connections::ResourceMode::Selected,
                vec![
                    floe_connections::ConnectionResource::new(
                        ResourceHandle::try_new("person.identity:a").unwrap(),
                        "A".into(),
                    )
                    .unwrap(),
                    floe_connections::ConnectionResource::new(
                        ResourceHandle::try_new("person.identity:b").unwrap(),
                        "B".into(),
                    )
                    .unwrap(),
                ],
                SUBJECT_B.into(),
            )
            .unwrap();
        let source = GrantSourceBinding::try_new(
            person_id,
            connection.connection_id().clone(),
            connection.connector_id().clone(),
            connection.execution_owner_id().clone(),
        )
        .unwrap();
        let logical = connection_view_resource(
            floe_context_contract::PEOPLE_VIEW_ID,
            connection.connection_id(),
        )
        .unwrap();
        let scope = GrantScope::try_new(
            vec![logical.clone()],
            vec![GrantDataCategory::Derived],
            vec![GrantOperation::Read],
            vec![GrantPurpose::Assistant],
            vec![GrantConsumer::builtin("floe.builtin.relationships").unwrap()],
            ProcessingRestriction::LocalOnly,
        )
        .unwrap();
        let mut grant =
            DataAccessGrant::new(GrantId::new(), Uuid::new_v4(), source, scope.clone()).unwrap();
        grant.activate_review(grant.authority(), scope).unwrap();
        let records = SwappingRecords {
            grants: vec![grant.clone()],
            reads: Mutex::new(0),
            queries: vec![],
            subjects: vec![],
        };
        let selected = crate::discover_source_candidates(crate::SourceCandidateRequest {
            person_id,
            device_id: "device",
            capability: floe_context_contract::PEOPLE_VIEW_ID,
            contract_version: 1,
            remote_connections: &[],
            remote_execution_owner: None,
            source_connections: std::slice::from_ref(&connection),
        })
        .unwrap()
        .remove(0)
        .reference;
        let driver = CapturingPeopleDriver::default();
        let outcome = read_selected_people_outcome(
            &FixtureConnections(connection.clone()),
            &records,
            &driver,
            person_id,
            "device",
            &selected,
            "floe.builtin.relationships",
            Instant::now() + std::time::Duration::from_secs(5),
            &Cancellation::default(),
        )
        .await
        .unwrap();
        let floe_context_contract::SourceReadOutcome::Ready((_, dependency)) = outcome else {
            panic!("current reviewed connection must serve the people read");
        };
        assert_eq!(
            *driver.selected.lock().unwrap(),
            ["person.identity:a", "person.identity:b"]
        );
        assert_eq!(dependency.resources(), [logical]);
        assert_eq!(
            dependency
                .source_resources()
                .iter()
                .map(ResourceHandle::as_str)
                .collect::<Vec<_>>(),
            ["person.identity:a", "person.identity:b"]
        );
        assert_eq!(dependency.source_authority(), connection.source_authority());
        assert_ne!(dependency.source_authority(), original_authority);
        assert_eq!(dependency.grant_id(), grant.id());
        assert_eq!(dependency.grant_authority(), grant.authority());
        let mut changed = connection.clone();
        changed
            .configure_reviewed_native(
                connection.revision(),
                floe_connections::ResourceMode::Selected,
                connection.resources().to_vec(),
                SUBJECT_A.into(),
            )
            .unwrap();
        assert_eq!(
            authorize_personal_dependency(
                &FixtureConnections(changed),
                &records,
                &driver,
                person_id,
                "device",
                &dependency,
                Instant::now() + std::time::Duration::from_secs(5),
                &Cancellation::default(),
            )
            .await,
            Err(AgentFailure::PolicyDenied)
        );
    }

    #[tokio::test]
    async fn selected_contacts_subject_drift_fails_without_adopting_source() {
        let person_id = PersonId::new();
        let connection = SourceConnection::establish_reviewed_native(
            person_id,
            floe_context_contract::ConnectorId::try_new("contacts.apple").unwrap(),
            floe_context_contract::ConnectionId::try_new("contacts.apple.local").unwrap(),
            floe_context_contract::ExecutionOwnerId::try_new("apple:device").unwrap(),
            floe_connections::ResourceMode::Selected,
            vec![
                floe_connections::ConnectionResource::new(
                    ResourceHandle::try_new("person.identity:a").unwrap(),
                    "A".into(),
                )
                .unwrap(),
            ],
            SUBJECT_A.into(),
        )
        .unwrap();
        let source = GrantSourceBinding::try_new(
            person_id,
            connection.connection_id().clone(),
            connection.connector_id().clone(),
            connection.execution_owner_id().clone(),
        )
        .unwrap();
        let scope = GrantScope::try_new(
            vec![
                connection_view_resource(
                    floe_context_contract::PEOPLE_VIEW_ID,
                    connection.connection_id(),
                )
                .unwrap(),
            ],
            vec![GrantDataCategory::Derived],
            vec![GrantOperation::Read],
            vec![GrantPurpose::Assistant],
            vec![GrantConsumer::builtin("assistant").unwrap()],
            ProcessingRestriction::LocalOnly,
        )
        .unwrap();
        let mut grant =
            DataAccessGrant::new(GrantId::new(), Uuid::new_v4(), source, scope.clone()).unwrap();
        grant.activate_review(grant.authority(), scope).unwrap();
        let records = SwappingRecords {
            grants: vec![grant.clone()],
            reads: Mutex::new(0),
            queries: vec![],
            subjects: vec![],
        };
        let selected = crate::discover_source_candidates(crate::SourceCandidateRequest {
            person_id,
            device_id: "device",
            capability: floe_context_contract::PEOPLE_VIEW_ID,
            contract_version: 1,
            remote_connections: &[],
            remote_execution_owner: None,
            source_connections: std::slice::from_ref(&connection),
        })
        .unwrap()
        .remove(0)
        .reference;
        let connections = FixtureConnections(connection.clone());
        let driver = CapturingPeopleDriver {
            selected: Mutex::new(Vec::new()),
            subject_after: Some(SUBJECT_B.into()),
        };
        let failure = read_selected_people_outcome(
            &connections,
            &records,
            &driver,
            person_id,
            "device",
            &selected,
            "assistant",
            Instant::now() + std::time::Duration::from_secs(5),
            &Cancellation::default(),
        )
        .await
        .unwrap_err();
        assert_eq!(failure, AgentFailure::AccessReviewRequired);
        assert_eq!(connections.0.native_subject_fingerprint(), Some(SUBJECT_A));
        assert_eq!(
            connections.0.source_authority(),
            connection.source_authority()
        );
        assert_eq!(grant.state(), floe_access::GrantState::Active);
    }

    #[tokio::test]
    async fn selected_apple_contacts_ignore_live_android_grant() {
        let person_id = PersonId::new();
        let device_id = "device";
        let source = GrantSourceBinding::try_new(
            person_id,
            floe_context_contract::ConnectionId::try_new(contacts_connection("contacts.android"))
                .unwrap(),
            floe_context_contract::ConnectorId::try_new("contacts.android").unwrap(),
            floe_context_contract::ExecutionOwnerId::try_new(contacts_execution_owner(
                "contacts.android",
                device_id,
            ))
            .unwrap(),
        )
        .unwrap();
        let scope = GrantScope::try_new(
            vec![ResourceHandle::try_new(PEOPLE_RESOURCE).unwrap()],
            vec![GrantDataCategory::Derived],
            vec![GrantOperation::Read],
            vec![GrantPurpose::Assistant],
            vec![GrantConsumer::builtin("floe.builtin.relationships").unwrap()],
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
        grant.activate_review(grant.authority(), scope).unwrap();
        let records = SwappingRecords {
            grants: vec![grant],
            reads: Mutex::new(0),
            queries: vec![],
            subjects: vec![],
        };
        let apple_connection = floe_connections::SourceConnection::establish_reviewed_native(
            person_id,
            floe_context_contract::ConnectorId::try_new("contacts.apple").unwrap(),
            floe_context_contract::ConnectionId::try_new("contacts.apple.local").unwrap(),
            floe_context_contract::ExecutionOwnerId::try_new("apple:device").unwrap(),
            floe_connections::ResourceMode::Selected,
            vec![
                floe_connections::ConnectionResource::new(
                    ResourceHandle::try_new("person.identity:a").unwrap(),
                    "A".into(),
                )
                .unwrap(),
            ],
            "a".repeat(64),
        )
        .unwrap();
        let selected = crate::discover_source_candidates(crate::SourceCandidateRequest {
            person_id,
            device_id,
            capability: "people.identity",
            contract_version: 1,
            remote_connections: &[],
            remote_execution_owner: None,
            source_connections: std::slice::from_ref(&apple_connection),
        })
        .unwrap()
        .remove(0)
        .reference;
        let outcome = read_selected_people_outcome(
            &FixtureConnections(apple_connection),
            &records,
            &EchoingDriver,
            person_id,
            device_id,
            &selected,
            "floe.builtin.relationships",
            Instant::now() + std::time::Duration::from_secs(5),
            &Cancellation::default(),
        )
        .await
        .unwrap();
        let floe_context_contract::SourceReadOutcome::NeedsUserAction(blockers) = outcome else {
            panic!("unselected Android grant must not authorize the Apple target");
        };
        assert_eq!(blockers.blockers().len(), 1);
        assert_eq!(
            blockers.blockers()[0].connector_id().unwrap().as_str(),
            "contacts.apple"
        );
    }
}
