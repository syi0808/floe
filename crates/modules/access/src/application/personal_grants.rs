//! The explicit contextual Feasibility review.
//!
//! Standing personal sources are reviewed by App against current Connections
//! state; only query-bound Feasibility retains its own contextual grant review.

use floe_context_contract::{
    GrantAuthority, GrantConsumer, GrantDataCategory, GrantId, GrantOperation, GrantPurpose,
    GrantScope, GrantSourceBinding, ProcessingRestriction, ResourceHandle, SourceAuthority,
};
use floe_execution::Cancellation;
use floe_kernel::{AgentFailure, PersonId};
use uuid::Uuid;

use crate::application::personal_read::FeasibilityGrantQuery;
use crate::application::personal_sources::{
    FEASIBILITY_CONNECTION, FEASIBILITY_CONNECTOR, FEASIBILITY_RESOURCE, feasibility_source,
};
use crate::data_access_grant::{DataAccessGrant, GrantState};
use crate::ports::personal_grants::{
    PersonalGrantStore, PersonalSubjectInspector, PersonalSubjectProbe,
};

pub const ATTENTION_ASSISTANT_CONSUMER: &str = "assistant";

/// What the Person is asking to change about one source.
#[derive(Clone, Debug, PartialEq)]
pub enum PersonalAccessChange {
    /// Show what is granted now, and what subject the device answers for.
    Inspect,
    Review {
        expected_native_subject_fingerprint: String,
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
    pub consumers: Vec<String>,
    pub change: PersonalAccessChange,
}

/// What the Person is asking to change about their contacts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContactsAccessChange {
    Inspect {
        selected_handles: Vec<String>,
    },
    Review {
        selected_handles: Vec<String>,
        expected_native_subject_fingerprint: String,
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
    pub consumers: Vec<String>,
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

async fn overview(
    store: &impl PersonalGrantStore,
    person_id: PersonId,
    connector: &str,
    device_id: &str,
    connection_id: String,
    grant: Option<DataAccessGrant>,
    presence: Option<Uuid>,
    native_subject_fingerprint: Option<String>,
) -> Result<PersonalAccessOverview, AgentFailure> {
    let source_authority = match grant.as_ref() {
        Some(grant) => Some(store.feasibility_review(grant.id()).await?.source_authority),
        None => None,
    };
    Ok(PersonalAccessOverview {
        person_id,
        connector: connector.to_owned(),
        device_id: device_id.to_owned(),
        connection_id,
        source_authority,
        grant_id: grant.as_ref().map(DataAccessGrant::id),
        grant_authority: grant.as_ref().map(DataAccessGrant::authority),
        state: state_of(grant.as_ref()),
        review_required: grant.as_ref().is_none_or(DataAccessGrant::review_required),
        presence_available: presence.is_some(),
        consumers: granted_consumers(grant.as_ref()),
        native_subject_fingerprint,
        process_incarnation: presence,
    })
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
    if request.connector != FEASIBILITY_CONNECTOR || !valid_device(&request.device_id) {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(())
}

fn same_source(grant: &DataAccessGrant, source: &GrantSourceBinding) -> bool {
    grant.source() == source
}

fn scope_for(resource: &str, consumer_names: Vec<String>) -> Result<GrantScope, AgentFailure> {
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

fn valid_consumer(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
        && GrantConsumer::builtin(value).is_ok()
}

fn reviewed_feasibility_consumers(consumers: &[String]) -> Result<Vec<String>, AgentFailure> {
    if consumers == ["assistant"] {
        Ok(vec!["assistant".into()])
    } else {
        Err(AgentFailure::InvalidInput)
    }
}

fn feasibility_scope(consumer_names: Vec<String>) -> Result<GrantScope, AgentFailure> {
    if consumer_names.is_empty()
        || consumer_names.len() > 1
        || consumer_names
            .iter()
            .any(|consumer| consumer != "assistant")
    {
        return Err(AgentFailure::InvalidInput);
    }
    scope_for(FEASIBILITY_RESOURCE, consumer_names)
}

async fn inspected_subject(
    inspector: &impl PersonalSubjectInspector,
    person_id: PersonId,
    device_id: &str,
    probe: PersonalSubjectProbe<'_>,
    expected: Option<String>,
    cancellation: Cancellation,
) -> Result<String, AgentFailure> {
    let evidence = inspector
        .inspect(
            person_id,
            device_id,
            probe,
            expected.clone(),
            None,
            cancellation,
        )
        .await?;
    if let Some(expected) = expected
        && (evidence.before != expected || evidence.after != expected)
    {
        return Err(AgentFailure::AccessReviewRequired);
    }
    Ok(evidence.before)
}

/// Apply one contextual Feasibility access change.
pub async fn apply_feasibility_access(
    store: &impl PersonalGrantStore,
    inspector: &impl PersonalSubjectInspector,
    person_id: PersonId,
    request: PersonalAccessConfiguration,
    cancellation: Cancellation,
) -> Result<PersonalAccessOverview, AgentFailure> {
    validate_request(&request)?;
    apply_feasibility(store, inspector, person_id, request, cancellation).await
}

async fn apply_feasibility(
    store: &impl PersonalGrantStore,
    inspector: &impl PersonalSubjectInspector,
    person_id: PersonId,
    request: PersonalAccessConfiguration,
    cancellation: Cancellation,
) -> Result<PersonalAccessOverview, AgentFailure> {
    let source = feasibility_source(person_id, &request.device_id)?;
    let grants = store.grants(128).await?;
    let existing = grants
        .iter()
        .find(|grant| same_source(grant, &source) && grant.state() != GrantState::Revoked)
        .cloned();
    let report = |grant: Option<&DataAccessGrant>, fingerprint: Option<String>| {
        overview(
            store,
            person_id,
            FEASIBILITY_CONNECTOR,
            &request.device_id,
            FEASIBILITY_CONNECTION.into(),
            grant.cloned(),
            None,
            fingerprint,
        )
    };
    match request.change {
        PersonalAccessChange::Inspect => report(existing.as_ref(), None).await,
        PersonalAccessChange::Review {
            expected_native_subject_fingerprint,
            feasibility_query: Some(query),
            expected_grant_id,
            expected_grant_authority,
        } => {
            query.validate()?;
            let consumers = reviewed_feasibility_consumers(&request.consumers)?;
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
            let grant = store
                .review_grant_with_feasibility_query(
                    feasibility_source(person_id, &request.device_id)?,
                    feasibility_scope(consumers)?,
                    &expected_native_subject_fingerprint,
                    expected,
                    query,
                )
                .await?;
            report(Some(&grant), Some(expected_native_subject_fingerprint)).await
        }
        PersonalAccessChange::Review { .. } => Err(AgentFailure::InvalidInput),
        PersonalAccessChange::SetEnabled { enabled } => {
            let grant = existing.ok_or(AgentFailure::AccessReviewRequired)?;
            if !enabled {
                let grant = store.pause_grant(grant.id(), grant.authority()).await?;
                return report(Some(&grant), None).await;
            }
            // Re-enabling re-runs the exact query the Person reviewed.
            let review = store.feasibility_review(grant.id()).await?;
            let query = review.query;
            let fingerprint = review.reviewed_subject;
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
                    feasibility_source(person_id, &request.device_id)?,
                    feasibility_scope(granted_consumers(Some(&grant)))?,
                    &fingerprint,
                    Some((grant.id(), grant.authority())),
                    query,
                )
                .await?;
            report(Some(&grant), Some(fingerprint)).await
        }
    }
}

pub fn attention_consumer(value: &str) -> Result<GrantConsumer, AgentFailure> {
    if !valid_consumer(value) {
        return Err(AgentFailure::PolicyDenied);
    }
    GrantConsumer::builtin(value).map_err(|_| AgentFailure::InvalidInput)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feasibility_review_admits_only_assistant() {
        assert_eq!(
            reviewed_feasibility_consumers(&["assistant".into()]),
            Ok(vec!["assistant".into()])
        );
        assert_eq!(
            reviewed_feasibility_consumers(&["floe.builtin.focus-attention".into()]),
            Err(AgentFailure::InvalidInput)
        );
        assert_eq!(
            attention_consumer("bad consumer"),
            Err(AgentFailure::PolicyDenied)
        );
    }
}
