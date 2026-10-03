use std::{collections::HashMap, future::Future, sync::Mutex};

use floe_agent_contract::PersonId;
use serde::{Deserialize, Serialize};
use tokio::time::Instant;
use uuid::Uuid;

use floe_agent_contract::AgentFailure;
use floe_execution::Cancellation;

pub use floe_agent_contract::{A2A_PROTOCOL_VERSION, AgentCard};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum A2AMessageRole {
    User,
    Agent,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum A2APart {
    Text { text: String },
    Data { media_type: String, data: String },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct A2AMessage {
    pub message_id: Uuid,
    pub context_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<Uuid>,
    pub role: A2AMessageRole,
    pub parts: Vec<A2APart>,
}

impl A2AMessage {
    pub fn text(&self) -> Result<&str, AgentFailure> {
        match self.parts.as_slice() {
            [A2APart::Text { text }] if bounded_text(text, 4096) => Ok(text),
            _ => Err(AgentFailure::InvalidInput),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct A2AArtifact {
    pub artifact_id: Uuid,
    pub name: String,
    pub parts: Vec<A2APart>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum A2ATaskState {
    Submitted,
    Working,
    Completed,
    Failed,
    Cancelled,
    Rejected,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct A2ATask {
    pub id: Uuid,
    pub context_id: Uuid,
    pub agent_id: String,
    pub state: A2ATaskState,
    pub history: Vec<A2AMessage>,
    pub artifacts: Vec<A2AArtifact>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<AgentFailure>,
    #[serde(skip)]
    pub settlement: Option<floe_agent_contract::EndpointSettlement>,
}

impl A2ATask {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        let valid_state = match self.state {
            A2ATaskState::Submitted | A2ATaskState::Working => {
                self.result.is_none() && self.failure.is_none()
            }
            A2ATaskState::Completed => {
                self.result
                    .as_deref()
                    .is_some_and(|result| bounded_text(result, floe_agent_contract::MAX_OUTPUT_BYTES))
                    && self.failure.is_none()
            }
            A2ATaskState::Failed | A2ATaskState::Cancelled | A2ATaskState::Rejected => {
                self.result.is_none() && self.failure.is_some()
            }
        };
        if !valid_state {
            return Err(AgentFailure::InvalidModelOutput);
        }
        Ok(())
    }

    pub fn result_text(&self) -> Result<&str, AgentFailure> {
        if self.state != A2ATaskState::Completed || self.failure.is_some() {
            return Err(self.failure.unwrap_or(AgentFailure::InvalidModelOutput));
        }
        self.result
            .as_deref()
            .filter(|text| bounded_text(text, floe_agent_contract::MAX_OUTPUT_BYTES))
            .ok_or(AgentFailure::InvalidModelOutput)
    }

    pub fn data_part(&self, media_type: &str) -> Option<&str> {
        self.artifacts
            .iter()
            .flat_map(|artifact| &artifact.parts)
            .find_map(|part| match part {
                A2APart::Data {
                    media_type: found,
                    data,
                } if found == media_type => Some(data.as_str()),
                _ => None,
            })
    }
}

pub struct A2ASendMessageRequest {
    pub schema_version: u32,
    pub person_id: PersonId,
    pub session_id: Uuid,
    pub parent_turn_id: Uuid,
    pub agent_id: String,
    pub message: A2AMessage,
    pub max_output_bytes: usize,
    pub deadline: Instant,
    pub cancellation: Cancellation,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct A2ATaskRequest {
    pub person_id: PersonId,
    pub agent_id: String,
    pub task_id: Uuid,
}

pub trait A2AHost: Sync {
    fn agent_cards(&self, person_id: PersonId) -> Vec<AgentCard>;

    fn send_message(
        &self,
        request: A2ASendMessageRequest,
    ) -> impl Future<Output = Result<A2ATask, AgentFailure>> + Send;

    fn get_task(
        &self,
        request: A2ATaskRequest,
    ) -> impl Future<Output = Result<A2ATask, AgentFailure>> + Send;

    fn cancel_task(
        &self,
        request: A2ATaskRequest,
    ) -> impl Future<Output = Result<A2ATask, AgentFailure>> + Send;
}

pub struct NoA2AHost;

impl A2AHost for NoA2AHost {
    fn agent_cards(&self, _: PersonId) -> Vec<AgentCard> {
        vec![]
    }

    async fn send_message(&self, _: A2ASendMessageRequest) -> Result<A2ATask, AgentFailure> {
        Err(AgentFailure::CapabilityUnavailable)
    }

    async fn get_task(&self, _: A2ATaskRequest) -> Result<A2ATask, AgentFailure> {
        Err(AgentFailure::CapabilityUnavailable)
    }

    async fn cancel_task(&self, _: A2ATaskRequest) -> Result<A2ATask, AgentFailure> {
        Err(AgentFailure::CapabilityUnavailable)
    }
}

pub trait InProcessAgent: Sync {
    fn agent_cards(&self, person_id: PersonId) -> Vec<AgentCard>;

    fn handle_message(
        &self,
        request: A2ASendMessageRequest,
    ) -> impl Future<Output = Result<A2ATask, AgentFailure>> + Send;
}

pub struct InProcessA2ATransport<'agent, Agent> {
    agent: &'agent Agent,
    tasks: Mutex<HashMap<Uuid, StoredTask>>,
}

struct StoredTask {
    person_id: PersonId,
    task: A2ATask,
    cancellation: Cancellation,
}

impl<'agent, Agent> InProcessA2ATransport<'agent, Agent> {
    pub fn new(agent: &'agent Agent) -> Self {
        Self {
            agent,
            tasks: Mutex::new(HashMap::new()),
        }
    }
}

impl<Agent: InProcessAgent> A2AHost for InProcessA2ATransport<'_, Agent> {
    fn agent_cards(&self, person_id: PersonId) -> Vec<AgentCard> {
        self.agent.agent_cards(person_id)
    }

    async fn send_message(&self, request: A2ASendMessageRequest) -> Result<A2ATask, AgentFailure> {
        let task_id = request.message.task_id.ok_or(AgentFailure::InvalidInput)?;
        let submitted = A2ATask {
            id: task_id,
            context_id: request.message.context_id,
            agent_id: request.agent_id.clone(),
            state: A2ATaskState::Submitted,
            history: vec![request.message.clone()],
            artifacts: vec![],
            result: None,
            failure: None,
            settlement: None,
        };
        {
            let mut tasks = self
                .tasks
                .lock()
                .map_err(|_| AgentFailure::CapabilityUnavailable)?;
            if tasks.contains_key(&task_id) {
                return Err(AgentFailure::Conflict);
            }
            tasks.insert(
                task_id,
                StoredTask {
                    person_id: request.person_id,
                    task: A2ATask {
                        state: A2ATaskState::Working,
                        ..submitted
                    },
                    cancellation: request.cancellation.clone(),
                },
            );
        }
        let result = self.agent.handle_message(request).await;
        let mut tasks = self
            .tasks
            .lock()
            .map_err(|_| AgentFailure::CapabilityUnavailable)?;
        let stored = tasks.get_mut(&task_id).ok_or(AgentFailure::NotFound)?;
        if stored.task.state == A2ATaskState::Cancelled {
            return Err(AgentFailure::Cancelled);
        }
        match result {
            Ok(task) => {
                task.validate()?;
                stored.task = task.clone();
                Ok(task)
            }
            Err(failure) => {
                stored.task.state = A2ATaskState::Failed;
                stored.task.failure = Some(failure);
                Err(failure)
            }
        }
    }

    async fn get_task(&self, request: A2ATaskRequest) -> Result<A2ATask, AgentFailure> {
        let tasks = self
            .tasks
            .lock()
            .map_err(|_| AgentFailure::CapabilityUnavailable)?;
        let stored = tasks.get(&request.task_id).ok_or(AgentFailure::NotFound)?;
        if stored.person_id != request.person_id || stored.task.agent_id != request.agent_id {
            return Err(AgentFailure::NotFound);
        }
        Ok(stored.task.clone())
    }

    async fn cancel_task(&self, request: A2ATaskRequest) -> Result<A2ATask, AgentFailure> {
        let mut tasks = self
            .tasks
            .lock()
            .map_err(|_| AgentFailure::CapabilityUnavailable)?;
        let stored = tasks
            .get_mut(&request.task_id)
            .ok_or(AgentFailure::NotFound)?;
        if stored.person_id != request.person_id || stored.task.agent_id != request.agent_id {
            return Err(AgentFailure::NotFound);
        }
        if matches!(
            stored.task.state,
            A2ATaskState::Completed
                | A2ATaskState::Failed
                | A2ATaskState::Cancelled
                | A2ATaskState::Rejected
        ) {
            return Err(AgentFailure::Conflict);
        }
        stored.cancellation.cancel();
        stored.task.state = A2ATaskState::Cancelled;
        stored.task.failure = Some(AgentFailure::Cancelled);
        Ok(stored.task.clone())
    }
}

pub struct A2ARouter<'transport, Transport> {
    transport: &'transport Transport,
}

impl<'transport, Transport> A2ARouter<'transport, Transport> {
    pub const fn new(transport: &'transport Transport) -> Self {
        Self { transport }
    }
}

impl<Transport: A2AHost> A2AHost for A2ARouter<'_, Transport> {
    fn agent_cards(&self, person_id: PersonId) -> Vec<AgentCard> {
        self.transport.agent_cards(person_id)
    }

    async fn send_message(&self, request: A2ASendMessageRequest) -> Result<A2ATask, AgentFailure> {
        self.transport.send_message(request).await
    }

    async fn get_task(&self, request: A2ATaskRequest) -> Result<A2ATask, AgentFailure> {
        self.transport.get_task(request).await
    }

    async fn cancel_task(&self, request: A2ATaskRequest) -> Result<A2ATask, AgentFailure> {
        self.transport.cancel_task(request).await
    }
}

fn bounded_text(value: &str, maximum: usize) -> bool {
    !value.trim().is_empty()
        && value.len() <= maximum
        && !value
            .chars()
            .any(|character| character.is_control() && character != '\n')
}
