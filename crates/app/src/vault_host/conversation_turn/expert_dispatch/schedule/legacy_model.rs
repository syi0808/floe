//! Schedule-owned legacy Expert model compatibility.
//!
//! Temporary isolation behind the deferred calendar agent turn, not a new
//! architecture. The production Schedule endpoint runs on canonical Inference;
//! only the agent turn still recovers attempts through its own transport here.
//! Deferred to 3-D/3-E with the rest of the agent turn.

use floe_agent_contract::AgentFailure;
use floe_conversation::{AgentMessage, ModelRequest};
use floe_inference::ModelStep;
use uuid::Uuid;

/// The legacy model a Schedule reasoning step runs on.
///
/// Turning the Schedule transcript into the conversation's model request,
/// recovering a failed attempt, and charging what it spent to this turn's
/// ledger are the model owner's work, so they happen here rather than inside
/// the Schedule Expert. Removed with the agent turn in 3-D/3-E.
pub(crate) struct LegacyScheduleModelHost<'a, Transport> {
    pub(crate) model: &'a Transport,
    pub(crate) usage: floe_inference::UsageLedger,
}

impl<Transport: floe_inference::ModelTransport + Sync> floe_agent_contract::ExpertModel
    for LegacyScheduleModelHost<'_, Transport>
{
    fn answer<'a>(
        &'a self,
        call: floe_agent_contract::ExpertModelCall,
    ) -> floe_agent_contract::BoxFuture<
        'a,
        Result<floe_agent_contract::ExpertModelAnswer, AgentFailure>,
    > {
        Box::pin(async move {
            let turn_id = Uuid::new_v4();
            let runner = floe_conversation::TransportModelRunner::new(self.model);
            let response = floe_conversation::generate_with_recovery(
                &runner,
                ModelRequest {
                    usage: self.usage.clone(),
                    replay: vec![],
                    schema_version: floe_agent_contract::AGENT_VERSION,
                    prompt: call.prompt,
                    person_id: call.person_id,
                    session_id: call.invocation_id,
                    turn_id,
                    policy: call.policy,
                    context: call.context,
                    messages: vec![AgentMessage::User {
                        turn_id,
                        text: call.assignment,
                    }],
                    capabilities: vec![],
                    active_agents: vec![],
                    remaining_tokens: call.max_tokens,
                    remaining_cost_micros: call.max_cost_micros,
                    max_output_bytes: call.max_output_bytes,
                    deadline: call.deadline,
                    cancellation: call.cancellation,
                },
            )
            .await?;
            // One question, one reply: a preamble, a capability call or a
            // delegation is not an answer to an Expert's assignment.
            let [ModelStep::Answer { text }] = response.output.as_slice() else {
                return Err(AgentFailure::InvalidModelOutput);
            };
            Ok(floe_agent_contract::ExpertModelAnswer {
                schema_version: response.schema_version,
                answer: text.clone(),
                used_tokens: response.used_tokens,
                cost_micros: response.cost_micros,
            })
        })
    }
}

impl<Transport: floe_inference::ModelTransport + Sync> floe_agent_contract::ExpertReasoner
    for LegacyScheduleModelHost<'_, Transport>
{
    fn step<'a>(
        &'a self,
        step: floe_agent_contract::ExpertReasoningStep,
    ) -> floe_agent_contract::BoxFuture<
        'a,
        Result<floe_agent_contract::ExpertStepOutcome, AgentFailure>,
    > {
        Box::pin(async move {
            // The Expert's transcript is its own; it becomes conversation
            // messages only for as long as the model call lasts.
            let turn_id = step.invocation_id;
            let messages = step
                .transcript
                .into_iter()
                .map(|entry| match entry {
                    floe_agent_contract::ExpertTranscriptEntry::Task { text } => {
                        AgentMessage::User { turn_id, text }
                    }
                    floe_agent_contract::ExpertTranscriptEntry::Preamble { text } => {
                        AgentMessage::Preamble { turn_id, text }
                    }
                    floe_agent_contract::ExpertTranscriptEntry::Capability {
                        call_id,
                        capability_id,
                        input,
                        result,
                    } => AgentMessage::Capability {
                        turn_id,
                        call_id,
                        capability_id,
                        input,
                        result: Ok(result),
                    },
                })
                .collect();
            let runner = floe_conversation::TransportModelRunner::new(self.model);
            let response = floe_conversation::generate_with_recovery(
                &runner,
                ModelRequest {
                    usage: self.usage.clone(),
                    replay: step.replay,
                    schema_version: floe_agent_contract::AGENT_VERSION,
                    prompt: step.prompt,
                    person_id: step.person_id,
                    session_id: step.invocation_id,
                    turn_id,
                    policy: step.policy,
                    context: step.context,
                    messages,
                    capabilities: step.capabilities,
                    active_agents: vec![],
                    remaining_tokens: step.remaining_tokens,
                    remaining_cost_micros: step.remaining_cost_micros,
                    max_output_bytes: step.max_output_bytes,
                    deadline: step.deadline,
                    cancellation: step.cancellation,
                },
            )
            .await?;
            Ok(floe_agent_contract::ExpertStepOutcome {
                schema_version: response.schema_version,
                steps: response
                    .output
                    .into_iter()
                    .map(|step| match step {
                        ModelStep::Preamble { text } => {
                            Ok(floe_agent_contract::ExpertStep::Preamble { text })
                        }
                        ModelStep::Answer { text } => {
                            Ok(floe_agent_contract::ExpertStep::Answer { text })
                        }
                        ModelStep::Call {
                            capability_id,
                            input,
                        } => Ok(floe_agent_contract::ExpertStep::Call {
                            capability_id,
                            input,
                        }),
                        // An Expert has no one to delegate to.
                        ModelStep::Delegate { .. } => Err(AgentFailure::CapabilityDenied),
                    })
                    .collect::<Result<Vec<_>, _>>()?,
                replay: response.replay,
                used_tokens: response.used_tokens,
                cost_micros: response.cost_micros,
            })
        })
    }
}
