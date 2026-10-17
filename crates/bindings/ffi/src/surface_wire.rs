//! Mechanical projection of Conversation-owned Surface/Work custody.
use crate::app_wire::{AppWireResult, agent_failure, internal_error, validation};
use floe_app::CallerContext;
use floe_conversation as owner;
use floe_protocol::*;

pub(crate) fn anchor_in(value: SurfaceAnchorDto) -> AppWireResult<owner::SurfaceAnchor> {
    value.validate().map_err(validation)?;
    let digest = crate::conversation_wire::digest_bytes(&value.digest)?;
    let value = owner::SurfaceAnchor {
        session_id: value.session_id.get(),
        sequence: value.sequence,
        digest,
    };
    value.validate().map_err(|_| validation("surface.anchor"))?;
    Ok(value)
}
pub(crate) fn anchor_out(value: owner::SurfaceAnchor) -> AppWireResult<SurfaceAnchorDto> {
    value.validate().map_err(|_| internal_error())?;
    Ok(SurfaceAnchorDto {
        session_id: SessionRefDto::new(value.session_id).ok_or_else(internal_error)?,
        sequence: value.sequence,
        digest: DigestHex64Dto::new(value.digest.iter().map(|b| format!("{b:02x}")).collect())
            .ok_or_else(internal_error)?,
    })
}
macro_rules! reference {
    ($kind:ident, $value:expr) => {
        $kind::new($value).ok_or_else(internal_error)?
    };
}
pub(crate) fn surface_out(value: owner::SurfaceSnapshot) -> AppWireResult<SurfaceSnapshotDto> {
    Ok(SurfaceSnapshotDto {
        through: anchor_out(value.through)?,
        older: value.older.map(anchor_out).transpose()?,
        messages: value
            .messages
            .into_iter()
            .map(|message| {
                let content = match message.content {
                    owner::SurfaceMessageContent::Input { text, origin } => {
                        SurfaceMessageContentDto::Input {
                            text,
                            origin: match origin {
                                owner::SurfaceInputKind::Text => SurfaceInputKindDto::Text,
                                owner::SurfaceInputKind::Voice => SurfaceInputKindDto::Voice,
                            },
                        }
                    }
                    owner::SurfaceMessageContent::Result {
                        run_id,
                        completeness,
                        text,
                    } => SurfaceMessageContentDto::Result {
                        run_id: reference!(RunRefDto, run_id.as_uuid()),
                        text,
                        completeness: match completeness {
                            owner::SurfaceResultCompleteness::Complete => {
                                SurfaceResultCompletenessDto::Complete
                            }
                            owner::SurfaceResultCompleteness::Partial => {
                                SurfaceResultCompletenessDto::Partial
                            }
                        },
                    },
                    owner::SurfaceMessageContent::Outcome {
                        run_id,
                        state,
                        issue,
                        interaction_refs,
                    } => SurfaceMessageContentDto::Outcome {
                        run_id: reference!(RunRefDto, run_id.as_uuid()),
                        issue: issue.map(agent_failure),
                        state: match state {
                            owner::RunState::Blocked => SurfaceOutcomeStateDto::Blocked,
                            owner::RunState::Failed => SurfaceOutcomeStateDto::Failed,
                            owner::RunState::Cancelled => SurfaceOutcomeStateDto::Cancelled,
                            owner::RunState::TimedOut => SurfaceOutcomeStateDto::TimedOut,
                            owner::RunState::Interrupted => SurfaceOutcomeStateDto::Interrupted,
                            owner::RunState::Working | owner::RunState::Completed => {
                                return Err(internal_error());
                            }
                        },
                        interaction_refs: interaction_refs
                            .into_iter()
                            .map(|id| UuidRefDto::new(id).ok_or_else(internal_error))
                            .collect::<AppWireResult<_>>()?,
                    },
                    owner::SurfaceMessageContent::PendingWorkCancelled { work_id, input_id } => {
                        SurfaceMessageContentDto::PendingWorkCancelled {
                            work_id: reference!(UuidRefDto, work_id.as_uuid()),
                            input_id: reference!(MessageRefDto, input_id.as_uuid()),
                        }
                    }
                };
                Ok(SurfaceMessageDto {
                    message_id: reference!(MessageRefDto, message.message_id.as_uuid()),
                    before: anchor_out(message.before)?,
                    through: anchor_out(message.through)?,
                    content,
                })
            })
            .collect::<AppWireResult<_>>()?,
    })
}
pub(crate) fn receipt_out(
    value: owner::SurfaceWorkReceipt,
    caller: &CallerContext,
) -> AppWireResult<SurfaceWorkReceiptDto> {
    value.validate().map_err(|_| internal_error())?;
    if value.person_id.0 != caller.person_id() || value.device_id != caller.device_id() {
        return Err(internal_error());
    }
    let origin = match &value.origin {
        owner::WorkOrigin::Text => SurfaceWorkOriginDto::Text {},
        owner::WorkOrigin::Voice { .. } => SurfaceWorkOriginDto::Voice {},
        owner::WorkOrigin::Retry { source_run_id } => SurfaceWorkOriginDto::Retry {
            source_run_id: reference!(RunRefDto, source_run_id.as_uuid()),
        },
    };
    Ok(SurfaceWorkReceiptDto {
        command_id: reference!(CommandIdDto, value.command_id.as_uuid()),
        work_id: reference!(UuidRefDto, value.work_id.as_uuid()),
        execution_command_id: reference!(CommandIdDto, value.execution_command_id().as_uuid()),
        input_id: reference!(MessageRefDto, value.input_id.as_uuid()),
        anchor: anchor_out(value.anchor)?,
        origin,
        runtime_epoch: caller.runtime_epoch(),
    })
}
pub(crate) fn work_out(
    value: owner::SurfaceWorkSnapshot,
    caller: &CallerContext,
) -> AppWireResult<SurfaceWorkSnapshotDto> {
    if matches!(value.state, owner::SurfaceWorkState::Linked(_)) != value.current_run_id.is_some() {
        return Err(internal_error());
    }
    Ok(SurfaceWorkSnapshotDto {
        receipt: receipt_out(value.receipt, caller)?,
        input_text: value.input_text,
        state: match value.state {
            owner::SurfaceWorkState::Queued => SurfaceWorkStateDto::Queued {},
            owner::SurfaceWorkState::Cancelled => SurfaceWorkStateDto::Cancelled {},
            owner::SurfaceWorkState::Linked(run_id) => SurfaceWorkStateDto::Linked {
                run_id: reference!(RunRefDto, run_id.as_uuid()),
            },
        },
        current_run_id: value
            .current_run_id
            .map(|id| RunRefDto::new(id.as_uuid()).ok_or_else(internal_error))
            .transpose()?,
    })
}
pub(crate) fn page_out(
    value: owner::SurfaceWorkPage,
    caller: &CallerContext,
) -> AppWireResult<SurfaceWorkPageDto> {
    Ok(SurfaceWorkPageDto {
        items: value
            .items
            .into_iter()
            .map(|item| work_out(item, caller))
            .collect::<AppWireResult<_>>()?,
        next: value
            .next
            .map(|cursor| {
                Ok(SurfaceWorkCursorDto {
                    through: reference!(CommandIdDto, cursor.through.as_uuid()),
                    before: reference!(CommandIdDto, cursor.before.as_uuid()),
                })
            })
            .transpose()?,
    })
}
pub(crate) fn cancellation_out(
    value: owner::PendingWorkCancellationReceipt,
    caller: &CallerContext,
) -> AppWireResult<PendingWorkCancellationReceiptDto> {
    value.validate().map_err(|_| internal_error())?;
    if value.person_id.0 != caller.person_id() || value.device_id != caller.device_id() {
        return Err(internal_error());
    }
    Ok(PendingWorkCancellationReceiptDto {
        command_id: reference!(CommandIdDto, value.command_id.as_uuid()),
        work_command_id: reference!(CommandIdDto, value.work_command_id.as_uuid()),
        work_id: reference!(UuidRefDto, value.work_id.as_uuid()),
        runtime_epoch: caller.runtime_epoch(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use floe_kernel::{CommandId, OwnerActor, PersonId};
    use uuid::Uuid;

    #[test]
    fn surface_voice_projection_omits_provider_identity_and_preserves_original_text() {
        let session_id = Uuid::new_v4();
        let actor = OwnerActor {
            person_id: PersonId::new(),
            device_id: "fixture-device".into(),
            runtime_epoch: 1,
        };
        let input = owner::AppendSurfaceInput {
            command_id: CommandId::new(),
            expected: owner::SurfaceAnchor::empty(session_id).unwrap(),
            input: owner::SurfaceInput {
                message_id: serde_json::from_value(serde_json::json!(Uuid::new_v4())).unwrap(),
                text: "원래 발화".into(),
                origin: owner::SurfaceInputOrigin::Voice {
                    voice_session_id: Uuid::new_v4(),
                    utterance_id: "provider-utterance-secret-marker".into(),
                },
            },
        }
        .admit(&actor)
        .unwrap();
        let snapshot = owner::SurfaceSnapshot::from_suffix(
            input.through,
            vec![owner::SurfaceMessage::input(&input).unwrap()],
        )
        .unwrap();
        let value = surface_out(snapshot).unwrap();
        let json = serde_json::to_string(&value).unwrap();
        assert!(json.contains("원래 발화"));
        assert!(!json.contains("provider-utterance-secret-marker"));
        assert!(!json.contains("voice_session_id"));
        assert!(!json.contains("turn_id"));
        assert_eq!(anchor_in(value.through).unwrap(), input.through);
    }

    #[test]
    fn text_work_command_maps_exact_prefix_and_rejects_a_forged_empty_prefix() {
        let fixture = include_str!(
            "../../protocol/tests/fixtures/conversation_surface/submit_text_work.json"
        );
        let request: AppCommandRequestDto = serde_json::from_str(fixture).unwrap();
        let AppCommandDto::Product(command) = request.command else {
            panic!("product command")
        };
        let floe_app::ConversationCommand::SubmitTextWork { expected, text } =
            crate::conversation_wire::command_in(command).unwrap()
        else {
            panic!("Work command")
        };
        assert_eq!(
            expected,
            owner::SurfaceAnchor::empty(expected.session_id).unwrap()
        );
        assert_eq!(text, "오늘 할 일의 우선순위를 같이 정리해줘");
        let mut forged = anchor_out(expected).unwrap();
        forged.digest = DigestHex64Dto::new("00".repeat(32)).unwrap();
        assert!(anchor_in(forged).is_err());
    }
}
