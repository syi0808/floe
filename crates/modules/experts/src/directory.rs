use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock},
};

use floe_agent_contract::{
    AgentDefinition, AgentEndpoint, AgentFailure, AllowedCatalog, PackageKind, PackageRef,
};
use serde::{Deserialize, Serialize};
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
                && value.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')
                })
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

pub struct ResolvedDirectoryEntry {
    pub admission: ExpertAdmissionIdentity,
    pub selection: ExpertExecutionSelection,
    pub endpoint: Arc<dyn AgentEndpoint>,
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
                        && entry.admission.assignment_id
                            == registered.entry.admission.assignment_id
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

    pub fn list_cards(&self, query: DirectoryQuery<'_>) -> Result<AllowedCatalog, AgentFailure> {
        validate_query(&query)?;
        let state = self
            .state
            .read()
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        Ok(AllowedCatalog {
            cards: state
                .endpoints
                .values()
                .filter(|registered| registered.entry.eligible(&query))
                .map(|registered| registered.entry.definition.clone())
                .collect(),
            tools: vec![],
            revision: state.revision,
        })
    }

    pub fn resolve(
        &self,
        agent_id: &str,
        definition_revision: u64,
        query: DirectoryQuery<'_>,
    ) -> Result<ResolvedDirectoryEntry, AgentFailure> {
        validate_query(&query)?;
        let state = self
            .state
            .read()
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        let registered = state
            .endpoints
            .get(agent_id)
            .ok_or(AgentFailure::CapabilityDenied)?;
        if registered.entry.definition.definition_revision != definition_revision {
            return Err(AgentFailure::Conflict);
        }
        if !registered.entry.eligible(&query) {
            return Err(AgentFailure::CapabilityDenied);
        }
        Ok(ResolvedDirectoryEntry {
            admission: registered.entry.admission.clone(),
            selection: registered.entry.selection.clone(),
            endpoint: Arc::clone(&registered.endpoint),
        })
    }
}

fn validate_query(query: &DirectoryQuery<'_>) -> Result<(), AgentFailure> {
    if query.principal.trim().is_empty() || query.purpose.trim().is_empty() {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(())
}
