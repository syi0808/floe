use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{AgentContext, AgentFailure, Artifact, DependencyCoverage, TaskId};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Submitted,
    Working,
    Completed,
    Failed,
    Rejected,
    Cancelled,
    TimedOut,
    Interrupted,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskSnapshot {
    pub task_id: TaskId,
    pub parent_run_id: Option<Uuid>,
    pub principal: String,
    pub agent_id: String,
    pub definition_revision: u64,
    pub state: TaskState,
    pub result: Option<String>,
    pub artifacts: Vec<Artifact>,
    pub coverage: DependencyCoverage,
    pub issue: Option<AgentFailure>,
}

impl TaskSnapshot {
    pub fn validate(&self, maximum_bytes: usize) -> Result<(), AgentFailure> {
        let result_valid = match self.state {
            TaskState::Completed => self
                .result
                .as_deref()
                .is_some_and(|result| crate::message::bounded(result, maximum_bytes))
                && self.issue.is_none()
                && self.coverage != DependencyCoverage::Unknown,
            TaskState::Submitted | TaskState::Working => {
                self.result.is_none() && self.issue.is_none()
            }
            _ => self.result.is_none(),
        };
        let mut artifact_ids = std::collections::HashSet::new();
        if !self.task_id.is_valid()
            || self.principal.trim().is_empty()
            || self.agent_id.trim().is_empty()
            || self.definition_revision == 0
            || !result_valid
            || self
                .result
                .as_deref()
                .is_some_and(|result| result.len() > maximum_bytes)
            || self
                .artifacts
                .iter()
                .any(|artifact| artifact.validate(maximum_bytes).is_err())
            || self
                .artifacts
                .iter()
                .any(|artifact| !artifact_ids.insert(artifact.artifact_id))
            || self.artifacts.iter().any(|artifact| {
                match (&self.coverage, &artifact.coverage) {
                    (_, DependencyCoverage::Unknown) => true,
                    (_, DependencyCoverage::Independent) => false,
                    (
                        DependencyCoverage::Dependent { dependencies: report },
                        DependencyCoverage::Dependent { dependencies: artifact },
                    ) => !artifact.iter().all(|dependency| report.contains(dependency)),
                    _ => true,
                }
            })
            || self.coverage.validate().is_err()
            || serde_json::to_vec(self)
                .map(|encoded| encoded.len() > maximum_bytes)
                .unwrap_or(true)
        {
            return Err(AgentFailure::InvalidModelOutput);
        }
        Ok(())
    }
}

/// Maximum device identity bytes on a delegation execution context: the same
/// limit connector device validation enforces.
pub const MAX_DELEGATION_DEVICE_ID_BYTES: usize = 128;

/// Maximum serialized bytes of one delegation execution context. The bound
/// keeps DelegationIntent and validated-batch journal entries sized for
/// durable persistence.
pub const MAX_DELEGATION_EXECUTION_CONTEXT_BYTES: usize = 96 * 1024;

/// The explicit secret-free host context one delegation executes under.
///
/// session/device/context/output-bound only: no saved connection, token,
/// base URL, model profile, recipient/consent, provider route, or
/// cancellation/deadline (ExecutionScope owns those). The Manager binds the
/// exact context to the validated batch before dispatch, journals it on the
/// DelegationIntent, and every replay check covers it, so a crash followed by
/// changed host state can never reuse a prior Task result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DelegationExecutionContext {
    pub session_id: Uuid,
    pub device_id: String,
    pub agent_context: AgentContext,
    pub max_output_bytes: usize,
}

impl DelegationExecutionContext {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.session_id.is_nil()
            || self.device_id.trim().is_empty()
            || self.device_id.len() > MAX_DELEGATION_DEVICE_ID_BYTES
            || self.max_output_bytes == 0
            || self.max_output_bytes > crate::MAX_OUTPUT_BYTES
        {
            return Err(AgentFailure::InvalidInput);
        }
        self.agent_context.validate()?;
        if serde_json::to_vec(self)
            .map(|encoded| encoded.len() > MAX_DELEGATION_EXECUTION_CONTEXT_BYTES)
            .unwrap_or(true)
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DelegationRequest {
    pub task_id: TaskId,
    pub parent_run_id: Option<Uuid>,
    pub principal: String,
    pub invocation_key: crate::InvocationKey,
    pub selected_agent_id: String,
    pub selected_definition_revision: u64,
    pub message: String,
    pub context_refs: Vec<String>,
    pub execution_context: DelegationExecutionContext,
}

/// One canonical delegation request digest: principal/parent linkage, selected
/// agent + definition revision, assignment message, context refs, and the
/// explicit execution context. Stable TaskId and InvocationKey stay separate
/// identity fields and are checked exactly alongside this digest.
///
/// Task admission/replay, Engine replay verification, and Conversation
/// recovery all use this digest, so a changed device/session/context conflicts
/// rather than reusing a prior Task result.
pub fn delegation_request_digest(request: &DelegationRequest) -> [u8; 32] {
    let encoded = serde_json::to_vec(&(
        &request.principal,
        &request.parent_run_id,
        &request.selected_agent_id,
        &request.selected_definition_revision,
        &request.message,
        &request.context_refs,
        &request.execution_context,
    ))
    .unwrap_or_default();
    Sha256::digest(&encoded).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AgentContext, InvocationKey};

    fn agent_context() -> AgentContext {
        AgentContext {
            projection_version: 1,
            persona: None,
            memories: vec![],
            optional_context_issues: vec![],
            evidence: vec![],
        }
    }

