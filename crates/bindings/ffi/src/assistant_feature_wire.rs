//! Translation between Conversation assistant-feature settings and the Expert
//! execution owner behind that product projection.
use crate::app_wire::{AppWireResult, agent_failure, internal_error, validation};
use floe_app::{
    AssistantFeatureCommand, AssistantFeatureCommandResult, AssistantFeatureQuery,
    AssistantFeatureQueryResult,
};
use floe_protocol::*;
use uuid::Uuid;

pub(crate) fn command_in(command: AppProductCommandDto) -> AppWireResult<AssistantFeatureCommand> {
    match command {
        AppProductCommandDto::AssistantFeatureSourcePrepareReview {
            feature_ref,
            source_scope_ref,
            source_requirement_ref,
            expected_binding_revision,
        } => {
            validate_revision(expected_binding_revision)?;
            validate_requirement(&source_requirement_ref)?;
            Ok(AssistantFeatureCommand::PrepareSourceReview {
                feature_ref: feature_ref.get(),
                source_scope_ref: source_scope_ref.get(),
                requirement_ref: source_requirement_ref,
                expected_binding_revision,
            })
        }
        AppProductCommandDto::AssistantFeatureConfigure {
            feature_ref,
            expected_revision,
            enabled,
            source_selections,
        } => {
            validate_revision(expected_revision)?;
            if source_selections.len() > MAX_ASSISTANT_FEATURE_SOURCE_SELECTIONS {
                return Err(validation("command.source_selections"));
            }
            let mut selections = Vec::with_capacity(source_selections.len());
            for selection in source_selections {
                selection.validate().map_err(validation)?;
                selections.push(floe_experts::AssistantFeatureSourceSelection {
                    source_scope_ref: selection.source_scope_ref.get(),
                    requirement_ref: selection.source_requirement_ref,
                    review_ref: source_review_ref_from_dto(selection.review_ref)?,
                    expected_binding_revision: selection.expected_binding_revision,
                    candidate_refs: selection
                        .candidate_refs
                        .into_iter()
                        .map(|reference| reference.get())
                        .collect(),
                });
            }
            Ok(AssistantFeatureCommand::Configure {
                feature_ref: feature_ref.get(),
                expected_revision,
                enabled,
                source_selections: selections,
            })
        }
        _ => Err(validation("command")),
    }
}

pub(crate) fn command_out(
    command: &AssistantFeatureCommand,
    result: AssistantFeatureCommandResult,
) -> AppWireResult<AppCommandResultDto> {
    match (command, result) {
        (
            AssistantFeatureCommand::PrepareSourceReview { .. },
            AssistantFeatureCommandResult::SourceReview(review),
        ) => Ok(AppCommandResultDto::AssistantFeatureSourceReview {
            review: source_review_to_dto(review)?,
        }),
        (
            AssistantFeatureCommand::Configure { .. },
            AssistantFeatureCommandResult::Snapshot(snapshot),
        ) => Ok(AppCommandResultDto::AssistantFeatureSnapshot {
            snapshot: assistant_feature_snapshot_to_dto(snapshot)?,
        }),
        _ => Err(internal_error()),
    }
}

pub(crate) fn query_in(query: AppProductQueryDto) -> AppWireResult<AssistantFeatureQuery> {
    match query {
        AppProductQueryDto::AssistantFeatureSnapshot {} => Ok(AssistantFeatureQuery::Snapshot),
        AppProductQueryDto::AssistantFeatureSourceReviewInspect { review_ref } => {
            Ok(AssistantFeatureQuery::InspectSourceReview {
                review_ref: source_review_ref_from_dto(review_ref)?,
            })
        }
        _ => Err(validation("query")),
    }
}

pub(crate) fn query_out(
    query: &AssistantFeatureQuery,
    result: AssistantFeatureQueryResult,
) -> AppWireResult<AppQueryResultDto> {
    match (query, result) {
        (AssistantFeatureQuery::Snapshot, AssistantFeatureQueryResult::Snapshot(snapshot)) => {
            Ok(AppQueryResultDto::AssistantFeatureSnapshot {
                snapshot: assistant_feature_snapshot_to_dto(snapshot)?,
            })
        }
        (
            AssistantFeatureQuery::InspectSourceReview { .. },
            AssistantFeatureQueryResult::SourceReview(review),
        ) => Ok(AppQueryResultDto::AssistantFeatureSourceReview {
            review: source_review_to_dto(review)?,
        }),
        _ => Err(internal_error()),
    }
}

