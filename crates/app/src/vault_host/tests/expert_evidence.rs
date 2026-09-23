use floe_conversation::AgentMessage;
use floe_experts::{
    A2AArtifact, A2AMessage, A2AMessageRole, A2APart, A2ATask, A2ATaskState,
    EXPERT_RESULT_MEDIA_TYPE, ExpertResult,
};
use uuid::Uuid;

pub(in crate::vault_host) fn delegation_message(
    turn_id: Uuid,
    evidence: &ExpertResult,
) -> AgentMessage {
    let context_id = Uuid::new_v4();
    AgentMessage::Delegation {
        turn_id,
        task: A2ATask {
            id: evidence.invocation_id,
            context_id,
            agent_id: evidence.package.id.clone(),
            state: A2ATaskState::Completed,
            history: vec![A2AMessage {
                message_id: Uuid::new_v4(),
                context_id,
                task_id: Some(evidence.invocation_id),
                role: A2AMessageRole::User,
                parts: vec![A2APart::Text {
                    text: "Prepare a bounded scheduling proposal.".into(),
                }],
            }],
            artifacts: vec![A2AArtifact {
                artifact_id: Uuid::new_v4(),
                name: "Schedule expert result".into(),
                parts: vec![
                    A2APart::Text {
                        text: "A scheduling proposal is available for review.".into(),
                    },
                    A2APart::Data {
                        media_type: EXPERT_RESULT_MEDIA_TYPE.into(),
                        data: serde_json::to_string(evidence).unwrap(),
                    },
                ],
            }],
            failure: None,
            settlement: None,
        },
    }
}
