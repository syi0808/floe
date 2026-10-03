//! Product-safe session view, excluding storage authority and artifact payloads.
use crate::{AgentMessage, AgentOutcome, AgentSession, AgentUsage, ContinuationToken};
use floe_agent_contract::{ArtifactPart, TaskState, UserInteractionKind};
use floe_kernel::{AgentFailure, PersonId, TaskId};
use uuid::Uuid;

const MAX_SESSION_MESSAGES: usize = 256;

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct SessionSnapshot {
    pub id: Uuid,
    pub person_id: PersonId,
    pub revision: u64,
    pub active_turn: Option<Uuid>,
    pub last_outcome: Option<AgentOutcome>,
    pub usage: AgentUsage,
    pub continuation_ref: Option<ContinuationToken>,
    pub messages: Vec<SessionMessage>,
    pub has_earlier_messages: bool,
}
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SessionMessage {
    User {
        message_id: Uuid,
        turn_id: Uuid,
        text: String,
    },
    Assistant {
        message_id: Uuid,
        turn_id: Uuid,
        text: String,
    },
    Preamble {
        message_id: Uuid,
        turn_id: Uuid,
        text: String,
    },
    Compaction {
        message_id: Uuid,
        turn_id: Uuid,
        summary: String,
    },
    Capability {
        message_id: Uuid,
        turn_id: Uuid,
        call_id: Uuid,
        capability_id: String,
        result: Result<String, AgentFailure>,
    },
    Delegation {
        message_id: Uuid,
        turn_id: Uuid,
        task: TaskSummary,
    },
    Interaction {
        message_id: Uuid,
        turn_id: Uuid,
        interaction_id: Uuid,
        interaction_kind: UserInteractionKind,
    },
}
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct TaskSummary {
    pub execution_receipt: Option<floe_agent_contract::TaskExecutionReceiptRef>,
    pub task_id: TaskId,
    pub agent_id: String,
    pub state: TaskState,
    pub result: Option<String>,
    pub issue: Option<AgentFailure>,
    pub artifacts: Vec<ArtifactSummary>,
}
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct ArtifactSummary {
    pub artifact_id: Uuid,
    pub name: String,
    pub media_types: Vec<String>,
}

pub(super) fn project_session_snapshot(
    session: AgentSession,
    continuation_ref: Option<ContinuationToken>,
) -> Result<SessionSnapshot, AgentFailure> {
    let start = session.messages.len().saturating_sub(MAX_SESSION_MESSAGES);
    let mut bytes = 0usize;
    let mut messages = Vec::new();
    for message in &session.messages[start..] {
        let turn_id = message.turn_id();
        if turn_id.is_nil() {
            return Err(AgentFailure::StorageUnavailable);
        }
        let projected = match message {
            AgentMessage::User {
                message_id, text, ..
            } => {
                bounded(text, crate::MAX_TURN_TEXT_BYTES, &mut bytes)?;
                if message_id.is_nil() {
                    return Err(AgentFailure::StorageUnavailable);
                }
                SessionMessage::User {
                    message_id: *message_id,
                    turn_id,
                    text: text.clone(),
                }
            }
            AgentMessage::Assistant { text, .. } => {
                bounded(text, floe_agent_contract::MAX_OUTPUT_BYTES, &mut bytes)?;
                SessionMessage::Assistant {
                    message_id: turn_id,
                    turn_id,
                    text: text.clone(),
                }
            }
            AgentMessage::Preamble { text, .. } => {
                bounded(text, floe_agent_contract::MAX_OUTPUT_BYTES, &mut bytes)?;
                SessionMessage::Preamble {
                    message_id: Uuid::new_v5(&turn_id, format!("preamble:{text}").as_bytes()),
                    turn_id,
                    text: text.clone(),
                }
            }
            AgentMessage::Compaction { summary, .. } => {
                bounded(summary, crate::MAX_COMPACTION_SUMMARY_BYTES, &mut bytes)?;
                SessionMessage::Compaction {
                    message_id: Uuid::new_v5(&turn_id, b"compaction"),
                    turn_id,
                    summary: summary.clone(),
                }
            }
            AgentMessage::Capability {
                call_id,
                capability_id,
                result,
                ..
            } => {
                bounded(capability_id, 256, &mut bytes)?;
                if let Ok(text) = result {
                    bounded(text, floe_agent_contract::MAX_OUTPUT_BYTES, &mut bytes)?;
                }
                SessionMessage::Capability {
                    message_id: *call_id,
                    turn_id,
                    call_id: *call_id,
                    capability_id: capability_id.clone(),
                    result: result.clone(),
                }
            }
            AgentMessage::Delegation { task, execution_receipt, .. } => {
                task.validate(floe_agent_contract::MAX_OUTPUT_BYTES)?;
                match execution_receipt {
                    Some(reference) => { reference.validate()?; if reference.execution.task_id != task.task_id { return Err(AgentFailure::StorageUnavailable); } }
                    None if task.state == TaskState::Rejected && task.issue.is_some() && task.artifacts.is_empty()
                        && task.coverage == floe_agent_contract::DependencyCoverage::Independent => {},
                    None => return Err(AgentFailure::StorageUnavailable),
                }
                bounded(&task.agent_id, 256, &mut bytes)?;
                if let Some(text) = &task.result {
                    bounded(text, floe_agent_contract::MAX_OUTPUT_BYTES, &mut bytes)?;
                }
                let artifacts = task
                    .artifacts
                    .iter()
                    .map(|artifact| {
                        let mut media_types = artifact
                            .parts
                            .iter()
                            .map(|part| match part {
                                ArtifactPart::Text { .. } => "text/plain".to_owned(),
                                ArtifactPart::Data { media_type, .. } => media_type.clone(),
                            })
                            .collect::<Vec<_>>();
                        media_types.sort();
                        media_types.dedup();
                        ArtifactSummary {
                            artifact_id: artifact.artifact_id,
                            name: artifact.name.clone(),
                            media_types,
                        }
                    })
                    .collect();
                SessionMessage::Delegation {
                    message_id: task.task_id.as_uuid(),
                    turn_id,
                    task: TaskSummary {
                        execution_receipt: execution_receipt.clone(),
                        task_id: task.task_id,
                        agent_id: task.agent_id.clone(),
                        state: task.state,
                        result: task.result.clone(),
                        issue: task.issue,
                        artifacts,
                    },
                }
            }
            AgentMessage::Interaction {
                interaction_id,
                interaction_kind,
                ..
            } => SessionMessage::Interaction {
                message_id: *interaction_id,
                turn_id,
                interaction_id: *interaction_id,
                interaction_kind: *interaction_kind,
            },
        };
        messages.push(projected);
    }
    Ok(SessionSnapshot {
        id: session.id,
        person_id: session.person_id,
        revision: session.revision,
        active_turn: session.active_turn,
        last_outcome: session.last_outcome,
        usage: session.usage,
        continuation_ref,
        messages,
        has_earlier_messages: start != 0,
    })
}

fn bounded(text: &str, limit: usize, total: &mut usize) -> Result<(), AgentFailure> {
    *total = total
        .checked_add(text.len())
        .ok_or(AgentFailure::StorageUnavailable)?;
    if text.len() > limit || *total > 2 * 1024 * 1024 {
        return Err(AgentFailure::StorageUnavailable);
    }
    Ok(())
}
