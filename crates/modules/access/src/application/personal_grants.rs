//! Configuring what the Person's own device sources may be read for.
//!
//! One shape for every source: look at the grant they already left, ask the
//! device which subject it answers for now, refuse if that is not the subject
//! they reviewed, and commit the review against the authority it expects. Which
//! consumers a source may serve, and which change is even allowed, is decided
//! here; the device and the store only report and commit.

use floe_context_contract::{
    GrantAuthority, GrantConsumer, GrantDataCategory, GrantId, GrantOperation, GrantPurpose,
    GrantScope, GrantSourceBinding, ProcessingRestriction, ResourceHandle, SourceAuthority,
};
use floe_execution::Cancellation;
use floe_kernel::{AgentFailure, PersonId};
use uuid::Uuid;

use crate::application::personal_read::FeasibilityGrantQuery;
use crate::application::personal_sources::{
    ATTENTION_CONNECTION, ATTENTION_CONNECTOR, ATTENTION_RESOURCE, FEASIBILITY_CONNECTION,
    FEASIBILITY_CONNECTOR, FEASIBILITY_RESOURCE, PEOPLE_RESOURCE, WELLBEING_CONNECTION,
    WELLBEING_CONNECTOR, WELLBEING_RESOURCE, attention_execution_owner, attention_source,
    contacts_connection, contacts_source, feasibility_source, wellbeing_source,
};
use crate::data_access_grant::{DataAccessGrant, GrantState};
use crate::ports::personal_grants::{
    PersonalGrantStore, PersonalSubjectInspector, PersonalSubjectProbe,
};

pub const ATTENTION_ASSISTANT_CONSUMER: &str = "assistant";
pub const ATTENTION_EXPERT_CONSUMER: &str = "attention.expert";

/// What the Person is asking to change about one source.
#[derive(Clone)]
pub enum PersonalAccessChange {
    /// Show what is granted now, and what subject the device answers for.
    Inspect,
    Review {
        expected_native_subject_fingerprint: String,
        consumers: Vec<String>,
        feasibility_query: Option<FeasibilityGrantQuery>,
        expected_grant_id: Option<GrantId>,
        expected_grant_authority: Option<GrantAuthority>,
    },
    SetEnabled {
        enabled: bool,
    },
}

#[derive(Clone)]
pub struct PersonalAccessConfiguration {
    pub connector: String,
    pub device_id: String,
    pub change: PersonalAccessChange,
}

/// What the Person is asking to change about their contacts.
#[derive(Clone)]
pub enum ContactsAccessChange {
    Inspect {
        selected_handles: Vec<String>,
    },
    Review {
        selected_handles: Vec<String>,
        expected_native_subject_fingerprint: String,
        consumers: Vec<String>,
        expected_grant_id: Option<GrantId>,
        expected_grant_authority: Option<GrantAuthority>,
    },
    SetEnabled {
        enabled: bool,
    },
}

#[derive(Clone)]
pub struct ContactsAccessConfiguration {
    pub connector: String,
    pub device_id: String,
    pub change: ContactsAccessChange,
}

/// What one source's access looks like after the change.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersonalAccessOverview {
    pub person_id: PersonId,
    pub connector: String,
    pub device_id: String,
    pub connection_id: String,
    pub source_authority: Option<SourceAuthority>,
    pub grant_id: Option<GrantId>,
    pub grant_authority: Option<GrantAuthority>,
    pub state: PersonalAccessState,
    pub review_required: bool,
    pub presence_available: bool,
    pub consumers: Vec<String>,
    pub native_subject_fingerprint: Option<String>,
    pub process_incarnation: Option<Uuid>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PersonalAccessState {
    NeedsReview,
    Paused,
    Active,
    Revoked,
}

fn state_of(grant: Option<&DataAccessGrant>) -> PersonalAccessState {
    match grant.map(DataAccessGrant::state) {
        None => PersonalAccessState::NeedsReview,
        Some(GrantState::Paused) => PersonalAccessState::Paused,
        Some(GrantState::Active) => PersonalAccessState::Active,
        Some(GrantState::Revoked) => PersonalAccessState::Revoked,
    }
}

