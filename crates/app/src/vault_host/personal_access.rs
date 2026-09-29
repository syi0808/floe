use floe_access::{
    ContactsAccessChange, ContactsAccessConfiguration, DataAccessGrant, GrantAuthority, GrantId,
    GrantScope, GrantSourceBinding, GrantState, PersonalAccessChange, PersonalAccessConfiguration,
    PersonalAccessOverview, PersonalAccessState, PersonalSubjectInspector, PersonalSubjectProbe,
};
use floe_agent_contract::AgentFailure;
use floe_connections::{SourceConnection, SourceServiceError};
use floe_context_contract::{
    ConnectionId, ConnectorId, ExecutionOwnerId, ResourceHandle, connection_view_resource,
};
use floe_execution::Cancellation;
use floe_kernel::PersonId;
use floe_vault::{AccessGrantActivation, EncryptedAgentVault, VaultKeyProvider};

use crate::FloeCore;
use crate::personal_source_spec::PersonalSourceSpec;
use crate::{
    ConnectionObserveExpectation, ConnectionObserveMember, ConnectionObserveOperation,
    ConnectionObserveOverview, ConnectionObserveReviewedMember, ConnectionObserveStatus,
};

enum StandingChange {
    Inspect {
        handles: Vec<String>,
    },
    Review {
        handles: Vec<String>,
        fingerprint: String,
        expected: Option<(GrantId, GrantAuthority)>,
    },
    SetEnabled {
        enabled: bool,
    },
}

fn grant_expectation(
    id: Option<GrantId>,
    authority: Option<GrantAuthority>,
) -> Result<Option<(GrantId, GrantAuthority)>, AgentFailure> {
    match (id, authority) {
        (Some(id), Some(authority)) if id.is_valid() && authority.is_valid() => {
            Ok(Some((id, authority)))
        }
        (None, None) => Ok(None),
        _ => Err(AgentFailure::InvalidInput),
    }
}

fn source_failure(error: SourceServiceError) -> AgentFailure {
    match error {
        SourceServiceError::NotFound => AgentFailure::AccessReviewRequired,
        SourceServiceError::Invalid(floe_connections::SourceConnectionError::Conflict) => {
            AgentFailure::Conflict
        }
        SourceServiceError::Invalid(floe_connections::SourceConnectionError::Disconnected) => {
            AgentFailure::AccessReviewRequired
        }
        SourceServiceError::Invalid(_) => AgentFailure::InvalidInput,
        SourceServiceError::Repository(floe_connections::SourceRepositoryError::Conflict) => {
            AgentFailure::Conflict
        }
        SourceServiceError::Repository(_) => AgentFailure::StorageUnavailable,
    }
}

fn binding(source: &SourceConnection) -> Result<GrantSourceBinding, AgentFailure> {
    GrantSourceBinding::try_new(
        source.person_id(),
        source.connection_id().clone(),
        source.connector_id().clone(),
        source.execution_owner_id().clone(),
    )
    .map_err(|_| AgentFailure::InvalidInput)
}

fn permission_resource(
    spec: PersonalSourceSpec,
    source: &SourceConnection,
) -> Result<ResourceHandle, AgentFailure> {
    connection_view_resource(spec.view, source.connection_id())
        .map_err(|_| AgentFailure::InvalidInput)
}

fn scope(spec: PersonalSourceSpec, source: &SourceConnection) -> Result<GrantScope, AgentFailure> {
    let policy = crate::first_party_observe::personal_policy(spec.connector)?;
    GrantScope::try_new(
        vec![permission_resource(spec, source)?],
        policy.categories,
        vec![policy.operation],
        vec![policy.purpose],
        policy.consumers,
        policy.processing,
    )
    .map_err(|_| AgentFailure::InvalidInput)
}

async fn current_grant<Keys: VaultKeyProvider>(
    vault: &EncryptedAgentVault<Keys>,
    spec: PersonalSourceSpec,
    source: Option<&SourceConnection>,
) -> Result<Option<DataAccessGrant>, AgentFailure> {
    let Some(source) = source else {
        return Ok(None);
    };
    vault
        .data_access_grant_for_source_resource(
            &binding(source)?,
            &permission_resource(spec, source)?,
        )
        .await
}

fn overview(
    person_id: PersonId,
    device_id: &str,
    spec: PersonalSourceSpec,
    source: Option<&SourceConnection>,
    grant: Option<&DataAccessGrant>,
    fingerprint: Option<String>,
    presence: Option<uuid::Uuid>,
) -> PersonalAccessOverview {
    PersonalAccessOverview {
        person_id,
        connector: spec.connector.into(),
        device_id: device_id.into(),
        connection_id: spec.connection.into(),
        source_authority: source.map(SourceConnection::source_authority),
        grant_id: grant.map(DataAccessGrant::id),
        grant_authority: grant.map(DataAccessGrant::authority),
        state: match grant.map(DataAccessGrant::state) {
            None => PersonalAccessState::NeedsReview,
            Some(GrantState::Active) => PersonalAccessState::Active,
            Some(GrantState::Paused) => PersonalAccessState::Paused,
            Some(GrantState::Revoked) => PersonalAccessState::Revoked,
        },
        review_required: grant.is_none_or(DataAccessGrant::review_required),
        presence_available: presence.is_some(),
        consumers: grant
            .map(|grant| {
                grant
                    .scope()
                    .consumers()
                    .iter()
                    .map(|consumer| consumer.identifier().to_owned())
                    .collect()
            })
            .unwrap_or_default(),
        native_subject_fingerprint: fingerprint,
        process_incarnation: presence,
    }
}

