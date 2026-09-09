use std::{future::Future, sync::Mutex, time::SystemTime};

use chrono::{DateTime, Datelike, FixedOffset, Timelike, Utc};
use floe_domain::PersonId;
use serde::{Deserialize, Serialize};
use tokio::time::Instant;
use uuid::Uuid;

use crate::{
    AGENT_VERSION, AgentContext, AgentFailure, AgentMessage, AgentRegistry, Cancellation,
    CapabilityDescriptor, DataClass, ExpertRule, InferencePolicyDecision, ModelRequest,
    ModelRunner, ModelStep, PackageImplementation, PackageRef, schedule_expert_prompt,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TimelineViewItem {
    pub evidence_handle: Uuid,
    pub untrusted_title: String,
    pub starts_at_unix_ms: u64,
    pub ends_at_unix_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertTimelineView {
    pub schema_version: u32,
    pub handle: Uuid,
    pub person_id: PersonId,
    pub data_class: DataClass,
    pub source_handle: String,
    pub range_start_unix_ms: u64,
    pub range_end_unix_ms: u64,
    pub expires_at_unix_ms: u64,
    pub items: Vec<TimelineViewItem>,
}

pub struct TimelineViewRead {
    pub person_id: PersonId,
    pub handle: Uuid,
    pub max_items: usize,
    pub max_bytes: usize,
    pub deadline: Instant,
    pub cancellation: Cancellation,
}

pub trait ExpertViews: Sync {
    fn timeline(
        &self,
        request: TimelineViewRead,
    ) -> impl Future<Output = Result<ExpertTimelineView, AgentFailure>> + Send;
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExpertInput {
    Briefing {
        focus_minutes: u16,
    },
    ProposeFocus {
        focus_minutes: u16,
    },
    Analyze {
        request: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        focus_minutes: Option<u16>,
    },
}

#[derive(Clone, Copy)]
pub struct ExpertBudget {
    pub max_view_calls: u32,
    pub max_view_bytes: usize,
    pub max_output_bytes: usize,
    pub max_insights: usize,
    pub max_model_calls: u32,
    pub max_tool_calls: u32,
    pub max_model_tokens: u64,
    pub max_model_cost_micros: u64,
}

impl Default for ExpertBudget {
    fn default() -> Self {
        Self {
            max_view_calls: 1,
            max_view_bytes: 16384,
            max_output_bytes: 16384,
            max_insights: 8,
            max_model_calls: 10,
            max_tool_calls: 9,
            max_model_tokens: 40_960,
            max_model_cost_micros: 50_000,
        }
    }
}

pub struct ExpertInvocation {
    pub usage: crate::UsageLedger,
    pub schema_version: u32,
    pub invocation_id: Uuid,
    pub instance_id: Uuid,
    pub person_id: PersonId,
    pub assignment_id: Uuid,
    pub expected_registry_revision: u64,
    pub granted_view_handles: Vec<Uuid>,
    pub allowed_data_classes: Vec<DataClass>,
    pub current_time_unix_ms: u64,
    pub timezone_offset_seconds: i32,
    pub input: ExpertInput,
    pub budget: ExpertBudget,
    pub deadline: Instant,
    pub cancellation: Cancellation,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExpertInsight {
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
pub struct ExpertFocusProposal {
    pub starts_at_unix_ms: u64,
    pub ends_at_unix_ms: u64,
    pub view_handle: Uuid,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertResult {
    pub schema_version: u32,
    pub invocation_id: Uuid,
    pub instance_id: Uuid,
    pub person_id: PersonId,
    pub assignment_id: Uuid,
    pub package: PackageRef,
    pub view_handle: Uuid,
    pub source_handle: String,
    pub data_class: DataClass,
    pub expires_at_unix_ms: u64,
    pub insights: Vec<ExpertInsight>,
    pub action_proposals: Vec<ExpertFocusProposal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default)]
    pub model_calls: u32,
    pub state_revision: u64,
    pub view_calls: u32,
}

pub struct ExpertHost<'host, Views> {
    pub registry: &'host Mutex<AgentRegistry>,
    pub views: &'host Views,
}

impl<Views: ExpertViews> ExpertHost<'_, Views> {
    pub async fn invoke(&self, invocation: ExpertInvocation) -> Result<ExpertResult, AgentFailure> {
        self.invoke_inner::<NoExpertModel>(invocation, ExpertReasoning::Deterministic)
            .await
    }

    pub async fn invoke_with_model<Model: ModelRunner + Sync>(
        &self,
        invocation: ExpertInvocation,
        model: &Model,
        policy: &InferencePolicyDecision,
    ) -> Result<ExpertResult, AgentFailure> {
        self.invoke_inner(invocation, ExpertReasoning::Lightweight { model, policy })
            .await
    }

    async fn invoke_inner<Model: ModelRunner + Sync>(
        &self,
        mut invocation: ExpertInvocation,
        reasoning: ExpertReasoning<'_, Model>,
    ) -> Result<ExpertResult, AgentFailure> {
        if invocation.schema_version != AGENT_VERSION {
            return Err(AgentFailure::UnsupportedVersion);
        }
        if invocation.timezone_offset_seconds.unsigned_abs() >= 86_400 {
            return Err(AgentFailure::InvalidInput);
        }
        check_running(&invocation)?;
        invocation.deadline = invocation
            .deadline
            .min(Instant::now() + std::time::Duration::from_secs(30));
        if invocation.budget.max_view_calls == 0
            || invocation.budget.max_view_bytes == 0
            || invocation.budget.max_output_bytes == 0
            || invocation.budget.max_insights == 0
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        let focus_minutes = match &invocation.input {
            ExpertInput::Briefing { focus_minutes }
            | ExpertInput::ProposeFocus { focus_minutes } => Some(*focus_minutes),
            ExpertInput::Analyze {
                request,
                focus_minutes,
            } => {
                if request.trim().is_empty() || request.len() > 2048 {
                    return Err(AgentFailure::InvalidInput);
                }
                *focus_minutes
            }
        };
        if focus_minutes.is_some_and(|minutes| !(1..=240).contains(&minutes)) {
            return Err(AgentFailure::InvalidInput);
        }
        let resolved = self
            .registry
            .lock()
            .map_err(|_| AgentFailure::CapabilityUnavailable)?
            .resolve(
                invocation.instance_id,
                invocation.person_id,
                invocation.assignment_id,
                invocation.expected_registry_revision,
                &invocation.granted_view_handles,
            )?;
        if !invocation
            .allowed_data_classes
            .contains(&resolved.data_class)
            || invocation
                .allowed_data_classes
                .iter()
                .any(|class| matches!(class, DataClass::Credential | DataClass::DeviceOnlyRaw))
        {
            return Err(AgentFailure::PolicyDenied);
        }
        if resolved.assignment.private_state.last_invocation_id == Some(invocation.invocation_id) {
            return Err(AgentFailure::Conflict);
        }
        let cancellation = Cancellation::default();
        let _guard = ViewCancellation(cancellation.clone());
        let read = TimelineViewRead {
            person_id: invocation.person_id,
            handle: invocation.granted_view_handles[0],
            max_items: 32,
            max_bytes: invocation.budget.max_view_bytes.min(16384),
            deadline: invocation.deadline,
            cancellation,
        };
        let view_output = crate::capability_execution::execute_recorded(
            &invocation.usage,
            crate::CapabilityExecution {
                scope_id: invocation.invocation_id,
                turn_id: invocation.invocation_id,
                call_id: Uuid::new_v4(),
                capability_id: "view.timeline".into(),
                input: serde_json::json!({"handle": read.handle}).to_string(),
                state: crate::CapabilityExecutionState::Started,
                result: None,
                replay: None,
            },
            invocation.deadline,
            &invocation.cancellation,
            invocation.budget.max_view_bytes.min(16384),
            Box::pin(async {
                let view = self.views.timeline(read).await?;
                validate_view(&view, &invocation, resolved.data_class)?;
                serde_json::to_string(&view).map_err(|_| AgentFailure::InvalidInput)
            }),
        )
        .await??;
        let view: ExpertTimelineView =
            serde_json::from_str(&view_output).map_err(|_| AgentFailure::InvalidInput)?;
        check_running(&invocation)?;
        validate_view(&view, &invocation, resolved.data_class)?;
        let minimum = match &resolved.package.implementation {
            PackageImplementation::Schedule => focus_minutes,
            PackageImplementation::Declarative { rules } => match rules.as_slice() {
                [ExpertRule::FindFocusWindow { minimum_minutes }] => Some(
                    focus_minutes
                        .unwrap_or(*minimum_minutes)
                        .max(*minimum_minutes),
                ),
                _ => return Err(AgentFailure::CapabilityDenied),
            },
            _ => return Err(AgentFailure::CapabilityDenied),
        };
        let mut insights = match minimum {
            Some(minimum) => analyze_schedule(&view, minimum),
            None => commitment_insights(&view, invocation.budget.max_insights.min(8)),
        };
        if insights.len() > invocation.budget.max_insights.min(8) {
            return Err(AgentFailure::BudgetExceeded);
        }
        let mut action_proposals = if matches!(&invocation.input, ExpertInput::ProposeFocus { .. })
        {
            insights
                .iter()
                .filter_map(|insight| match insight {
                    ExpertInsight::FocusWindow {
                        starts_at_unix_ms,
                        ends_at_unix_ms,
                    } => Some(ExpertFocusProposal {
                        starts_at_unix_ms: *starts_at_unix_ms,
                        ends_at_unix_ms: *ends_at_unix_ms,
                        view_handle: view.handle,
                    }),
                    _ => None,
                })
                .collect()
        } else {
            vec![]
        };
        let (summary, model_calls) = match (&resolved.package.implementation, reasoning) {
            (PackageImplementation::Schedule, ExpertReasoning::Lightweight { model, policy }) => {
                let (summary, model_calls, model_proposals) =
                    run_schedule_reasoning(model, policy, &invocation, &view, view.data_class)
                        .await?;
                if !model_proposals.is_empty() {
                    action_proposals = model_proposals;
                    for proposal in &action_proposals {
                        let insight = ExpertInsight::FocusWindow {
                            starts_at_unix_ms: proposal.starts_at_unix_ms,
                            ends_at_unix_ms: proposal.ends_at_unix_ms,
                        };
                        if !insights.contains(&insight) {
                            insights.push(insight);
                        }
                    }
                }
                (Some(summary), model_calls)
            }
            _ => (None, 0),
        };
        if insights.len() > invocation.budget.max_insights.min(8) {
            return Err(AgentFailure::BudgetExceeded);
        }
        let mut result = ExpertResult {
            schema_version: AGENT_VERSION,
            invocation_id: invocation.invocation_id,
            instance_id: invocation.instance_id,
            person_id: invocation.person_id,
            assignment_id: invocation.assignment_id,
            package: resolved.package.reference.clone(),
            view_handle: view.handle,
            source_handle: view.source_handle,
            data_class: view.data_class,
            expires_at_unix_ms: view.expires_at_unix_ms,
            insights,
            action_proposals,
            summary,
            model_calls,
            state_revision: resolved
                .assignment
                .private_state
                .revision
                .checked_add(1)
                .ok_or(AgentFailure::BudgetExceeded)?,
            view_calls: 1,
        };
        if serde_json::to_vec(&result)
            .map_err(|_| AgentFailure::InvalidModelOutput)?
            .len()
            > invocation.budget.max_output_bytes.min(16384)
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        let mut registry = self
            .registry
            .lock()
            .map_err(|_| AgentFailure::CapabilityUnavailable)?;
        check_running(&invocation)?;
        if result.expires_at_unix_ms <= now_unix_ms()? {
            return Err(AgentFailure::StaleContext);
        }
        registry.resolve(
            invocation.instance_id,
            invocation.person_id,
            invocation.assignment_id,
            resolved.registry_revision,
            &invocation.granted_view_handles,
        )?;
        result.state_revision = registry.complete(&resolved, invocation.invocation_id)?;
        Ok(result)
    }
}

enum ExpertReasoning<'model, Model> {
    Deterministic,
    Lightweight {
        model: &'model Model,
        policy: &'model InferencePolicyDecision,
    },
}

struct NoExpertModel;

impl ModelRunner for NoExpertModel {
    fn placement(&self) -> crate::ModelPlacement {
        crate::ModelPlacement::DeviceLocal
    }

    async fn generate(&self, _: ModelRequest) -> Result<crate::ModelResponse, AgentFailure> {
        Err(AgentFailure::ModelUnavailable)
    }
}

async fn run_schedule_reasoning<Model: ModelRunner + Sync>(
    model: &Model,
    policy: &InferencePolicyDecision,
    invocation: &ExpertInvocation,
    view: &ExpertTimelineView,
    data_class: DataClass,
) -> Result<(String, u32, Vec<ExpertFocusProposal>), AgentFailure> {
    if invocation.budget.max_model_calls == 0
        || invocation.budget.max_model_calls > 10
        || invocation.budget.max_model_tokens == 0
        || invocation.budget.max_model_cost_micros == 0
    {
        return Err(AgentFailure::BudgetExceeded);
    }
    let turn_id = invocation.invocation_id;
    let capabilities = schedule_capabilities(data_class);
    let task = serde_json::json!({
        "request": &invocation.input,
        "runtime_context": {
            "current_time_unix_ms": invocation.current_time_unix_ms,
            "current_datetime_local": format_datetime(
                invocation.current_time_unix_ms,
                invocation.timezone_offset_seconds,
                DateTimePrecision::Full,
            )?,
            "timezone_offset_seconds": invocation.timezone_offset_seconds,
        },
        "authorized_range": {
            "starts_at_unix_ms": view.range_start_unix_ms,
            "ends_at_unix_ms": view.range_end_unix_ms
        }
    })
    .to_string();
    let mut messages = vec![AgentMessage::User {
        turn_id,
        text: task,
    }];
    let mut replay = vec![];
    let mut used_tokens = 0;
    let mut used_cost = 0;
    let mut expert_policy = policy.clone();
    expert_policy.purpose = "schedule-summary".into();
    expert_policy.performance_class = "fast".into();
    let mut tool_calls = 0;
    let mut action_proposals = vec![];
    for model_call in 1..=invocation.budget.max_model_calls {
        let available_capabilities = if tool_calls < invocation.budget.max_tool_calls {
            capabilities.clone()
        } else {
            vec![]
        };
        let response = generate_schedule_step(
            model,
            &expert_policy,
            invocation,
            &messages,
            &replay,
            available_capabilities,
            used_tokens,
            used_cost,
        )
        .await?;
        used_tokens = used_tokens
            .checked_add(response.used_tokens)
            .ok_or(AgentFailure::BudgetExceeded)?;
        used_cost = used_cost
            .checked_add(response.cost_micros)
            .ok_or(AgentFailure::BudgetExceeded)?;
        if response.call_count()
            > invocation.budget.max_tool_calls.saturating_sub(tool_calls) as usize
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        let mut call_index = 0;
        for step in response.output.clone() {
            match step {
                ModelStep::Preamble { text } => {
                    messages.push(AgentMessage::Preamble { turn_id, text });
                }
                ModelStep::Answer { text } => {
                    let summary = text.trim();
                    if summary.is_empty() || summary.len() > 2048 {
                        return Err(AgentFailure::InvalidModelOutput);
                    }
                    return Ok((summary.into(), model_call, action_proposals));
                }
                ModelStep::Call {
                    capability_id,
                    input,
                } => {
                    if !capabilities
                        .iter()
                        .any(|capability| capability.id == capability_id)
                        || tool_calls >= invocation.budget.max_tool_calls
                    {
                        return Err(AgentFailure::InvalidModelOutput);
                    }
                    tool_calls += 1;
                    if capability_id == "schedule.propose_window" {
                        if !action_proposals.is_empty() {
                            return Err(AgentFailure::InvalidModelOutput);
                        }
                        let proposal: ScheduleProposalInput = serde_json::from_str(&input)
                            .map_err(|_| AgentFailure::InvalidModelOutput)?;
                        validate_schedule_proposal(view, &proposal)?;
                        action_proposals.push(ExpertFocusProposal {
                            starts_at_unix_ms: proposal.starts_at_unix_ms,
                            ends_at_unix_ms: proposal.ends_at_unix_ms,
                            view_handle: view.handle,
                        });
                    }
                    let call_id = Uuid::new_v4();
                    let output = crate::capability_execution::execute_recorded(
                        &invocation.usage,
                        crate::CapabilityExecution {
                            scope_id: invocation.invocation_id,
                            turn_id,
                            call_id,
                            capability_id: capability_id.clone(),
                            input: input.clone(),
                            state: crate::CapabilityExecutionState::Started,
                            result: None,
                            replay: response.replay_for(call_index)?,
                        },
                        invocation.deadline,
                        &invocation.cancellation,
                        invocation.budget.max_output_bytes,
                        Box::pin(async {
                            check_running(invocation)?;
                            validate_view(view, invocation, data_class)?;
                            schedule_capability_result(
                                &capability_id,
                                view,
                                &input,
                                invocation.current_time_unix_ms,
                                invocation.timezone_offset_seconds,
                            )
                        }),
                    )
                    .await??;
                    if let Some(provider_replay) = response.replay_for(call_index)? {
                        replay.push(crate::ModelReplay {
                            call_id,
                            replay: provider_replay,
                        });
                    }
                    call_index += 1;
                    messages.push(AgentMessage::Capability {
                        turn_id,
                        call_id,
                        capability_id,
                        input,
                        result: Ok(output),
                    });
                }
                ModelStep::Delegate { .. } => return Err(AgentFailure::CapabilityDenied),
            }
        }
    }
    Err(AgentFailure::BudgetExceeded)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CalendarRangeInput {
    range_start_unix_ms: Option<u64>,
    range_end_unix_ms: Option<u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CalendarSearchInput {
    query: String,
    range_start_unix_ms: Option<u64>,
    range_end_unix_ms: Option<u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FreeWindowInput {
    minimum_minutes: u16,
    range_start_unix_ms: Option<u64>,
    range_end_unix_ms: Option<u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScheduleProposalInput {
    starts_at_unix_ms: u64,
    ends_at_unix_ms: u64,
}

fn schedule_capabilities(data_class: DataClass) -> Vec<CapabilityDescriptor> {
    let range_schema = serde_json::json!({
        "type": "object",
        "properties": {
            "range_start_unix_ms": {"type": "integer", "minimum": 0},
            "range_end_unix_ms": {"type": "integer", "minimum": 1}
        },
        "additionalProperties": false
    });
    let mut capabilities = vec![
        CapabilityDescriptor {
            schema_version: AGENT_VERSION,
            id: "calendar.read".into(),
            version: "1.0.0".into(),
            read_only: true,
            output_data_class: data_class,
            input_schema: Some(range_schema.clone()),
        },
        CapabilityDescriptor {
            schema_version: AGENT_VERSION,
            id: "calendar.search".into(),
            version: "1.0.0".into(),
            read_only: true,
            output_data_class: data_class,
            input_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "minLength": 1, "maxLength": 128},
                    "range_start_unix_ms": {"type": "integer", "minimum": 0},
                    "range_end_unix_ms": {"type": "integer", "minimum": 1}
                },
                "required": ["query"],
                "additionalProperties": false
            })),
        },
    ];
    capabilities.push(CapabilityDescriptor {
        schema_version: AGENT_VERSION,
        id: "schedule.find_free_windows".into(),
        version: "1.0.0".into(),
        read_only: true,
        output_data_class: data_class,
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "minimum_minutes": {"type": "integer", "minimum": 1, "maximum": 240},
                "range_start_unix_ms": {"type": "integer", "minimum": 0},
                "range_end_unix_ms": {"type": "integer", "minimum": 1}
            },
            "required": ["minimum_minutes"],
            "additionalProperties": false
        })),
    });
    capabilities.push(CapabilityDescriptor {
        schema_version: AGENT_VERSION,
        id: "schedule.propose_window".into(),
        version: "1.0.0".into(),
        read_only: true,
        output_data_class: data_class,
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "starts_at_unix_ms": {"type": "integer", "minimum": 0},
                "ends_at_unix_ms": {"type": "integer", "minimum": 1}
            },
            "required": ["starts_at_unix_ms", "ends_at_unix_ms"],
            "additionalProperties": false
        })),
    });
    capabilities
}

