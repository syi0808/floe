//! Acquiring an optional source without letting a refusal look like no data.
//!
//! A source a turn may do without still has to say why it is missing: denied,
//! unavailable or over budget are three different answers, and none of them is
//! an empty list. Every owner that composes a context states it the same way,
//! so the rule lives beside the issue it records.

use std::future::Future;

use floe_kernel::AgentFailure;

use crate::{ContextIssue, ContextIssueReason, ContextSource};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OptionalSource<Value> {
    pub value: Option<Value>,
    pub issue: Option<ContextIssue>,
}

pub async fn acquire_optional_source<Value>(
    source: ContextSource,
    acquisition: impl Future<Output = Result<Value, AgentFailure>>,
) -> Result<OptionalSource<Value>, AgentFailure> {
    let reason = match acquisition.await {
        Ok(value) => {
            return Ok(OptionalSource {
                value: Some(value),
                issue: None,
            });
        }
        Err(AgentFailure::CapabilityUnavailable) => ContextIssueReason::Unavailable,
        Err(AgentFailure::CapabilityDenied) => ContextIssueReason::Denied,
        Err(AgentFailure::BudgetExceeded) => ContextIssueReason::BudgetExceeded,
        Err(failure) => return Err(failure),
    };
    Ok(OptionalSource {
        value: None,
        issue: Some(ContextIssue { source, reason }),
    })
}

pub fn record_source_issue(
    issues: &mut Vec<ContextIssue>,
    source: ContextSource,
    reason: Option<ContextIssueReason>,
) {
    issues.retain(|issue| issue.source != source);
    if let Some(reason) = reason {
        issues.push(ContextIssue { source, reason });
    }
}
