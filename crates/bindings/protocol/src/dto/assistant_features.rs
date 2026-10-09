use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::{DigestHex64Dto, UuidRefDto};

const MAX_FEATURES: usize = 128;
const MAX_SOURCE_GROUPS_PER_FEATURE: usize = 256;
const MAX_REQUIREMENTS_PER_GROUP: usize = 32;
const MAX_BINDING_CANDIDATES: usize = 64;
pub const MAX_ASSISTANT_FEATURE_SOURCE_SELECTIONS: usize = 128;
const MAX_SELECTED_SOURCES: usize = 16;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AssistantFeatureSourceReviewActionDto {
    Replace,
    Refresh,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AssistantFeatureSourceAvailabilityDto {
    Available,
    Unavailable,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AssistantFeatureSourceReviewRefDto {
    pub id: UuidRefDto,
    pub digest: DigestHex64Dto,
}

impl AssistantFeatureSourceReviewRefDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.digest.as_str()
            == "0000000000000000000000000000000000000000000000000000000000000000"
        {
            return Err("assistant_features.source_review_ref.digest");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AssistantFeatureSourceCandidateDto {
    pub candidate_ref: UuidRefDto,
    pub label: String,
    pub availability: AssistantFeatureSourceAvailabilityDto,
    pub selected: bool,
}

impl AssistantFeatureSourceCandidateDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.label.trim().is_empty() || self.label.len() > 256 {
            Err("assistant_features.source_candidate.label")
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AssistantFeatureSourceReviewDto {
    pub review_ref: AssistantFeatureSourceReviewRefDto,
    pub source_scope_ref: UuidRefDto,
    pub source_requirement_ref: String,
    pub binding_revision: u64,
    pub candidates: Vec<AssistantFeatureSourceCandidateDto>,
    pub expires_at_unix_ms: i64,
    pub allowed_actions: Vec<AssistantFeatureSourceReviewActionDto>,
}

impl AssistantFeatureSourceReviewDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        self.review_ref.validate()?;
        validate_requirement_ref(&self.source_requirement_ref)?;
        positive_revision(
            self.binding_revision,
            "assistant_features.source_review.binding_revision",
        )?;
        if self.expires_at_unix_ms <= 0
            || self.candidates.len() > MAX_BINDING_CANDIDATES
            || self.allowed_actions.is_empty()
            || self.allowed_actions.len() > 2
        {
            return Err("assistant_features.source_review");
        }
        if self
            .candidates
            .iter()
            .filter(|candidate| candidate.selected)
            .count()
            > MAX_SELECTED_SOURCES
            || self.candidates.iter().any(|candidate| {
                candidate.availability == AssistantFeatureSourceAvailabilityDto::Unavailable
                    && !candidate.selected
            })
        {
            return Err("assistant_features.source_review.selection");
        }
        let mut refs = HashSet::with_capacity(self.candidates.len());
        for candidate in &self.candidates {
            candidate.validate()?;
            if !refs.insert(candidate.candidate_ref) {
                return Err("assistant_features.source_review.duplicate_candidate");
            }
        }
        for (index, action) in self.allowed_actions.iter().enumerate() {
            if self.allowed_actions[..index].contains(action) {
                return Err("assistant_features.source_review.duplicate_action");
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AssistantFeatureSourceSelectionDto {
    pub source_scope_ref: UuidRefDto,
    pub source_requirement_ref: String,
    pub review_ref: AssistantFeatureSourceReviewRefDto,
    pub expected_binding_revision: u64,
    pub candidate_refs: Vec<UuidRefDto>,
}

impl AssistantFeatureSourceSelectionDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        validate_requirement_ref(&self.source_requirement_ref)?;
        self.review_ref.validate()?;
        positive_revision(
            self.expected_binding_revision,
            "assistant_features.source_selection.binding_revision",
        )?;
        if self.candidate_refs.len() > MAX_SELECTED_SOURCES
            || self.candidate_refs.iter().collect::<HashSet<_>>().len() != self.candidate_refs.len()
        {
            return Err("assistant_features.source_selection.candidates");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AssistantFeatureSourceRequirementDto {
    pub requirement_ref: String,
    pub label: String,
    pub selected_count: u32,
    pub minimum_sources: u8,
}

impl AssistantFeatureSourceRequirementDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        validate_requirement_ref(&self.requirement_ref)?;
        if self.label != self.requirement_ref
            || self.selected_count > MAX_SELECTED_SOURCES as u32
            || u32::from(self.minimum_sources) > MAX_SELECTED_SOURCES as u32
        {
            return Err("assistant_features.source_requirement");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AssistantFeatureSourceGroupDto {
    pub source_scope_ref: UuidRefDto,
    pub display_name: String,
    pub enabled: bool,
    pub binding_revision: u64,
    pub requirements: Vec<AssistantFeatureSourceRequirementDto>,
}

impl AssistantFeatureSourceGroupDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        validate_display_name(&self.display_name)?;
        positive_revision(
            self.binding_revision,
            "assistant_features.source_group.binding_revision",
        )?;
        if self.requirements.len() > MAX_REQUIREMENTS_PER_GROUP {
            return Err("assistant_features.source_group.requirements");
        }
        let mut refs = HashSet::with_capacity(self.requirements.len());
        for requirement in &self.requirements {
            requirement.validate()?;
            if !refs.insert(requirement.requirement_ref.as_str()) {
                return Err("assistant_features.source_group.duplicate_requirement");
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AssistantFeatureDto {
    pub feature_ref: UuidRefDto,
    pub display_name: String,
    pub description: String,
    pub enabled: bool,
    pub source_groups: Vec<AssistantFeatureSourceGroupDto>,
}

impl AssistantFeatureDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        validate_display_name(&self.display_name)?;
        if !bounded_owner_text(&self.description, 512)
            || self.source_groups.len() > MAX_SOURCE_GROUPS_PER_FEATURE
        {
            return Err("assistant_features.feature");
        }
        let mut refs = HashSet::with_capacity(self.source_groups.len());
        for group in &self.source_groups {
            group.validate()?;
            if !refs.insert(group.source_scope_ref) {
                return Err("assistant_features.feature.duplicate_source_group");
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AssistantFeatureSnapshotDto {
    pub revision: u64,
    pub features: Vec<AssistantFeatureDto>,
}

impl AssistantFeatureSnapshotDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.features.len() > MAX_FEATURES {
            return Err("assistant_features.snapshot.features");
        }
        let mut refs = HashSet::with_capacity(self.features.len());
        for feature in &self.features {
            feature.validate()?;
            if !refs.insert(feature.feature_ref) {
                return Err("assistant_features.snapshot.duplicate_feature");
            }
        }
        Ok(())
    }
}

fn validate_display_name(value: &str) -> Result<(), &'static str> {
    if bounded_owner_text(value, 128) {
        Ok(())
    } else {
        Err("assistant_features.display_name")
    }
}

fn validate_requirement_ref(value: &str) -> Result<(), &'static str> {
    if !valid_identifier(value) {
        Err("assistant_features.requirement_ref")
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
    if value == 0 { Err(field) } else { Ok(()) }
}