fn schedule_capability_result(
    capability_id: &str,
    view: &ExpertTimelineView,
    input: &str,
    current_time_unix_ms: u64,
    timezone_offset_seconds: i32,
) -> Result<String, AgentFailure> {
    match capability_id {
        "calendar.read" => {
            let input: CalendarRangeInput =
                serde_json::from_str(input).map_err(|_| AgentFailure::InvalidModelOutput)?;
            calendar_view_model_output(
                &bounded_calendar_view(view, input.range_start_unix_ms, input.range_end_unix_ms)?,
                current_time_unix_ms,
                timezone_offset_seconds,
            )
        }
        "calendar.search" => {
            let input: CalendarSearchInput =
                serde_json::from_str(input).map_err(|_| AgentFailure::InvalidModelOutput)?;
            let query = input.query.trim().to_lowercase();
            if query.is_empty() || query.len() > 128 {
                return Err(AgentFailure::InvalidModelOutput);
            }
            let mut bounded =
                bounded_calendar_view(view, input.range_start_unix_ms, input.range_end_unix_ms)?;
            bounded
                .items
                .retain(|item| item.untrusted_title.to_lowercase().contains(query.as_str()));
            calendar_view_model_output(&bounded, current_time_unix_ms, timezone_offset_seconds)
        }
        "schedule.find_free_windows" => {
            let input: FreeWindowInput =
                serde_json::from_str(input).map_err(|_| AgentFailure::InvalidModelOutput)?;
            if input.range_start_unix_ms.is_none() && input.range_end_unix_ms.is_none() {
                return schedule_insights_model_output(
                    &analyze_schedule(view, input.minimum_minutes),
                    view,
                    current_time_unix_ms,
                    timezone_offset_seconds,
                );
            }
            let bounded =
                bounded_calendar_view(view, input.range_start_unix_ms, input.range_end_unix_ms)?;
            schedule_insights_model_output(
                &analyze_schedule(&bounded, input.minimum_minutes),
                &bounded,
                current_time_unix_ms,
                timezone_offset_seconds,
            )
        }
        "schedule.propose_window" => {
            let proposal: ScheduleProposalInput =
                serde_json::from_str(input).map_err(|_| AgentFailure::InvalidModelOutput)?;
            validate_schedule_proposal(view, &proposal)?;
            Ok(serde_json::json!({"accepted": true}).to_string())
        }
        _ => Err(AgentFailure::CapabilityDenied),
    }
}

