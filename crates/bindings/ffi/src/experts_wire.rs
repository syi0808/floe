//! Mechanical translation between safe Experts product values and App calls.
use crate::app_wire::{AppWireResult, agent_failure, internal_error, validation};
use floe_app::{
    AppComposition, CallerContext, ExpertCommand, ExpertCommandResult, ExpertCommands,
    ExpertQueries, ExpertQuery, ExpertQueryResult,
};
use floe_protocol::*;
use uuid::Uuid;

pub(crate) fn handles_command(command: &AppProductCommandDto) -> bool {
    matches!(
        command,
        AppProductCommandDto::ExpertsSetInstallationEnabled { .. }
            | AppProductCommandDto::ExpertsPrepareBindingReview { .. }
            | AppProductCommandDto::ExpertsBindingReplace { .. }
    )
}

pub(crate) fn handles_query(query: &AppProductQueryDto) -> bool {
    matches!(
        query,
        AppProductQueryDto::ExpertsDirectory { .. }
            | AppProductQueryDto::ExpertsInspectBinding { .. }
            | AppProductQueryDto::ExpertsInspectBindingReview { .. }
    )
}

pub(crate) fn command(
    app: &AppComposition,
    caller: &CallerContext,
    command_id: Uuid,
    command: AppProductCommandDto,
) -> AppWireResult<AppCommandResultDto> {
    if command_id.is_nil() {
        return Err(validation("command_id"));
    }
    let (command, review_result) = match command {
        AppProductCommandDto::ExpertsSetInstallationEnabled {
            installation_ref,
            expected_revision,
            enabled,
        } => {
            validate_revision(expected_revision)?;
            (
                ExpertCommand::SetInstallationEnabled {
                    installation_ref: installation_ref.get(),
                    expected_revision,
                    enabled,
                },
                false,
            )
        }
        AppProductCommandDto::ExpertsPrepareBindingReview {
            assignment_ref,
            requirement_ref,
            expected_binding_revision,
        } => {
            validate_revision(expected_binding_revision)?;
            validate_requirement(&requirement_ref)?;
            (
                ExpertCommand::PrepareBindingReview {
                    assignment_ref: assignment_ref.get(),
                    requirement_ref,
                    expected_binding_revision,
                },
                true,
            )
        }
        AppProductCommandDto::ExpertsBindingReplace {
            review_ref,
            expected_binding_revision,
            candidate_refs,
        } => {
            validate_revision(expected_binding_revision)?;
            if candidate_refs.len() > 16
                || candidate_refs
                    .iter()
                    .enumerate()
                    .any(|(index, id)| candidate_refs[..index].contains(id))
            {
                return Err(validation("command.candidate_refs"));
            }
            (
                ExpertCommand::ReplaceBinding {
                    review_ref: binding_review_ref_from_dto(review_ref)?,
                    expected_binding_revision,
                    candidate_refs: candidate_refs.into_iter().map(|id| id.get()).collect(),
                },
                false,
            )
        }
        _ => return Err(validation("command")),
    };
    match (
        review_result,
        app.expert_command(caller, command_id, command)
            .map_err(agent_failure)?,
    ) {
        (false, ExpertCommandResult::Directory(directory)) => {
            Ok(AppCommandResultDto::ExpertsDirectory {
                directory: directory_to_dto(directory)?,
            })
        }
        (true, ExpertCommandResult::BindingReview(review)) => {
            Ok(AppCommandResultDto::ExpertsBindingReview {
                review: binding_review_to_dto(review)?,
            })
        }
        _ => Err(internal_error()),
    }
}

#[derive(Clone, Copy)]
enum QueryResultKind {
    Directory,
    Binding,
    Review,
}

pub(crate) fn query(
    app: &AppComposition,
    caller: &CallerContext,
    request_id: Uuid,
    query: AppProductQueryDto,
) -> AppWireResult<AppQueryResultDto> {
    if request_id.is_nil() {
        return Err(validation("request_id"));
    }
    let (query, expected) = match query {
        AppProductQueryDto::ExpertsDirectory {} => {
            (ExpertQuery::Directory, QueryResultKind::Directory)
        }
        AppProductQueryDto::ExpertsInspectBinding {
            assignment_ref,
            requirement_ref,
        } => {
            validate_requirement(&requirement_ref)?;
            (
                ExpertQuery::InspectBinding {
                    assignment_ref: assignment_ref.get(),
                    requirement_ref,
                },
                QueryResultKind::Binding,
            )
        }
        AppProductQueryDto::ExpertsInspectBindingReview { review_ref } => (
            ExpertQuery::InspectBindingReview {
                review_ref: binding_review_ref_from_dto(review_ref)?,
            },
            QueryResultKind::Review,
        ),
        _ => return Err(validation("query")),
    };
    match (
        expected,
        app.expert_query(caller, request_id, query)
            .map_err(agent_failure)?,
    ) {
        (QueryResultKind::Directory, ExpertQueryResult::Directory(directory)) => {
            Ok(AppQueryResultDto::ExpertsDirectory {
                directory: directory_to_dto(directory)?,
            })
        }
        (QueryResultKind::Binding, ExpertQueryResult::Binding(binding)) => {
            Ok(AppQueryResultDto::ExpertsBinding {
                binding: binding_inspection_to_dto(binding)?,
            })
        }
        (QueryResultKind::Review, ExpertQueryResult::BindingReview(review)) => {
            Ok(AppQueryResultDto::ExpertsBindingReview {
                review: binding_review_to_dto(review)?,
            })
        }
        _ => Err(internal_error()),
    }
}

