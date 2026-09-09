use std::{collections::HashMap, future::Future, sync::Mutex};

use floe_domain::PersonId;
use serde::{Deserialize, Serialize};
use tokio::time::Instant;
use uuid::Uuid;

use crate::{AGENT_VERSION, AgentFailure, Cancellation, UsageLedger};

pub const A2A_PROTOCOL_VERSION: &str = "1.0";
pub const EXPERT_RESULT_MEDIA_TYPE: &str = "application/vnd.floe.expert-result+json;version=1";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCard {
    pub schema_version: u32,
    pub protocol_version: String,
    pub id: String,
    pub version: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub domain_tags: Vec<String>,
    #[serde(default)]
    pub skills: Vec<String>,
}

impl AgentCard {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.schema_version != AGENT_VERSION
            || self.protocol_version != A2A_PROTOCOL_VERSION
            || !bounded_text(&self.id, 128)
            || !bounded_text(&self.version, 64)
            || !bounded_text(&self.name, 128)
            || !bounded_text(&self.description, 512)
            || self.domain_tags.len() > 8
            || self.skills.len() > 8
            || self
                .domain_tags
                .iter()
                .any(|value| !bounded_text(value, 64))
            || self.skills.iter().any(|value| !bounded_text(value, 256))
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

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
    pub failure: Option<AgentFailure>,
}

impl A2ATask {
    pub fn result_text(&self) -> Result<&str, AgentFailure> {
        if self.state != A2ATaskState::Completed || self.failure.is_some() {
            return Err(self.failure.unwrap_or(AgentFailure::InvalidModelOutput));
        }
        self.artifacts
            .iter()
            .flat_map(|artifact| &artifact.parts)
            .find_map(|part| match part {
                A2APart::Text { text } => Some(text.as_str()),
                A2APart::Data { .. } => None,
            })
            .filter(|text| bounded_text(text, 4096))
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
    pub usage: UsageLedger,
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
            failure: None,
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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::Notify;

    struct TestAgent {
        person_id: PersonId,
        wait_for_cancellation: bool,
        started: Notify,
    }

    impl InProcessAgent for TestAgent {
        fn agent_cards(&self, person_id: PersonId) -> Vec<AgentCard> {
            (person_id == self.person_id)
                .then(|| AgentCard {
                    schema_version: AGENT_VERSION,
                    protocol_version: A2A_PROTOCOL_VERSION.into(),
                    id: "schedule".into(),
                    version: "1.0.0".into(),
                    name: "Schedule Expert".into(),
                    description: "Reviews schedules in an isolated context.".into(),
                    domain_tags: vec!["schedule".into()],
                    skills: vec![],
                })
                .into_iter()
                .collect()
        }

        async fn handle_message(
            &self,
            request: A2ASendMessageRequest,
        ) -> Result<A2ATask, AgentFailure> {
            self.started.notify_one();
            if self.wait_for_cancellation {
                request.cancellation.cancelled().await;
                return Err(AgentFailure::Cancelled);
            }
            Ok(A2ATask {
                id: request.message.task_id.unwrap(),
                context_id: request.message.context_id,
                agent_id: request.agent_id,
                state: A2ATaskState::Completed,
                history: vec![request.message],
                artifacts: vec![A2AArtifact {
                    artifact_id: Uuid::new_v4(),
                    name: "Result".into(),
                    parts: vec![A2APart::Text {
                        text: "The schedule has room.".into(),
                    }],
                }],
                failure: None,
            })
        }
    }

    fn request(person_id: PersonId, task_id: Uuid) -> A2ASendMessageRequest {
        let context_id = Uuid::new_v4();
        A2ASendMessageRequest {
            usage: UsageLedger::default(),
            schema_version: AGENT_VERSION,
            person_id,
            session_id: Uuid::new_v4(),
            parent_turn_id: Uuid::new_v4(),
            agent_id: "schedule".into(),
            message: A2AMessage {
                message_id: Uuid::new_v4(),
                context_id,
                task_id: Some(task_id),
                role: A2AMessageRole::User,
                parts: vec![A2APart::Text {
                    text: "Review tomorrow's schedule.".into(),
                }],
            },
            max_output_bytes: 4096,
            deadline: Instant::now() + std::time::Duration::from_secs(1),
            cancellation: Cancellation::default(),
        }
    }

    #[tokio::test]
    async fn in_process_transport_routes_and_retains_terminal_tasks() {
        let person_id = PersonId::new();
        let task_id = Uuid::new_v4();
        let agent = TestAgent {
            person_id,
            wait_for_cancellation: false,
            started: Notify::new(),
        };
        let transport = InProcessA2ATransport::new(&agent);
        let completed = transport
            .send_message(request(person_id, task_id))
            .await
            .unwrap();
        assert_eq!(completed.state, A2ATaskState::Completed);
        let lookup = A2ATaskRequest {
            person_id,
            agent_id: "schedule".into(),
            task_id,
        };
        assert_eq!(transport.get_task(lookup.clone()).await.unwrap(), completed);
        assert_eq!(
            transport.cancel_task(lookup).await,
            Err(AgentFailure::Conflict)
        );
    }

    #[tokio::test]
    async fn in_process_transport_cancels_an_active_task() {
        let person_id = PersonId::new();
        let task_id = Uuid::new_v4();
        let agent = TestAgent {
            person_id,
            wait_for_cancellation: true,
            started: Notify::new(),
        };
        let transport = InProcessA2ATransport::new(&agent);
        let lookup = A2ATaskRequest {
            person_id,
            agent_id: "schedule".into(),
            task_id,
        };
        let send = transport.send_message(request(person_id, task_id));
        let cancel = async {
            agent.started.notified().await;
            transport.cancel_task(lookup.clone()).await
        };
        let (sent, cancelled) = tokio::join!(send, cancel);
        assert_eq!(sent, Err(AgentFailure::Cancelled));
        assert_eq!(cancelled.unwrap().state, A2ATaskState::Cancelled);
        assert_eq!(
            transport.get_task(lookup).await.unwrap().state,
            A2ATaskState::Cancelled
        );
    }
}