#[derive(Clone, Copy)]
enum DateTimePrecision {
    Time,
    MonthDay,
    Full,
}

fn calendar_view_model_output(
    view: &ExpertTimelineView,
    current_time_unix_ms: u64,
    timezone_offset_seconds: i32,
) -> Result<String, AgentFailure> {
    let precision = datetime_precision(view, current_time_unix_ms, timezone_offset_seconds)?;
    let mut value = serde_json::to_value(view).map_err(|_| AgentFailure::InvalidInput)?;
    value["range_start_local"] =
        format_datetime(view.range_start_unix_ms, timezone_offset_seconds, precision)?.into();
    value["range_end_local"] =
        format_datetime(view.range_end_unix_ms, timezone_offset_seconds, precision)?.into();
    for (value, item) in value["items"]
        .as_array_mut()
        .ok_or(AgentFailure::InvalidInput)?
        .iter_mut()
        .zip(&view.items)
    {
        value["starts_at_local"] =
            format_datetime(item.starts_at_unix_ms, timezone_offset_seconds, precision)?.into();
        value["ends_at_local"] =
            format_datetime(item.ends_at_unix_ms, timezone_offset_seconds, precision)?.into();
    }
    serde_json::to_string(&value).map_err(|_| AgentFailure::InvalidInput)
}

