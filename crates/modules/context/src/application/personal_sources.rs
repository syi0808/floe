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
    GrantSourceBinding, PersonalReadRequirement, ResourceHandle, active_read_grant,
    grant_unchanged, subject_unchanged,
};
use floe_agent_contract::{AgentFailure, PersonId};
use floe_connections::SourceConnection;
use floe_context_contract::connection_view_resource;
use floe_execution::Cancellation;
use tokio::time::Instant;

use crate::application::personal_lineage::{
    attention_query_fingerprint, people_query_fingerprint, wellbeing_query_fingerprint,
};
use crate::ports::personal_source::{
    AttentionAcquisition, AttentionAcquisitionMode, PersonalAcquisition, PersonalConnectionReader,
    PersonalDomain, PersonalSourceDriver,
};
use crate::{
    AttentionView, PeopleView, WellbeingView, validate_people_view, validate_wellbeing_view,
};

/// The names a grant binds a personal source under belong to Access; a read
/// only quotes them.
pub use floe_access::{
    ATTENTION_CONNECTOR, ATTENTION_RESOURCE, PEOPLE_RESOURCE, WELLBEING_CONNECTOR,
    WELLBEING_RESOURCE, apple_execution_owner,
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

fn decode<View: DeserializeOwned>(value: serde_json::Value) -> Result<View, AgentFailure> {
    serde_json::from_value(value).map_err(|_| AgentFailure::CapabilityUnavailable)
}

/// Whether the Person's grant still admits a stored personal dependency.
///
/// Holding is not the same as being allowed: the grant behind the dependency
/// has to still be the grant it was recorded under, reviewed against the same
/// device subject, with the same consumer policy — and for attention, against
/// the device that would answer right now.
///
/// Source reauthorization is independent of model planning. Whether a
/// reauthorized dependency may reach a Device or Gateway model target is
/// decided by Access model dispatch.
#[allow(clippy::too_many_arguments)]
pub async fn authorize_personal_dependency(
    connections: &(impl PersonalConnectionReader + ?Sized),
    records: &(impl floe_access::GrantRepository + ?Sized),
    driver: &(impl PersonalSourceDriver + ?Sized),
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
    if connections
        .source_is_fenced(person_id, &selected.connection_id)
        .await?
    {
        return Err(AgentFailure::PolicyDenied);
    }
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
    let grant = active_read_grant(
        &records.snapshot(source.clone()).await?.grants,
        &requirement,
    )
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
        let current_grant = active_read_grant(
            &records.snapshot(source.clone()).await?.grants,
            &requirement,
        )
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
            || dependency.health_transform() != observation.health_transform.as_ref()
        {
            return Err(AgentFailure::PolicyDenied);
        }
    }
    if view == floe_context_contract::WELLBEING_VIEW_ID {
        dependency.validate_health_transform(device_id, Utc::now())?;
    }
    if connections
        .source_is_fenced(person_id, connection.connection_id())
        .await?
    {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(())
}

fn observed_grant(
    grant: &floe_access::DataAccessGrant,
) -> Result<floe_context_contract::ObservedGrant, AgentFailure> {
    floe_context_contract::ObservedGrant::try_new(grant.id(), grant.authority())
        .map_err(|_| AgentFailure::InvalidInput)
}

async fn selected_standing_grant(
    records: &(impl floe_access::GrantRepository + ?Sized),
    connection: &SourceConnection,
    source: &GrantSourceBinding,
    logical: &ResourceHandle,
    consumer: &GrantConsumer,
    source_id: &str,
    inline: bool,
) -> Result<floe_context_contract::SourceReadOutcome<floe_access::DataAccessGrant>, AgentFailure> {
    let grants = records.snapshot(source.clone()).await?.grants;
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
            let blockers = floe_context_contract::SourceAccessBlockers::try_new(vec![requirement])
                .map_err(|_| AgentFailure::InvalidInput)?;
            Ok(floe_context_contract::SourceReadOutcome::NeedsUserAction(
                blockers,
            ))
        }
        Err(error) => Err(error),
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn read_selected_people_outcome(
    connections: &(impl PersonalConnectionReader + ?Sized),
    records: &(impl floe_access::GrantRepository + ?Sized),
    driver: &(impl PersonalSourceDriver + ?Sized),
    person_id: PersonId,
    device_id: &str,
    selected: &floe_context_contract::SourceSelectionReference,
    consumer_name: &str,
    deadline: Instant,
    cancellation: &Cancellation,
) -> Result<floe_context_contract::SourceReadOutcome<(PeopleView, ContextDependency)>, AgentFailure>
{
    within_read_window(deadline, cancellation)?;
    if connections
        .source_is_fenced(person_id, &selected.connection_id)
        .await?
    {
        return Err(AgentFailure::PolicyDenied);
    }
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
    if connections
        .source_is_fenced(person_id, connection.connection_id())
        .await?
    {
        return Err(AgentFailure::PolicyDenied);
    }
    let current_grant = active_read_grant(
        &records.snapshot(source.clone()).await?.grants,
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
        current_grant.scope().processing().clone(),
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
        None,
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
    connections: &(impl PersonalConnectionReader + ?Sized),
    records: &(impl floe_access::GrantRepository + ?Sized),
    driver: &(impl PersonalSourceDriver + ?Sized),
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
    if connections
        .source_is_fenced(person_id, &selected.connection_id)
        .await?
    {
        return Err(AgentFailure::PolicyDenied);
    }
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
    if connections
        .source_is_fenced(person_id, connection.connection_id())
        .await?
    {
        return Err(AgentFailure::PolicyDenied);
    }
    let current_grant = active_read_grant(
        &records.snapshot(source.clone()).await?.grants,
        &requirement,
    )?;
    grant_unchanged(&grant, &current_grant)?;
    let transform = acquired
        .health_transform
        .ok_or(AgentFailure::PolicyDenied)?;
    let view: WellbeingView = decode(acquired.view.ok_or(AgentFailure::CapabilityUnavailable)?)?;
    transform.validate_view(device_id, &view, Utc::now())?;
    validate_wellbeing_view(&view, Utc::now().timestamp_millis())
        .map_err(|_| AgentFailure::CapabilityUnavailable)?;
    let observation_id = Uuid::new_v4();
    let process = driver.process_incarnation();
    let query_fingerprint =
        wellbeing_query_fingerprint(&view, &transform, &subject, observation_id, process);
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
        current_grant.scope().processing().clone(),
        observation_id,
        query_fingerprint.clone(),
        lease_invocation_id,
        process,
        observed,
        expires,
    )
    .map_err(|_| AgentFailure::InvalidInput)?
    .with_health_transform(transform.clone())
    .map_err(|_| AgentFailure::PolicyDenied)?;
    driver.commit_personal_observation(
        person_id,
        device_id,
        observation_id,
        process,
        &subject,
        view.observed_at_unix_ms,
        view.expires_at_unix_ms,
        query_fingerprint,
        Some(transform),
    )?;
    Ok(floe_context_contract::SourceReadOutcome::Ready((
        view, dependency,
    )))
}

#[allow(clippy::too_many_arguments)]
pub async fn admit_selected_attention_outcome(
    connections: &(impl PersonalConnectionReader + ?Sized),
    records: &(impl floe_access::GrantRepository + ?Sized),
    driver: &(impl PersonalSourceDriver + ?Sized),
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
    if connections
        .source_is_fenced(person_id, &selected.connection_id)
        .await?
    {
        return Err(AgentFailure::PolicyDenied);
    }
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
    if connections
        .source_is_fenced(person_id, connection.connection_id())
        .await?
    {
        return Err(AgentFailure::PolicyDenied);
    }
    let current_grant = active_read_grant(
        &records.snapshot(source.clone()).await?.grants,
        &requirement,
    )?;
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
        current_grant.scope().processing().clone(),
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
