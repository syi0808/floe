//! Which builtin Experts exist, what each one reads and how it is packaged.
//!
//! This is a declaration, not an installation: nothing here issues a grant or
//! reveals that a Person has a source. The registry stores what the composition
//! root installs from it.

use serde::{Deserialize, Serialize};

use floe_agent_contract::DataClass;

pub const BUILTIN_EXPERT_PACKAGE_VERSION: &str = "1.0.1";
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
                "Assesses availability, conflicts, and time constraints from selected calendar evidence. Develops scheduling recommendations and reviewable calendar proposals within the observed time range.",
                vec!["schedule", "calendar"],
                "Provide independent scheduling judgment",
            ),
            Self::Commitments => (
                "Commitments Expert",
                "Identifies obligations, deadlines, expected replies, and unresolved follow-ups from selected communication evidence, with available calendar, task, and confirmed-memory context. Focuses on what remains owed or unfinished.",
                vec!["commitments", "planning"],
                "Review commitments and follow-ups",
            ),
            Self::Communication => (
                "Communication Expert",
                "Assesses selected communications for response need and appropriate tone, and prepares reviewable drafts grounded in their content. Focuses on understanding and responding to communications rather than sending them.",
                vec!["communication"],
                "Recommend bounded communication actions",
            ),
            Self::Relationships => (
                "Relationships Expert",
                "Resolves people from selected identity evidence and uses available confirmed-interaction context to identify relationship-relevant follow-ups. Preserves uncertainty where identity or interaction history is incomplete.",
                vec!["relationships"],
                "Identify relationship follow-ups",
            ),
            Self::FocusAttention => (
                "Focus & Attention Expert",
                "Interprets coarse attention observations, with available calendar and work context, to assess interruption pressure and context switching and recommend focus protection.",
                vec!["focus", "attention"],
                "Recommend focus protection",
            ),
            Self::Wellbeing => (
                "Wellbeing Expert",
                "Interprets coarse, derived wellbeing signals and available calendar context to recommend sustainable load and recovery. Provides non-diagnostic guidance rather than medical conclusions.",
                vec!["wellbeing"],
                "Recommend sustainable schedule load",
            ),
            Self::WorkContext => (
                "Work Context Expert",
                "Connects selected project, file, meeting, decision, and communication evidence within the supplied workspace scope to identify grounded blockers and next actions.",
                vec!["work"],
                "Identify work blockers and next actions",
            ),
            Self::LifeLogistics => (
                "Life Logistics Expert",
                "Interprets selected reservation, travel, delivery, errand, and supported home-state evidence to identify preparation needs and reviewable change recommendations. Recommendations do not execute purchases or home actions.",
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
