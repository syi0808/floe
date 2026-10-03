//! Pure validation shared by builtin domain judgments.
use floe_agent_contract::AgentFailure;

pub(crate) const MAX_MAIL_EXPERT_FINDINGS: usize = 16;

pub(crate) fn valid_text(value: &str, maximum: usize) -> bool {
    !value.trim().is_empty() && value.len() <= maximum
}

pub(crate) fn validate_summary(summary: &str) -> Result<(), AgentFailure> {
    if valid_text(summary, 2048) { Ok(()) } else { Err(AgentFailure::InvalidModelOutput) }
}