async fn inspect_subject(
    inspector: &impl PersonalSubjectInspector,
    person_id: PersonId,
    device_id: &str,
    spec: PersonalSourceSpec,
    handles: Vec<String>,
    expected: Option<&str>,
    cancellation: Cancellation,
) -> Result<String, AgentFailure> {
    let probe = match spec.connector {
        "contacts.apple" | "contacts.android" => PersonalSubjectProbe::People {
            selected_handles: handles,
        },
        "attention.macos" => PersonalSubjectProbe::Attention,
        "health.apple" => PersonalSubjectProbe::Wellbeing,
        _ => return Err(AgentFailure::InvalidInput),
    };
    let evidence = inspector
        .inspect(
            person_id,
            device_id,
            probe,
            expected.map(str::to_owned),
            None,
            cancellation,
        )
        .await?;
    if evidence.before != evidence.after
        || expected.is_some_and(|expected| evidence.before != expected)
        || !floe_access::valid_subject_fingerprint(&evidence.before)
    {
        return Err(AgentFailure::AccessReviewRequired);
    }
    Ok(evidence.before)
}

async fn apply_standing<Keys: VaultKeyProvider>(
    core: &FloeCore,
    vault: &EncryptedAgentVault<Keys>,
    inspector: &impl PersonalSubjectInspector,
    person_id: PersonId,
    device_id: &str,
    spec: PersonalSourceSpec,
    change: StandingChange,
    cancellation: Cancellation,
) -> Result<PersonalAccessOverview, AgentFailure> {
    if vault.person_id() != person_id {
        return Err(AgentFailure::CapabilityDenied);
    }
    let owner = spec.execution_owner(device_id)?;
    let connection_id =
        ConnectionId::try_new(spec.connection).map_err(|_| AgentFailure::InvalidInput)?;
    let service = core.source_service();
    let source = service
        .load(person_id, &connection_id)
        .await
        .map_err(source_failure)?;
    if let Some(source) = source.as_ref() {
        if source.person_id() != person_id
            || source.connector_id().as_str() != spec.connector
            || source.connection_id() != &connection_id
            || source.execution_owner_id().as_str() != owner
        {
            return Err(AgentFailure::AccessReviewRequired);
        }
    }
    let grant = current_grant(vault, spec, source.as_ref()).await?;
    let presence = if spec.connector == "attention.macos" {
        inspector.attention_presence(person_id, device_id)
    } else {
        None
    };
    match change {
        StandingChange::Inspect { handles } => {
            let resources = if spec.mode == floe_connections::ResourceMode::Selected {
                spec.resources(handles)?
            } else {
                spec.resources(Vec::new())?
            };
            let fingerprint = inspect_subject(
                inspector,
                person_id,
                device_id,
                spec,
                resources
                    .iter()
                    .map(|resource| resource.handle().as_str().to_owned())
                    .collect(),
                None,
                cancellation,
            )
            .await?;
            Ok(overview(
                person_id,
                device_id,
                spec,
                source.as_ref(),
                grant.as_ref(),
                Some(fingerprint),
                presence,
            ))
        }
        StandingChange::Review {
            handles,
            fingerprint,
            expected,
        } => {
            let resources = spec.resources(handles)?;
            inspect_subject(
                inspector,
                person_id,
                device_id,
                spec,
                resources
                    .iter()
                    .map(|resource| resource.handle().as_str().to_owned())
                    .collect(),
                Some(&fingerprint),
                cancellation,
            )
            .await?;
            let source = match source {
                Some(current) => service
                    .configure_reviewed_native(
                        person_id,
                        &connection_id,
                        current.revision(),
                        spec.mode,
                        resources,
                        fingerprint.clone(),
                    )
                    .await
                    .map_err(source_failure)?,
                None => service
                    .establish_reviewed_native(
                        person_id,
                        ConnectorId::try_new(spec.connector)
                            .map_err(|_| AgentFailure::InvalidInput)?,
                        connection_id,
                        ExecutionOwnerId::try_new(owner).map_err(|_| AgentFailure::InvalidInput)?,
                        spec.mode,
                        resources,
                        fingerprint.clone(),
                    )
                    .await
                    .map_err(source_failure)?,
            };
            spec.validate_connection(&source, device_id)?;
            let expected_id = expected.map(|(id, _)| id).unwrap_or_else(GrantId::new);
            let [grant] = vault
                .activate_access_grants(vec![AccessGrantActivation {
                    grant_id: expected_id,
                    expected: expected.map(|(_, authority)| authority),
                    source: binding(&source)?,
                    scope: scope(spec, &source)?,
                }])
                .await?
                .try_into()
                .map_err(|_| AgentFailure::StorageUnavailable)?;
            Ok(overview(
                person_id,
                device_id,
                spec,
                Some(&source),
                Some(&grant),
                Some(fingerprint),
                presence,
            ))
        }
        StandingChange::SetEnabled { enabled } => {
            let source = source.ok_or(AgentFailure::AccessReviewRequired)?;
            spec.validate_connection(&source, device_id)?;
            let grant = grant.ok_or(AgentFailure::AccessReviewRequired)?;
            if !enabled {
                let grant = vault
                    .pause_data_access_grant(grant.id(), grant.authority())
                    .await?;
                return Ok(overview(
                    person_id,
                    device_id,
                    spec,
                    Some(&source),
                    Some(&grant),
                    source.native_subject_fingerprint().map(str::to_owned),
                    presence,
                ));
            }
            let fingerprint = source
                .native_subject_fingerprint()
                .ok_or(AgentFailure::AccessReviewRequired)?;
            inspect_subject(
                inspector,
                person_id,
                device_id,
                spec,
                source
                    .resources()
                    .iter()
                    .map(|resource| resource.handle().as_str().to_owned())
                    .collect(),
                Some(fingerprint),
                cancellation,
            )
            .await?;
            let reloaded = service
                .load(person_id, source.connection_id())
                .await
                .map_err(source_failure)?
                .ok_or(AgentFailure::AccessReviewRequired)?;
            if reloaded.source_authority() != source.source_authority()
                || reloaded.resources() != source.resources()
                || reloaded.native_subject_fingerprint() != source.native_subject_fingerprint()
            {
                return Err(AgentFailure::AccessReviewRequired);
            }
            let [grant] = vault
                .activate_access_grants(vec![AccessGrantActivation {
                    grant_id: grant.id(),
                    expected: Some(grant.authority()),
                    source: binding(&source)?,
                    scope: scope(spec, &source)?,
                }])
                .await?
                .try_into()
                .map_err(|_| AgentFailure::StorageUnavailable)?;
            Ok(overview(
                person_id,
                device_id,
                spec,
                Some(&source),
                Some(&grant),
                Some(fingerprint.into()),
                presence,
            ))
        }
    }
}

