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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn optional_failures_are_explicit_and_never_masquerade_as_empty_data() {
        for source in [
            ContextSource::Memory,
            ContextSource::Tasks,
            ContextSource::Notes,
            ContextSource::Calendar,
        ] {
            for (failure, reason) in [
                (
                    AgentFailure::CapabilityUnavailable,
                    ContextIssueReason::Unavailable,
                ),
                (AgentFailure::CapabilityDenied, ContextIssueReason::Denied),
                (
                    AgentFailure::BudgetExceeded,
                    ContextIssueReason::BudgetExceeded,
                ),
            ] {
                let result = acquire_optional_source::<Vec<String>>(source, async { Err(failure) })
                    .await
                    .unwrap();
                assert_eq!(result.value, None);
                assert_eq!(result.issue, Some(ContextIssue { source, reason }));
            }
            let empty = acquire_optional_source(source, async { Ok(Vec::<String>::new()) })
                .await
                .unwrap();
            assert_eq!(empty.value, Some(vec![]));
            assert_eq!(empty.issue, None);
        }
    }

    #[tokio::test]
    async fn integrity_policy_and_cancellation_remain_fatal() {
        for failure in [
            AgentFailure::StorageUnavailable,
            AgentFailure::VaultUnavailable,
            AgentFailure::PolicyDenied,
            AgentFailure::Cancelled,
            AgentFailure::InvalidInput,
            AgentFailure::StaleContext,
            AgentFailure::DeadlineExceeded,
        ] {
            assert_eq!(
                acquire_optional_source::<()>(ContextSource::Tasks, async { Err(failure) }).await,
                Err(failure)
            );
        }
    }

    #[test]
    fn refresh_replaces_only_the_selected_source_issue() {
        let mut issues = vec![];
        record_source_issue(
            &mut issues,
            ContextSource::Memory,
            Some(ContextIssueReason::Unavailable),
        );
        record_source_issue(
            &mut issues,
            ContextSource::Tasks,
            Some(ContextIssueReason::Denied),
        );
        record_source_issue(
            &mut issues,
            ContextSource::Tasks,
            Some(ContextIssueReason::BudgetExceeded),
        );
        assert_eq!(issues.len(), 2);
        record_source_issue(&mut issues, ContextSource::Tasks, None);
        assert_eq!(
            issues,
            vec![ContextIssue {
                source: ContextSource::Memory,
                reason: ContextIssueReason::Unavailable
            }]
        );
    }
}
