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
    ConnectionId, ConnectorId, ContextDependency, ExecutionOwnerId, GrantConsumer,
    GrantDataCategory, GrantOperation, GrantPurpose, GrantSourceBinding, PersonalReadRequirement,
    ProcessingRestriction, ResourceHandle, SourceAuthority, active_read_grant, grant_unchanged,
    subject_unchanged, valid_subject_fingerprint,
};
use floe_agent_contract::{AgentFailure, PersonId};
use floe_execution::Cancellation;
use tokio::time::Instant;

use crate::application::personal_lineage::{
    FeasibilityQueryLineage, feasibility_query_fingerprint, people_query_fingerprint,
    wellbeing_query_fingerprint,
};
use crate::ports::personal_source::{
    PersonalAcquisition, PersonalDomain, PersonalGrantRecords, PersonalSourceDriver,
};
use crate::{
    AttentionView, FeasibilityView, PeopleView, WellbeingView, validate_feasibility_view,
    validate_people_view, validate_wellbeing_view,
};

pub const ATTENTION_CONNECTOR: &str = "attention.macos";
pub const ATTENTION_CONNECTION: &str = "attention.macos.local";
pub const ATTENTION_RESOURCE: &str = "attention.coarse";
pub const PEOPLE_RESOURCE: &str = "people.identity";
pub const FEASIBILITY_CONNECTOR: &str = "feasibility.apple";
pub const FEASIBILITY_CONNECTION: &str = "feasibility.apple.local";
pub const FEASIBILITY_RESOURCE: &str = "schedule.feasibility";
pub const WELLBEING_CONNECTOR: &str = "health.apple";
pub const WELLBEING_CONNECTION: &str = "health.apple.local";
pub const WELLBEING_RESOURCE: &str = "wellbeing.derived";

/// The device that answers for attention on this Person's behalf.
pub fn attention_execution_owner(device_id: &str) -> String {
    format!("macos:{device_id}")
}

/// The device that answers for the Apple personal sources.
pub fn apple_execution_owner(device_id: &str) -> String {
    format!("apple:{device_id}")
}

fn source_binding(
    person_id: PersonId,
    connection: &str,
    connector: &str,
    execution_owner: String,
    authority: SourceAuthority,
) -> Result<GrantSourceBinding, AgentFailure> {
    GrantSourceBinding::try_new(
        person_id,
        ConnectionId::try_new(connection).map_err(|_| AgentFailure::InvalidInput)?,
        ConnectorId::try_new(connector).map_err(|_| AgentFailure::InvalidInput)?,
        ExecutionOwnerId::try_new(execution_owner).map_err(|_| AgentFailure::InvalidInput)?,
        authority,
    )
    .map_err(|_| AgentFailure::InvalidInput)
}

/// The source binding a feasibility grant is bound to.
pub fn feasibility_source(
    person_id: PersonId,
    device_id: &str,
    authority: SourceAuthority,
) -> Result<GrantSourceBinding, AgentFailure> {
    source_binding(
        person_id,
        FEASIBILITY_CONNECTION,
        FEASIBILITY_CONNECTOR,
        apple_execution_owner(device_id),
        authority,
    )
}

/// The source binding a wellbeing grant is bound to.
pub fn wellbeing_source(
    person_id: PersonId,
    device_id: &str,
    authority: SourceAuthority,
) -> Result<GrantSourceBinding, AgentFailure> {
    source_binding(
        person_id,
        WELLBEING_CONNECTION,
        WELLBEING_CONNECTOR,
        apple_execution_owner(device_id),
        authority,
    )
}

/// The source binding an attention grant is bound to.
pub fn attention_source(
    person_id: PersonId,
    device_id: &str,
    authority: SourceAuthority,
) -> Result<GrantSourceBinding, AgentFailure> {
    source_binding(
        person_id,
        ATTENTION_CONNECTION,
        ATTENTION_CONNECTOR,
        attention_execution_owner(device_id),
        authority,
    )
}

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
    same_authority: bool,
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
    policy: floe_access::ConsumerPolicyAuthority,
}