fn observe_overview(
    spec: PersonalSourceSpec,
    source: Option<&SourceConnection>,
    grant: Option<&DataAccessGrant>,
) -> ConnectionObserveOverview {
    let members = grant
        .map(|grant| {
            vec![ConnectionObserveMember {
                view_id: spec.view.to_owned(),
                state: grant.state(),
                review_required: grant.review_required(),
            }]
        })
        .unwrap_or_default();
    let mut overview = ConnectionObserveOverview::from_members(
        spec.connector,
        spec.connection,
        source
            .map(|source| {
                source
                    .resources()
                    .iter()
                    .map(|resource| resource.handle().as_str().to_owned())
                    .collect()
            })
            .unwrap_or_default(),
        &[spec.view],
        members,
    );
    if source.is_none() {
        overview.status = ConnectionObserveStatus::NeedsSystemAccess;
    } else if source.is_some_and(|source| !source.is_serving()) {
        overview.status = ConnectionObserveStatus::Unavailable;
        overview.enabled = false;
    }
    overview
}

async fn observe_source(
    core: &FloeCore,
    person_id: PersonId,
    device_id: &str,
    spec: PersonalSourceSpec,
) -> Result<Option<SourceConnection>, AgentFailure> {
    let owner = spec.execution_owner(device_id)?;
    let connection_id =
        ConnectionId::try_new(spec.connection).map_err(|_| AgentFailure::InvalidInput)?;
    let source = core
        .source_service()
        .load(person_id, &connection_id)
        .await
        .map_err(source_failure)?;
    if source.as_ref().is_some_and(|source| {
        source.person_id() != person_id
            || source.connector_id().as_str() != spec.connector
            || source.execution_owner_id().as_str() != owner
    }) {
        return Err(AgentFailure::CapabilityDenied);
    }
    Ok(source)
}

