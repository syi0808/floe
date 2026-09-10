use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::{AgentFailure, PromptRole};

pub const MAX_PLAYBOOK_DEPTH: usize = 4;
pub const MAX_VISIBLE_PLAYBOOKS: usize = 32;
pub const MAX_LOADED_PLAYBOOKS: usize = 8;
pub const MAX_LOADED_PLAYBOOK_BYTES: usize = 32 * 1024;

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlaybookRef {
    pub id: String,
    pub revision: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlaybookIndexEntry {
    pub reference: PlaybookRef,
    pub name: String,
    pub summary: String,
    pub triggers: Vec<String>,
    pub roles: Vec<PromptRole>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlaybookChild {
    pub reference: PlaybookRef,
    pub name: String,
    pub summary: String,
    pub triggers: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlaybookBody {
    pub instructions: String,
    pub required_capabilities: Vec<String>,
    pub references: Vec<String>,
    pub children: Vec<PlaybookChild>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Playbook {
    pub index: PlaybookIndexEntry,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<PlaybookRef>,
    pub body: PlaybookBody,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedPlaybook {
    pub reference: PlaybookRef,
    pub instructions: String,
    pub required_capabilities: Vec<String>,
    pub references: Vec<String>,
    pub children: Vec<PlaybookChild>,
}

pub struct PlaybookRegistry {
    playbooks: HashMap<PlaybookRef, Playbook>,
}

impl PlaybookRegistry {
    pub fn new(playbooks: Vec<Playbook>) -> Result<Self, AgentFailure> {
        let mut indexed = HashMap::new();
        for playbook in playbooks {
            validate_playbook(&playbook)?;
            if indexed
                .insert(playbook.index.reference.clone(), playbook)
                .is_some()
            {
                return Err(AgentFailure::Conflict);
            }
        }
        let registry = Self { playbooks: indexed };
        registry.validate_hierarchy()?;
        Ok(registry)
    }

    pub fn begin(&self, role: PromptRole) -> Result<PlaybookSession, AgentFailure> {
        let mut roots: Vec<_> = self
            .playbooks
            .values()
            .filter(|playbook| playbook.parent.is_none() && playbook.index.roles.contains(&role))
            .map(|playbook| playbook.index.clone())
            .collect();
        roots.sort_by(|left, right| left.reference.id.cmp(&right.reference.id));
        if roots.len() > MAX_VISIBLE_PLAYBOOKS {
            return Err(AgentFailure::BudgetExceeded);
        }
        Ok(PlaybookSession {
            visible: roots.iter().map(|entry| entry.reference.clone()).collect(),
            roots,
            loaded: HashSet::new(),
            loaded_bytes: 0,
        })
    }

    fn validate_hierarchy(&self) -> Result<(), AgentFailure> {
        for playbook in self.playbooks.values() {
            let mut ancestry = HashSet::new();
            let mut current = playbook;
            let mut depth = 1;
            while let Some(parent) = &current.parent {
                if !ancestry.insert(current.index.reference.clone()) || depth >= MAX_PLAYBOOK_DEPTH
                {
                    return Err(AgentFailure::InvalidInput);
                }
                let parent_playbook = self
                    .playbooks
                    .get(parent)
                    .ok_or(AgentFailure::InvalidInput)?;
                if !parent_playbook
                    .body
                    .children
                    .iter()
                    .any(|child| child.reference == current.index.reference)
                {
                    return Err(AgentFailure::InvalidInput);
                }
                current = parent_playbook;
                depth += 1;
            }
            for child in &playbook.body.children {
                let child_playbook = self
                    .playbooks
                    .get(&child.reference)
                    .ok_or(AgentFailure::InvalidInput)?;
                if child_playbook.parent.as_ref() != Some(&playbook.index.reference)
                    || child.name != child_playbook.index.name
                    || child.summary != child_playbook.index.summary
                    || child.triggers != child_playbook.index.triggers
                {
                    return Err(AgentFailure::InvalidInput);
                }
            }
        }
        Ok(())
    }

    pub fn load(
        &self,
        session: &mut PlaybookSession,
        reference: &PlaybookRef,
    ) -> Result<LoadedPlaybook, AgentFailure> {
        if !session.visible.contains(reference) {
            return Err(AgentFailure::CapabilityDenied);
        }
        if session.loaded.contains(reference) {
            return Err(AgentFailure::Conflict);
        }
        let playbook = self
            .playbooks
            .get(reference)
            .ok_or(AgentFailure::NotFound)?;
        let bytes = serde_json::to_vec(&playbook.body)
            .map_err(|_| AgentFailure::InvalidInput)?
            .len();
        let loaded_bytes = session
            .loaded_bytes
            .checked_add(bytes)
            .ok_or(AgentFailure::BudgetExceeded)?;
        let visible_count = session
            .visible
            .iter()
            .chain(playbook.body.children.iter().map(|child| &child.reference))
            .collect::<HashSet<_>>()
            .len();
        if session.loaded.len() >= MAX_LOADED_PLAYBOOKS
            || loaded_bytes > MAX_LOADED_PLAYBOOK_BYTES
            || visible_count > MAX_VISIBLE_PLAYBOOKS
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        session.loaded.insert(reference.clone());
        session.loaded_bytes = loaded_bytes;
        session.visible.extend(
            playbook
                .body
                .children
                .iter()
                .map(|child| child.reference.clone()),
        );
        Ok(LoadedPlaybook {
            reference: reference.clone(),
            instructions: playbook.body.instructions.clone(),
            required_capabilities: playbook.body.required_capabilities.clone(),
            references: playbook.body.references.clone(),
            children: playbook.body.children.clone(),
        })
    }
}

pub struct PlaybookSession {
    roots: Vec<PlaybookIndexEntry>,
    visible: HashSet<PlaybookRef>,
    loaded: HashSet<PlaybookRef>,
    loaded_bytes: usize,
}

impl PlaybookSession {
    pub fn roots(&self) -> &[PlaybookIndexEntry] {
        &self.roots
    }

    pub fn is_visible(&self, reference: &PlaybookRef) -> bool {
        self.visible.contains(reference)
    }

    pub fn loaded_bytes(&self) -> usize {
        self.loaded_bytes
    }
}

fn validate_playbook(playbook: &Playbook) -> Result<(), AgentFailure> {
    let index = &playbook.index;
    if index.reference.id.trim().is_empty()
        || index.reference.id.len() > 128
        || index.reference.revision == 0
        || index.name.trim().is_empty()
        || index.name.len() > 128
        || index.summary.trim().is_empty()
        || index.summary.len() > 512
        || index.roles.is_empty()
        || index.roles.contains(&PromptRole::Learner)
        || playbook.body.instructions.trim().is_empty()
        || playbook.body.instructions.len() > MAX_LOADED_PLAYBOOK_BYTES
        || playbook.body.children.len() > MAX_VISIBLE_PLAYBOOKS
    {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn playbook(id: &str, parent: Option<&str>, children: &[&str]) -> Playbook {
        Playbook {
            index: PlaybookIndexEntry {
                reference: PlaybookRef {
                    id: id.into(),
                    revision: 1,
                },
                name: id.into(),
                summary: format!("{id} summary"),
                triggers: vec![id.into()],
                roles: vec![PromptRole::ScheduleExpert],
            },
            parent: parent.map(|id| PlaybookRef {
                id: id.into(),
                revision: 1,
            }),
            body: PlaybookBody {
                instructions: format!("Use {id} when relevant."),
                required_capabilities: vec!["calendar.read".into()],
                references: vec![],
                children: children
                    .iter()
                    .map(|id| PlaybookChild {
                        reference: PlaybookRef {
                            id: (*id).into(),
                            revision: 1,
                        },
                        name: (*id).into(),
                        summary: format!("{id} summary"),
                        triggers: vec![(*id).into()],
                    })
                    .collect(),
            },
        }
    }

    #[test]
    fn nested_summaries_are_hidden_until_the_parent_is_loaded() {
        let registry = PlaybookRegistry::new(vec![
            playbook("calendar", None, &["focus"]),
            playbook("focus", Some("calendar"), &["deep-work"]),
            playbook("deep-work", Some("focus"), &[]),
        ])
        .unwrap();
        let calendar = PlaybookRef {
            id: "calendar".into(),
            revision: 1,
        };
        let focus = PlaybookRef {
            id: "focus".into(),
            revision: 1,
        };
        let deep_work = PlaybookRef {
            id: "deep-work".into(),
            revision: 1,
        };
        let mut session = registry.begin(PromptRole::ScheduleExpert).unwrap();
        assert_eq!(session.roots().len(), 1);
        assert!(session.is_visible(&calendar));
        assert!(!session.is_visible(&focus));
        assert_eq!(
            registry.load(&mut session, &focus),
            Err(AgentFailure::CapabilityDenied)
        );
        registry.load(&mut session, &calendar).unwrap();
        assert!(session.is_visible(&focus));
        assert!(!session.is_visible(&deep_work));
        registry.load(&mut session, &focus).unwrap();
        assert!(session.is_visible(&deep_work));
    }

    #[test]
    fn hierarchy_rejects_missing_parent_links_and_cycles() {
        assert!(matches!(
            PlaybookRegistry::new(vec![playbook("child", Some("missing"), &[])]),
            Err(AgentFailure::InvalidInput)
        ));
        assert!(matches!(
            PlaybookRegistry::new(vec![
                playbook("first", Some("second"), &["second"]),
                playbook("second", Some("first"), &["first"]),
            ]),
            Err(AgentFailure::InvalidInput)
        ));
    }

    #[test]
    fn learner_role_cannot_receive_playbooks() {
        let mut learner_playbook = playbook("memory-curation", None, &[]);
        learner_playbook.index.roles = vec![PromptRole::Learner];

        assert!(matches!(
            PlaybookRegistry::new(vec![learner_playbook]),
            Err(AgentFailure::InvalidInput)
        ));
    }
}
