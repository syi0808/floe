use sha2::{Digest, Sha256};
use uuid::Uuid;

use floe_agent_contract::{AgentFailure, CommandId, RunId};

use super::TurnMode;

pub const MAX_TURN_TEXT_BYTES: usize = 8_192;
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct ContinuationToken {
    pub id: Uuid,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StartTurn {
    pub command_id: CommandId,
    pub session_id: Uuid,
    pub expected_revision: u64,
    pub text: String,
    pub continuation_ref: Option<ContinuationToken>,
    pub retry_of: Option<RunId>,
}

impl StartTurn {
    pub fn normalize_text(&mut self) -> Result<(), AgentFailure> {
        self.text = normalize_turn_text(&self.text)?;
        Ok(())
    }

    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.validate_fields()?;
        normalize_turn_text(&self.text)?;
        Ok(())
    }

    fn validate_fields(&self) -> Result<(), AgentFailure> {
        if !self.command_id.is_valid()
            || self.session_id.is_nil()
            || self.retry_of.is_some_and(|run_id| !run_id.is_valid())
            || self.retry_of.is_some() && self.continuation_ref.is_some()
            || self.continuation_ref.as_ref().is_some_and(|reference| reference.id.is_nil())
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonicalTurnIntent {
    pub session_id: Uuid,
    pub expected_revision: u64,
    pub text: String,
    pub mode: TurnMode,
    pub retry_of: Option<RunId>,
}

impl CanonicalTurnIntent {
    pub fn digest(&self, principal: &str) -> Result<[u8; 32], AgentFailure> {
        validate_principal(principal)?;
        let mut bytes = Vec::with_capacity(256 + self.text.len());
        bytes.extend_from_slice(b"floe.conversation.command\0start_turn\0");
        append_string(&mut bytes, principal)?;
        bytes.extend_from_slice(self.session_id.as_bytes());
        bytes.extend_from_slice(&self.expected_revision.to_be_bytes());
        append_string(&mut bytes, &self.text)?;
        append_mode(&mut bytes, &self.mode)?;
        match self.retry_of {
            Some(run_id) => {
                bytes.push(1);
                bytes.extend_from_slice(run_id.as_uuid().as_bytes());
            }
            None => bytes.push(0),
        }
        Ok(Sha256::digest(bytes).into())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdmittedExecution {
    pub receipt: super::RunReceipt,
    pub intent: CanonicalTurnIntent,
}

impl AdmittedExecution {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.receipt.validate()?;
        Ok(())
    }
}

pub fn normalize_turn_text(text: &str) -> Result<String, AgentFailure> {
    let normalized = text.trim_matches(char::is_whitespace);
    if normalized.is_empty()
        || normalized.len() > MAX_TURN_TEXT_BYTES
        || normalized
            .chars()
            .any(|character| character.is_control() && character != '\n')
    {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(normalized.to_owned())
}

fn append_mode(bytes: &mut Vec<u8>, mode: &TurnMode) -> Result<(), AgentFailure> {
    match mode {
        TurnMode::New => bytes.push(0),
        TurnMode::Continue(reference) => {
            reference.validate()?;
            bytes.push(1);
            bytes.extend_from_slice(reference.run_id.as_uuid().as_bytes());
            bytes.extend_from_slice(&reference.executor_generation.to_be_bytes());
            bytes.push(reference.level);
        }
        TurnMode::Resume(reference) => {
            reference.validate()?;
            bytes.push(2);
            bytes.extend_from_slice(reference.origin_run_id.as_uuid().as_bytes());
            bytes.push(reference.lineage);
        }
    }
    Ok(())
}

fn append_string(bytes: &mut Vec<u8>, value: &str) -> Result<(), AgentFailure> {
    let length = u32::try_from(value.len()).map_err(|_| AgentFailure::InvalidInput)?;
    bytes.extend_from_slice(&length.to_be_bytes());
    bytes.extend_from_slice(value.as_bytes());
    Ok(())
}

fn validate_principal(principal: &str) -> Result<(), AgentFailure> {
    if principal.trim() != principal
        || principal.is_empty()
        || principal.len() > 256
        || principal.chars().any(char::is_control)
    {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(())
}