async fn review_observe<Keys: VaultKeyProvider>(
    core: &FloeCore,
    vault: &EncryptedAgentVault<Keys>,
    inspector: &impl PersonalSubjectInspector,
    person_id: PersonId,
    device_id: &str,
    spec: PersonalSourceSpec,
    cancellation: Cancellation,
) -> Result<ConnectionObserveExpectation, AgentFailure> {
    let source = observe_source(core, person_id, device_id, spec)
        .await?
        .ok_or(AgentFailure::AccessReviewRequired)?;
    spec.validate_connection(&source, device_id)?;
    let fingerprint = source
        .native_subject_fingerprint()
        .ok_or(AgentFailure::AccessReviewRequired)?;
    inspect_subject(
        inspector,
        person_id,
        device_id,
        spec,
        source
            .resources()
            .iter()
            .map(|resource| resource.handle().as_str().to_owned())
            .collect(),
        Some(fingerprint),
        cancellation,
    )
    .await?;
    let current = observe_source(core, person_id, device_id, spec)
        .await?
        .ok_or(AgentFailure::AccessReviewRequired)?;
    if current != source {
        return Err(AgentFailure::StaleContext);
    }
    let grant = current_grant(vault, spec, Some(&current)).await?;
    let policy = crate::first_party_observe::personal_policy(spec.connector)?;
    let expectation = ConnectionObserveExpectation {
        connector_id: spec.connector.to_owned(),
        connection_id: spec.connection.to_owned(),
        source_authority: current.source_authority(),
        connection_revision: Some(current.revision()),
        native_subject: Some(fingerprint.to_owned()),
        producer_fingerprint: None,
        members: vec![ConnectionObserveReviewedMember {
            view_id: spec.view.to_owned(),
            policy_digest: crate::first_party_observe::policy_digest(&policy)?,
            resource: permission_resource(spec, &current)?.as_str().to_owned(),
            expected_grant_id: grant.as_ref().map(DataAccessGrant::id),
            expected_grant_authority: grant.as_ref().map(DataAccessGrant::authority),
        }],
    };
    expectation.validate()?;
    Ok(expectation)
}

pub(super) async fn apply_connection_observe<Keys: VaultKeyProvider>(
    core: &FloeCore,
    vault: &EncryptedAgentVault<Keys>,
    inspector: &impl PersonalSubjectInspector,
    person_id: PersonId,
    device_id: &str,
    operation: &ConnectionObserveOperation,
    cancellation: Cancellation,
) -> Result<
    (
        Option<ConnectionObserveOverview>,
        Option<ConnectionObserveExpectation>,
    ),
    AgentFailure,
> {
    operation.validate()?;
    if vault.person_id() != person_id {
        return Err(AgentFailure::CapabilityDenied);
    }
    let (connector_id, connection_id) = operation.identity();
    let spec = PersonalSourceSpec::for_connector(connector_id)?;
    if connection_id != spec.connection {
        return Err(AgentFailure::InvalidInput);
    }
    match operation {
        ConnectionObserveOperation::Inspect { .. } => {
            let source = observe_source(core, person_id, device_id, spec).await?;
            let grant = current_grant(vault, spec, source.as_ref()).await?;
            Ok((
                Some(observe_overview(spec, source.as_ref(), grant.as_ref())),
                None,
            ))
        }
        ConnectionObserveOperation::Review { .. } => Ok((
            None,
            Some(
                review_observe(
                    core,
                    vault,
                    inspector,
                    person_id,
                    device_id,
                    spec,
                    cancellation,
                )
                .await?,
            ),
        )),
        ConnectionObserveOperation::SetEnabled {
            enabled,
            disconnecting,
            expected,
            ..
        } => {
            let source = observe_source(core, person_id, device_id, spec)
                .await?
                .ok_or(AgentFailure::AccessReviewRequired)?;
            let grant = current_grant(vault, spec, Some(&source)).await?;
            if *enabled {
                let reviewed = review_observe(
                    core,
                    vault,
                    inspector,
                    person_id,
                    device_id,
                    spec,
                    cancellation,
                )
                .await?;
                if Some(&reviewed) != expected.as_ref()
                    || source.source_authority() != reviewed.source_authority
                    || source.revision() != reviewed.connection_revision.unwrap_or_default()
                {
                    return Err(AgentFailure::AccessReviewRequired);
                }
                let expected_grant = grant.as_ref().map(|grant| (grant.id(), grant.authority()));
                let reviewed_member = &reviewed.members[0];
                if expected_grant
                    != reviewed_member
                        .expected_grant_id
                        .zip(reviewed_member.expected_grant_authority)
                {
                    return Err(AgentFailure::AccessReviewRequired);
                }
                let current = observe_source(core, person_id, device_id, spec)
                    .await?
                    .ok_or(AgentFailure::AccessReviewRequired)?;
                if current != source {
                    return Err(AgentFailure::StaleContext);
                }
                let [active] = vault
                    .activate_access_grants(vec![AccessGrantActivation {
                        grant_id: expected_grant
                            .map(|(id, _)| id)
                            .unwrap_or_else(GrantId::new),
                        expected: expected_grant.map(|(_, authority)| authority),
                        source: binding(&current)?,
                        scope: scope(spec, &current)?,
                    }])
                    .await?
                    .try_into()
                    .map_err(|_| AgentFailure::StorageUnavailable)?;
                Ok((
                    Some(observe_overview(spec, Some(&source), Some(&active))),
                    None,
                ))
            } else {
                let grant = grant.ok_or(AgentFailure::AccessReviewRequired)?;
                let changed = if *disconnecting {
                    vault
                        .revoke_data_access_grant(grant.id(), grant.authority())
                        .await?
                } else {
                    vault
                        .pause_data_access_grant(grant.id(), grant.authority())
                        .await?
                };
                Ok((
                    Some(observe_overview(spec, Some(&source), Some(&changed))),
                    None,
                ))
            }
        }
    }
}