fn granted_consumers(grant: Option<&DataAccessGrant>) -> Vec<String> {
    grant
        .map(|value| {
            value
                .scope()
                .consumers()
                .iter()
                .map(|consumer| consumer.identifier().to_owned())
                .collect()
        })
        .unwrap_or_default()
}

fn overview(
    person_id: PersonId,
    connector: &str,
    device_id: &str,
    connection_id: String,
    grant: Option<&DataAccessGrant>,
    presence: Option<Uuid>,
    native_subject_fingerprint: Option<String>,
) -> PersonalAccessOverview {
    PersonalAccessOverview {
        person_id,
        connector: connector.to_owned(),
        device_id: device_id.to_owned(),
        connection_id,
        source_authority: grant.map(|value| value.source().source_authority()),
        grant_id: grant.map(DataAccessGrant::id),
        grant_authority: grant.map(DataAccessGrant::authority),
        state: state_of(grant),
        review_required: grant.is_none_or(DataAccessGrant::review_required),
        presence_available: presence.is_some(),
        consumers: granted_consumers(grant),
        native_subject_fingerprint,
        process_incarnation: presence,
    }
}

fn expected_grant(
    id: Option<GrantId>,
    authority: Option<GrantAuthority>,
) -> Result<Option<(GrantId, GrantAuthority)>, AgentFailure> {
    match (id, authority) {
        (Some(id), Some(authority)) => Ok(Some((id, authority))),
        (None, None) => Ok(None),
        _ => Err(AgentFailure::InvalidInput),
    }
}

fn valid_device(device_id: &str) -> bool {
    !device_id.is_empty() && device_id.len() <= 128 && !device_id.chars().any(char::is_whitespace)
}

