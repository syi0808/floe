//! Bounded normalized live-model control values. Raw audio, SDP, credentials,
//! provider wire names and Manager/Tool commands do not belong in these values.
use crate::ModelContractError;
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

pub const MAX_LIVE_ID_BYTES: usize = 256;
pub const MAX_LIVE_TEXT_BYTES: usize = 64 * 1024;

fn identifier(value: &str) -> Result<(), ModelContractError> {
    if value.is_empty()
        || value.len() > MAX_LIVE_ID_BYTES
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(ModelContractError::Invalid);
    }
    Ok(())
}
fn text(value: &str, final_text: bool) -> Result<(), ModelContractError> {
    if value.len() > MAX_LIVE_TEXT_BYTES {
        return Err(ModelContractError::Bounds);
    }
    if final_text && value.trim().is_empty() {
        return Err(ModelContractError::Invalid);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveSpeaker {
    Person,
    Assistant,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiveEventKind {
    PersonUtterance,
    AssistantUtterance,
    Delegation,
}

/// This identity is meaningful only inside its owning live session. It never
/// supplies a Conversation MessageId, RunId, user intent or approval by itself.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct LiveEventKey<'a> {
    pub kind: LiveEventKind,
    pub id: &'a str,
}
impl fmt::Debug for LiveEventKey<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LiveEventKey")
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LiveUsageObservation {
    pub tokens: Option<u64>,
    pub cost_micros: Option<u64>,
}
impl LiveUsageObservation {
    pub fn validate(&self) -> Result<(), ModelContractError> {
        if [self.tokens, self.cost_micros]
            .into_iter()
            .flatten()
            .any(|v| v > 9_007_199_254_740_991)
        {
            return Err(ModelContractError::Bounds);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveEndEvidence {
    ProviderConfirmed,
    Disconnected,
}

/// A normalized control stream, not provider JSON. Only final utterances and
/// delegation identities have durable event keys; partial captions/activity do
/// not manufacture stable user messages. A delegation's hint is untrusted data.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum LiveProviderEvent {
    Ready {},
    TranscriptDelta {
        speaker: LiveSpeaker,
        utterance_id: Option<String>,
        text: String,
    },
    PersonUtteranceFinal {
        utterance_id: String,
        text: String,
    },
    AssistantUtteranceFinal {
        utterance_id: String,
        text: String,
    },
    DelegationRequested {
        delegation_id: String,
        utterance_ref: Option<String>,
        input_hint: Option<String>,
    },
    InputActivity {
        active: bool,
    },
    OutputActivity {
        active: bool,
    },
    Ended {
        evidence: LiveEndEvidence,
        usage: LiveUsageObservation,
    },
}
impl LiveProviderEvent {
    pub fn validate(&self) -> Result<(), ModelContractError> {
        match self {
            Self::TranscriptDelta {
                utterance_id,
                text: value,
                ..
            } => {
                if let Some(id) = utterance_id {
                    identifier(id)?;
                }
                text(value, false)
            }
            Self::PersonUtteranceFinal {
                utterance_id,
                text: value,
            }
            | Self::AssistantUtteranceFinal {
                utterance_id,
                text: value,
            } => {
                identifier(utterance_id)?;
                text(value, true)
            }
            Self::DelegationRequested {
                delegation_id,
                utterance_ref,
                input_hint,
            } => {
                identifier(delegation_id)?;
                if let Some(id) = utterance_ref {
                    identifier(id)?;
                }
                if let Some(hint) = input_hint {
                    text(hint, false)?;
                }
                Ok(())
            }
            Self::Ended { usage, .. } => usage.validate(),
            Self::Ready {} | Self::InputActivity { .. } | Self::OutputActivity { .. } => Ok(()),
        }
    }
    pub fn durable_key(&self) -> Option<LiveEventKey<'_>> {
        match self {
            Self::PersonUtteranceFinal { utterance_id, .. } => Some(LiveEventKey {
                kind: LiveEventKind::PersonUtterance,
                id: utterance_id,
            }),
            Self::AssistantUtteranceFinal { utterance_id, .. } => Some(LiveEventKey {
                kind: LiveEventKind::AssistantUtterance,
                id: utterance_id,
            }),
            Self::DelegationRequested { delegation_id, .. } => Some(LiveEventKey {
                kind: LiveEventKind::Delegation,
                id: delegation_id,
            }),
            _ => None,
        }
    }
}
impl fmt::Debug for LiveProviderEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self {
            Self::Ready {} => "Ready",
            Self::TranscriptDelta { .. } => "TranscriptDelta",
            Self::PersonUtteranceFinal { .. } => "PersonUtteranceFinal",
            Self::AssistantUtteranceFinal { .. } => "AssistantUtteranceFinal",
            Self::DelegationRequested { .. } => "DelegationRequested",
            Self::InputActivity { .. } => "InputActivity",
            Self::OutputActivity { .. } => "OutputActivity",
            Self::Ended { .. } => "Ended",
        };
        f.debug_struct(kind).finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveAppendKind {
    Progress,
    PresentableResult,
    Instruction,
}

/// One host-owned logical append. Adapter framing may split its text only after
/// whole-batch preflight. This ID is correlation, not a provider idempotency promise.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LiveAppendPayload {
    pub operation_id: Uuid,
    pub kind: LiveAppendKind,
    pub delegation_id: Option<String>,
    pub text: String,
}
impl LiveAppendPayload {
    pub fn validate(&self) -> Result<(), ModelContractError> {
        if self.operation_id.is_nil() {
            return Err(ModelContractError::Invalid);
        }
        if let Some(id) = &self.delegation_id {
            identifier(id)?;
        }
        text(&self.text, true)
    }
}
impl fmt::Debug for LiveAppendPayload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LiveAppendPayload")
            .field("kind", &self.kind)
            .field("text_bytes", &self.text.len())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_only_delegation_is_valid_and_does_not_invent_a_user_message() {
        let event = LiveProviderEvent::DelegationRequested {
            delegation_id: "d-1".into(),
            utterance_ref: None,
            input_hint: None,
        };
        event.validate().unwrap();
        assert_eq!(event.durable_key().unwrap().kind, LiveEventKind::Delegation);
        let json = serde_json::to_value(&event).unwrap();
        assert!(json.get("text").is_none());
        assert_eq!(
            serde_json::from_value::<LiveProviderEvent>(json).unwrap(),
            event
        );
    }
    #[test]
    fn deltas_and_activity_are_ephemeral_and_both_directions_are_valid() {
        for event in [
            LiveProviderEvent::TranscriptDelta {
                speaker: LiveSpeaker::Person,
                utterance_id: None,
                text: "아직".into(),
            },
            LiveProviderEvent::InputActivity { active: true },
            LiveProviderEvent::OutputActivity { active: true },
        ] {
            event.validate().unwrap();
            assert!(event.durable_key().is_none());
        }
    }
    #[test]
    fn final_identity_requires_actual_id_and_keeps_distinct_event_kinds() {
        let a = LiveProviderEvent::PersonUtteranceFinal {
            utterance_id: "turn-1".into(),
            text: "Original text".into(),
        };
        let b = LiveProviderEvent::AssistantUtteranceFinal {
            utterance_id: "turn-1".into(),
            text: "Original text".into(),
        };
        assert_ne!(a.durable_key(), b.durable_key());
        let missing = LiveProviderEvent::PersonUtteranceFinal {
            utterance_id: String::new(),
            text: "Do not invent an ID".into(),
        };
        assert_eq!(missing.validate(), Err(ModelContractError::Invalid));
    }
    #[test]
    fn byte_limits_preserve_unicode_instead_of_silently_truncating() {
        let text = "가".repeat(MAX_LIVE_TEXT_BYTES / 3 + 1);
        let event = LiveProviderEvent::PersonUtteranceFinal {
            utterance_id: "turn".into(),
            text,
        };
        assert_eq!(event.validate(), Err(ModelContractError::Bounds));
        assert!(
            serde_json::from_str::<LiveProviderEvent>(r#"{"kind":"ready","token":"not-allowed"}"#)
                .is_err()
        );
        assert!(
            serde_json::from_str::<LiveProviderEvent>(r#"{"kind":"tool_call","text":"do it"}"#)
                .is_err()
        );
    }
    #[test]
    fn unknown_usage_is_not_zero_and_remote_end_is_not_playback() {
        assert_eq!(LiveUsageObservation::default().tokens, None);
        assert_ne!(
            LiveUsageObservation::default(),
            LiveUsageObservation {
                tokens: Some(0),
                cost_micros: Some(0)
            }
        );
        let ended = LiveProviderEvent::Ended {
            evidence: LiveEndEvidence::ProviderConfirmed,
            usage: LiveUsageObservation::default(),
        };
        ended.validate().unwrap();
        assert!(ended.durable_key().is_none());
    }
    #[test]
    fn diagnostics_redact_transcript_hint_and_append_content() {
        let event = LiveProviderEvent::DelegationRequested {
            delegation_id: "private-id".into(),
            utterance_ref: None,
            input_hint: Some("private-body".into()),
        };
        let debug = format!("{event:?} {:?}", event.durable_key());
        assert!(!debug.contains("private-id") && !debug.contains("private-body"));
        let append = LiveAppendPayload {
            operation_id: Uuid::new_v4(),
            kind: LiveAppendKind::PresentableResult,
            delegation_id: None,
            text: "private-result".into(),
        };
        append.validate().unwrap();
        assert!(!format!("{append:?}").contains("private-result"));
    }
}
