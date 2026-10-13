//! Mechanical conversion between the Memory DTOs and Knowledge owner types.
use crate::app_wire::{AppWireResult, internal_error, validation};
use floe_app::{CallerContext, MemoryCommand, MemoryQuery, MemoryQueryResult};
use floe_protocol::*;
use uuid::Uuid;

pub(crate) fn command_in(value: AppProductCommandDto) -> AppWireResult<MemoryCommand> {
    match value {
        AppProductCommandDto::MemoryDecide {
            candidate_id,
            decision,
        } => Ok(MemoryCommand::Decide {
            candidate_id,
            decision: match decision {
                AgentMemoryReviewDecisionKindDto::Approve => {
                    floe_knowledge::KnowledgeDecisionKind::Approve
                }
                AgentMemoryReviewDecisionKindDto::Reject => {
                    floe_knowledge::KnowledgeDecisionKind::Reject
                }
            },
        }),
        AppProductCommandDto::MemoryForget {
            target_id,
            expected_revision,
        } => Ok(MemoryCommand::Forget {
            target_id,
            expected_revision,
        }),
        _ => Err(validation("command")),
    }
}

pub(crate) fn command_out(
    request: &MemoryCommand,
    command_id: Uuid,
    value: floe_app::MemoryCommandOutcome,
) -> AppWireResult<AppCommandResultDto> {
    match (request, value) {
        (
            MemoryCommand::Decide {
                candidate_id,
                decision: kind,
            },
            floe_app::MemoryCommandOutcome::Decided(value),
        ) => {
            if value.command_id.as_uuid() != command_id
                || value.candidate_id != *candidate_id
                || value.decision != *kind
            {
                return Err(internal_error());
            }
            let result = MemoryDecisionAcknowledgementDto {
                command_id: CommandIdDto::new(command_id).ok_or_else(internal_error)?,
                candidate_id: reference(*candidate_id)?,
                decision: match kind {
                    floe_knowledge::KnowledgeDecisionKind::Approve => {
                        AgentMemoryReviewDecisionKindDto::Approve
                    }
                    floe_knowledge::KnowledgeDecisionKind::Reject => {
                        AgentMemoryReviewDecisionKindDto::Reject
                    }
                },
                committed_at: value.committed_at,
                resulting_target_id: value.resulting_target_id.map(reference).transpose()?,
                resulting_revision: value.resulting_revision,
            };
            result.validate().map_err(|_| internal_error())?;
            Ok(AppCommandResultDto::MemoryDecision {
                acknowledgement: result,
            })
        }
        (
            MemoryCommand::Forget {
                target_id,
                expected_revision,
            },
            floe_app::MemoryCommandOutcome::Forgotten(value),
        ) => {
            if value.command_id.as_uuid() != command_id
                || value.target_id != *target_id
                || value.forgotten_revision != *expected_revision
            {
                return Err(internal_error());
            }
            let result = MemoryForgetAcknowledgementDto {
                command_id: CommandIdDto::new(command_id).ok_or_else(internal_error)?,
                target_id: reference(*target_id)?,
                forgotten_revision: value.forgotten_revision,
                committed_at: value.committed_at,
            };
            result.validate().map_err(|_| internal_error())?;
            Ok(AppCommandResultDto::MemoryForgotten {
                acknowledgement: result,
            })
        }
        _ => Err(internal_error()),
    }
}

pub(crate) fn query_in(value: AppProductQueryDto) -> AppWireResult<MemoryQuery> {
    match value {
        AppProductQueryDto::MemoryOverview {} => Ok(MemoryQuery::Overview { limit: 100 }),
        AppProductQueryDto::MemoryReview {} => Ok(MemoryQuery::Review),
        _ => return Err(validation("query")),
    }
}

