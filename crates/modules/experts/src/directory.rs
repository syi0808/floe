use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock},
};

use floe_agent_contract::{
    AgentDefinition, AgentEndpoint, AgentFailure, AllowedCatalog, PackageKind, PackageRef,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::ExpertExecutionSelection;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertAdmissionIdentity {
    pub registry_instance_id: Uuid,
    pub assignment_id: Uuid,
    pub installation_id: Uuid,
    pub package: PackageRef,
    pub definition_revision: u64,
}

impl ExpertAdmissionIdentity {
    pub fn validate_task(
        &self,
        agent_id: &str,
        definition_revision: u64,
    ) -> Result<(), AgentFailure> {
        let valid_name = |value: &str| {
            !value.is_empty()
                && value.len() <= 128
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
        };
        if self.registry_instance_id.is_nil()
            || self.assignment_id.is_nil()
            || self.installation_id.is_nil()
            || self.package.kind != PackageKind::Expert
            || !valid_name(&self.package.id)
            || !valid_name(&self.package.version)
            || self.package.id != agent_id
            || self.definition_revision == 0
            || self.definition_revision != definition_revision
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }

    pub fn validate(&self, definition: &AgentDefinition) -> Result<(), AgentFailure> {
        self.validate_task(&definition.card.id, definition.definition_revision)?;
        if self.package.version != definition.card.version {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectoryQuery<'a> {
    pub principal: &'a str,
    pub purpose: &'a str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectoryEntry {
    pub definition: AgentDefinition,
    pub admission: ExpertAdmissionIdentity,
    pub selection: ExpertExecutionSelection,
    pub reviewed: bool,
    pub enabled: bool,
    pub admitted_principals: Vec<String>,
    pub purposes: Vec<String>,
}

impl DirectoryEntry {
    fn validate(&self) -> Result<(), AgentFailure> {
        self.definition.validate()?;
        self.admission.validate(&self.definition)?;
        self.selection.validate()?;
        if self
            .admitted_principals
            .iter()
            .any(|value| value.trim().is_empty())
            || self.purposes.is_empty()
            || self.purposes.iter().any(|value| value.trim().is_empty())
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }

    fn eligible(&self, query: &DirectoryQuery<'_>) -> bool {
        self.reviewed
            && self.enabled
            && (self.admitted_principals.is_empty()
                || self
                    .admitted_principals
                    .iter()
                    .any(|principal| principal == query.principal))
            && self.purposes.iter().any(|purpose| purpose == query.purpose)
    }
}

struct RegisteredEndpoint {
    entry: DirectoryEntry,
    endpoint: Arc<dyn AgentEndpoint>,
    owner: Option<String>,
}

pub(crate) struct ResolvedDirectoryEntry {
    pub admission: ExpertAdmissionIdentity,
    pub selection: ExpertExecutionSelection,
    pub endpoint: Arc<dyn AgentEndpoint>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunExpertEnvironmentIdentity {
    pub revision: u64,
    pub digest: [u8; 32],
}

impl RunExpertEnvironmentIdentity {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        (self.digest != [0; 32])
            .then_some(())
            .ok_or(AgentFailure::InvalidInput)
    }
}

pub(crate) struct DirectorySnapshot {
    identity: RunExpertEnvironmentIdentity,
    entries: BTreeMap<String, (AgentDefinition, ResolvedDirectoryEntry)>,
}

impl DirectorySnapshot {
    pub(crate) fn identity(&self) -> RunExpertEnvironmentIdentity {
        self.identity
    }

    pub(crate) fn catalog(&self) -> AllowedCatalog {
        AllowedCatalog {
            cards: self
                .entries
                .values()
                .map(|(definition, _)| definition.clone())
                .collect(),
            tools: vec![],
            revision: self.identity.revision,
        }
    }

    pub(crate) fn resolve(
        &self,
        agent_id: &str,
        definition_revision: u64,
    ) -> Result<ResolvedDirectoryEntry, AgentFailure> {
        let (definition, entry) = self
            .entries
            .get(agent_id)
            .ok_or(AgentFailure::CapabilityDenied)?;
        if definition.definition_revision != definition_revision {
            return Err(AgentFailure::Conflict);
        }
        Ok(ResolvedDirectoryEntry {
            admission: entry.admission.clone(),
            selection: entry.selection.clone(),
            endpoint: Arc::clone(&entry.endpoint),
        })
    }
}

#[derive(Default)]
struct DirectoryState {
    revision: u64,
    endpoints: BTreeMap<String, RegisteredEndpoint>,
}

#[derive(Clone, Default)]
pub struct Directory {
    state: Arc<RwLock<DirectoryState>>,
}

impl Directory {
    pub fn register(
        &self,
        entry: DirectoryEntry,
        endpoint: Arc<dyn AgentEndpoint>,
    ) -> Result<u64, AgentFailure> {
        entry.validate()?;
        let agent_id = entry.definition.card.id.clone();
        let mut state = self
            .state
            .write()
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        if state.endpoints.contains_key(&agent_id) {
            return Err(AgentFailure::Conflict);
        }
        state.revision = state
            .revision
            .checked_add(1)
            .ok_or(AgentFailure::Conflict)?;
        state.endpoints.insert(
            agent_id,
            RegisteredEndpoint {
                entry,
                endpoint,
                owner: None,
            },
        );
        Ok(state.revision)
    }

    pub fn publish(
        &self,
        owner: &str,
        entries: Vec<(DirectoryEntry, Arc<dyn AgentEndpoint>)>,
    ) -> Result<u64, AgentFailure> {
        if owner.trim().is_empty() || owner.len() > 128 {
            return Err(AgentFailure::InvalidInput);
        }
        let mut candidates = BTreeMap::new();
        for (entry, endpoint) in entries {
            entry.validate()?;
            let agent_id = entry.definition.card.id.clone();
            if candidates.insert(agent_id, (entry, endpoint)).is_some() {
                return Err(AgentFailure::Conflict);
            }
        }
        let mut assignments = std::collections::HashSet::new();
        if candidates
            .values()
            .any(|(entry, _)| !assignments.insert(entry.admission.assignment_id))
        {
            return Err(AgentFailure::Conflict);
        }
        let mut state = self
            .state
            .write()
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        for agent_id in candidates.keys() {
            if state
                .endpoints
                .get(agent_id)
                .is_some_and(|registered| registered.owner.as_deref() != Some(owner))
            {
                return Err(AgentFailure::Conflict);
            }
        }
        if state.endpoints.values().any(|registered| {
            registered.owner.as_deref() != Some(owner)
                && candidates.values().any(|(entry, _)| {
                    entry.admission.registry_instance_id
                        == registered.entry.admission.registry_instance_id
                        && entry.admission.assignment_id == registered.entry.admission.assignment_id
                })
        }) {
            return Err(AgentFailure::Conflict);
        }
        let previous = state
            .endpoints
            .iter()
            .filter(|(_, registered)| registered.owner.as_deref() == Some(owner))
            .collect::<Vec<_>>();
        let unchanged = previous.len() == candidates.len()
            && previous.iter().all(|(agent_id, registered)| {
                candidates.get(*agent_id).is_some_and(|(entry, endpoint)| {
                    registered.entry == *entry && Arc::ptr_eq(&registered.endpoint, endpoint)
                })
            });
        if unchanged {
            return Ok(state.revision);
        }
        let next_revision = state
            .revision
            .checked_add(1)
            .ok_or(AgentFailure::Conflict)?;
        state
            .endpoints
            .retain(|_, registered| registered.owner.as_deref() != Some(owner));
        for (agent_id, (entry, endpoint)) in candidates {
            state.endpoints.insert(
                agent_id,
                RegisteredEndpoint {
                    entry,
                    endpoint,
                    owner: Some(owner.to_owned()),
                },
            );
        }
        state.revision = next_revision;
        Ok(next_revision)
    }

    pub fn set_enabled(
        &self,
        agent_id: &str,
        definition_revision: u64,
        enabled: bool,
    ) -> Result<u64, AgentFailure> {
        let mut state = self
            .state
            .write()
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        let registered = state
            .endpoints
            .get_mut(agent_id)
            .ok_or(AgentFailure::NotFound)?;
        if registered.entry.definition.definition_revision != definition_revision {
            return Err(AgentFailure::Conflict);
        }
        registered.entry.enabled = enabled;
        state.revision = state
            .revision
            .checked_add(1)
            .ok_or(AgentFailure::Conflict)?;
        Ok(state.revision)
    }

    pub fn unregister(&self, agent_id: &str) -> Result<u64, AgentFailure> {
        if agent_id.trim().is_empty() {
            return Err(AgentFailure::InvalidInput);
        }
        let mut state = self
            .state
            .write()
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        state
            .endpoints
            .remove(agent_id)
            .ok_or(AgentFailure::NotFound)?;
        state.revision = state
            .revision
            .checked_add(1)
            .ok_or(AgentFailure::Conflict)?;
        Ok(state.revision)
    }

    pub(crate) fn snapshot(
        &self,
        query: DirectoryQuery<'_>,
    ) -> Result<DirectorySnapshot, AgentFailure> {
        validate_query(&query)?;
        let state = self
            .state
            .read()
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        let entries = state
            .endpoints
            .iter()
            .filter(|(_, registered)| registered.entry.eligible(&query))
            .map(|(agent_id, registered)| {
                (
                    agent_id.clone(),
                    (
                        registered.entry.definition.clone(),
                        ResolvedDirectoryEntry {
                            admission: registered.entry.admission.clone(),
                            selection: registered.entry.selection.clone(),
                            endpoint: Arc::clone(&registered.endpoint),
                        },
                    ),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let configuration = entries
            .values()
            .map(|(definition, entry)| (definition, &entry.admission, &entry.selection))
            .collect::<Vec<_>>();
        let encoded = serde_json::to_vec(&(
            "floe.run-expert-environment.sha256.v1",
            state.revision,
            configuration,
        ))
        .map_err(|_| AgentFailure::StorageUnavailable)?;
        let identity = RunExpertEnvironmentIdentity {
            revision: state.revision,
            digest: Sha256::digest(encoded).into(),
        };
        identity.validate()?;
        Ok(DirectorySnapshot { identity, entries })
    }
}

fn validate_query(query: &DirectoryQuery<'_>) -> Result<(), AgentFailure> {
    if query.principal.trim().is_empty() || query.purpose.trim().is_empty() {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DeniedEndpoint;

    impl AgentEndpoint for DeniedEndpoint {
        fn execute<'a>(
            &'a self,
            _: floe_agent_contract::EndpointInvocation,
            _: &'a floe_agent_contract::ExecutionScope,
        ) -> floe_agent_contract::BoxFuture<
            'a,
            Result<floe_agent_contract::ExpertReport, AgentFailure>,
        > {
            Box::pin(async { Err(AgentFailure::CapabilityDenied) })
        }
    }

    fn entry(agent_id: &str) -> DirectoryEntry {
        DirectoryEntry {
            definition: AgentDefinition {
                card: floe_agent_contract::AgentCard {
                    schema_version: floe_agent_contract::AGENT_SCHEMA_VERSION,
                    protocol_version: crate::A2A_PROTOCOL_VERSION.into(),
                    id: agent_id.into(),
                    version: "1.0.0".into(),
                    name: agent_id.into(),
                    description: "Synthetic Expert".into(),
                    domain_tags: vec![],
                    skills: vec!["Analyze an admitted request".into()],
                    supported_placements: vec![floe_agent_contract::ModelPlacement::DeviceLocal],
                },
                definition_revision: 1,
            },
            admission: ExpertAdmissionIdentity {
                registry_instance_id: Uuid::new_v4(),
                assignment_id: Uuid::new_v4(),
                installation_id: Uuid::new_v4(),
                package: PackageRef {
                    kind: PackageKind::Expert,
                    id: agent_id.into(),
                    version: "1.0.0".into(),
                },
                definition_revision: 1,
            },
            selection: ExpertExecutionSelection::without_requirements(1).unwrap(),
            reviewed: true,
            enabled: true,
            admitted_principals: vec!["person-a".into()],
            purposes: vec!["test".into()],
        }
    }

    fn snapshot(directory: &Directory) -> DirectorySnapshot {
        directory
            .snapshot(DirectoryQuery {
                principal: "person-a",
                purpose: "test",
            })
            .unwrap()
    }

    #[test]
    fn empty_revision_zero_has_stable_valid_identity() {
        let directory = Directory::default();
        directory.publish("test", vec![]).unwrap();
        let environment = snapshot(&directory);
        assert_eq!(environment.identity().revision, 0);
        assert_eq!(environment.catalog().revision, 0);
        assert!(environment.catalog().cards.is_empty());
        assert_ne!(environment.identity().digest, [0; 32]);
        assert_eq!(environment.identity(), snapshot(&directory).identity());
        assert!(
            RunExpertEnvironmentIdentity {
                revision: 0,
                digest: [0; 32]
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn identity_is_sorted_configuration_not_endpoint_pointer() {
        let first = entry("example.first");
        let second = entry("example.second");
        let left = Directory::default();
        let right = Directory::default();
        left.publish(
            "test",
            vec![
                (first.clone(), Arc::new(DeniedEndpoint)),
                (second.clone(), Arc::new(DeniedEndpoint)),
            ],
        )
        .unwrap();
        right
            .publish(
                "test",
                vec![
                    (second, Arc::new(DeniedEndpoint)),
                    (first, Arc::new(DeniedEndpoint)),
                ],
            )
            .unwrap();
        assert_eq!(snapshot(&left).identity(), snapshot(&right).identity());
    }

    #[test]
    fn each_configuration_component_and_revision_bind_identity() {
        let original = entry("example.first");
        let identity_for = |candidate: DirectoryEntry| {
            let directory = Directory::default();
            directory
                .publish("test", vec![(candidate, Arc::new(DeniedEndpoint))])
                .unwrap();
            snapshot(&directory).identity()
        };
        let baseline = identity_for(original.clone());
        let mut definition = original.clone();
        definition.definition.card.description = "Different description".into();
        assert_ne!(baseline, identity_for(definition));
        let mut admission = original.clone();
        admission.admission.installation_id = Uuid::new_v4();
        assert_ne!(baseline, identity_for(admission));
        let mut selection = original.clone();
        selection.selection = ExpertExecutionSelection::without_requirements(2).unwrap();
        assert_ne!(baseline, identity_for(selection));
        let directory = Directory::default();
        directory
            .publish("test", vec![(original, Arc::new(DeniedEndpoint))])
            .unwrap();
        directory.set_enabled("example.first", 1, true).unwrap();
        assert_ne!(baseline, snapshot(&directory).identity());
    }

    #[test]
    fn snapshot_retains_exact_endpoint_after_replacement_and_disable() {
        let directory = Directory::default();
        let original = entry("example.first");
        let old_endpoint: Arc<dyn AgentEndpoint> = Arc::new(DeniedEndpoint);
        directory
            .publish("test", vec![(original.clone(), Arc::clone(&old_endpoint))])
            .unwrap();
        let old = snapshot(&directory);
        let new_endpoint: Arc<dyn AgentEndpoint> = Arc::new(DeniedEndpoint);
        directory
            .publish("test", vec![(original, Arc::clone(&new_endpoint))])
            .unwrap();
        assert!(Arc::ptr_eq(
            &old.resolve("example.first", 1).unwrap().endpoint,
            &old_endpoint
        ));
        assert!(Arc::ptr_eq(
            &snapshot(&directory)
                .resolve("example.first", 1)
                .unwrap()
                .endpoint,
            &new_endpoint
        ));
        assert!(matches!(
            old.resolve("example.first", 2),
            Err(AgentFailure::Conflict)
        ));
        directory.set_enabled("example.first", 1, false).unwrap();
        assert_eq!(old.catalog().cards.len(), 1);
        assert!(old.resolve("example.first", 1).is_ok());
        assert!(snapshot(&directory).catalog().cards.is_empty());
    }
}
