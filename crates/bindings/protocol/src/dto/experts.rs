use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::{AssignmentRefDto, CommandIdDto, DigestHex64Dto, UuidRefDto};

const MAX_BINDING_CANDIDATES: usize = 64;
const MAX_INSTALLATIONS: usize = 128;
const MAX_ASSIGNMENTS: usize = 256;
const MAX_REQUIREMENTS_PER_ASSIGNMENT: usize = 32;
const MAX_SELECTED_SOURCES: u32 = 16;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingReviewActionDto {
    Replace,
    Refresh,
}

/// This mirrors the current owner enum, whose variants carry no extra data.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateAvailabilityDto {
    Available,
    Unavailable,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BindingReviewRefDto {
    pub id: UuidRefDto,
    pub digest: DigestHex64Dto,
}

impl BindingReviewRefDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.digest.as_str() == "0000000000000000000000000000000000000000000000000000000000000000" {
            return Err("experts.binding_review_ref.digest");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BindingCandidateSummaryDto {
    pub candidate_ref: UuidRefDto,
    pub label: String,
    pub availability: CandidateAvailabilityDto,
    pub selected: bool,
}

impl BindingCandidateSummaryDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        validate_candidate_label(&self.label)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BindingReviewDto {
    pub review_ref: BindingReviewRefDto,
    pub assignment_ref: AssignmentRefDto,
    pub requirement_ref: String,
    pub binding_revision: u64,
    pub candidate_refs_and_labels: Vec<BindingCandidateSummaryDto>,
    pub expires_at_unix_ms: i64,
    pub allowed_actions: Vec<BindingReviewActionDto>,
}

impl BindingReviewDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        self.review_ref.validate()?;
        validate_requirement_ref(&self.requirement_ref)?;
        positive_revision(self.binding_revision, "experts.binding_review.binding_revision")?;
        if self.expires_at_unix_ms <= 0
            || self.candidate_refs_and_labels.len() > MAX_BINDING_CANDIDATES
            || self.allowed_actions.is_empty()
            || self.allowed_actions.len() > 2
        {
            return Err("experts.binding_review");
        }