pub(super) async fn apply_personal<Keys: VaultKeyProvider>(
    core: &FloeCore,
    vault: &EncryptedAgentVault<Keys>,
    inspector: &impl PersonalSubjectInspector,
    person_id: PersonId,
    command: PersonalAccessConfiguration,
    cancellation: Cancellation,
) -> Result<PersonalAccessOverview, AgentFailure> {
    let spec = PersonalSourceSpec::for_connector(&command.connector)?;
    if spec.connector == "contacts.apple" || spec.connector == "contacts.android" {
        return Err(AgentFailure::InvalidInput);
    }
    let change = match command.change {
        PersonalAccessChange::Inspect => StandingChange::Inspect {
            handles: Vec::new(),
        },
        PersonalAccessChange::Review {
            expected_native_subject_fingerprint,
            feasibility_query: None,
            expected_grant_id,
            expected_grant_authority,
        } => StandingChange::Review {
            handles: Vec::new(),
            fingerprint: expected_native_subject_fingerprint,
            expected: grant_expectation(expected_grant_id, expected_grant_authority)?,
        },
        PersonalAccessChange::SetEnabled { enabled } => StandingChange::SetEnabled { enabled },
        _ => return Err(AgentFailure::InvalidInput),
    };
    apply_standing(
        core,
        vault,
        inspector,
        person_id,
        &command.device_id,
        spec,
        change,
        cancellation,
    )
    .await
}

