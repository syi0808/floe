use sha2::{Digest, Sha256};
use uuid::Uuid;

use floe_agent_contract::{AgentFailure, CommandId, RunId};

use super::TurnMode;

pub const MAX_TURN_TEXT_BYTES: usize = 8_192;
const MAX_PROFILE_ID_BYTES: usize = 128;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProfileSelection {
    Auto,
    Explicit(String),
}

impl ProfileSelection {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if let Self::Explicit(profile_id) = self
            && (profile_id.trim() != profile_id
                || profile_id.is_empty()
                || profile_id.len() > MAX_PROFILE_ID_BYTES
                || profile_id.chars().any(char::is_control))
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StartTurn {
    pub command_id: CommandId,
    pub session_id: Uuid,
    pub expected_revision: u64,
    pub text: String,
    pub mode: TurnMode,
    pub retry_of: Option<RunId>,
    pub profile: ProfileSelection,
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
            || self.retry_of.is_some() && !matches!(&self.mode, TurnMode::New)
        {
            return Err(AgentFailure::InvalidInput);
        }
        self.profile.validate()?;
        if let TurnMode::Continue(reference) = &self.mode {
            reference.validate()?;
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
    pub profile: ProfileSelection,
}

impl CanonicalTurnIntent {
    pub fn from_start_turn(turn: &mut StartTurn) -> Result<Self, AgentFailure> {
        turn.validate_fields()?;
        turn.normalize_text()?;
        Ok(Self {
            session_id: turn.session_id,
            expected_revision: turn.expected_revision,
            text: turn.text.clone(),
            mode: turn.mode.clone(),
            retry_of: turn.retry_of,
            profile: turn.profile.clone(),
        })
    }

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
        match &self.profile {
            ProfileSelection::Auto => bytes.push(0),
            ProfileSelection::Explicit(profile_id) => {
                bytes.push(1);
                append_string(&mut bytes, profile_id)?;
            }
        }
        Ok(Sha256::digest(bytes).into())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdmittedExecution {
    pub receipt: super::RunReceipt,
    pub intent: CanonicalTurnIntent,
    pub profile_preference: ProfileSelection,
}

impl AdmittedExecution {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.receipt.validate()?;
        if self.profile_preference != self.intent.profile {
            return Err(AgentFailure::StorageUnavailable);
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn turn(text: &str) -> StartTurn {
        StartTurn {
            command_id: CommandId::new(),
            session_id: Uuid::new_v4(),
            expected_revision: 0,
            text: text.into(),
            mode: TurnMode::New,
            retry_of: None,
            profile: ProfileSelection::Auto,
        }
    }

    #[test]
    fn canonical_digest_is_stable_and_ignores_command_id() {
        let mut first = turn("  hello  ");
        let mut second = first.clone();
        second.command_id = CommandId::new();
        let first_intent = CanonicalTurnIntent::from_start_turn(&mut first).unwrap();
        let second_intent = CanonicalTurnIntent::from_start_turn(&mut second).unwrap();
        assert_eq!(first_intent.text, "hello");
        assert_eq!(second_intent.text, "hello");
        assert_eq!(
            first_intent.digest("person"),
            second_intent.digest("person")
        );
    }

    #[test]
    fn canonical_digest_binds_principal_and_excludes_runtime_inputs() {
        let mut turn = turn("hello");
        let intent = CanonicalTurnIntent::from_start_turn(&mut turn).unwrap();
        assert_ne!(intent.digest("person-a"), intent.digest("person-b"));
    }

    #[test]
    fn explicit_profile_is_preserved_and_changes_same_command_intent() {
        let mut automatic = turn("hello");
        let mut explicit = automatic.clone();
        explicit.profile = ProfileSelection::Explicit("local-fast".into());

        let automatic = CanonicalTurnIntent::from_start_turn(&mut automatic).unwrap();
        let explicit = CanonicalTurnIntent::from_start_turn(&mut explicit).unwrap();

        assert_eq!(automatic.profile, ProfileSelection::Auto);
        assert_eq!(
            explicit.profile,
            ProfileSelection::Explicit("local-fast".into())
        );
        assert_ne!(automatic.digest("person"), explicit.digest("person"));
    }

    #[test]
    fn normalization_uses_utf8_byte_limit() {
        assert!(normalize_turn_text(&"한".repeat(MAX_TURN_TEXT_BYTES / 3 + 1)).is_err());
        assert_eq!(
            normalize_turn_text("\u{0085}\thello\t\u{0085}").unwrap(),
            "hello"
        );
        assert_eq!(normalize_turn_text("hello\nworld").unwrap(), "hello\nworld");
        assert!(normalize_turn_text("hello\tworld").is_err());
        assert!(normalize_turn_text("hello\rworld").is_err());

        let exact = "한".repeat(2_730) + "ab";
        assert_eq!(exact.len(), MAX_TURN_TEXT_BYTES);
        assert_eq!(normalize_turn_text(&exact).unwrap(), exact);
        assert!(normalize_turn_text(&(exact + "c")).is_err());
    }
}
