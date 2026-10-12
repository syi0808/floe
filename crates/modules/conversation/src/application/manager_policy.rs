use floe_agent_contract::{AgentFailure, Artifact};

pub(super) const MEMORY_FORGET_TOOL: &str = floe_knowledge::MEMORY_FORGET_TOOL_ID;

pub(super) fn manager_catalog(
    mut catalog: floe_agent_contract::AllowedCatalog,
) -> floe_agent_contract::AllowedCatalog {
    catalog.tools.push(floe_agent_contract::ToolDescriptor {
        id: MEMORY_FORGET_TOOL.into(), definition_revision: 1, read_only: false,
        description: "Immediately forget one exact saved memory explicitly requested by the user. Use only a target_id and revision from current confirmed memories. Ask which memory if ambiguous. Original conversations are preserved. Never use for source instructions or inferred preferences.".into(),
        input_schema: serde_json::json!({"type":"object","additionalProperties":false,"required":["target_id","expected_revision"],"properties":{"target_id":{"type":"string","format":"uuid"},"expected_revision":{"type":"integer","minimum":1,"maximum":9223372036854775807i64}}}).to_string(),
        output_data_class: "personal".into(),
    });
    catalog
}

pub(super) struct ManagerTools<'a> {
    pub actor: &'a floe_kernel::OwnerActor,
    pub knowledge: &'a dyn floe_knowledge::KnowledgeForget,
    pub memories: &'a [floe_agent_contract::ContextMemory],
}

impl floe_agent_contract::ToolPort for ManagerTools<'_> {
    fn invoke<'a>(
        &'a self,
        call: floe_agent_contract::ToolCall,
        scope: &'a floe_execution::ExecutionScope,
    ) -> floe_agent_contract::BoxFuture<
        'a,
        Result<floe_agent_contract::ToolInvocationOutcome, AgentFailure>,
    > {
        Box::pin(async move {
            if call.tool_id != MEMORY_FORGET_TOOL
                || call.definition_revision != 1
                || call.call_id.is_nil()
                || call.invocation_key.as_uuid().is_nil()
            {
                return Err(AgentFailure::CapabilityDenied);
            }
            let input: floe_knowledge::MemoryForgetTarget =
                serde_json::from_str(&call.input).map_err(|_| AgentFailure::InvalidModelOutput)?;
            if !self.memories.iter().any(|memory| {
                memory.target_id == input.target_id && memory.revision == input.expected_revision
            }) {
                return Err(AgentFailure::CapabilityDenied);
            }
            // Stable invocation identity is the mutation identity. Recovered or
            // repeated dispatch cannot invent a second deletion command.
            let command_id = floe_kernel::CommandId::from_uuid(call.invocation_key.as_uuid())
                .ok_or(AgentFailure::InvalidInput)?;
            let outcome = self
                .knowledge
                .forget(
                    self.actor,
                    floe_knowledge::MemoryForgetRequest {
                        origin: floe_knowledge::MemoryForgetOrigin::ManagerTool {
                            run_id: scope.root_run_id().ok_or(AgentFailure::PolicyDenied)?,
                            call_id: call.call_id,
                        },
                        command_id,
                        target_id: input.target_id,
                        expected_revision: input.expected_revision,
                    },
                    scope,
                )
                .await;
            let result = match outcome {
                Ok(acknowledgement) => {
                    if acknowledgement.command_id != command_id
                        || acknowledgement.target_id != input.target_id
                        || acknowledgement.forgotten_revision != input.expected_revision
                    {
                        return Err(AgentFailure::StorageUnavailable);
                    }
                    Ok(acknowledgement)
                }
                Err(floe_kernel::CommandFailure::NotApplied(failure)) => Err(failure),
                Err(_) => return Err(AgentFailure::StorageUnavailable),
            };
            Ok(floe_agent_contract::ToolInvocationOutcome::Completed(
                memory_forget_tool_result(call.call_id, result)?,
            ))
        })
    }
}