fn schedule_insights_model_output(
    insights: &[ExpertInsight],
    view: &ExpertTimelineView,
    current_time_unix_ms: u64,
    timezone_offset_seconds: i32,
) -> Result<String, AgentFailure> {
    let precision = datetime_precision(view, current_time_unix_ms, timezone_offset_seconds)?;
    let mut value = serde_json::to_value(insights).map_err(|_| AgentFailure::InvalidInput)?;
    for insight in value
        .as_array_mut()
        .ok_or(AgentFailure::InvalidInput)?
        .iter_mut()
    {
        if let Some(starts_at) = insight
            .get("starts_at_unix_ms")
            .and_then(|value| value.as_u64())
        {
            insight["starts_at_local"] =
                format_datetime(starts_at, timezone_offset_seconds, precision)?.into();
        }
        if let Some(ends_at) = insight
            .get("ends_at_unix_ms")
            .and_then(|value| value.as_u64())
        {
            insight["ends_at_local"] =
                format_datetime(ends_at, timezone_offset_seconds, precision)?.into();
        }
    }
    serde_json::to_string(&value).map_err(|_| AgentFailure::InvalidInput)
}

fn datetime_precision(
    view: &ExpertTimelineView,
    current_time_unix_ms: u64,
    timezone_offset_seconds: i32,
) -> Result<DateTimePrecision, AgentFailure> {
    let current = local_datetime(current_time_unix_ms, timezone_offset_seconds)?;
    let start = local_datetime(view.range_start_unix_ms, timezone_offset_seconds)?;
    let end = local_datetime(
        view.range_end_unix_ms.saturating_sub(1),
        timezone_offset_seconds,
    )?;
    if start.date_naive() == current.date_naive() && end.date_naive() == current.date_naive() {
        Ok(DateTimePrecision::Time)
    } else if start.year() == end.year() && start.year() == current.year() {
        Ok(DateTimePrecision::MonthDay)
    } else {
        Ok(DateTimePrecision::Full)
    }
}

