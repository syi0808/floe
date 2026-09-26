//! Which builtin Experts exist, what each one reads and how it is packaged.
//!
//! This is a declaration, not an installation: nothing here issues a grant or
//! reveals that a Person has a source. The registry stores what the composition
//! root installs from it.

use serde::{Deserialize, Serialize};

use floe_agent_contract::DataClass;

pub const BUILTIN_EXPERT_PACKAGE_VERSION: &str = "1.0.0";
pub const BUILTIN_EXPERT_PUBLISHER: &str = "floe";
pub const BUILTIN_EXPERT_STATE_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BuiltinExpertKind {
    Schedule,
    Commitments,
    Communication,
    Relationships,
    FocusAttention,
    Wellbeing,
    WorkContext,
    LifeLogistics,
}

impl BuiltinExpertKind {
    pub const ALL: [Self; 8] = [
        Self::Schedule,
        Self::Commitments,
        Self::Communication,
        Self::Relationships,
        Self::FocusAttention,
        Self::Wellbeing,
        Self::WorkContext,
        Self::LifeLogistics,
    ];

    pub const fn package_id(self) -> &'static str {
        match self {
            Self::Schedule => "floe.builtin.schedule",
            Self::Commitments => "floe.builtin.commitments",
            Self::Communication => "floe.builtin.communication",
            Self::Relationships => "floe.builtin.relationships",
            Self::FocusAttention => "floe.builtin.focus-attention",
            Self::Wellbeing => "floe.builtin.wellbeing",
            Self::WorkContext => "floe.builtin.work-context",
            Self::LifeLogistics => "floe.builtin.life-logistics",
        }
    }

    pub fn from_package_id(package_id: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|expert| expert.package_id() == package_id)
    }

    pub const fn context_data_class(self) -> DataClass {
        if matches!(self, Self::Wellbeing) {
            DataClass::HighlySensitive
        } else {
            DataClass::Personal
        }
    }

    pub const fn required_sources(self) -> &'static [BuiltinContextSource] {
        use BuiltinContextSource::*;

        match self {
            Self::Schedule => &[Calendar],
            Self::Commitments => &[Mail, Calendar, Tasks, ConfirmedMemory],
            Self::Communication => &[Mail],
            Self::Relationships => &[Contacts, ConfirmedInteractions],
            Self::FocusAttention => &[Attention, Calendar, WorkContext],
            Self::Wellbeing => &[Wellbeing, Calendar],
            Self::WorkContext => &[WorkContext],
            Self::LifeLogistics => &[Logistics],
        }
    }

    pub const fn mandatory_source(self) -> BuiltinContextSource {
        match self {
            Self::Schedule => BuiltinContextSource::Calendar,
            Self::Commitments | Self::Communication => BuiltinContextSource::Mail,
            Self::Relationships => BuiltinContextSource::Contacts,
            Self::FocusAttention => BuiltinContextSource::Attention,
            Self::Wellbeing => BuiltinContextSource::Wellbeing,
            Self::WorkContext => BuiltinContextSource::WorkContext,
            Self::LifeLogistics => BuiltinContextSource::Logistics,
        }
    }

    pub const fn supports_device_model(self) -> bool {
        matches!(
            self,
            Self::Schedule | Self::Relationships | Self::FocusAttention | Self::Wellbeing
        )
    }

    pub const fn result_artifact_name(self) -> &'static str {
        match self {
            Self::Schedule => "Schedule expert result",
            Self::Commitments => "Commitments expert result",
            Self::Communication => "Communication expert result",
            Self::Relationships => "Relationships expert result",
            Self::FocusAttention => "Focus & Attention expert result",
            Self::Wellbeing => "Wellbeing expert result",
            Self::WorkContext => "Work Context expert result",
            Self::LifeLogistics => "Life Logistics expert result",
        }
    }

    pub(crate) fn metadata(self) -> (&'static str, &'static str, Vec<&'static str>, &'static str) {
        match self {
            Self::Schedule => (
                "Schedule Expert",
                "Reviews calendars, availability, conflicts, and the realism of plans from a scheduling perspective.",
                vec!["schedule", "calendar"],
                "Provide independent scheduling judgment",
            ),
            Self::Commitments => (
                "Commitments Expert",
                "Finds obligations and follow-ups across the bounded personal context granted to it.",
                vec!["commitments", "planning"],
                "Review commitments and follow-ups",
            ),
            Self::Communication => (
                "Communication Expert",
                "Assesses whether communication needs a response and prepares reviewable drafts.",
                vec!["communication"],
                "Recommend bounded communication actions",
            ),
            Self::Relationships => (
                "Relationships Expert",
                "Reviews explicitly granted people and confirmed-interaction context for follow-ups.",
                vec!["relationships"],
                "Identify relationship follow-ups",
            ),
            Self::FocusAttention => (
                "Focus & Attention Expert",
                "Combines bounded attention, schedule, and active-work context into focus guidance.",
                vec!["focus", "attention"],
                "Recommend focus protection",
            ),
            Self::Wellbeing => (
                "Wellbeing Expert",
                "Uses coarse derived wellbeing and schedule context to recommend sustainable load.",
                vec!["wellbeing"],
                "Recommend sustainable schedule load",
            ),
            Self::WorkContext => (
                "Work Context Expert",
                "Synthesizes bounded work context into blockers and next actions.",
                vec!["work"],
                "Identify work blockers and next actions",
            ),
            Self::LifeLogistics => (
                "Life Logistics Expert",
                "Synthesizes bounded logistics context into preparation recommendations.",
                vec!["life", "logistics"],
                "Recommend logistics preparation",
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BuiltinContextSource {
    Calendar,
    Mail,
    Tasks,
    ConfirmedMemory,
    Contacts,
    ConfirmedInteractions,
    Attention,
    WorkContext,
    Wellbeing,
    Logistics,
}

/// What has to be answering for a builtin source to serve a run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuiltinSourceRequirement {
    /// This device's own connection to the source serves it.
    DeviceConnection,
    /// This device serves it with nothing to connect to.
    Device,
    /// Only the Person's paired server serves it.
    PairedServer,
    /// Nothing serves it yet.
    Unserved,
}

impl BuiltinContextSource {
    pub const fn capability_id(self) -> &'static str {
        match self {
            Self::Calendar => "calendar.timeline",
            Self::Mail => "mail.communication",
            Self::Tasks => "floe.tasks",
            Self::ConfirmedMemory => "memory.confirmed",
            Self::Contacts => "people.identity",
            Self::ConfirmedInteractions => "relationships.confirmed_interactions",
            Self::Attention => "attention.coarse",
            Self::WorkContext => "work.context",
            Self::Wellbeing => "wellbeing.derived",
            Self::Logistics => "life.logistics",
        }
    }

    /// What has to be answering for this source to serve a run.
    pub const fn requirement(self) -> BuiltinSourceRequirement {
        match self {
            Self::Calendar => BuiltinSourceRequirement::DeviceConnection,
            Self::Tasks | Self::ConfirmedMemory => BuiltinSourceRequirement::Device,
            Self::Mail | Self::ConfirmedInteractions | Self::WorkContext | Self::Logistics => {
                BuiltinSourceRequirement::PairedServer
            }
            Self::Contacts | Self::Attention | Self::Wellbeing => {
                BuiltinSourceRequirement::Unserved
            }
        }
    }

    /// The stable id this source is bound under in the registry.
    pub const fn source_id(self) -> &'static str {
        match self {
            Self::Calendar => "floe.source.calendar",
            Self::Mail => "floe.source.mail",
            Self::Tasks => "floe.source.tasks",
            Self::ConfirmedMemory => "floe.source.confirmed-memory",
            Self::Contacts => "floe.source.contacts",
            Self::ConfirmedInteractions => "floe.source.confirmed-interactions",
            Self::Attention => "floe.source.attention",
            Self::WorkContext => "floe.source.work-context",
            Self::Wellbeing => "floe.source.wellbeing",
            Self::Logistics => "floe.source.logistics",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_package_id_round_trips() {
        for expert in BuiltinExpertKind::ALL {
            assert_eq!(
                BuiltinExpertKind::from_package_id(expert.package_id()),
                Some(expert)
            );
            assert!(
                expert
                    .required_sources()
                    .contains(&expert.mandatory_source())
            );
        }
    }
}