    fn context() -> DelegationExecutionContext {
        DelegationExecutionContext {
            session_id: Uuid::new_v4(),
            device_id: "mac-local".into(),
            agent_context: agent_context(),
            max_output_bytes: crate::MAX_OUTPUT_BYTES,
        }
    }

    fn request(context: DelegationExecutionContext) -> DelegationRequest {
        DelegationRequest {
            task_id: TaskId::new(),
            parent_run_id: Some(Uuid::new_v4()),
            principal: "person-a".into(),
            invocation_key: InvocationKey::new(),
            selected_agent_id: "expert-a".into(),
            selected_definition_revision: 2,
            message: "summarize".into(),
            context_refs: vec![],
            execution_context: context,
        }
    }

    #[test]
    fn malformed_execution_context_is_rejected() {
        assert_eq!(context().validate(), Ok(()));
        let nil_session = DelegationExecutionContext {
            session_id: Uuid::nil(),
            ..context()
        };
        assert_eq!(
            nil_session.validate(),
            Err(AgentFailure::InvalidInput)
        );
        for device_id in ["", "   ", &"d".repeat(MAX_DELEGATION_DEVICE_ID_BYTES + 1)] {
            let bad_device = DelegationExecutionContext {
                device_id: device_id.into(),
                ..context()
            };
            assert_eq!(bad_device.validate(), Err(AgentFailure::InvalidInput));
        }
        let mut bad_issue = context();
        bad_issue.agent_context.optional_context_issues = vec![crate::ContextIssue {
            source: crate::ContextSource::Memory,
            reason: crate::ContextIssueReason::Unavailable,
        }];
        bad_issue.agent_context.memories = vec![floe_context_contract::ContextMemory {
            target_id: Uuid::new_v4(),
            revision: 1,
            kind: floe_context_contract::PersonalMemoryKind::Fact,
            statement: "memory".into(),
            epistemic_status: floe_context_contract::EpistemicStatus::Fact,
            confidence_millis: 500,
            observed_at_unix_ms: 0,
            source_refs: vec![floe_context_contract::LearningEvidenceRef {
                session_id: Uuid::new_v4(),
                turn_id: Uuid::new_v4(),
            }],
            valid_from_unix_ms: None,
            valid_until_unix_ms: None,
        }];
        assert!(bad_issue.validate().is_err());
        for max_output_bytes in [0, crate::MAX_OUTPUT_BYTES + 1] {
            let bad_bound = DelegationExecutionContext {
                max_output_bytes,
                ..context()
            };
            assert_eq!(bad_bound.validate(), Err(AgentFailure::InvalidInput));
        }
    }

    #[test]
    fn serialized_context_carries_no_secret_shaped_field() {
        let encoded = serde_json::to_value(context()).unwrap();
        let object = encoded.as_object().unwrap();
        assert_eq!(object.len(), 4);
        for key in ["session_id", "device_id", "agent_context", "max_output_bytes"] {
            assert!(object.contains_key(key), "missing {key}");
        }
        let rendered = serde_json::to_string(&encoded).unwrap();
        for forbidden in [
            "token",
            "bearer",
            "base_url",
            "credential",
            "secret",
            "password",
            "connection",
            "profile",
            "placement",
            "recipient",
            "consent",
            "route",
            "deadline",
            "cancellation",
        ] {
            assert!(
                !rendered.contains(forbidden),
                "secret-shaped field leaked: {forbidden}"
            );
        }
    }

    #[test]
    fn identical_context_hashes_identically_and_host_changes_diverge() {
        let first = request(context());
        let mut second = first.clone();
        second.task_id = TaskId::new();
        second.invocation_key = InvocationKey::new();
        assert_eq!(
            delegation_request_digest(&first),
            delegation_request_digest(&second)
        );
        let mut changed = first.clone();
        changed.execution_context.device_id = "iphone".into();
        assert_ne!(
            delegation_request_digest(&first),
            delegation_request_digest(&changed)
        );
        let mut changed = first.clone();
        changed.execution_context.session_id = Uuid::new_v4();
        assert_ne!(
            delegation_request_digest(&first),
            delegation_request_digest(&changed)
        );
        let mut changed = first.clone();
        changed.execution_context.max_output_bytes = 1024;
        assert_ne!(
            delegation_request_digest(&first),
            delegation_request_digest(&changed)
        );
        let mut changed = first.clone();
        changed.message = "summarize differently".into();
        assert_ne!(
            delegation_request_digest(&first),
            delegation_request_digest(&changed)
        );
    }

    #[test]
    fn oversized_context_is_rejected() {
        let mut big = context();
        big.agent_context.evidence = vec![floe_context_contract::ContextEvidence {
            source_handle: "h".into(),
            data_class: floe_context_contract::DataClass::Personal,
            untrusted_text: "e".repeat(MAX_DELEGATION_EXECUTION_CONTEXT_BYTES),
            expires_at_unix_ms: u64::MAX,
        }];
        assert_eq!(big.validate(), Err(AgentFailure::InvalidInput));
    }
}

/// Context references carried on one delegation: bounded count, bounded bytes
/// each. Every layer that accepts model-produced references — provider
/// decode, legacy and canonical validation, the journal exchange — enforces
/// this same bound and maps a violation to its own failure.
pub fn valid_context_refs(references: &[String]) -> bool {
    references.len() <= crate::MAX_CONTEXT_REFS
        && references
            .iter()
            .all(|reference| reference.len() <= crate::MAX_OUTPUT_BYTES)
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskReceipt {
    pub task_id: TaskId,
    pub snapshot: TaskSnapshot,
    pub replay: Option<crate::ReplayReceipt>,
}