fn format_datetime(
    unix_ms: u64,
    timezone_offset_seconds: i32,
    precision: DateTimePrecision,
) -> Result<String, AgentFailure> {
    let time = local_datetime(unix_ms, timezone_offset_seconds)?;
    let include_seconds = time.second() != 0 || time.nanosecond() != 0;
    let format = match (precision, include_seconds) {
        (DateTimePrecision::Time, false) => "%H:%M",
        (DateTimePrecision::Time, true) => "%H:%M:%S",
        (DateTimePrecision::MonthDay, false) => "%m-%d %H:%M",
        (DateTimePrecision::MonthDay, true) => "%m-%d %H:%M:%S",
        (DateTimePrecision::Full, false) => "%Y-%m-%d %H:%M",
        (DateTimePrecision::Full, true) => "%Y-%m-%d %H:%M:%S",
    };
    Ok(time.format(format).to_string())
}

fn local_datetime(
    unix_ms: u64,
    timezone_offset_seconds: i32,
) -> Result<DateTime<FixedOffset>, AgentFailure> {
    let unix_ms = i64::try_from(unix_ms).map_err(|_| AgentFailure::InvalidInput)?;
    let offset =
        FixedOffset::east_opt(timezone_offset_seconds).ok_or(AgentFailure::InvalidInput)?;
    DateTime::<Utc>::from_timestamp_millis(unix_ms)
        .map(|time| time.with_timezone(&offset))
        .ok_or(AgentFailure::InvalidInput)
}

