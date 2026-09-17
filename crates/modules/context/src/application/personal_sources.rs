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
    valid_subject_fingerprint,
};
use floe_agent_contract::{AgentFailure, ModelPlacement, PersonId};
use floe_execution::Cancellation;
use tokio::time::Instant;

use crate::application::personal_lineage::{
    FeasibilityQueryLineage, attention_query_fingerprint, feasibility_query_fingerprint,
    people_query_fingerprint, wellbeing_query_fingerprint,
};
use crate::ports::personal_source::{
    AttentionAcquisition, AttentionAcquisitionMode, PersonalAcquisition, PersonalDomain,
    PersonalGrantRecords, PersonalSourceDriver,
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
    attention_source, contacts_connection, contacts_execution_owner, contacts_source,
    feasibility_source, wellbeing_source,
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

fn read_requirement<'a>(read: &'a PersonalRead<'a>) -> PersonalReadRequirement<'a> {
    PersonalReadRequirement {
        source: &read.source,
        resource: read.resource,
        consumer: &read.consumer,
        same_authority: read.same_authority,
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
    grant_unchanged(grant, &current)?;
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
    let grant = admit_personal_read(records, &read).await?;
    let completed =
        acquire_personal_source(records, driver, &read, &grant, None, cancellation).await?;
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
    let grant = admit_personal_read(records, &read).await?;
    let completed =
        acquire_personal_source(records, driver, &read, &grant, None, cancellation).await?;
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
                mode: crate::ports::personal_source::AttentionAcquisitionMode::ReadProjection,
                expected_subject: Some(reviewed_subject.clone()),
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
        attention_query_fingerprint(
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

/// The consumer an Expert reads attention as.
pub const ATTENTION_EXPERT_CONSUMER: &str = "attention.expert";

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
    if dependency.person_id() != person_id
        || dependency.source().person_id() != person_id
    {
        return Err(AgentFailure::PolicyDenied);
    }
    if dependency.source().connector().as_str() == ATTENTION_CONNECTOR {
        if dependency.source().connection_id().as_str() != ATTENTION_CONNECTION
            || dependency.source().execution_owner().as_str() != attention_execution_owner(device_id)
            || !matches!(
                dependency.consumer().identifier(),
                crate::ASSISTANT_CONSUMER | ATTENTION_EXPERT_CONSUMER
            )
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
            || dependency.source().execution_owner().as_str()
                != apple_execution_owner(device_id)
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
            || dependency.source().execution_owner().as_str()
                != apple_execution_owner(device_id)
            || dependency.consumer().identifier() != crate::ASSISTANT_CONSUMER
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
            != contacts_execution_owner(
                dependency.source().connector().as_str(),
                device_id,
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
#[allow(clippy::too_many_arguments)]
pub async fn authorize_personal_dependency(
    records: &impl PersonalGrantRecords,
    driver: &impl PersonalSourceDriver,
    person_id: PersonId,
    device_id: &str,
    dependency: &ContextDependency,
    placements: &[ModelPlacement],
    deadline: Instant,
    cancellation: &Cancellation,
) -> Result<(), AgentFailure> {
    if matches!(
        dependency.source().connector().as_str(),
        "contacts.apple" | "contacts.android"
    ) {
        if placements != [ModelPlacement::DeviceLocal]
            || dependency.source().connection_id().as_str()
                != contacts_connection(dependency.source().connector().as_str())
            || dependency.source().execution_owner().as_str()
                != contacts_execution_owner(
                    dependency.source().connector().as_str(),
                    device_id,
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
        let grants = records.grants().await?;
        let grant = active_read_grant(
            &grants,
            &PersonalReadRequirement {
                source: dependency.source(),
                resource: PEOPLE_RESOURCE,
                consumer: dependency.consumer(),
                same_authority: true,
                reject_ambiguous: false,
            },
        )?;
        if dependency.grant_id() != grant.id()
            || dependency.grant_authority() != grant.authority()
            || dependency.source() != grant.source()
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let observation = driver.trusted_personal_observation(
                person_id,
                device_id,
                dependency.observation_id(),
                dependency.process_incarnation_id(),
            )
            .map_err(|_| AgentFailure::PolicyDenied)?;
        let reviewed_subject = records.reviewed_subject(grant.id()).await?;
        if observation.native_subject_fingerprint != reviewed_subject
            || dependency.observed_at().timestamp_millis()
                != observation.observed_at_unix_ms
            || dependency.expires_at().timestamp_millis() != observation.expires_at_unix_ms
            || dependency.query_fingerprint() != observation.query_fingerprint
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let policy = records.consumer_policy(grant.id()).await?;
        if dependency.consumer_policy() != policy {
            return Err(AgentFailure::PolicyDenied);
        }
        return Ok(());
    }
    if dependency.source().connector().as_str() == FEASIBILITY_CONNECTOR {
        if placements != [ModelPlacement::DeviceLocal]
            || dependency.source().connection_id().as_str() != FEASIBILITY_CONNECTION
            || dependency.source().execution_owner().as_str()
                != apple_execution_owner(device_id)
            || dependency.consumer().identifier() != crate::ASSISTANT_CONSUMER
            || dependency.operation() != GrantOperation::Read
            || dependency.purpose() != GrantPurpose::Assistant
            || dependency.processing() != &ProcessingRestriction::LocalOnly
            || dependency.resources()
                != [ResourceHandle::try_new(FEASIBILITY_RESOURCE)
                    .map_err(|_| AgentFailure::PolicyDenied)?]
            || dependency.categories() != [GrantDataCategory::Derived]
            || dependency.lease_invocation_id().is_nil()
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let grants = records.grants().await?;
        let grant =
            active_read_grant(
                &grants,
                &PersonalReadRequirement {
                    source: dependency.source(),
                    resource: FEASIBILITY_RESOURCE,
                    consumer: dependency.consumer(),
                    same_authority: false,
                    reject_ambiguous: true,
                },
            )?;
        if dependency.grant_id() != grant.id()
            || dependency.grant_authority() != grant.authority()
            || dependency.source() != grant.source()
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let observation = driver.trusted_personal_observation(
                person_id,
                device_id,
                dependency.observation_id(),
                dependency.process_incarnation_id(),
            )
            .map_err(|_| AgentFailure::PolicyDenied)?;
        if dependency.observed_at().timestamp_millis() != observation.observed_at_unix_ms
            || dependency.expires_at().timestamp_millis() != observation.expires_at_unix_ms
            || dependency.query_fingerprint() != observation.query_fingerprint
            || observation.native_subject_fingerprint
                != records.reviewed_subject(grant.id()).await?
        {
            return Err(AgentFailure::PolicyDenied);
        }
        if dependency.consumer_policy()
            != records.consumer_policy(grant.id()).await?
        {
            return Err(AgentFailure::PolicyDenied);
        }
        return Ok(());
    }
    if dependency.source().connector().as_str() == WELLBEING_CONNECTOR {
        if placements != [ModelPlacement::DeviceLocal]
            || dependency.source().connection_id().as_str() != WELLBEING_CONNECTION
            || dependency.source().execution_owner().as_str()
                != apple_execution_owner(device_id)
            || dependency.consumer().identifier() != crate::ASSISTANT_CONSUMER
            || dependency.operation() != GrantOperation::Read
            || dependency.purpose() != GrantPurpose::Assistant
            || dependency.processing() != &ProcessingRestriction::LocalOnly
            || dependency.resources()
                != [ResourceHandle::try_new(WELLBEING_RESOURCE)
                    .map_err(|_| AgentFailure::PolicyDenied)?]
            || dependency.categories() != [GrantDataCategory::Derived]
            || dependency.lease_invocation_id().is_nil()
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let grants = records.grants().await?;
        let grant = active_read_grant(
            &grants,
            &PersonalReadRequirement {
                source: dependency.source(),
                resource: WELLBEING_RESOURCE,
                consumer: dependency.consumer(),
                same_authority: false,
                reject_ambiguous: true,
            },
        )?;
        if dependency.grant_id() != grant.id()
            || dependency.grant_authority() != grant.authority()
            || dependency.source() != grant.source()
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let observation = driver.trusted_personal_observation(
                person_id,
                device_id,
                dependency.observation_id(),
                dependency.process_incarnation_id(),
            )
            .map_err(|_| AgentFailure::PolicyDenied)?;
        if dependency.observed_at().timestamp_millis() != observation.observed_at_unix_ms
            || dependency.expires_at().timestamp_millis() != observation.expires_at_unix_ms
            || dependency.query_fingerprint() != observation.query_fingerprint
            || observation.native_subject_fingerprint
                != records.reviewed_subject(grant.id()).await?
        {
            return Err(AgentFailure::PolicyDenied);
        }
        if dependency.consumer_policy()
            != records.consumer_policy(grant.id()).await?
        {
            return Err(AgentFailure::PolicyDenied);
        }
        return Ok(());
    }
    if placements != [ModelPlacement::DeviceLocal]
        || dependency.person_id() != person_id
        || dependency.source().person_id() != person_id
        || dependency.source().connector().as_str() != ATTENTION_CONNECTOR
        || dependency.source().connection_id().as_str() != ATTENTION_CONNECTION
        || dependency.source().execution_owner().as_str() != attention_execution_owner(device_id)
        || dependency.operation() != GrantOperation::Read
        || dependency.purpose() != GrantPurpose::Assistant
        || dependency.processing() != &ProcessingRestriction::LocalOnly
        || dependency.resources().len() != 1
        || dependency.resources()[0].as_str() != ATTENTION_RESOURCE
        || dependency.categories() != [GrantDataCategory::Derived]
        || dependency.consumer().identifier() != crate::ASSISTANT_CONSUMER
            && dependency.consumer().identifier() != ATTENTION_EXPERT_CONSUMER
    {
        return Err(AgentFailure::PolicyDenied);
    }
    let grants = records.grants().await?;
    let attention_source = attention_source(person_id, device_id, SourceAuthority::new())?;
    let grant = active_read_grant(
        &grants,
        &PersonalReadRequirement {
            source: &attention_source,
            resource: ATTENTION_RESOURCE,
            consumer: dependency.consumer(),
            same_authority: false,
            reject_ambiguous: false,
        },
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
    let (current_view, trusted_observation_subject) = driver.trusted_attention_observation(
            person_id,
            device_id,
            dependency.observation_id(),
            dependency.process_incarnation_id(),
        )
        .map_err(|_| AgentFailure::PolicyDenied)?;
    let reviewed_subject = records.reviewed_subject(grant.id()).await?;
    if trusted_observation_subject != reviewed_subject
        || dependency.lease_invocation_id().is_nil()
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
    let policy = records.consumer_policy(grant.id()).await?;
    if dependency.consumer_policy() != policy {
        return Err(AgentFailure::PolicyDenied);
    }
    // The device is asked once more which subject would answer now: a Person
    // who re-paired or switched devices has not re-reviewed this grant.
    let probe = driver
        .acquire_attention(
            AttentionAcquisition {
                person_id,
                device_id,
                host_epoch: driver
                    .attention_host_epoch(person_id)
                    .map_err(|_| AgentFailure::PolicyDenied)?,
                mode: AttentionAcquisitionMode::InspectSubject,
                expected_subject: None,
                deadline,
            },
            cancellation.clone(),
        )
        .await
        .map_err(|_| AgentFailure::PolicyDenied)?;
    if probe.subject_before != reviewed_subject || probe.subject_after != reviewed_subject {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(())
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
        let source = feasibility_source(person_id, device_id, SourceAuthority::new()).unwrap();
        let mut grant =
            DataAccessGrant::new(GrantId::new(), Uuid::new_v4(), source.clone(), scope()).unwrap();
        grant
            .activate_review(grant.authority(), source, scope())
            .unwrap();
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

        fn consumer_policy<'a>(
            &'a self,
            _: GrantId,
        ) -> BoxFuture<'a, Result<floe_access::ConsumerPolicyAuthority, AgentFailure>> {
            Box::pin(async { Ok(floe_access::ConsumerPolicyAuthority::default()) })
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
}