pub fn validate_request(request: &PersonalAccessConfiguration) -> Result<(), AgentFailure> {
    if !matches!(
        request.connector.as_str(),
        ATTENTION_CONNECTOR | FEASIBILITY_CONNECTOR | WELLBEING_CONNECTOR
    ) || !valid_device(&request.device_id)
    {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(())
}

fn validate_contacts_request(request: &ContactsAccessConfiguration) -> Result<(), AgentFailure> {
    if !matches!(
        request.connector.as_str(),
        "contacts.apple" | "contacts.android"
    ) || !valid_device(&request.device_id)
    {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(())
}

/// Whether a grant is the attention grant for this Person on this device.
pub fn matches_source(grant: &DataAccessGrant, person_id: PersonId, device_id: &str) -> bool {
    grant.source().person_id() == person_id
        && grant.source().connector().as_str() == ATTENTION_CONNECTOR
        && grant.source().connection_id().as_str() == ATTENTION_CONNECTION
        && grant.source().execution_owner().as_str() == attention_execution_owner(device_id)
}

fn same_source(grant: &DataAccessGrant, source: &GrantSourceBinding) -> bool {
    let binding = grant.source();
    binding.person_id() == source.person_id()
        && binding.connector() == source.connector()
        && binding.connection_id() == source.connection_id()
        && binding.execution_owner() == source.execution_owner()
}

fn scope_for(
    resource: &str,
    consumer_names: Vec<String>,
) -> Result<GrantScope, AgentFailure> {
    let consumers = consumer_names
        .into_iter()
        .map(GrantConsumer::builtin)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AgentFailure::InvalidInput)?;
    GrantScope::try_new(
        vec![ResourceHandle::try_new(resource).map_err(|_| AgentFailure::InvalidInput)?],
        vec![GrantDataCategory::Derived],
        vec![GrantOperation::Read],
        vec![GrantPurpose::Assistant],
        consumers,
        ProcessingRestriction::LocalOnly,
    )
    .map_err(|_| AgentFailure::InvalidInput)
}

/// The attention source and scope one review commits.
pub fn source_and_scope(
    person_id: PersonId,
    device_id: &str,
    authority: SourceAuthority,
    consumer_names: Vec<String>,
) -> Result<(GrantSourceBinding, GrantScope), AgentFailure> {
    Ok((
        attention_source(person_id, device_id, authority)?,
        scope_for(ATTENTION_RESOURCE, consumer_names)?,
    ))
}

/// The consumers an attention grant may name, canonical and without repeats.
fn reviewed_consumers(consumers: &[String]) -> Result<Vec<String>, AgentFailure> {
    canonical_consumers(consumers, 2, |value| {
        matches!(
            value,
            ATTENTION_ASSISTANT_CONSUMER | ATTENTION_EXPERT_CONSUMER
        )
    })
}

fn reviewed_contacts_consumers(consumers: &[String]) -> Result<Vec<String>, AgentFailure> {
    canonical_consumers(consumers, 2, |value| {
        matches!(value, "assistant" | "contacts.expert")
    })
}

fn reviewed_feasibility_consumers(consumers: &[String]) -> Result<Vec<String>, AgentFailure> {
    if consumers == ["assistant"] {
        Ok(vec!["assistant".into()])
    } else {
        Err(AgentFailure::InvalidInput)
    }
}

fn canonical_consumers(
    consumers: &[String],
    maximum: usize,
    admissible: impl Fn(&str) -> bool,
) -> Result<Vec<String>, AgentFailure> {
    if consumers.is_empty()
        || consumers.len() > maximum
        || consumers.iter().any(|value| !admissible(value.as_str()))
    {
        return Err(AgentFailure::InvalidInput);
    }
    let mut values = consumers.to_vec();
    values.sort();
    values.dedup();
    if values.len() == consumers.len() {
        Ok(values)
    } else {
        Err(AgentFailure::InvalidInput)
    }
}

fn feasibility_scope(consumer_names: Vec<String>) -> Result<GrantScope, AgentFailure> {
    if consumer_names.is_empty()
        || consumer_names.len() > 1
        || consumer_names.iter().any(|consumer| consumer != "assistant")
    {
        return Err(AgentFailure::InvalidInput);
    }
    scope_for(FEASIBILITY_RESOURCE, consumer_names)
}

fn wellbeing_scope() -> Result<GrantScope, AgentFailure> {
    scope_for(
        WELLBEING_RESOURCE,
        vec![ATTENTION_ASSISTANT_CONSUMER.to_owned()],
    )
}

/// The subject the device answers for now, refused if it moved mid-inspection.
async fn inspected_subject(
    inspector: &impl PersonalSubjectInspector,
    person_id: PersonId,
    device_id: &str,
    probe: PersonalSubjectProbe<'_>,
    expected: Option<String>,
    cancellation: Cancellation,
) -> Result<String, AgentFailure> {
    let evidence = inspector
        .inspect(person_id, device_id, probe, expected.clone(), None, cancellation)
        .await?;
    if let Some(expected) = expected
        && (evidence.before != expected || evidence.after != expected)
    {
        return Err(AgentFailure::AccessReviewRequired);
    }
    Ok(evidence.before)
}

/// Apply one change to a personal source's access.
pub async fn apply(
    store: &impl PersonalGrantStore,
    inspector: &impl PersonalSubjectInspector,
    person_id: PersonId,
    request: PersonalAccessConfiguration,
    cancellation: Cancellation,
) -> Result<PersonalAccessOverview, AgentFailure> {
    validate_request(&request)?;
    match request.connector.as_str() {
        FEASIBILITY_CONNECTOR => {
            apply_feasibility(store, inspector, person_id, request, cancellation).await
        }
        WELLBEING_CONNECTOR => {
            apply_wellbeing(store, inspector, person_id, request, cancellation).await
        }
        ATTENTION_CONNECTOR => {
            apply_attention(store, inspector, person_id, request, cancellation).await
        }
        _ => Err(AgentFailure::CapabilityUnavailable),
    }
}

async fn apply_attention(
    store: &impl PersonalGrantStore,
    inspector: &impl PersonalSubjectInspector,
    person_id: PersonId,
    request: PersonalAccessConfiguration,
    cancellation: Cancellation,
) -> Result<PersonalAccessOverview, AgentFailure> {
    let presence = inspector.attention_presence(person_id, &request.device_id);
    let mut grants = store.grants(128).await?;
    let matching: Vec<_> = grants
        .drain(..)
        .filter(|grant| matches_source(grant, person_id, &request.device_id))
        .collect();
    // A live grant outranks a revoked one for the same source.
    let existing = matching
        .iter()
        .find(|grant| grant.state() != GrantState::Revoked)
        .or_else(|| {
            matching
                .iter()
                .find(|grant| grant.state() == GrantState::Revoked)
        })
        .cloned();
    let report = |grant: Option<&DataAccessGrant>, fingerprint: Option<String>| {
        overview(
            person_id,
            ATTENTION_CONNECTOR,
            &request.device_id,
            ATTENTION_CONNECTION.into(),
            grant,
            presence,
            fingerprint,
        )
    };
    match request.change {
        PersonalAccessChange::Inspect => {
            let subject = inspected_subject(
                inspector,
                person_id,
                &request.device_id,
                PersonalSubjectProbe::Attention,
                None,
                cancellation,
            )
            .await?;
            Ok(report(existing.as_ref(), Some(subject)))
        }
        PersonalAccessChange::Review {
            expected_native_subject_fingerprint,
            consumers,
            feasibility_query: None,
            expected_grant_id,
            expected_grant_authority,
        } => {
            let subject = inspected_subject(
                inspector,
                person_id,
                &request.device_id,
                PersonalSubjectProbe::Attention,
                None,
                cancellation,
            )
            .await?;
            if subject != expected_native_subject_fingerprint {
                return Err(AgentFailure::AccessReviewRequired);
            }
            // A source authority is only carried forward when the Person is
            // reviewing the same subject the grant was last bound to.
            let source_authority = match existing.as_ref() {
                Some(grant)
                    if store.reviewed_subject(grant.id()).await?
                        == expected_native_subject_fingerprint =>
                {
                    grant.source().source_authority()
                }
                Some(_) | None => SourceAuthority::new(),
            };
            let (source, scope) = source_and_scope(
                person_id,
                &request.device_id,
                source_authority,
                reviewed_consumers(&consumers)?,
            )?;
            let expected = expected_grant(expected_grant_id, expected_grant_authority)?;
            let grant = store
                .review_grant(source, scope, &expected_native_subject_fingerprint, expected)
                .await?;
            Ok(report(
                Some(&grant),
                Some(expected_native_subject_fingerprint),
            ))
        }
        PersonalAccessChange::Review {
            feasibility_query: Some(_),
            ..
        } => Err(AgentFailure::InvalidInput),
        PersonalAccessChange::SetEnabled { enabled } => {
            let grant = existing.ok_or(AgentFailure::AccessReviewRequired)?;
            if !enabled {
                let grant = store.pause_grant(grant.id(), grant.authority()).await?;
                return Ok(report(Some(&grant), None));
            }
            let fingerprint = store.reviewed_subject(grant.id()).await?;
            let subject = inspected_subject(
                inspector,
                person_id,
                &request.device_id,
                PersonalSubjectProbe::Attention,
                None,
                cancellation,
            )
            .await?;
            if subject != fingerprint {
                return Err(AgentFailure::AccessReviewRequired);
            }
            let (source, scope) = source_and_scope(
                person_id,
                &request.device_id,
                grant.source().source_authority(),
                granted_consumers(Some(&grant)),
            )?;
            let grant = store
                .review_grant(
                    source,
                    scope,
                    &fingerprint,
                    Some((grant.id(), grant.authority())),
                )
                .await?;
            Ok(report(Some(&grant), Some(fingerprint)))
        }
    }
}

async fn apply_feasibility(
    store: &impl PersonalGrantStore,
    inspector: &impl PersonalSubjectInspector,
    person_id: PersonId,
    request: PersonalAccessConfiguration,
    cancellation: Cancellation,
) -> Result<PersonalAccessOverview, AgentFailure> {
    let source = feasibility_source(person_id, &request.device_id, SourceAuthority::new())?;
    let grants = store.grants(128).await?;
    let existing = grants
        .iter()
        .find(|grant| same_source(grant, &source) && grant.state() != GrantState::Revoked)
        .cloned();
    let report = |grant: Option<&DataAccessGrant>, fingerprint: Option<String>| {
        overview(
            person_id,
            FEASIBILITY_CONNECTOR,
            &request.device_id,
            FEASIBILITY_CONNECTION.into(),
            grant,
            None,
            fingerprint,
        )
    };
    match request.change {
        PersonalAccessChange::Inspect => Ok(report(existing.as_ref(), None)),
        PersonalAccessChange::Review {
            expected_native_subject_fingerprint,
            consumers,
            feasibility_query: Some(query),
            expected_grant_id,
            expected_grant_authority,
        } => {
            query.validate()?;
            let consumers = reviewed_feasibility_consumers(&consumers)?;
            inspected_subject(
                inspector,
                person_id,
                &request.device_id,
                PersonalSubjectProbe::Feasibility { query: &query },
                Some(expected_native_subject_fingerprint.clone()),
                cancellation,
            )
            .await?;
            let expected = expected_grant(expected_grant_id, expected_grant_authority)?;
            let authority = existing
                .as_ref()
                .map(|grant| grant.source().source_authority())
                .unwrap_or_else(SourceAuthority::new);
            let grant = store
                .review_grant_with_feasibility_query(
                    feasibility_source(person_id, &request.device_id, authority)?,
                    feasibility_scope(consumers)?,
                    &expected_native_subject_fingerprint,
                    expected,
                    query,
                )
                .await?;
            Ok(report(
                Some(&grant),
                Some(expected_native_subject_fingerprint),
            ))
        }
        PersonalAccessChange::Review { .. } => Err(AgentFailure::InvalidInput),
        PersonalAccessChange::SetEnabled { enabled } => {
            let grant = existing.ok_or(AgentFailure::AccessReviewRequired)?;
            if !enabled {
                let grant = store.pause_grant(grant.id(), grant.authority()).await?;
                return Ok(report(Some(&grant), None));
            }
            // Re-enabling re-runs the exact query the Person reviewed.
            let query = store.feasibility_query(grant.id()).await?;
            let fingerprint = store.reviewed_subject(grant.id()).await?;
            inspected_subject(
                inspector,
                person_id,
                &request.device_id,
                PersonalSubjectProbe::Feasibility { query: &query },
                Some(fingerprint.clone()),
                cancellation,
            )
            .await?;
            let grant = store
                .review_grant_with_feasibility_query(
                    feasibility_source(
                        person_id,
                        &request.device_id,
                        grant.source().source_authority(),
                    )?,
                    feasibility_scope(granted_consumers(Some(&grant)))?,
                    &fingerprint,
                    Some((grant.id(), grant.authority())),
                    query,
                )
                .await?;
            Ok(report(Some(&grant), Some(fingerprint)))
        }
    }
}

async fn apply_wellbeing(
    store: &impl PersonalGrantStore,
    inspector: &impl PersonalSubjectInspector,
    person_id: PersonId,
    request: PersonalAccessConfiguration,
    cancellation: Cancellation,
) -> Result<PersonalAccessOverview, AgentFailure> {
    let source = wellbeing_source(person_id, &request.device_id, SourceAuthority::new())?;
    let grants = store.grants(128).await?;
    let mut live = grants
        .iter()
        .filter(|grant| same_source(grant, &source) && grant.state() != GrantState::Revoked);
    // Two live grants for one source is an ambiguity only the Person can settle.
    let existing = match (live.next(), live.next()) {
        (Some(_), Some(_)) => return Err(AgentFailure::AccessReviewRequired),
        (Some(grant), None) => Some(grant.clone()),
        (None, _) => None,
    };
    let report = |grant: Option<&DataAccessGrant>, fingerprint: Option<String>| {
        overview(
            person_id,
            WELLBEING_CONNECTOR,
            &request.device_id,
            WELLBEING_CONNECTION.into(),
            grant,
            None,
            fingerprint,
        )
    };
    match request.change {
        PersonalAccessChange::Inspect => Ok(report(existing.as_ref(), None)),
        PersonalAccessChange::Review {
            expected_native_subject_fingerprint,
            consumers,
            feasibility_query: None,
            expected_grant_id,
            expected_grant_authority,
        } => {
            let expected = expected_grant(expected_grant_id, expected_grant_authority)?;
            if consumers != [ATTENTION_ASSISTANT_CONSUMER] {
                return Err(AgentFailure::InvalidInput);
            }
            inspected_subject(
                inspector,
                person_id,
                &request.device_id,
                PersonalSubjectProbe::Wellbeing,
                Some(expected_native_subject_fingerprint.clone()),
                cancellation,
            )
            .await?;
            let authority = existing
                .as_ref()
                .map(|grant| grant.source().source_authority())
                .unwrap_or_else(SourceAuthority::new);
            let grant = store
                .review_grant(
                    wellbeing_source(person_id, &request.device_id, authority)?,
                    wellbeing_scope()?,
                    &expected_native_subject_fingerprint,
                    expected,
                )
                .await?;
            Ok(report(
                Some(&grant),
                Some(expected_native_subject_fingerprint),
            ))
        }
        PersonalAccessChange::Review { .. } => Err(AgentFailure::InvalidInput),
        PersonalAccessChange::SetEnabled { enabled } => {
            let grant = existing.ok_or(AgentFailure::AccessReviewRequired)?;
            if !enabled {
                let grant = store.pause_grant(grant.id(), grant.authority()).await?;
                return Ok(report(Some(&grant), None));
            }
            let fingerprint = store.reviewed_subject(grant.id()).await?;
            inspected_subject(
                inspector,
                person_id,
                &request.device_id,
                PersonalSubjectProbe::Wellbeing,
                Some(fingerprint.clone()),
                cancellation,
            )
            .await?;
            let grant = store
                .review_grant(
                    wellbeing_source(
                        person_id,
                        &request.device_id,
                        grant.source().source_authority(),
                    )?,
                    wellbeing_scope()?,
                    &fingerprint,
                    Some((grant.id(), grant.authority())),
                )
                .await?;
            Ok(report(Some(&grant), Some(fingerprint)))
        }
    }
}

/// The handles a contacts inspection may name.
fn admitted_handles(mut selected_handles: Vec<String>) -> Result<Vec<String>, AgentFailure> {
    if selected_handles.is_empty() || selected_handles.len() > 64 {
        return Err(AgentFailure::InvalidInput);
    }
    selected_handles.sort();
    if selected_handles.windows(2).any(|pair| pair[0] >= pair[1])
        || selected_handles
            .iter()
            .any(|value| value.is_empty() || value.chars().any(char::is_whitespace))
    {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(selected_handles)
}

/// Apply one change to what may be read from the Person's contacts.
pub async fn apply_contacts(
    store: &impl PersonalGrantStore,
    inspector: &impl PersonalSubjectInspector,
    person_id: PersonId,
    request: ContactsAccessConfiguration,
    cancellation: Cancellation,
) -> Result<PersonalAccessOverview, AgentFailure> {
    validate_contacts_request(&request)?;
    let source = contacts_source(
        person_id,
        &request.device_id,
        &request.connector,
        SourceAuthority::new(),
    )?;
    let grants = store.grants(128).await?;
    let existing = grants
        .iter()
        .filter(|grant| {
            grant.source().person_id() == person_id
                && grant.source().connector() == source.connector()
        })
        .find(|grant| {
            grant.source().connection_id() == source.connection_id()
                && grant.source().execution_owner() == source.execution_owner()
                && grant.state() != GrantState::Revoked
        })
        .cloned();
    let report = |grant: Option<&DataAccessGrant>, fingerprint: Option<String>| {
        overview(
            person_id,
            &request.connector,
            &request.device_id,
            contacts_connection(&request.connector),
            grant,
            None,
            fingerprint,
        )
    };
    match request.change {
        ContactsAccessChange::Inspect { selected_handles } => {
            let subject = inspected_subject(
                inspector,
                person_id,
                &request.device_id,
                PersonalSubjectProbe::People {
                    selected_handles: admitted_handles(selected_handles)?,
                },
                None,
                cancellation,
            )
            .await?;
            Ok(report(existing.as_ref(), Some(subject)))
        }
        ContactsAccessChange::Review {
            selected_handles,
            expected_native_subject_fingerprint,
            consumers,
            expected_grant_id,
            expected_grant_authority,
        } => {
            let selected_handles = admitted_handles(selected_handles)?;
            inspected_subject(
                inspector,
                person_id,
                &request.device_id,
                PersonalSubjectProbe::People {
                    selected_handles: selected_handles.clone(),
                },
                Some(expected_native_subject_fingerprint.clone()),
                cancellation,
            )
            .await?;
            let expected = expected_grant(expected_grant_id, expected_grant_authority)?;
            let consumers = reviewed_contacts_consumers(&consumers)?;
            let authority = existing
                .as_ref()
                .map(|grant| grant.source().source_authority())
                .unwrap_or_else(SourceAuthority::new);
            let grant = store
                .review_grant_with_selection(
                    contacts_source(person_id, &request.device_id, &request.connector, authority)?,
                    scope_for(PEOPLE_RESOURCE, consumers)?,
                    &expected_native_subject_fingerprint,
                    expected,
                    &selected_handles,
                )
                .await?;
            Ok(report(
                Some(&grant),
                Some(expected_native_subject_fingerprint),
            ))
        }
        ContactsAccessChange::SetEnabled { enabled } => {
            let grant = existing.ok_or(AgentFailure::AccessReviewRequired)?;
            if !enabled {
                let grant = store.pause_grant(grant.id(), grant.authority()).await?;
                return Ok(report(Some(&grant), None));
            }
            // Re-enabling re-inspects exactly the handles the Person selected.
            let selected_handles = store.selected_handles(grant.id()).await?;
            if selected_handles.is_empty() {
                return Err(AgentFailure::AccessReviewRequired);
            }
            let fingerprint = store.reviewed_subject(grant.id()).await?;
            inspected_subject(
                inspector,
                person_id,
                &request.device_id,
                PersonalSubjectProbe::People {
                    selected_handles: selected_handles.clone(),
                },
                Some(fingerprint.clone()),
                cancellation,
            )
            .await?;
            let grant = store
                .review_grant_with_selection(
                    contacts_source(
                        person_id,
                        &request.device_id,
                        &request.connector,
                        grant.source().source_authority(),
                    )?,
                    scope_for(PEOPLE_RESOURCE, granted_consumers(Some(&grant)))?,
                    &fingerprint,
                    Some((grant.id(), grant.authority())),
                    &selected_handles,
                )
                .await?;
            Ok(report(Some(&grant), Some(fingerprint)))
        }
    }
}

/// Which consumer may read attention, and under which name.
pub fn attention_consumer(value: &str) -> Result<GrantConsumer, AgentFailure> {
    if !matches!(
        value,
        ATTENTION_ASSISTANT_CONSUMER | ATTENTION_EXPERT_CONSUMER
    ) {
        return Err(AgentFailure::PolicyDenied);
    }
    GrantConsumer::builtin(value).map_err(|_| AgentFailure::InvalidInput)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review_consumers_are_finite_and_canonical() {
        assert_eq!(
            reviewed_consumers(&[
                ATTENTION_EXPERT_CONSUMER.into(),
                ATTENTION_ASSISTANT_CONSUMER.into()
            ])
            .unwrap(),
            vec![ATTENTION_ASSISTANT_CONSUMER, ATTENTION_EXPERT_CONSUMER]
        );
        assert_eq!(
            reviewed_consumers(&[
                ATTENTION_ASSISTANT_CONSUMER.into(),
                ATTENTION_ASSISTANT_CONSUMER.into()
            ]),
            Err(AgentFailure::InvalidInput)
        );
        assert_eq!(
            reviewed_consumers(&["unknown".into()]),
            Err(AgentFailure::InvalidInput)
        );
    }
}