fn validate_schedule_proposal(
    view: &ExpertTimelineView,
    proposal: &ScheduleProposalInput,
) -> Result<(), AgentFailure> {
    if proposal.starts_at_unix_ms < view.range_start_unix_ms
        || proposal.ends_at_unix_ms > view.range_end_unix_ms
        || proposal.starts_at_unix_ms >= proposal.ends_at_unix_ms
        || view.items.iter().any(|item| {
            proposal.starts_at_unix_ms < item.ends_at_unix_ms
                && proposal.ends_at_unix_ms > item.starts_at_unix_ms
        })
    {
        return Err(AgentFailure::InvalidModelOutput);
    }
    Ok(())
}

fn bounded_calendar_view(
    view: &ExpertTimelineView,
    range_start_unix_ms: Option<u64>,
    range_end_unix_ms: Option<u64>,
) -> Result<ExpertTimelineView, AgentFailure> {
    let (starts_at, ends_at) = match (range_start_unix_ms, range_end_unix_ms) {
        (None, None) => return Ok(view.clone()),
        (Some(starts_at), Some(ends_at))
            if starts_at >= view.range_start_unix_ms
                && ends_at <= view.range_end_unix_ms
                && starts_at < ends_at
                && ends_at - starts_at <= 86_400_000 =>
        {
            (starts_at, ends_at)
        }
        _ => return Err(AgentFailure::InvalidModelOutput),
    };
    let mut bounded = view.clone();
    bounded.range_start_unix_ms = starts_at;
    bounded.range_end_unix_ms = ends_at;
    bounded.items = view
        .items
        .iter()
        .filter_map(|item| {
            let starts_at_unix_ms = item.starts_at_unix_ms.max(starts_at);
            let ends_at_unix_ms = item.ends_at_unix_ms.min(ends_at);
            (starts_at_unix_ms < ends_at_unix_ms).then(|| TimelineViewItem {
                evidence_handle: item.evidence_handle,
                untrusted_title: item.untrusted_title.clone(),
                starts_at_unix_ms,
                ends_at_unix_ms,
            })
        })
        .collect();
    Ok(bounded)
}