async fn acquire_personal_source(
    records: &impl PersonalGrantRecords,
    driver: &impl PersonalSourceDriver,
    read: &PersonalRead<'_>,
    feasibility: Option<&floe_access::FeasibilityGrantQuery>,
    cancellation: &Cancellation,
) -> Result<CompletedRead, AgentFailure> {
    within_read_window(read.deadline, cancellation)?;
    let requirement = PersonalReadRequirement {
        source: &read.source,
        resource: read.resource,
        consumer: &read.consumer,
        same_authority: read.same_authority,
        reject_ambiguous: read.reject_ambiguous,
    };
    let grant = active_read_grant(&records.grants().await?, &requirement)?;
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
    subject_unchanged(
        &subject,
        &acquired.subject_before,
        &acquired.subject_after,
    )?;
    let value = acquired.view.ok_or(AgentFailure::CapabilityUnavailable)?;
    // The Person can revoke or re-review while the device is answering, so the
    // grant the read started under has to be the one it finished under.
    let current = active_read_grant(&records.grants().await?, &requirement)?;
    grant_unchanged(&grant, &current)?;
    let policy = records.consumer_policy(current.id()).await?;
    Ok(CompletedRead {
        value,
        subject,
        observation_id: Uuid::new_v4(),
        process: driver.process_incarnation(),
        grant: current,
        policy,
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
    ContextDependency::try_new(
        read.person_id,
        completed.grant.id(),
        completed.grant.authority(),
        completed.grant.source().clone(),
        vec![ResourceHandle::try_new(read.resource).map_err(|_| AgentFailure::InvalidInput)?],
        vec![GrantDataCategory::Derived],
        GrantOperation::Read,
        GrantPurpose::Assistant,
        read.consumer.clone(),
        ProcessingRestriction::LocalOnly,
        completed.policy,
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

/// Read the people the Person selected, as the identities they granted.
#[allow(clippy::too_many_arguments)]
pub async fn read_people(
    records: &impl PersonalGrantRecords,
    driver: &impl PersonalSourceDriver,
    person_id: PersonId,
    device_id: &str,
    source: GrantSourceBinding,
    selected_handles: &[String],
    expected_native_subject_fingerprint: &str,
    consumer_name: &str,
    deadline: Instant,
    cancellation: &Cancellation,
) -> Result<(PeopleView, ContextDependency), AgentFailure> {
    if source.person_id() != person_id
        || !matches!(
            source.connector().as_str(),
            "contacts.apple" | "contacts.android"
        )
        || selected_handles.is_empty()
        || selected_handles.len() > 64
        || selected_handles.windows(2).any(|pair| pair[0] >= pair[1])
        || !valid_subject_fingerprint(expected_native_subject_fingerprint)
    {
        return Err(AgentFailure::InvalidInput);
    }
    let read = PersonalRead {
        person_id,
        device_id,
        source,
        resource: PEOPLE_RESOURCE,
        domain: PersonalDomain::People,
        consumer: GrantConsumer::builtin(consumer_name)
            .map_err(|_| AgentFailure::InvalidInput)?,
        same_authority: true,
        reject_ambiguous: false,
        selected_handles: selected_handles.to_vec(),
        expected_subject: Some(expected_native_subject_fingerprint.to_owned()),
        lease_invocation_id: Uuid::new_v4(),
        deadline,
    };
    let completed =
        acquire_personal_source(records, driver, &read, None, cancellation).await?;
    let view: PeopleView = decode(completed.value.clone())?;
    validate_people_view(&view, Utc::now().timestamp_millis())
        .map_err(|_| AgentFailure::CapabilityUnavailable)?;
    let query_fingerprint = people_query_fingerprint(
        &view,
        selected_handles,
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
    let consumer =
        GrantConsumer::builtin(consumer_name).map_err(|_| AgentFailure::InvalidInput)?;
    let source = feasibility_source(person_id, device_id, SourceAuthority::new())?;
    let requirement = PersonalReadRequirement {
        source: &source,
        resource: FEASIBILITY_RESOURCE,
        consumer: &consumer,
        same_authority: false,
        reject_ambiguous: true,
    };
    // The query is part of the grant, so it is read before anything is asked.
    let query = records
        .feasibility_query(active_read_grant(&records.grants().await?, &requirement)?.id())
        .await?;
    let read = PersonalRead {
        person_id,
        device_id,
        source,
        resource: FEASIBILITY_RESOURCE,
        domain: PersonalDomain::Feasibility,
        consumer,
        same_authority: false,
        reject_ambiguous: true,
        selected_handles: vec![],
        expected_subject: None,
        lease_invocation_id,
        deadline,
    };
    let completed =
        acquire_personal_source(records, driver, &read, Some(&query), cancellation).await?;
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

/// Read what the Person's own device says about their capacity today.
pub async fn read_wellbeing(
    records: &impl PersonalGrantRecords,
    driver: &impl PersonalSourceDriver,
    person_id: PersonId,
    device_id: &str,
    consumer_name: &str,
    lease_invocation_id: Uuid,
    deadline: Instant,
    cancellation: &Cancellation,
) -> Result<(WellbeingView, ContextDependency), AgentFailure> {
    let consumer =
        GrantConsumer::builtin(consumer_name).map_err(|_| AgentFailure::InvalidInput)?;
    // Wellbeing is the Person's own; no Expert reads it on their behalf.
    if consumer.identifier() != crate::ASSISTANT_CONSUMER {
        return Err(AgentFailure::PolicyDenied);
    }
    let read = PersonalRead {
        person_id,
        device_id,
        source: wellbeing_source(person_id, device_id, SourceAuthority::new())?,
        resource: WELLBEING_RESOURCE,
        domain: PersonalDomain::Wellbeing,
        consumer,
        same_authority: false,
        reject_ambiguous: true,
        selected_handles: vec![],
        expected_subject: None,
        lease_invocation_id,
        deadline,
    };
    let completed =
        acquire_personal_source(records, driver, &read, None, cancellation).await?;
    let view: WellbeingView = decode(completed.value.clone())?;
    validate_wellbeing_view(&view, Utc::now().timestamp_millis())
        .map_err(|_| AgentFailure::CapabilityUnavailable)?;
    let query_fingerprint = wellbeing_query_fingerprint(
        &view,
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

/// The attention projection an Expert or the assistant may read.
///
/// Attention is a live projection rather than a stored view: the observation is
/// committed by the driver as it is admitted, and the dependency is bound to the
/// observation it committed.
pub async fn admit_attention(
    records: &impl PersonalGrantRecords,
    driver: &impl PersonalSourceDriver,
    person_id: PersonId,
    device_id: &str,
    consumer: GrantConsumer,
    lease_invocation_id: Uuid,
    deadline: Instant,
    cancellation: &Cancellation,
) -> Result<(AttentionView, ContextDependency), AgentFailure> {
    use crate::ports::personal_source::AttentionAcquisition;

    within_read_window(deadline, cancellation)?;
    let source = attention_source(person_id, device_id, SourceAuthority::new())?;
    let requirement = PersonalReadRequirement {
        source: &source,
        resource: ATTENTION_RESOURCE,
        consumer: &consumer,
        same_authority: false,
        reject_ambiguous: false,
    };
    let grant = active_read_grant(&records.grants().await?, &requirement)?;
    let reviewed_subject = records.reviewed_subject(grant.id()).await?;
    let host_epoch = driver.attention_host_epoch(person_id)?;
    let acquired = driver
        .acquire_attention(
            AttentionAcquisition {
                person_id,
                device_id,
                host_epoch: host_epoch.clone(),
                expected_subject: reviewed_subject.clone(),
                deadline,
            },
            cancellation.clone(),
        )
        .await?;
    subject_unchanged(
        &reviewed_subject,
        &acquired.subject_before,
        &acquired.subject_after,
    )?;
    let native_subject = acquired.subject_before.clone();
    let view: AttentionView = decode(acquired.view.ok_or(AgentFailure::CapabilityUnavailable)?)?;
    if view.state == crate::AttentionState::Unknown || view.evidence_handles.is_empty() {
        return Err(AgentFailure::CapabilityUnavailable);
    }
    let (observed_at, expires_at) = freshness(view.observed_at_unix_ms, view.expires_at_unix_ms)?;
    within_read_window(deadline, cancellation)?;
    let current_grant = active_read_grant(&records.grants().await?, &requirement)?;
    grant_unchanged(&grant, &current_grant)?;
    // The reviewed subject is read again, because the Person may have
    // re-reviewed the same grant against a different device while this ran.
    let current_subject = records.reviewed_subject(current_grant.id()).await?;
    if current_subject != reviewed_subject || current_subject != native_subject {
        return Err(AgentFailure::AccessReviewRequired);
    }
    let policy = records.consumer_policy(current_grant.id()).await?;
    let (observation_id, process_incarnation_id) =
        driver.commit_attention_projection(person_id, &host_epoch, device_id, &view, &native_subject)?;
    let dependency = ContextDependency::try_new(
        person_id,
        current_grant.id(),
        current_grant.authority(),
        current_grant.source().clone(),
        vec![ResourceHandle::try_new(ATTENTION_RESOURCE).map_err(|_| AgentFailure::InvalidInput)?],
        vec![GrantDataCategory::Derived],
        GrantOperation::Read,
        GrantPurpose::Assistant,
        consumer,
        ProcessingRestriction::LocalOnly,
        policy,
        observation_id,
        crate::application::personal_lineage::attention_query_fingerprint(
            person_id,
            device_id,
            &view,
            observation_id,
            process_incarnation_id,
        ),
        lease_invocation_id,
        process_incarnation_id,
        observed_at,
        expires_at,
    )
    .map_err(|_| AgentFailure::InvalidInput)?;
    within_read_window(deadline, cancellation)?;
    Ok((view, dependency))
}
