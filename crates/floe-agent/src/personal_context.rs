use serde::{Deserialize, Serialize};

use crate::{AGENT_VERSION, AgentFailure, ContextEvidence, DataClass};

pub const PEOPLE_VIEW_ID: &str = "people.identity";
pub const FEASIBILITY_VIEW_ID: &str = "schedule.feasibility";
pub const ATTENTION_VIEW_ID: &str = "attention.coarse";
pub const WELLBEING_VIEW_ID: &str = "wellbeing.derived";
pub const MAX_PERSONAL_CONTEXT_BYTES: usize = 32_768;
const MAX_FRESHNESS_MS: i64 = 300_000;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PeopleIdentity {
    pub identity_handle: String,
    pub display_name: String,
    pub aliases: Vec<String>,
    pub confidence_millis: u16,
    pub evidence_handles: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PeopleView {
    pub schema_version: u32,
    pub view_id: String,
    pub source_handle: String,
    pub observed_at_unix_ms: i64,
    pub expires_at_unix_ms: i64,
    pub coverage_complete: bool,
    pub identities: Vec<PeopleIdentity>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WeatherImpact {
    None,
    Minor,
    Significant,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FeasibilityItem {
    pub event_handle: String,
    pub evidence_handles: Vec<String>,
    pub travel_duration_seconds: u32,
    pub leave_by_unix_ms: i64,
    pub weather_impact: WeatherImpact,
    pub confidence_millis: u16,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FeasibilityView {
    pub schema_version: u32,
    pub view_id: String,
    pub source_handle: String,
    pub observed_at_unix_ms: i64,
    pub expires_at_unix_ms: i64,
    pub items: Vec<FeasibilityItem>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AttentionState {
    Available,
    Focused,
    HighInterruptionPressure,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AttentionView {
    pub schema_version: u32,
    pub view_id: String,
    pub source_handle: String,
    pub observed_at_unix_ms: i64,
    pub expires_at_unix_ms: i64,
    pub state: AttentionState,
    pub confidence_millis: u16,
    pub evidence_handles: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapacityState {
    Reduced,
    Typical,
    Strong,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryState {
    NeedsRecovery,
    Typical,
    Recovered,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WellbeingView {
    pub schema_version: u32,
    pub view_id: String,
    pub source_handle: String,
    pub observed_at_unix_ms: i64,
    pub expires_at_unix_ms: i64,
    pub capacity: CapacityState,
    pub recovery: RecoveryState,
    pub confidence_millis: u16,
    pub evidence_handles: Vec<String>,
}

pub trait PersonalContextProjection: Serialize {
    fn schema_version(&self) -> u32;
    fn view_id(&self) -> &str;
    fn source_handle(&self) -> &str;
    fn observed_at_unix_ms(&self) -> i64;
    fn expires_at_unix_ms(&self) -> i64;
}

macro_rules! projection {
    ($type:ty) => {
        impl PersonalContextProjection for $type {
            fn schema_version(&self) -> u32 {
                self.schema_version
            }
            fn view_id(&self) -> &str {
                &self.view_id
            }
            fn source_handle(&self) -> &str {
                &self.source_handle
            }
            fn observed_at_unix_ms(&self) -> i64 {
                self.observed_at_unix_ms
            }
            fn expires_at_unix_ms(&self) -> i64 {
                self.expires_at_unix_ms
            }
        }
    };
}

projection!(PeopleView);
projection!(FeasibilityView);
projection!(AttentionView);
projection!(WellbeingView);

pub fn validate_people_view(view: &PeopleView, now_unix_ms: i64) -> Result<(), AgentFailure> {
    validate_envelope(view, PEOPLE_VIEW_ID, now_unix_ms)?;
    if view.identities.len() > 64 {
        return Err(AgentFailure::BudgetExceeded);
    }
    for (index, identity) in view.identities.iter().enumerate() {
        if !valid_handle(&identity.identity_handle)
            || identity.display_name.trim().is_empty()
            || identity.display_name.len() > 256
            || identity.aliases.len() > 8
            || identity
                .aliases
                .iter()
                .any(|alias| alias.trim().is_empty() || alias.len() > 256)
            || identity.confidence_millis == 0
            || identity.confidence_millis > 1000
            || identity.evidence_handles.is_empty()
            || identity.evidence_handles.len() > 16
            || identity
                .evidence_handles
                .iter()
                .any(|value| !valid_handle(value))
            || view.identities[..index]
                .iter()
                .any(|other| other.identity_handle == identity.identity_handle)
        {
            return Err(AgentFailure::InvalidInput);
        }
    }
    validate_size(view)
}

pub fn validate_feasibility_view(
    view: &FeasibilityView,
    now_unix_ms: i64,
) -> Result<(), AgentFailure> {
    validate_envelope(view, FEASIBILITY_VIEW_ID, now_unix_ms)?;
    if view.items.len() > 16 {
        return Err(AgentFailure::BudgetExceeded);
    }
    for (index, item) in view.items.iter().enumerate() {
        if !valid_handle(&item.event_handle)
            || item.evidence_handles.is_empty()
            || item.evidence_handles.len() > 8
            || item
                .evidence_handles
                .iter()
                .any(|value| !valid_handle(value))
            || item.travel_duration_seconds > 86_400
            || item.leave_by_unix_ms < 0
            || item.confidence_millis == 0
            || item.confidence_millis > 1000
            || view.items[..index]
                .iter()
                .any(|other| other.event_handle == item.event_handle)
        {
            return Err(AgentFailure::InvalidInput);
        }
    }
    validate_size(view)
}

pub fn validate_attention_view(view: &AttentionView, now_unix_ms: i64) -> Result<(), AgentFailure> {
    validate_envelope(view, ATTENTION_VIEW_ID, now_unix_ms)?;
    validate_derived(
        view.confidence_millis,
        &view.evidence_handles,
        matches!(view.state, AttentionState::Unknown),
    )?;
    validate_size(view)
}

pub fn validate_wellbeing_view(view: &WellbeingView, now_unix_ms: i64) -> Result<(), AgentFailure> {
    validate_envelope(view, WELLBEING_VIEW_ID, now_unix_ms)?;
    validate_derived(
        view.confidence_millis,
        &view.evidence_handles,
        matches!(view.capacity, CapacityState::Unknown)
            && matches!(view.recovery, RecoveryState::Unknown),
    )?;
    validate_size(view)
}

pub fn personal_context_evidence(
    view: &impl PersonalContextProjection,
) -> Result<ContextEvidence, AgentFailure> {
    Ok(ContextEvidence {
        source_handle: view.source_handle().into(),
        data_class: DataClass::Personal,
        untrusted_text: serde_json::to_string(view).map_err(|_| AgentFailure::InvalidInput)?,
        expires_at_unix_ms: u64::try_from(view.expires_at_unix_ms())
            .map_err(|_| AgentFailure::InvalidInput)?,
    })
}

fn validate_envelope(
    view: &impl PersonalContextProjection,
    expected_view_id: &str,
    now_unix_ms: i64,
) -> Result<(), AgentFailure> {
    if view.schema_version() != AGENT_VERSION
        || view.view_id() != expected_view_id
        || !valid_handle(view.source_handle())
        || view.observed_at_unix_ms() > now_unix_ms
        || view.expires_at_unix_ms() <= now_unix_ms
        || view.expires_at_unix_ms() <= view.observed_at_unix_ms()
        || view.expires_at_unix_ms() - view.observed_at_unix_ms() > MAX_FRESHNESS_MS
    {
        Err(AgentFailure::InvalidInput)
    } else {
        Ok(())
    }
}

fn validate_derived(
    confidence_millis: u16,
    evidence_handles: &[String],
    unknown: bool,
) -> Result<(), AgentFailure> {
    if confidence_millis > 1000
        || unknown != (confidence_millis == 0)
        || evidence_handles.len() > 16
        || (!unknown && evidence_handles.is_empty())
        || evidence_handles.iter().any(|value| !valid_handle(value))
    {
        Err(AgentFailure::InvalidInput)
    } else {
        Ok(())
    }
}

fn validate_size(view: &impl Serialize) -> Result<(), AgentFailure> {
    if serde_json::to_vec(view)
        .map_err(|_| AgentFailure::InvalidInput)?
        .len()
        > MAX_PERSONAL_CONTEXT_BYTES
    {
        Err(AgentFailure::BudgetExceeded)
    } else {
        Ok(())
    }
}

fn valid_handle(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 128
}