async fn generate_schedule_step<Model: ModelRunner + Sync>(
    model: &Model,
    policy: &InferencePolicyDecision,
    invocation: &ExpertInvocation,
    messages: &[AgentMessage],
    replay: &[crate::ModelReplay],
    capabilities: Vec<CapabilityDescriptor>,
    used_tokens: u64,
    used_cost: u64,
) -> Result<crate::ModelResponse, AgentFailure> {
    check_running(invocation)?;
    let remaining_tokens = invocation
        .budget
        .max_model_tokens
        .checked_sub(used_tokens)
        .ok_or(AgentFailure::BudgetExceeded)?;
    let remaining_cost_micros = invocation
        .budget
        .max_model_cost_micros
        .checked_sub(used_cost)
        .ok_or(AgentFailure::BudgetExceeded)?;
    let response = crate::generate_with_recovery(
        model,
        ModelRequest {
            usage: invocation.usage.clone(),
            replay: replay.to_vec(),
            schema_version: AGENT_VERSION,
            prompt: schedule_expert_prompt(),
            person_id: invocation.person_id,
            session_id: invocation.invocation_id,
            turn_id: messages[0].turn_id(),
            policy: policy.clone(),
            context: AgentContext {
                projection_version: policy.projection_version,
                persona: None,
                evidence: vec![],
            },
            messages: messages.to_vec(),
            capabilities,
            active_agents: vec![],
            remaining_tokens,
            remaining_cost_micros,
            max_output_bytes: invocation.budget.max_output_bytes.min(4096),
            deadline: invocation.deadline,
            cancellation: invocation.cancellation.clone(),
        },
    )
    .await?;
    if response.schema_version != AGENT_VERSION
        || response.used_tokens > remaining_tokens
        || response.cost_micros > remaining_cost_micros
    {
        return Err(AgentFailure::BudgetExceeded);
    }
    Ok(response)
}

