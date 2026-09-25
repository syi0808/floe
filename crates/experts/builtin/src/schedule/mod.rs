//! The Schedule Expert: what it plans and judges through the common built-in host.

pub mod dispatch;
mod expert;
mod plan;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ScheduleInsight {
    Commitment {
        evidence_handle: Uuid,
        untrusted_title: String,
        starts_at_unix_ms: u64,
        ends_at_unix_ms: u64,
    },
    FocusWindow {
        starts_at_unix_ms: u64,
        ends_at_unix_ms: u64,
    },
    NoFocusWindow,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScheduleAssessment {
    pub insights: Vec<ScheduleInsight>,
}

impl ScheduleAssessment {
    pub fn validate(&self) -> Result<(), floe_agent_contract::AgentFailure> {
        let valid_interval = |start: u64, end: u64| end > start && end - start <= 86_400_000;
        if self.insights.len() > 8
            || self.insights.iter().any(|insight| match insight {
                ScheduleInsight::Commitment {
                    evidence_handle,
                    untrusted_title,
                    starts_at_unix_ms,
                    ends_at_unix_ms,
                } => {
                    evidence_handle.is_nil()
                        || untrusted_title.len() > 256
                        || !valid_interval(*starts_at_unix_ms, *ends_at_unix_ms)
                }
                ScheduleInsight::FocusWindow {
                    starts_at_unix_ms,
                    ends_at_unix_ms,
                } => !valid_interval(*starts_at_unix_ms, *ends_at_unix_ms),
                ScheduleInsight::NoFocusWindow => false,
            })
        {
            return Err(floe_agent_contract::AgentFailure::InvalidModelOutput);
        }
        Ok(())
    }
}

#[cfg(test)]
mod assessment_tests {
    use super::*;

    #[test]
    fn assessment_rejects_invalid_intervals_and_oversized_titles() {
        let mut assessment = ScheduleAssessment {
            insights: vec![ScheduleInsight::FocusWindow {
                starts_at_unix_ms: 10,
                ends_at_unix_ms: 10,
            }],
        };
        assert!(assessment.validate().is_err());
        assessment.insights = vec![ScheduleInsight::Commitment {
            evidence_handle: Uuid::new_v4(),
            untrusted_title: "x".repeat(257),
            starts_at_unix_ms: 10,
            ends_at_unix_ms: 20,
        }];
        assert!(assessment.validate().is_err());
        assessment.insights = vec![ScheduleInsight::NoFocusWindow; 9];
        assert!(assessment.validate().is_err());
    }
}

pub use plan::{
    FOCUS_REQUEST, ScheduleExecutionIntent, ScheduleRequestPlan, day_bounds, plan_request,
    requested_range, run_policy,
};
pub const RESULT_MEDIA_TYPE: &str = "application/vnd.floe.expert.schedule+json;version=1";
