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
