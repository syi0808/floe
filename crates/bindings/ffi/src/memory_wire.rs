//! Mechanical conversion between the Memory DTOs and Knowledge owner types.
use crate::app_wire::{AppWireResult, internal_error, validation};
use floe_app::{CallerContext, MemoryCommand, MemoryQuery, MemoryQueryResult};
use floe_protocol::*;
use uuid::Uuid;

pub(crate) fn command_in(value: AppProductCommandDto) -> AppWireResult<MemoryCommand> {
    let AppProductCommandDto::MemoryDecide {
        candidate_id,
        decision,
    } = value
    else {
        return Err(validation("command"));
    };
    let kind = match decision {
        AgentMemoryReviewDecisionKindDto::Approve => floe_knowledge::KnowledgeDecisionKind::Approve,
        AgentMemoryReviewDecisionKindDto::Reject => floe_knowledge::KnowledgeDecisionKind::Reject,
    };
    Ok(MemoryCommand::Decide {
        candidate_id,
        decision: kind,
    })
}

pub(crate) fn command_out(
    request: &MemoryCommand,
    command_id: Uuid,
    value: floe_knowledge::MemoryDecisionAcknowledgement,
) -> AppWireResult<AppCommandResultDto> {
    let MemoryCommand::Decide {
        candidate_id,
        decision: kind,
    } = request;
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