pub(crate) fn binding_review_to_dto(
    review: floe_experts::BindingReview,
) -> AppWireResult<BindingReviewDto> {
    let dto = BindingReviewDto {
        review_ref: binding_review_ref_to_dto(review.review_ref)?,
        assignment_ref: AssignmentRefDto::new(review.assignment_ref).ok_or_else(internal_error)?,
        requirement_ref: review.requirement_ref,
        binding_revision: review.binding_revision,
        candidate_refs_and_labels: review
            .candidate_refs_and_labels
            .into_iter()
            .map(|candidate| {
                Ok(BindingCandidateSummaryDto {
                    candidate_ref: uuid_ref(candidate.candidate_ref)?,
                    label: candidate.label,
                    availability: availability_to_dto(candidate.availability),
                    selected: candidate.selected,
                })
            })
            .collect::<AppWireResult<Vec<_>>>()?,
        expires_at_unix_ms: review.expires_at_unix_ms,
        allowed_actions: review
            .allowed_actions
            .into_iter()
            .map(|action| match action {
                floe_experts::BindingReviewAction::Replace => BindingReviewActionDto::Replace,
                floe_experts::BindingReviewAction::Refresh => BindingReviewActionDto::Refresh,
            })
            .collect(),
    };
    // Includes exact selected/unavailable shape and immutable reference checks;
    // no candidate, action or expiry is synthesized by the transport.
    dto.validate().map_err(|_| internal_error())?;
    Ok(dto)
}

fn binding_inspection_to_dto(
    binding: floe_experts::BindingInspection,
) -> AppWireResult<BindingInspectionDto> {
    let dto = BindingInspectionDto {
        assignment_ref: AssignmentRefDto::new(binding.assignment_ref).ok_or_else(internal_error)?,
        requirement_ref: binding.requirement_ref,
        binding_revision: binding.binding_revision,
        candidates: binding
            .candidates
            .into_iter()
            .map(|candidate| BindingInspectionCandidateDto {
                label: candidate.label,
                availability: availability_to_dto(candidate.availability),
                selected: candidate.selected,
            })
            .collect(),
    };
    dto.validate().map_err(|_| internal_error())?;
    Ok(dto)
}

fn directory_to_dto(
    directory: floe_experts::ExpertDirectorySnapshot,
) -> AppWireResult<ExpertDirectorySnapshotDto> {
    let installations = directory
        .installations
        .into_iter()
        .map(|installation| {
            Ok(ExpertInstallationSummaryDto {
                installation_ref: uuid_ref(installation.installation_ref)?,
                display_name: installation.display_name,
                description: installation.description,
                version: installation.version,
                enabled: installation.enabled,
            })
        })
        .collect::<AppWireResult<Vec<_>>>()?;
    let assignments = directory
        .assignments
        .into_iter()
        .map(|assignment| {
            let requirements = assignment
                .requirements
                .into_iter()
                .map(|requirement| {
                    Ok(ExpertRequirementSummaryDto {
                        requirement_ref: requirement.requirement_ref,
                        label: requirement.label,
                        selected_count: u32::try_from(requirement.selected_count)
                            .map_err(|_| internal_error())?,
                        minimum_sources: requirement.minimum_sources,
                    })
                })
                .collect::<AppWireResult<Vec<_>>>()?;
            Ok(ExpertAssignmentSummaryDto {
                assignment_ref: AssignmentRefDto::new(assignment.assignment_ref)
                    .ok_or_else(internal_error)?,
                installation_ref: uuid_ref(assignment.installation_ref)?,
                display_name: assignment.display_name,
                enabled: assignment.enabled,
                binding_revision: assignment.binding_revision,
                requirements,
            })
        })
        .collect::<AppWireResult<Vec<_>>>()?;
    let dto = ExpertDirectorySnapshotDto {
        revision: directory.revision,
        installations,
        assignments,
    };
    // An empty initial directory legitimately has revision zero.
    dto.validate().map_err(|_| internal_error())?;
    Ok(dto)
}

fn availability_to_dto(
    availability: floe_experts::CandidateAvailability,
) -> CandidateAvailabilityDto {
    match availability {
        floe_experts::CandidateAvailability::Available => CandidateAvailabilityDto::Available,
        floe_experts::CandidateAvailability::Unavailable => CandidateAvailabilityDto::Unavailable,
    }
}

fn binding_review_ref_to_dto(
    reference: floe_experts::BindingReviewRef,
) -> AppWireResult<BindingReviewRefDto> {
    reference.validate().map_err(|_| internal_error())?;
    let dto = BindingReviewRefDto {
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

fn binding_review_ref_from_dto(
    reference: BindingReviewRefDto,
) -> AppWireResult<floe_experts::BindingReviewRef> {
    reference.validate().map_err(validation)?;
    let mut digest = [0u8; 32];
    let text = reference.digest.as_str();
    if text.len() != 64 {
        return Err(validation("experts.binding_review_ref.digest"));
    }
    for (index, value) in digest.iter_mut().enumerate() {
        *value = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16)
            .map_err(|_| validation("experts.binding_review_ref.digest"))?;
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
        return Err(validation("requirement_ref"));
    }
    Ok(())
}
