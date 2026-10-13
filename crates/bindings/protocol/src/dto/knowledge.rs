use super::{AgentMemoryReviewDecisionKindDto, CommandIdDto, UuidRefDto};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PersonalMemoryKindDto {
    Fact,
    Observation,
    Inference,
    Preference,
    Commitment,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EpistemicStatusDto {
    Fact,
    Inference,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeOperationDto {
    Create,
    Revise,
    Retire,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryCandidateSummaryDto {
    pub candidate_id: UuidRefDto,
    pub operation: KnowledgeOperationDto,
    pub statement: String,
    pub memory_kind: PersonalMemoryKindDto,
    pub epistemic_status: EpistemicStatusDto,
    pub confidence_millis: u16,
    pub source_count: usize,
    pub created_at: DateTime<Utc>,
    pub valid_from: Option<DateTime<Utc>>,
    pub valid_until: Option<DateTime<Utc>>,
    pub allowed_actions: Vec<AgentMemoryReviewDecisionKindDto>,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryReviewDisplayDto {
    pub person_id: UuidRefDto,
    pub candidates: Vec<MemoryCandidateSummaryDto>,
    pub has_more: bool,
}
impl MemoryReviewDisplayDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.candidates.len() > 100 || (self.has_more && self.candidates.len() != 100) {
            return Err("knowledge.candidates");
        }
        let mut ids = std::collections::HashSet::new();
        for value in &self.candidates {
            if !ids.insert(value.candidate_id)
                || value.statement.trim().is_empty()
                || value.statement.len() > 2048
                || value.confidence_millis > 1000
                || value.allowed_actions.len() > 2
                || value
                    .allowed_actions
                    .iter()
                    .enumerate()
                    .any(|(i, a)| value.allowed_actions[i + 1..].contains(a))
            {
                return Err("knowledge.candidate");
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryDecisionAcknowledgementDto {
    pub command_id: CommandIdDto,
    pub candidate_id: UuidRefDto,
    pub decision: AgentMemoryReviewDecisionKindDto,
    pub committed_at: DateTime<Utc>,
    pub resulting_target_id: Option<UuidRefDto>,
    pub resulting_revision: Option<u64>,
}
impl MemoryDecisionAcknowledgementDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.resulting_target_id.is_some() != self.resulting_revision.is_some()
            || self
                .resulting_revision
                .is_some_and(|value| value == 0 || value > i64::MAX as u64)
        {
            return Err("knowledge.acknowledgement");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryForgetAcknowledgementDto {
    pub command_id: CommandIdDto,
    pub target_id: UuidRefDto,
    pub forgotten_revision: u64,
    pub committed_at: DateTime<Utc>,
}
impl MemoryForgetAcknowledgementDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.forgotten_revision == 0 || self.forgotten_revision > i64::MAX as u64 {
            return Err("knowledge.forget_acknowledgement");
        }
        Ok(())
    }
}

#[cfg(test)]
mod review_page_tests {
    use super::*;
    #[test]
    fn review_page_requires_explicit_noncontradictory_continuation() {
        let missing = serde_json::json!({"person_id": "11111111-1111-4111-8111-111111111111", "candidates": []});
        assert!(serde_json::from_value::<MemoryReviewDisplayDto>(missing.clone()).is_err());
        let mut complete = missing;
        complete["has_more"] = serde_json::json!(false);
        let page: MemoryReviewDisplayDto = serde_json::from_value(complete.clone()).unwrap();
        assert!(page.validate().is_ok());
        complete["has_more"] = serde_json::json!(true);
        let invalid: MemoryReviewDisplayDto = serde_json::from_value(complete).unwrap();
        assert!(invalid.validate().is_err());
    }
}