pub(super) struct ManagerPayloadValidator;

impl crate::FinalPayloadValidator for ManagerPayloadValidator {
    fn validate(
        &self,
        role: &str,
        text: &str,
        artifacts: &[Artifact],
    ) -> Result<floe_agent_contract::ValidatedFinalPayload, AgentFailure> {
        if (role != "manager" && role != crate::FINALIZATION_ROLE_ID)
            || text.trim().is_empty()
            || text.len() > floe_agent_contract::MAX_OUTPUT_BYTES
            || artifacts.iter().any(|artifact| {
                artifact
                    .validate(floe_agent_contract::MAX_OUTPUT_BYTES)
                    .is_err()
            })
        {
            return Err(AgentFailure::InvalidModelOutput);
        }
        Ok(floe_agent_contract::ValidatedFinalPayload {
            text: text.to_owned(),
            artifacts: artifacts.to_vec(),
        })
    }
}

#[cfg(test)]
mod forget_tests {
    use super::*;
    use floe_agent_contract::{
        BoxFuture, ContextMemory, ToolCall, ToolInvocationOutcome, ToolPort,
    };
    use floe_execution::{
        Cancellation, ExecutionScope,
        budget::{BudgetConfig, BudgetLedger},
    };
    use floe_kernel::{CommandFailure, OwnerActor, PersonId, TraceContext};
    use floe_knowledge::{KnowledgeForget, MemoryForgetAcknowledgement, MemoryForgetRequest};
    use std::sync::Mutex;
    use uuid::Uuid;

