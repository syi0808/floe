use serde::{Deserialize, Serialize};

use crate::{DataClass, ExpertMetadata};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BuiltinExpertKind {
    Commitments,
    Communication,
    Relationships,
    FocusAttention,
    Wellbeing,
    WorkContext,
    LifeLogistics,
}

impl BuiltinExpertKind {
    pub const ALL: [Self; 7] = [
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

    pub(crate) fn tool_id(self) -> String {
        format!("{}.context", self.package_id())
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
            Self::Relationships | Self::FocusAttention | Self::Wellbeing
        )
    }

    pub const fn result_artifact_name(self) -> &'static str {
        match self {
            Self::Commitments => "Commitments expert result",
            Self::Communication => "Communication expert result",
            Self::Relationships => "Relationships expert result",
            Self::FocusAttention => "Focus & Attention expert result",
            Self::Wellbeing => "Wellbeing expert result",
            Self::WorkContext => "Work Context expert result",
            Self::LifeLogistics => "Life Logistics expert result",
        }
    }

    pub(crate) fn metadata(self) -> ExpertMetadata {
        let (name, description, domain_tags, skills) = match self {
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
        };
        ExpertMetadata {
            name: name.into(),
            description: description.into(),
            domain_tags: domain_tags.into_iter().map(str::to_owned).collect(),
            skills: vec![skills.into()],
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