        if self.candidate_refs_and_labels.iter().filter(|candidate| candidate.selected).count() > MAX_SELECTED_SOURCES as usize
            || self.candidate_refs_and_labels.iter().any(|candidate|
                candidate.availability == CandidateAvailabilityDto::Unavailable && !candidate.selected) {
            return Err("experts.binding_review.selection");
        }
        let mut candidate_refs = HashSet::with_capacity(self.candidate_refs_and_labels.len());
        for candidate in &self.candidate_refs_and_labels {
            candidate.validate()?;
            if !candidate_refs.insert(candidate.candidate_ref) {
                return Err("experts.binding_review.duplicate_candidate_ref");
            }
        }
        for (index, action) in self.allowed_actions.iter().enumerate() {
            if self.allowed_actions[..index].contains(action) {
                return Err("experts.binding_review.duplicate_allowed_action");
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BindingInspectionCandidateDto {
    pub label: String,
    pub availability: CandidateAvailabilityDto,
    pub selected: bool,
}

impl BindingInspectionCandidateDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        validate_candidate_label(&self.label)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BindingInspectionDto {
    pub assignment_ref: AssignmentRefDto,
    pub requirement_ref: String,
    pub binding_revision: u64,
    pub candidates: Vec<BindingInspectionCandidateDto>,
}

impl BindingInspectionDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        validate_requirement_ref(&self.requirement_ref)?;
        positive_revision(self.binding_revision, "experts.binding_inspection.binding_revision")?;
        if self.candidates.len() > MAX_BINDING_CANDIDATES {
            return Err("experts.binding_inspection.candidates");
        }
        for candidate in &self.candidates {
            candidate.validate()?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertRequirementSummaryDto {
    pub requirement_ref: String,
    pub label: String,
    pub selected_count: u32,
    pub minimum_sources: u8,
}

impl ExpertRequirementSummaryDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        validate_requirement_ref(&self.requirement_ref)?;
        if self.label != self.requirement_ref
            || self.selected_count > MAX_SELECTED_SOURCES
            || u32::from(self.minimum_sources) > MAX_SELECTED_SOURCES
        {
            return Err("experts.requirement_summary");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertAssignmentSummaryDto {
    pub assignment_ref: AssignmentRefDto,
    pub installation_ref: UuidRefDto,
    pub display_name: String,
    pub enabled: bool,
    pub binding_revision: u64,
    pub requirements: Vec<ExpertRequirementSummaryDto>,
}

impl ExpertAssignmentSummaryDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        validate_display_name(&self.display_name)?;
        positive_revision(self.binding_revision, "experts.assignment.binding_revision")?;
        if self.requirements.len() > MAX_REQUIREMENTS_PER_ASSIGNMENT {
            return Err("experts.assignment.requirements");
        }
        let mut requirement_refs = HashSet::with_capacity(self.requirements.len());
        for requirement in &self.requirements {
            requirement.validate()?;
            if !requirement_refs.insert(requirement.requirement_ref.as_str()) {
                return Err("experts.assignment.duplicate_requirement");
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertInstallationSummaryDto {
    pub installation_ref: UuidRefDto,
    pub display_name: String,
    pub version: String,
    pub enabled: bool,
}

impl ExpertInstallationSummaryDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        validate_display_name(&self.display_name)?;
        if !bounded_owner_text(&self.version, 64) {
            return Err("experts.installation.version");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertDirectorySnapshotDto {
    pub revision: u64,
    pub installations: Vec<ExpertInstallationSummaryDto>,
    pub assignments: Vec<ExpertAssignmentSummaryDto>,
}

impl ExpertDirectorySnapshotDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.installations.len() > MAX_INSTALLATIONS
            || self.assignments.len() > MAX_ASSIGNMENTS
        {
            return Err("experts.directory.collection_limit");
        }

        let mut installation_refs = HashSet::with_capacity(self.installations.len());
        for installation in &self.installations {
            installation.validate()?;
            if !installation_refs.insert(installation.installation_ref) {
                return Err("experts.directory.duplicate_installation");
            }
        }

        let mut assignment_refs = HashSet::with_capacity(self.assignments.len());
        for assignment in &self.assignments {
            assignment.validate()?;
            if !assignment_refs.insert(assignment.assignment_ref)
                || !installation_refs.contains(&assignment.installation_ref)
            {
                return Err("experts.directory.assignment_reference");
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BindingMutationReceiptDto {
    pub command_id: CommandIdDto,
    pub review_ref: BindingReviewRefDto,
    pub assignment_ref: AssignmentRefDto,
    pub binding_revision: u64,
    pub registry_revision: u64,
    pub committed_at_unix_ms: i64,
}

impl BindingMutationReceiptDto {
    /// Checks the safe receipt fields only; it does not prove that a commit occurred.
    pub fn validate(&self) -> Result<(), &'static str> {
        self.review_ref.validate()?;
        positive_revision(self.binding_revision, "experts.receipt.binding_revision")?;
        positive_revision(self.registry_revision, "experts.receipt.registry_revision")?;
        if self.committed_at_unix_ms < 0 {
            return Err("experts.receipt.committed_at_unix_ms");
        }
        Ok(())
    }
}

fn validate_candidate_label(value: &str) -> Result<(), &'static str> {
    if value.trim().is_empty() || value.len() > 256 {
        Err("experts.candidate.label")
    } else {
        Ok(())
    }
}

fn validate_display_name(value: &str) -> Result<(), &'static str> {
    if bounded_owner_text(value, 128) {
        Ok(())
    } else {
        Err("experts.display_name")
    }
}

fn validate_requirement_ref(value: &str) -> Result<(), &'static str> {
    if !valid_identifier(value) {
        Err("experts.requirement_ref")
    } else {
        Ok(())
    }
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn bounded_owner_text(value: &str, max_bytes: usize) -> bool {
    !value.trim().is_empty()
        && value.len() <= max_bytes
        && !value
            .chars()
            .any(|character| character.is_control() && character != '\n')
}

fn positive_revision(value: u64, field: &'static str) -> Result<(), &'static str> {
    if value == 0 {
        Err(field)
    } else {
        Ok(())
    }
}