pub(crate) fn source_review_to_dto(
    review: floe_experts::BindingReview,
) -> AppWireResult<AssistantFeatureSourceReviewDto> {
    let dto = AssistantFeatureSourceReviewDto {
        review_ref: source_review_ref_to_dto(review.review_ref)?,
        source_scope_ref: uuid_ref(review.assignment_ref)?,
        source_requirement_ref: review.requirement_ref,
        binding_revision: review.binding_revision,
        candidates: review
            .candidate_refs_and_labels
            .into_iter()
            .map(|candidate| {
                Ok(AssistantFeatureSourceCandidateDto {
                    candidate_ref: uuid_ref(candidate.candidate_ref)?,
                    label: candidate.label,
                    availability: match candidate.availability {
                        floe_experts::CandidateAvailability::Available => {
                            AssistantFeatureSourceAvailabilityDto::Available
                        }
                        floe_experts::CandidateAvailability::Unavailable => {
                            AssistantFeatureSourceAvailabilityDto::Unavailable
                        }
                    },
                    selected: candidate.selected,
                })
            })
            .collect::<AppWireResult<Vec<_>>>()?,
        expires_at_unix_ms: review.expires_at_unix_ms,
        allowed_actions: review
            .allowed_actions
            .into_iter()
            .map(|action| match action {
                floe_experts::BindingReviewAction::Replace => {
                    AssistantFeatureSourceReviewActionDto::Replace
                }
                floe_experts::BindingReviewAction::Refresh => {
                    AssistantFeatureSourceReviewActionDto::Refresh
                }
            })
            .collect(),
    };
    dto.validate().map_err(|_| internal_error())?;
    Ok(dto)
}

pub(crate) fn assistant_feature_snapshot_to_dto(
    snapshot: floe_experts::AssistantFeatureSnapshot,
) -> AppWireResult<AssistantFeatureSnapshotDto> {
    let features = snapshot
        .features
        .into_iter()
        .map(|feature| {
            let source_groups = feature
                .source_groups
                .into_iter()
                .map(|group| {
                    let requirements = group
                        .requirements
                        .into_iter()
                        .map(|requirement| {
                            Ok(AssistantFeatureSourceRequirementDto {
                                requirement_ref: requirement.requirement_ref,
                                label: requirement.label,
                                selected_count: u32::try_from(requirement.selected_count)
                                    .map_err(|_| internal_error())?,
                                minimum_sources: requirement.minimum_sources,
                            })
                        })
                        .collect::<AppWireResult<Vec<_>>>()?;
                    Ok(AssistantFeatureSourceGroupDto {
                        source_scope_ref: uuid_ref(group.source_scope_ref)?,
                        display_name: group.display_name,
                        enabled: group.enabled,
                        binding_revision: group.binding_revision,
                        requirements,
                    })
                })
                .collect::<AppWireResult<Vec<_>>>()?;
            Ok(AssistantFeatureDto {
                feature_ref: uuid_ref(feature.feature_ref)?,
                display_name: feature.display_name,
                description: feature.description,
                enabled: feature.enabled,
                source_groups,
            })
        })
        .collect::<AppWireResult<Vec<_>>>()?;
    let dto = AssistantFeatureSnapshotDto {
        revision: snapshot.revision,
        features,
    };
    // Empty initial settings legitimately have revision zero.
    dto.validate().map_err(|_| internal_error())?;
    Ok(dto)
}

pub(crate) fn source_review_ref_to_dto(
    reference: floe_experts::BindingReviewRef,
) -> AppWireResult<AssistantFeatureSourceReviewRefDto> {
    reference.validate().map_err(|_| internal_error())?;
    let dto = AssistantFeatureSourceReviewRefDto {
        id: uuid_ref(reference.id)?,
        digest: DigestHex64Dto::new(
            reference
                .digest
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
        )
        .ok_or_else(internal_error)?,
    };
    dto.validate().map_err(|_| internal_error())?;
    Ok(dto)
}

fn source_review_ref_from_dto(
    reference: AssistantFeatureSourceReviewRefDto,
) -> AppWireResult<floe_experts::BindingReviewRef> {
    reference.validate().map_err(validation)?;
    let mut digest = [0u8; 32];
    let text = reference.digest.as_str();
    if text.len() != 64 {
        return Err(validation("assistant_features.source_review_ref.digest"));
    }
    for (index, value) in digest.iter_mut().enumerate() {
        *value = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16)
            .map_err(|_| validation("assistant_features.source_review_ref.digest"))?;
    }
    let reference = floe_experts::BindingReviewRef {
        id: reference.id.get(),
        digest,
    };
    reference.validate().map_err(agent_failure)?;
    Ok(reference)
}

fn uuid_ref(value: Uuid) -> AppWireResult<UuidRefDto> {
    UuidRefDto::new(value).ok_or_else(internal_error)
}

fn validate_revision(value: u64) -> AppWireResult<()> {
    if value == 0 || value > i64::MAX as u64 {
        return Err(validation("expected_revision"));
    }
    Ok(())
}

fn validate_requirement(value: &str) -> AppWireResult<()> {
    if value.is_empty()
        || value.len() > 128
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(validation("source_requirement_ref"));
    }
    Ok(())
}