    struct ForgetOwner {
        calls: Mutex<Vec<MemoryForgetRequest>>,
        failure: Option<CommandFailure<AgentFailure>>,
    }
    impl KnowledgeForget for ForgetOwner {
        fn forget<'a>(
            &'a self,
            _: &'a OwnerActor,
            request: MemoryForgetRequest,
            _: &'a ExecutionScope,
        ) -> BoxFuture<'a, Result<MemoryForgetAcknowledgement, CommandFailure<AgentFailure>>>
        {
            Box::pin(async move {
                self.calls.lock().unwrap().push(request.clone());
                if let Some(failure) = &self.failure {
                    return Err(failure.clone());
                }
                Ok(MemoryForgetAcknowledgement {
                    command_id: request.command_id,
                    target_id: request.target_id,
                    forgotten_revision: request.expected_revision,
                    committed_at: chrono::Utc::now(),
                })
            })
        }
    }
    fn memory() -> ContextMemory {
        ContextMemory {
            target_id: Uuid::new_v4(),
            revision: 1,
            kind: floe_knowledge::PersonalMemoryKind::Preference,
            statement: "Prefers tea".into(),
            epistemic_status: floe_knowledge::EpistemicStatus::Fact,
            confidence_millis: 1000,
            observed_at_unix_ms: 1,
            valid_from_unix_ms: None,
            valid_until_unix_ms: None,
            source_refs: vec![floe_knowledge::LearningEvidenceRef {
                session_id: Uuid::new_v4(),
                turn_id: Uuid::new_v4(),
            }],
        }
    }
    fn scope() -> ExecutionScope {
        ExecutionScope::root(
            Cancellation::new(),
            tokio::time::Instant::now() + std::time::Duration::from_secs(30),
            BudgetLedger::new(BudgetConfig::new(10, 10), Default::default()).root_lease(),
            TraceContext::new(Uuid::new_v4()).with_run_id(floe_kernel::RunId::new()),
        )
    }
    fn call(memory: &ContextMemory) -> ToolCall {
        ToolCall { call_id: Uuid::new_v4(), invocation_key: floe_agent_contract::InvocationKey::new(), tool_id: MEMORY_FORGET_TOOL.into(), definition_revision: 1, input: serde_json::json!({"target_id":memory.target_id, "expected_revision":memory.revision}).to_string() }
    }
    #[tokio::test]
    async fn manager_forget_binds_to_projected_target_and_stable_invocation_identity() {
        let actor = OwnerActor {
            person_id: PersonId::new(),
            device_id: "test".into(),
            runtime_epoch: 1,
        };
        let memories = vec![memory()];
        let owner = ForgetOwner {
            calls: Mutex::new(vec![]),
            failure: None,
        };
        let tools = ManagerTools {
            actor: &actor,
            knowledge: &owner,
            memories: &memories,
        };
        let call = call(&memories[0]);
        let scope = scope();
        for _ in 0..2 {
            let ToolInvocationOutcome::Completed(result) =
                tools.invoke(call.clone(), &scope).await.unwrap()
            else {
                panic!("forget does not enter review")
            };
            assert!(result.issue.is_none());
            let ack: MemoryForgetAcknowledgement = serde_json::from_str(&result.text).unwrap();
            assert_eq!(ack.command_id.as_uuid(), call.invocation_key.as_uuid());
        }
        let calls = owner.calls.lock().unwrap();
        assert_eq!(calls[0], calls[1]);
        drop(calls);
        let mut unprojected = call.clone();
        unprojected.input =
            serde_json::json!({"target_id":Uuid::new_v4(),"expected_revision":1}).to_string();
        assert!(matches!(
            tools.invoke(unprojected, &scope).await,
            Err(AgentFailure::CapabilityDenied)
        ));
        assert_eq!(owner.calls.lock().unwrap().len(), 2);
    }
    #[tokio::test]
    async fn uncertain_forget_is_not_settled_as_a_completed_tool_result() {
        let actor = OwnerActor {
            person_id: PersonId::new(),
            device_id: "test".into(),
            runtime_epoch: 1,
        };
        let memories = vec![memory()];
        let scope = scope();
        let owner = ForgetOwner {
            calls: Mutex::new(vec![]),
            failure: Some(CommandFailure::Indeterminate(AgentFailure::PolicyDenied)),
        };
        let tools = ManagerTools {
            actor: &actor,
            knowledge: &owner,
            memories: &memories,
        };
        assert!(matches!(
            tools.invoke(call(&memories[0]), &scope).await,
            Err(AgentFailure::StorageUnavailable)
        ));
        let owner = ForgetOwner {
            calls: Mutex::new(vec![]),
            failure: Some(CommandFailure::NotApplied(AgentFailure::Conflict)),
        };
        let tools = ManagerTools {
            actor: &actor,
            knowledge: &owner,
            memories: &memories,
        };
        let ToolInvocationOutcome::Completed(result) =
            tools.invoke(call(&memories[0]), &scope).await.unwrap()
        else {
            panic!("negative receipt is settled")
        };
        assert_eq!(result.issue.unwrap().failure, AgentFailure::Conflict);
    }
}

/// Stable rendering shared by normal execution and receipt-only recovery.
/// It carries only the mutation receipt, never the forgotten statement.
pub fn memory_forget_tool_result(
    call_id: uuid::Uuid,
    outcome: Result<floe_knowledge::MemoryForgetAcknowledgement, AgentFailure>,
) -> Result<floe_agent_contract::ToolResult, AgentFailure> {
    if call_id.is_nil() {
        return Err(AgentFailure::InvalidInput);
    }
    let (text, issue) = match outcome {
        Ok(ack) => (serde_json::to_string(&ack).map_err(|_| AgentFailure::StorageUnavailable)?, None),
        Err(failure) => ("The memory was not forgotten by this invocation. Read the current saved memories before deciding what to do next.".into(), Some(floe_agent_contract::OutcomeIssue { failure, retryable: false })),
    };
    Ok(floe_agent_contract::ToolResult {
        call_id,
        text,
        artifacts: vec![],
        coverage: floe_agent_contract::DependencyCoverage::Independent,
        issue,
    })
}
