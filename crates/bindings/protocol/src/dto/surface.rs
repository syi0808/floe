//! Safe Person-facing history and Work values. Provider event identities,
//! transport payloads and source authorization proofs never cross this DTO.
use super::{CommandIdDto, DigestHex64Dto, MessageRefDto, RunRefDto, SessionRefDto, UuidRefDto};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceAnchorDto {
    pub session_id: SessionRefDto,
    pub sequence: u64,
    pub digest: DigestHex64Dto,
}
impl SurfaceAnchorDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.sequence > i64::MAX as u64 {
            return Err("surface.sequence");
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceInputKindDto {
    Text,
    Voice,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceResultCompletenessDto {
    Complete,
    Partial,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceOutcomeStateDto {
    Blocked,
    Failed,
    Cancelled,
    TimedOut,
    Interrupted,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SurfaceMessageContentDto {
    Input {
        text: String,
        origin: SurfaceInputKindDto,
    },
    Result {
        run_id: RunRefDto,
        completeness: SurfaceResultCompletenessDto,
        text: String,
    },
    Outcome {
        run_id: RunRefDto,
        state: SurfaceOutcomeStateDto,
        issue: Option<super::AppWireErrorDto>,
        interaction_refs: Vec<UuidRefDto>,
    },
    PendingWorkCancelled {
        work_id: UuidRefDto,
        input_id: MessageRefDto,
    },
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceMessageDto {
    pub message_id: MessageRefDto,
    pub before: SurfaceAnchorDto,
    pub through: SurfaceAnchorDto,
    pub content: SurfaceMessageContentDto,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceSnapshotDto {
    pub through: SurfaceAnchorDto,
    pub messages: Vec<SurfaceMessageDto>,
    pub older: Option<SurfaceAnchorDto>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SurfaceWorkOriginDto {
    Text {},
    Voice {},
    Retry { source_run_id: RunRefDto },
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceWorkReceiptDto {
    pub command_id: CommandIdDto,
    pub work_id: UuidRefDto,
    pub execution_command_id: CommandIdDto,
    pub input_id: MessageRefDto,
    pub anchor: SurfaceAnchorDto,
    pub origin: SurfaceWorkOriginDto,
    pub runtime_epoch: u64,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SurfaceWorkStateDto {
    Queued {},
    Linked { run_id: RunRefDto },
    Cancelled {},
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceWorkSnapshotDto {
    pub receipt: SurfaceWorkReceiptDto,
    pub input_text: String,
    pub state: SurfaceWorkStateDto,
    pub current_run_id: Option<RunRefDto>,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceWorkCursorDto {
    pub through: CommandIdDto,
    pub before: CommandIdDto,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceWorkPageDto {
    pub items: Vec<SurfaceWorkSnapshotDto>,
    pub next: Option<SurfaceWorkCursorDto>,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PendingWorkCancellationReceiptDto {
    pub command_id: CommandIdDto,
    pub work_command_id: CommandIdDto,
    pub work_id: UuidRefDto,
    pub runtime_epoch: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AppCommandRequestDto, AppProductCommandDto};
    use serde_json::json;

    #[test]
    fn text_work_fixture_round_trips_without_session_revision_or_provider_fields() {
        let source =
            include_str!("../../tests/fixtures/conversation_surface/submit_text_work.json");
        let request: AppCommandRequestDto = serde_json::from_str(source).unwrap();
        request.validate().unwrap();
        let encoded = serde_json::to_value(&request).unwrap();
        assert_eq!(
            encoded,
            serde_json::from_str::<serde_json::Value>(source).unwrap()
        );
        assert!(encoded["command"].get("expected_revision").is_none());
        let mut injected = encoded;
        injected["command"]["provider_event_id"] = json!("provider-only");
        assert!(serde_json::from_value::<AppCommandRequestDto>(injected).is_err());
    }

    #[test]
    fn removed_direct_turn_route_is_not_a_compatibility_alias() {
        let legacy = json!({"kind":"conversation.start_turn", "session_id":"00000000-0000-0000-0000-000000000003",
            "expected_revision":0,"text":"hello"});
        assert!(serde_json::from_value::<AppProductCommandDto>(legacy).is_err());
        let continue_without_reference = json!({"kind":"conversation.continue_turn",
            "session_id":"00000000-0000-0000-0000-000000000003", "expected_revision":1});
        assert!(
            serde_json::from_value::<AppProductCommandDto>(continue_without_reference).is_err()
        );
    }

    #[test]
    fn prefix_bounds_and_work_origin_reject_untyped_provider_metadata() {
        let value = json!({"session_id":"00000000-0000-0000-0000-000000000003",
            "sequence":(i64::MAX as u64)+1,"digest":"ab".repeat(32)});
        assert!(
            serde_json::from_value::<SurfaceAnchorDto>(value)
                .unwrap()
                .validate()
                .is_err()
        );
        assert!(
            serde_json::from_value::<SurfaceWorkOriginDto>(
                json!({"kind":"voice","delegation_id":"external"})
            )
            .is_err()
        );
        assert!(serde_json::from_value::<SurfaceWorkStateDto>(json!({"kind":"linked"})).is_err());
    }
}