fn validate_view(
    view: &ExpertTimelineView,
    invocation: &ExpertInvocation,
    data_class: DataClass,
) -> Result<(), AgentFailure> {
    if view.schema_version != AGENT_VERSION {
        return Err(AgentFailure::UnsupportedVersion);
    }
    if view.person_id != invocation.person_id
        || view.handle != invocation.granted_view_handles[0]
        || view.data_class != data_class
    {
        return Err(AgentFailure::CapabilityDenied);
    }
    if view.expires_at_unix_ms <= now_unix_ms()? {
        return Err(AgentFailure::StaleContext);
    }
    if view.items.len() > 32
        || serde_json::to_vec(view)
            .map_err(|_| AgentFailure::InvalidInput)?
            .len()
            > invocation.budget.max_view_bytes.min(16384)
    {
        return Err(AgentFailure::BudgetExceeded);
    }
    if view.source_handle.trim().is_empty()
        || view.source_handle.len() > 128
        || view.range_start_unix_ms >= view.range_end_unix_ms
        || view.range_end_unix_ms - view.range_start_unix_ms > 14 * 86_400_000
        || view.items.iter().enumerate().any(|(index, item)| {
            item.untrusted_title.len() > 256
                || item.starts_at_unix_ms < view.range_start_unix_ms
                || item.ends_at_unix_ms > view.range_end_unix_ms
                || item.starts_at_unix_ms >= item.ends_at_unix_ms
                || view.items[..index]
                    .iter()
                    .any(|other| other.evidence_handle == item.evidence_handle)
        })
    {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(())
}

fn analyze_schedule(view: &ExpertTimelineView, minimum_minutes: u16) -> Vec<ExpertInsight> {
    let mut items: Vec<_> = view.items.iter().collect();
    items.sort_by_key(|item| (item.starts_at_unix_ms, item.ends_at_unix_ms));
    let mut insights = vec![];
    if let Some(item) = items.first() {
        insights.push(ExpertInsight::Commitment {
            evidence_handle: item.evidence_handle,
            untrusted_title: item.untrusted_title.clone(),
            starts_at_unix_ms: item.starts_at_unix_ms,
            ends_at_unix_ms: item.ends_at_unix_ms,
        });
    }
    let duration = u64::from(minimum_minutes) * 60_000;
    let mut cursor = view.range_start_unix_ms;
    let mut window = None;
    for item in &items {
        if item.starts_at_unix_ms.saturating_sub(cursor) >= duration {
            window = Some((cursor, cursor + duration));
            break;
        }
        cursor = cursor.max(item.ends_at_unix_ms);
    }
    if window.is_none() && view.range_end_unix_ms.saturating_sub(cursor) >= duration {
        window = Some((cursor, cursor + duration));
    }
    insights.push(match window {
        Some((starts_at_unix_ms, ends_at_unix_ms)) => ExpertInsight::FocusWindow {
            starts_at_unix_ms,
            ends_at_unix_ms,
        },
        None => ExpertInsight::NoFocusWindow,
    });
    insights
}

fn commitment_insights(view: &ExpertTimelineView, limit: usize) -> Vec<ExpertInsight> {
    let mut items: Vec<_> = view.items.iter().collect();
    items.sort_by_key(|item| (item.starts_at_unix_ms, item.ends_at_unix_ms));
    items
        .into_iter()
        .take(limit)
        .map(|item| ExpertInsight::Commitment {
            evidence_handle: item.evidence_handle,
            untrusted_title: item.untrusted_title.clone(),
            starts_at_unix_ms: item.starts_at_unix_ms,
            ends_at_unix_ms: item.ends_at_unix_ms,
        })
        .collect()
}

fn now_unix_ms() -> Result<u64, AgentFailure> {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .ok_or(AgentFailure::StaleContext)
}

fn check_running(invocation: &ExpertInvocation) -> Result<(), AgentFailure> {
    if invocation.cancellation.is_cancelled() {
        Err(AgentFailure::Cancelled)
    } else if invocation.deadline <= Instant::now() {
        Err(AgentFailure::DeadlineExceeded)
    } else {
        Ok(())
    }
}

struct ViewCancellation(Cancellation);

impl Drop for ViewCancellation {
    fn drop(&mut self) {
        self.0.cancel();
    }
}