pub(crate) fn query_out(
    query: &MemoryQuery,
    result: MemoryQueryResult,
    caller: &CallerContext,
) -> AppWireResult<AppQueryResultDto> {
    match result {
        MemoryQueryResult::Overview(snapshot) if matches!(query, MemoryQuery::Overview { .. }) => {
            if snapshot.person_id.0 != caller.person_id() || snapshot.memories.len() > 100 {
                return Err(internal_error());
            }
            let overview = AgentMemoryOverviewDto {
                schema_version: 1,
                person_id: snapshot.person_id.to_string(),
                saved_count: snapshot.saved_count,
                pending_count: snapshot.pending_count,
                memories: snapshot
                    .memories
                    .into_iter()
                    .map(|memory| AgentMemorySummaryDto {
                        target_id: memory.target_id.to_string(),
                        revision: memory.revision,
                        statement: memory.statement,
                        memory_kind: memory_kind(memory.memory_kind),
                        epistemic_status: epistemic(memory.epistemic_status),
                        confidence_millis: memory.confidence_millis,
                        source_count: memory.source_count,
                        origin: match memory.origin {
                            floe_knowledge::MemoryOrigin::UserProvided => {
                                AgentMemoryOriginDto::UserProvided
                            }
                            floe_knowledge::MemoryOrigin::Learned => AgentMemoryOriginDto::Learned,
                        },
                        created_at: memory.created_at,
                        valid_from: memory.valid_from,
                        valid_until: memory.valid_until,
                    })
                    .collect(),
            };
            Ok(AppQueryResultDto::MemoryOverview { overview })
        }
        MemoryQueryResult::Review(snapshot) if matches!(query, MemoryQuery::Review) => {
            if snapshot.person_id.0 != caller.person_id() {
                return Err(internal_error());
            }
            let review = MemoryReviewDisplayDto {
                person_id: reference(snapshot.person_id.0)?,
                has_more: snapshot.has_more,
                candidates: snapshot
                    .candidates
                    .into_iter()
                    .map(|candidate| {
                        Ok(MemoryCandidateSummaryDto {
                            candidate_id: reference(candidate.candidate_id)?,
                            operation: match candidate.operation {
                                floe_knowledge::KnowledgeOperation::Create => {
                                    KnowledgeOperationDto::Create
                                }
                                floe_knowledge::KnowledgeOperation::Revise => {
                                    KnowledgeOperationDto::Revise
                                }
                                floe_knowledge::KnowledgeOperation::Retire => {
                                    KnowledgeOperationDto::Retire
                                }
                            },
                            statement: candidate.statement,
                            memory_kind: memory_kind(candidate.memory_kind),
                            epistemic_status: epistemic(candidate.epistemic_status),
                            confidence_millis: candidate.confidence_millis,
                            source_count: candidate.source_count,
                            created_at: candidate.created_at,
                            valid_from: candidate.valid_from,
                            valid_until: candidate.valid_until,
                            allowed_actions: candidate
                                .allowed_actions
                                .into_iter()
                                .map(|action| match action {
                                    floe_knowledge::MemoryReviewAction::Approve => {
                                        AgentMemoryReviewDecisionKindDto::Approve
                                    }
                                    floe_knowledge::MemoryReviewAction::Reject => {
                                        AgentMemoryReviewDecisionKindDto::Reject
                                    }
                                })
                                .collect(),
                        })
                    })
                    .collect::<AppWireResult<Vec<_>>>()?,
            };
            review.validate().map_err(|_| internal_error())?;
            Ok(AppQueryResultDto::MemoryReview { review })
        }
        _ => Err(internal_error()),
    }
}
fn reference(id: Uuid) -> AppWireResult<UuidRefDto> {
    UuidRefDto::new(id).ok_or_else(internal_error)
}
fn memory_kind(value: floe_knowledge::PersonalMemoryKind) -> PersonalMemoryKindDto {
    match value {
        floe_knowledge::PersonalMemoryKind::Fact => PersonalMemoryKindDto::Fact,
        floe_knowledge::PersonalMemoryKind::Observation => PersonalMemoryKindDto::Observation,
        floe_knowledge::PersonalMemoryKind::Inference => PersonalMemoryKindDto::Inference,
        floe_knowledge::PersonalMemoryKind::Preference => PersonalMemoryKindDto::Preference,
        floe_knowledge::PersonalMemoryKind::Commitment => PersonalMemoryKindDto::Commitment,
    }
}
fn epistemic(value: floe_knowledge::EpistemicStatus) -> EpistemicStatusDto {
    match value {
        floe_knowledge::EpistemicStatus::Fact => EpistemicStatusDto::Fact,
        floe_knowledge::EpistemicStatus::Inference => EpistemicStatusDto::Inference,
    }
}

#[cfg(test)]
mod forget_tests {
    use super::*;
    #[test]
    fn forget_wire_preserves_exact_command_target_and_revision() {
        let target_id = Uuid::new_v4();
        let command_id = Uuid::new_v4();
        let request = command_in(AppProductCommandDto::MemoryForget {
            target_id,
            expected_revision: 7,
        })
        .unwrap();
        let ack = floe_knowledge::MemoryForgetAcknowledgement {
            command_id: floe_app::CommandId::from_uuid(command_id).unwrap(),
            target_id,
            forgotten_revision: 7,
            committed_at: chrono::Utc::now(),
        };
        let result = command_out(
            &request,
            command_id,
            floe_app::MemoryCommandOutcome::Forgotten(ack.clone()),
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(result).unwrap()["kind"],
            "memory.forgotten"
        );
        let mut mismatch = ack.clone();
        mismatch.forgotten_revision = 8;
        assert!(
            command_out(
                &request,
                command_id,
                floe_app::MemoryCommandOutcome::Forgotten(mismatch)
            )
            .is_err()
        );
        assert!(
            command_out(
                &request,
                Uuid::new_v4(),
                floe_app::MemoryCommandOutcome::Forgotten(ack)
            )
            .is_err()
        );
    }
}