pub(super) async fn apply_contacts<Keys: VaultKeyProvider>(
    core: &FloeCore,
    vault: &EncryptedAgentVault<Keys>,
    inspector: &impl PersonalSubjectInspector,
    person_id: PersonId,
    command: ContactsAccessConfiguration,
    cancellation: Cancellation,
) -> Result<PersonalAccessOverview, AgentFailure> {
    let spec = PersonalSourceSpec::for_connector(&command.connector)?;
    if spec.view != floe_context_contract::PEOPLE_VIEW_ID {
        return Err(AgentFailure::InvalidInput);
    }
    let change = match command.change {
        ContactsAccessChange::Inspect { selected_handles } => StandingChange::Inspect {
            handles: selected_handles,
        },
        ContactsAccessChange::Review {
            selected_handles,
            expected_native_subject_fingerprint,
            expected_grant_id,
            expected_grant_authority,
        } => StandingChange::Review {
            handles: selected_handles,
            fingerprint: expected_native_subject_fingerprint,
            expected: grant_expectation(expected_grant_id, expected_grant_authority)?,
        },
        ContactsAccessChange::SetEnabled { enabled } => StandingChange::SetEnabled { enabled },
    };
    apply_standing(
        core,
        vault,
        inspector,
        person_id,
        &command.device_id,
        spec,
        change,
        cancellation,
    )
    .await
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::{Arc, Mutex};

    use floe_access::PersonalSubjectEvidence;
    use floe_agent_contract::BoxFuture;
    use floe_vault::VaultKey;
    use uuid::Uuid;

    use super::*;

    #[derive(Clone, Default)]
    struct Keys(Arc<Mutex<HashMap<(PersonId, Uuid), [u8; 32]>>>);

    impl VaultKeyProvider for Keys {
        fn load(&self, person: PersonId, vault: Uuid) -> Result<VaultKey, AgentFailure> {
            self.0
                .lock()
                .unwrap()
                .get(&(person, vault))
                .copied()
                .map(VaultKey::from_bytes)
                .ok_or(AgentFailure::VaultUnavailable)
        }

        fn insert(
            &self,
            person: PersonId,
            vault: Uuid,
            key: &VaultKey,
        ) -> Result<(), AgentFailure> {
            self.0
                .lock()
                .unwrap()
                .insert((person, vault), *key.as_bytes());
            Ok(())
        }
    }

    struct Subject {
        fingerprint: Mutex<String>,
        selected: Mutex<Vec<String>>,
    }

    impl PersonalSubjectInspector for Subject {
        fn inspect<'a>(
            &'a self,
            _person_id: PersonId,
            _device_id: &'a str,
            probe: PersonalSubjectProbe<'a>,
            _expected_native_subject_fingerprint: Option<String>,
            _deadline: Option<tokio::time::Instant>,
            _cancellation: Cancellation,
        ) -> BoxFuture<'a, Result<PersonalSubjectEvidence, AgentFailure>> {
            Box::pin(async move {
                if let PersonalSubjectProbe::People { selected_handles } = probe {
                    *self.selected.lock().unwrap() = selected_handles;
                }
                let fingerprint = self.fingerprint.lock().unwrap().clone();
                Ok(PersonalSubjectEvidence {
                    before: fingerprint.clone(),
                    after: fingerprint,
                })
            })
        }

        fn attention_presence(&self, _person_id: PersonId, _device_id: &str) -> Option<Uuid> {
            None
        }
    }

    fn contacts_review(
        handles: &[&str],
        fingerprint: &str,
        expected: Option<&DataAccessGrant>,
    ) -> ContactsAccessConfiguration {
        ContactsAccessConfiguration {
            connector: "contacts.apple".into(),
            device_id: "device-1".into(),
            consumers: Vec::new(),
            change: ContactsAccessChange::Review {
                selected_handles: handles.iter().map(|handle| (*handle).into()).collect(),
                expected_native_subject_fingerprint: fingerprint.into(),
                expected_grant_id: expected.map(DataAccessGrant::id),
                expected_grant_authority: expected.map(DataAccessGrant::authority),
            },
        }
    }

    fn standing_review(
        connector: &str,
        fingerprint: &str,
        expected: Option<&DataAccessGrant>,
    ) -> PersonalAccessConfiguration {
        PersonalAccessConfiguration {
            connector: connector.into(),
            device_id: "device-1".into(),
            consumers: Vec::new(),
            change: PersonalAccessChange::Review {
                expected_native_subject_fingerprint: fingerprint.into(),
                feasibility_query: None,
                expected_grant_id: expected.map(DataAccessGrant::id),
                expected_grant_authority: expected.map(DataAccessGrant::authority),
            },
        }
    }

    #[tokio::test]
    async fn common_contacts_observe_survives_source_edit_without_grant_review() {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let core = FloeCore::open(root.path().join("core.db")).await.unwrap();
        let person = PersonId::new();
        let vault = EncryptedAgentVault::create(root.path(), person, Keys::default())
            .await
            .unwrap();
        let subject = Subject {
            fingerprint: Mutex::new("a".repeat(64)),
            selected: Mutex::new(Vec::new()),
        };
        let spec = PersonalSourceSpec::for_connector("contacts.apple").unwrap();
        let initial = core
            .source_service()
            .establish_reviewed_native(
                person,
                ConnectorId::try_new(spec.connector).unwrap(),
                ConnectionId::try_new(spec.connection).unwrap(),
                ExecutionOwnerId::try_new(spec.execution_owner("device-1").unwrap()).unwrap(),
                spec.mode,
                spec.resources(vec!["A".into()]).unwrap(),
                "a".repeat(64),
            )
            .await
            .unwrap();
        let inspect = ConnectionObserveOperation::Inspect {
            connector_id: spec.connector.into(),
            connection_id: spec.connection.into(),
        };
        let review = ConnectionObserveOperation::Review {
            connector_id: spec.connector.into(),
            connection_id: spec.connection.into(),
        };
        let (before, _) = apply_connection_observe(
            &core,
            &vault,
            &subject,
            person,
            "device-1",
            &inspect,
            Cancellation::default(),
        )
        .await
        .unwrap();
        assert_eq!(before.unwrap().status, ConnectionObserveStatus::NeedsReview);
        let (_, expected) = apply_connection_observe(
            &core,
            &vault,
            &subject,
            person,
            "device-1",
            &review,
            Cancellation::default(),
        )
        .await
        .unwrap();
        let expected = expected.unwrap();
        assert_eq!(expected.members.len(), 1);
        assert_eq!(
            expected.members[0].view_id,
            floe_context_contract::PEOPLE_VIEW_ID
        );
        assert!(!expected.members[0].resource.contains("A"));
        let enable = |expected| ConnectionObserveOperation::SetEnabled {
            connector_id: spec.connector.into(),
            connection_id: spec.connection.into(),
            enabled: true,
            disconnecting: false,
            expected: Some(expected),
        };
        apply_connection_observe(
            &core,
            &vault,
            &subject,
            person,
            "device-1",
            &enable(expected.clone()),
            Cancellation::default(),
        )
        .await
        .unwrap();
        let grant_before = current_grant(&vault, spec, Some(&initial))
            .await
            .unwrap()
            .unwrap();
        let changed = core
            .source_service()
            .configure_reviewed_native(
                person,
                initial.connection_id(),
                initial.revision(),
                spec.mode,
                spec.resources(vec!["A".into(), "B".into()]).unwrap(),
                "a".repeat(64),
            )
            .await
            .unwrap();
        assert_eq!(
            changed.source_authority(),
            initial.source_authority().advance().unwrap()
        );
        let grant_after = current_grant(&vault, spec, Some(&changed))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(grant_after.id(), grant_before.id());
        assert_eq!(grant_after.authority(), grant_before.authority());
        let (overview, _) = apply_connection_observe(
            &core,
            &vault,
            &subject,
            person,
            "device-1",
            &inspect,
            Cancellation::default(),
        )
        .await
        .unwrap();
        let overview = overview.unwrap();
        assert_eq!(overview.status, ConnectionObserveStatus::Active);
        assert_eq!(overview.source_resources, ["A", "B"]);
        assert_eq!(
            apply_connection_observe(
                &core,
                &vault,
                &subject,
                person,
                "device-1",
                &enable(expected),
                Cancellation::default(),
            )
            .await,
            Err(AgentFailure::AccessReviewRequired)
        );
        let grant_after_stale = current_grant(&vault, spec, Some(&changed))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(grant_after_stale.authority(), grant_before.authority());
        let pause = ConnectionObserveOperation::SetEnabled {
            connector_id: spec.connector.into(),
            connection_id: spec.connection.into(),
            enabled: false,
            disconnecting: false,
            expected: None,
        };
        let (paused, _) = apply_connection_observe(
            &core,
            &vault,
            &subject,
            person,
            "device-1",
            &pause,
            Cancellation::default(),
        )
        .await
        .unwrap();
        assert_eq!(paused.unwrap().status, ConnectionObserveStatus::Paused);
        let (_, current_review) = apply_connection_observe(
            &core,
            &vault,
            &subject,
            person,
            "device-1",
            &review,
            Cancellation::default(),
        )
        .await
        .unwrap();
        let (active, _) = apply_connection_observe(
            &core,
            &vault,
            &subject,
            person,
            "device-1",
            &enable(current_review.unwrap()),
            Cancellation::default(),
        )
        .await
        .unwrap();
        assert_eq!(active.unwrap().status, ConnectionObserveStatus::Active);
        let grant_final = current_grant(&vault, spec, Some(&changed))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(grant_final.id(), grant_before.id());
    }

    #[tokio::test]
    async fn common_attention_and_wellbeing_review_current_native_subject() {
        for connector in ["attention.macos", "health.apple"] {
            let root = tempfile::tempdir().unwrap();
            std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
            let core = FloeCore::open(root.path().join("core.db")).await.unwrap();
            let person = PersonId::new();
            let vault = EncryptedAgentVault::create(root.path(), person, Keys::default())
                .await
                .unwrap();
            let subject = Subject {
                fingerprint: Mutex::new("a".repeat(64)),
                selected: Mutex::new(Vec::new()),
            };
            let spec = PersonalSourceSpec::for_connector(connector).unwrap();
            let initial = core
                .source_service()
                .establish_reviewed_native(
                    person,
                    ConnectorId::try_new(spec.connector).unwrap(),
                    ConnectionId::try_new(spec.connection).unwrap(),
                    ExecutionOwnerId::try_new(spec.execution_owner("device-1").unwrap()).unwrap(),
                    spec.mode,
                    spec.resources(Vec::new()).unwrap(),
                    "a".repeat(64),
                )
                .await
                .unwrap();
            let review = ConnectionObserveOperation::Review {
                connector_id: spec.connector.into(),
                connection_id: spec.connection.into(),
            };
            let (_, expected) = apply_connection_observe(
                &core,
                &vault,
                &subject,
                person,
                "device-1",
                &review,
                Cancellation::default(),
            )
            .await
            .unwrap();
            let expected = expected.unwrap();
            assert_eq!(expected.members[0].view_id, spec.view);
            let enable = |expected| ConnectionObserveOperation::SetEnabled {
                connector_id: spec.connector.into(),
                connection_id: spec.connection.into(),
                enabled: true,
                disconnecting: false,
                expected: Some(expected),
            };
            apply_connection_observe(
                &core,
                &vault,
                &subject,
                person,
                "device-1",
                &enable(expected.clone()),
                Cancellation::default(),
            )
            .await
            .unwrap();
            let grant = current_grant(&vault, spec, Some(&initial))
                .await
                .unwrap()
                .unwrap();
            *subject.fingerprint.lock().unwrap() = "b".repeat(64);
            let changed = core
                .source_service()
                .configure_reviewed_native(
                    person,
                    initial.connection_id(),
                    initial.revision(),
                    spec.mode,
                    spec.resources(Vec::new()).unwrap(),
                    "b".repeat(64),
                )
                .await
                .unwrap();
            assert_eq!(
                changed.source_authority(),
                initial.source_authority().advance().unwrap()
            );
            assert_eq!(
                apply_connection_observe(
                    &core,
                    &vault,
                    &subject,
                    person,
                    "device-1",
                    &enable(expected),
                    Cancellation::default(),
                )
                .await,
                Err(AgentFailure::AccessReviewRequired)
            );
            let current = current_grant(&vault, spec, Some(&changed))
                .await
                .unwrap()
                .unwrap();
            assert_eq!(current.id(), grant.id());
            assert_eq!(current.authority(), grant.authority());
            let (_, current_review) = apply_connection_observe(
                &core,
                &vault,
                &subject,
                person,
                "device-1",
                &review,
                Cancellation::default(),
            )
            .await
            .unwrap();
            assert_eq!(
                current_review.unwrap().native_subject.as_deref(),
                Some("b".repeat(64).as_str())
            );
        }
    }

    #[tokio::test]
    async fn singleton_personal_subject_review_advances_only_source_authority() {
        for connector in ["attention.macos", "health.apple"] {
            let root = tempfile::tempdir().unwrap();
            std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
            let core = FloeCore::open(root.path().join("core.db")).await.unwrap();
            let person = PersonId::new();
            let vault = EncryptedAgentVault::create(root.path(), person, Keys::default())
                .await
                .unwrap();
            let subject = Subject {
                fingerprint: Mutex::new("a".repeat(64)),
                selected: Mutex::new(Vec::new()),
            };
            let first = apply_personal(
                &core,
                &vault,
                &subject,
                person,
                standing_review(connector, &"a".repeat(64), None),
                Cancellation::default(),
            )
            .await
            .unwrap();
            let spec = PersonalSourceSpec::for_connector(connector).unwrap();
            let connection_id = ConnectionId::try_new(spec.connection).unwrap();
            let initial = core
                .source_service()
                .load(person, &connection_id)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(initial.revision(), 1);
            assert_eq!(
                initial.resource_mode(),
                floe_connections::ResourceMode::AllAvailable
            );
            let grant = current_grant(&vault, spec, Some(&initial))
                .await
                .unwrap()
                .unwrap();
            assert_eq!(first.grant_id, Some(grant.id()));
            *subject.fingerprint.lock().unwrap() = "b".repeat(64);
            let next = apply_personal(
                &core,
                &vault,
                &subject,
                person,
                standing_review(connector, &"b".repeat(64), Some(&grant)),
                Cancellation::default(),
            )
            .await
            .unwrap();
            let changed = core
                .source_service()
                .load(person, &connection_id)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(changed.revision(), 2);
            assert_eq!(
                changed.source_authority(),
                initial.source_authority().advance().unwrap()
            );
            assert_eq!(changed.resources(), initial.resources());
            assert_eq!(next.grant_id, Some(grant.id()));
            assert_eq!(next.grant_authority, Some(grant.authority()));
        }
    }

    #[tokio::test]
    async fn contacts_source_review_preserves_grant_and_does_not_roll_back_on_grant_conflict() {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let core = FloeCore::open(root.path().join("core.db")).await.unwrap();
        let person = PersonId::new();
        let vault = EncryptedAgentVault::create(root.path(), person, Keys::default())
            .await
            .unwrap();
        let subject = Subject {
            fingerprint: Mutex::new("a".repeat(64)),
            selected: Mutex::new(Vec::new()),
        };
        let first = apply_contacts(
            &core,
            &vault,
            &subject,
            person,
            contacts_review(&["a"], &"a".repeat(64), None),
            Cancellation::default(),
        )
        .await
        .unwrap();
        let source_id = ConnectionId::try_new("contacts.apple.local").unwrap();
        let initial = core
            .source_service()
            .load(person, &source_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(initial.revision(), 1);
        assert!(initial.is_serving());
        let grant = current_grant(
            &vault,
            PersonalSourceSpec::for_connector("contacts.apple").unwrap(),
            Some(&initial),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(first.grant_id, Some(grant.id()));
        let first_candidate =
            floe_context::discover_source_candidates(floe_context::SourceCandidateRequest {
                person_id: person,
                device_id: "device-1",
                capability: floe_context_contract::PEOPLE_VIEW_ID,
                contract_version: 1,
                remote_connections: &[],
                remote_execution_owner: None,
                source_connections: std::slice::from_ref(&initial),
            })
            .unwrap()
            .remove(0);
        assert_eq!(
            grant.scope().resources(),
            [first_candidate.reference.resource.clone()]
        );

        *subject.fingerprint.lock().unwrap() = "b".repeat(64);
        let next = apply_contacts(
            &core,
            &vault,
            &subject,
            person,
            contacts_review(&["a", "b"], &"b".repeat(64), Some(&grant)),
            Cancellation::default(),
        )
        .await
        .unwrap();
        let source = core
            .source_service()
            .load(person, &source_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(source.revision(), 2);
        assert_eq!(
            source.source_authority().epoch().get(),
            initial.source_authority().epoch().get() + 1
        );
        assert_eq!(
            source
                .resources()
                .iter()
                .map(|resource| resource.handle().as_str())
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
        assert_eq!(*subject.selected.lock().unwrap(), ["a", "b"]);
        assert_eq!(next.grant_id, Some(grant.id()));
        assert_eq!(next.grant_authority, Some(grant.authority()));
        let next_candidate =
            floe_context::discover_source_candidates(floe_context::SourceCandidateRequest {
                person_id: person,
                device_id: "device-1",
                capability: floe_context_contract::PEOPLE_VIEW_ID,
                contract_version: 1,
                remote_connections: &[],
                remote_execution_owner: None,
                source_connections: std::slice::from_ref(&source),
            })
            .unwrap()
            .remove(0);
        assert_eq!(next_candidate.candidate_id, first_candidate.candidate_id);
        assert_eq!(next_candidate.reference, first_candidate.reference);

        *subject.fingerprint.lock().unwrap() = "c".repeat(64);
        let wrong = DataAccessGrant::new(
            GrantId::new(),
            Uuid::new_v4(),
            binding(&source).unwrap(),
            scope(
                PersonalSourceSpec::for_connector("contacts.apple").unwrap(),
                &source,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            apply_contacts(
                &core,
                &vault,
                &subject,
                person,
                contacts_review(&["a", "b", "c"], &"c".repeat(64), Some(&wrong)),
                Cancellation::default(),
            )
            .await,
            Err(AgentFailure::Conflict)
        );
        let after = core
            .source_service()
            .load(person, &source_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(after.revision(), 3);
        assert_eq!(
            after.source_authority().epoch().get(),
            source.source_authority().epoch().get() + 1
        );
        let still_grant = current_grant(
            &vault,
            PersonalSourceSpec::for_connector("contacts.apple").unwrap(),
            Some(&after),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(still_grant.id(), grant.id());
        assert_eq!(still_grant.authority(), grant.authority());
    }
}
