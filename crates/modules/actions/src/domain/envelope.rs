use floe_agent_contract::AgentFailure;
use floe_context_contract::ContextDependency;
use floe_day::{CalendarProvider, PersonId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::domain::{ActionAuthorityMode, CalendarAction, CalendarActionState};

pub const MAX_AGENT_ACTION_BYTES: usize = 65_536;

/// Durable pre-dispatch intent: the proposal, the context it was derived from and
/// whether the person already granted the external write.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentActionEnvelope {
    pub action: CalendarAction,
    pub dependency: ContextDependency,
    pub write_approval: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentActionAdmission {
    pub envelope: AgentActionEnvelope,
    pub digest: String,
}

impl AgentActionEnvelope {
    pub fn validate(&self, person_id: PersonId) -> Result<(), AgentFailure> {
        if self.action.person_id != person_id
            || self.action.execution_id.is_nil()
            || self.action.direct
            || self.action.mutation.is_some()
            || self.action.agent_origin.is_none()
            || self.dependency.person_id() != person_id
            || self.action.expires_at > self.dependency.expires_at()
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let origin = self
            .action
            .agent_origin
            .as_ref()
            .ok_or(AgentFailure::PolicyDenied)?;
        if origin.invocation_id != self.action.execution_id
            || !self
                .dependency
                .resources()
                .iter()
                .any(|resource| resource.as_str() == self.action.calendar_id)
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let connector = match self.action.provider {
            CalendarProvider::EventKit => "calendar.event_kit",
            CalendarProvider::Android => "calendar.android",
            CalendarProvider::Fixture => "calendar.fixture",
            CalendarProvider::Google => "calendar.google",
            CalendarProvider::Microsoft => "calendar.microsoft",
        };
        if self.dependency.source().connector().as_str() != connector {
            return Err(AgentFailure::PolicyDenied);
        }
        self.dependency
            .validate()
            .map_err(|_| AgentFailure::InvalidInput)?;
        let bytes = self.canonical_bytes()?;
        if bytes.is_empty() || bytes.len() > MAX_AGENT_ACTION_BYTES {
            return Err(AgentFailure::BudgetExceeded);
        }
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, AgentFailure> {
        let mut action = self.action.clone();
        action.state = CalendarActionState::Pending;
        serde_json::to_vec(&(action, &self.dependency, self.write_approval))
            .map_err(|_| AgentFailure::InvalidInput)
    }

    pub fn digest(&self) -> Result<String, AgentFailure> {
        Ok(format!("{:x}", Sha256::digest(self.canonical_bytes()?)))
    }
}

pub fn valid_action_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub fn action_state_name(state: &CalendarActionState) -> &'static str {
    match state {
        CalendarActionState::Pending => "pending",
        CalendarActionState::Approved => "approved",
        CalendarActionState::Rejected => "rejected",
        CalendarActionState::Executing => "executing",
        CalendarActionState::Blocked { .. } => "blocked",
        CalendarActionState::Unknown { .. } => "unknown",
        CalendarActionState::Succeeded { .. } => "succeeded",
    }
}

pub fn action_policy_mode_name(mode: ActionAuthorityMode) -> &'static str {
    match mode {
        ActionAuthorityMode::Allow => "allow",
        ActionAuthorityMode::Ask => "ask",
        ActionAuthorityMode::Deny => "deny",
    }
}
