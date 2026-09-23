//! Schedule Expert host: timeline views, reasoning and focus analysis.

use std::{future::Future, time::SystemTime};

use chrono::{DateTime, Datelike, FixedOffset, Timelike, Utc};
use serde::Deserialize;
use tokio::time::Instant;
use uuid::Uuid;

use crate::BuiltinExpertKind;
use crate::prompts::schedule_expert_prompt;
use floe_agent_contract::AGENT_VERSION;
use floe_agent_contract::InferencePolicyDecision;
use super::ScheduleExecutionIntent;
use floe_agent_contract::{
    AgentFailure, CapabilityDescriptor, CapabilityExecution, CapabilityExecutionState, DataClass,
    ExpertAssignments, ExpertFocusProposal, ExpertInput, ExpertInsight, ExpertInvocation,
    ExpertReasoner, ExpertReasoningStep, ExpertResult, ExpertStep, ExpertStepOutcome,
    ExpertTranscriptEntry, ModelReplay, ViewCancellation, check_running,
};
use floe_agent_runtime::execute_recorded;
use floe_execution::Cancellation;

pub use floe_agent_contract::{
    ExpertTimelineView, MAX_TIMELINE_VIEW_BYTES, MAX_TIMELINE_VIEW_DAYS, MAX_TIMELINE_VIEW_ITEMS,
    TimelineViewItem, TimelineViewRead,
};

const MAX_TIMELINE_VIEW_DURATION_MS: u64 = (MAX_TIMELINE_VIEW_DAYS as u64 + 1) * 86_400_000;

pub trait ExpertViews: Sync {
    fn timeline(
        &self,
        request: TimelineViewRead,
    ) -> impl Future<Output = Result<ExpertTimelineView, AgentFailure>> + Send;
}

pub struct ExpertHost<'host, Assignments, Views> {
    /// The registry this invocation is admitted against and settled with.
    pub assignments: &'host Assignments,
    pub views: &'host Views,
}

impl<Assignments: ExpertAssignments, Views: ExpertViews> ExpertHost<'_, Assignments, Views> {
    pub async fn invoke(&self, invocation: ExpertInvocation) -> Result<ExpertResult, AgentFailure> {
        self.invoke_inner::<NoExpertModel>(invocation, ExpertReasoning::Deterministic)
            .await
    }

    pub async fn invoke_with_model<Model: ExpertReasoner>(
        &self,
        invocation: ExpertInvocation,
        model: &Model,
        policy: &InferencePolicyDecision,
        intent: ScheduleExecutionIntent,
    ) -> Result<ExpertResult, AgentFailure> {
        self.invoke_inner(
            invocation,
            ExpertReasoning::Lightweight {
                model,
                policy,
                intent,
            },
        )
        .await
    }

    async fn invoke_inner<Model: ExpertReasoner>(
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
        let admitted = self.assignments.admit(&invocation, focus_minutes)?;
        let minimum = admitted.focus_minimum_minutes;
        let (view, summary, model_calls, view_calls, action_proposals) =
            match (admitted.builtin_expert.as_deref(), reasoning) {
                (Some(expert), ExpertReasoning::Lightweight {
                    model,
                    policy,
                    intent,
                }) if expert == BuiltinExpertKind::Schedule.package_id() => {
                    let (summary, model_calls, view, view_calls) = run_schedule_reasoning(
                        model,
                        policy,
                        intent,
                        &invocation,
                        self.views,
                        admitted.data_class,
                    )
                    .await?;
                    let proposals = if matches!(&invocation.input, ExpertInput::ProposeFocus { .. })
                    {
                        analyze_schedule(&view, minimum.ok_or(AgentFailure::InvalidInput)?)
                            .into_iter()
                            .filter_map(|insight| match insight {
                                ExpertInsight::FocusWindow {
                                    starts_at_unix_ms,
                                    ends_at_unix_ms,
                                } => Some(ExpertFocusProposal {
                                    starts_at_unix_ms,
                                    ends_at_unix_ms,
                                    view_handle: view.handle,
                                }),
                                _ => None,
                            })
                            .collect()
                    } else {
                        vec![]
                    };
                    (view, Some(summary), model_calls, view_calls, proposals)
                }
                _ => {
                    let view = self
                        .read_timeline(&invocation, admitted.data_class, None, None, None)
                        .await?;
                    let proposals = if matches!(&invocation.input, ExpertInput::ProposeFocus { .. })
                    {
                        analyze_schedule(&view, minimum.ok_or(AgentFailure::InvalidInput)?)
                            .into_iter()
                            .filter_map(|insight| match insight {
                                ExpertInsight::FocusWindow {
                                    starts_at_unix_ms,
                                    ends_at_unix_ms,
                                } => Some(ExpertFocusProposal {
                                    starts_at_unix_ms,
                                    ends_at_unix_ms,
                                    view_handle: view.handle,
                                }),
                                _ => None,
                            })
                            .collect()
                    } else {
                        vec![]
                    };
                    (view, None, 0, 1, proposals)
                }
            };
        let mut insights = match minimum {
            Some(minimum) => analyze_schedule(&view, minimum),
            None => commitment_insights(&view, invocation.budget.max_insights.min(8)),
        };
        for proposal in &action_proposals {
            let insight = ExpertInsight::FocusWindow {
                starts_at_unix_ms: proposal.starts_at_unix_ms,
                ends_at_unix_ms: proposal.ends_at_unix_ms,
            };
            if !insights.contains(&insight) {
                insights.push(insight);
            }
        }
        if insights.len() > invocation.budget.max_insights.min(8) {
            return Err(AgentFailure::BudgetExceeded);
        }
        let mut result = ExpertResult {
            schema_version: AGENT_VERSION,
            invocation_id: invocation.invocation_id,
            instance_id: invocation.instance_id,
            person_id: invocation.person_id,
            assignment_id: invocation.assignment_id,
            package: admitted.package.clone(),
            view_handle: view.handle,
            source_handle: view.source_handle,
            data_class: view.data_class,
            expires_at_unix_ms: view.expires_at_unix_ms,
            insights,
            action_proposals,
            summary,
            model_calls,
            state_revision: 0,
            view_calls,
        };
        result.state_revision = self.assignments.settle(&invocation, &admitted, &result)?;
        Ok(result)
    }

    async fn read_timeline(
        &self,
        invocation: &ExpertInvocation,
        data_class: DataClass,
        range_start_unix_ms: Option<u64>,
        range_end_unix_ms: Option<u64>,
        cursor: Option<String>,
    ) -> Result<ExpertTimelineView, AgentFailure> {
        let cancellation = Cancellation::default();
        let _guard = ViewCancellation(cancellation.clone());
        let read = TimelineViewRead {
            person_id: invocation.person_id,
            handle: invocation.granted_view_handles[0],
            range_start_unix_ms,
            range_end_unix_ms,
            cursor,
            max_items: MAX_TIMELINE_VIEW_ITEMS,
            max_bytes: invocation
                .budget
                .max_view_bytes
                .min(MAX_TIMELINE_VIEW_BYTES),
            deadline: invocation.deadline,
            cancellation,
        };
        let input = serde_json::json!({
            "handle": read.handle,
            "range_start_unix_ms": read.range_start_unix_ms,
            "range_end_unix_ms": read.range_end_unix_ms,
            "cursor": read.cursor,
        })
        .to_string();
        let view_output = execute_recorded(
            invocation.capabilities.as_ref(),
            CapabilityExecution {
                scope_id: invocation.invocation_id,
                turn_id: invocation.invocation_id,
                call_id: Uuid::new_v4(),
                capability_id: "view.timeline".into(),
                input,
                state: CapabilityExecutionState::Started,
                result: None,
                replay: None,
            },
            invocation.deadline,
            &invocation.cancellation,
            invocation
                .budget
                .max_view_bytes
                .min(MAX_TIMELINE_VIEW_BYTES),
            Box::pin(async {
                let view = self.views.timeline(read).await?;
                validate_view(&view, invocation, data_class)?;
                serde_json::to_string(&view).map_err(|_| AgentFailure::InvalidInput)
            }),
        )
        .await??;
        let view = serde_json::from_str(&view_output).map_err(|_| AgentFailure::InvalidInput)?;
        check_running(invocation)?;
        validate_view(&view, invocation, data_class)?;
        Ok(view)
    }
}

enum ExpertReasoning<'model, Model> {
    Deterministic,
    Lightweight {
        model: &'model Model,
        policy: &'model InferencePolicyDecision,
        intent: ScheduleExecutionIntent,
    },
}

/// The model a deterministic invocation runs on: there isn't one.
struct NoExpertModel;

impl floe_agent_contract::ExpertModel for NoExpertModel {
    fn answer<'a>(
        &'a self,
        _: floe_agent_contract::ExpertModelCall,
    ) -> floe_agent_contract::BoxFuture<
        'a,
        Result<floe_agent_contract::ExpertModelAnswer, AgentFailure>,
    > {
        Box::pin(async { Err(AgentFailure::ModelUnavailable) })
    }
}

impl ExpertReasoner for NoExpertModel {
    fn step<'a>(
        &'a self,
        _: ExpertReasoningStep,
    ) -> floe_agent_contract::BoxFuture<'a, Result<ExpertStepOutcome, AgentFailure>> {
        Box::pin(async { Err(AgentFailure::ModelUnavailable) })
    }
}

async fn run_schedule_reasoning<Model: ExpertReasoner, Views: ExpertViews>(
    model: &Model,
    policy: &InferencePolicyDecision,
    intent: ScheduleExecutionIntent,
    invocation: &ExpertInvocation,
    views: &Views,
    data_class: DataClass,
) -> Result<(String, u32, ExpertTimelineView, u32), AgentFailure> {
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
        "suggested_query_range": {
            "starts_at_unix_ms": invocation.suggested_range_start_unix_ms,
            "ends_at_unix_ms": invocation.suggested_range_end_unix_ms
        }
    })
    .to_string();
    let mut transcript = vec![ExpertTranscriptEntry::Task { text: task }];
    let mut replay = vec![];
    let mut used_tokens = 0;
    let mut used_cost = 0;
    let mut expert_policy = policy.clone();
    expert_policy.purpose = "schedule-summary".into();
    expert_policy.performance_class = "fast".into();
    let mut tool_calls = 0;
    let mut view_calls = 0;
    let mut active_view: Option<ExpertTimelineView> = None;
    for model_call in 1..=invocation.budget.max_model_calls {
        let available_capabilities = if tool_calls < invocation.budget.max_tool_calls {
            capabilities.clone()
        } else {
            vec![]
        };
        let response = generate_schedule_step(
            model,
            &expert_policy,
            intent,
            invocation,
            &transcript,
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
        for step in response.steps.clone() {
            match step {
                ExpertStep::Preamble { text } => {
                    transcript.push(ExpertTranscriptEntry::Preamble { text });
                }
                ExpertStep::Answer { text } => {
                    let summary = text.trim();
                    if summary.is_empty() || summary.len() > 2048 {
                        return Err(AgentFailure::InvalidModelOutput);
                    }
                    let view = active_view.ok_or(AgentFailure::InvalidModelOutput)?;
                    if !view.coverage_complete {
                        return Err(AgentFailure::CapabilityUnavailable);
                    }
                    return Ok((summary.into(), model_call, view, view_calls));
                }
                ExpertStep::Call {
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
                    let requested_range = schedule_read_range(&capability_id, &input)?;
                    if let Some((range_start_unix_ms, range_end_unix_ms)) = requested_range {
                        if view_calls >= invocation.budget.max_view_calls {
                            return Err(AgentFailure::BudgetExceeded);
                        }
                        let cancellation = Cancellation::default();
                        let _guard = ViewCancellation(cancellation.clone());
                        let read = TimelineViewRead {
                            person_id: invocation.person_id,
                            handle: invocation.granted_view_handles[0],
                            range_start_unix_ms: Some(range_start_unix_ms),
                            range_end_unix_ms: Some(range_end_unix_ms),
                            cursor: None,
                            max_items: MAX_TIMELINE_VIEW_ITEMS,
                            max_bytes: invocation
                                .budget
                                .max_view_bytes
                                .min(MAX_TIMELINE_VIEW_BYTES),
                            deadline: invocation.deadline,
                            cancellation,
                        };
                        let view_input = serde_json::json!({
                            "handle": read.handle,
                            "range_start_unix_ms": range_start_unix_ms,
                            "range_end_unix_ms": range_end_unix_ms,
                        })
                        .to_string();
                        let encoded = execute_recorded(
                            invocation.capabilities.as_ref(),
                            CapabilityExecution {
                                scope_id: invocation.invocation_id,
                                turn_id,
                                call_id: Uuid::new_v4(),
                                capability_id: "view.timeline".into(),
                                input: view_input,
                                state: CapabilityExecutionState::Started,
                                result: None,
                                replay: None,
                            },
                            invocation.deadline,
                            &invocation.cancellation,
                            invocation
                                .budget
                                .max_view_bytes
                                .min(MAX_TIMELINE_VIEW_BYTES),
                            Box::pin(async {
                                let observed = views.timeline(read).await?;
                                validate_view(&observed, invocation, data_class)?;
                                serde_json::to_string(&observed)
                                    .map_err(|_| AgentFailure::InvalidInput)
                            }),
                        )
                        .await??;
                        let observed: ExpertTimelineView = serde_json::from_str(&encoded)
                            .map_err(|_| AgentFailure::InvalidInput)?;
                        validate_view(&observed, invocation, data_class)?;
                        active_view = Some(observed);
                        view_calls += 1;
                    }
                    let call_id = Uuid::new_v4();
                    let output = execute_recorded(
                        invocation.capabilities.as_ref(),
                        CapabilityExecution {
                            scope_id: invocation.invocation_id,
                            turn_id,
                            call_id,
                            capability_id: capability_id.clone(),
                            input: input.clone(),
                            state: CapabilityExecutionState::Started,
                            result: None,
                            replay: response.replay_for(call_index)?,
                        },
                        invocation.deadline,
                        &invocation.cancellation,
                        invocation.budget.max_output_bytes,
                        Box::pin(async {
                            check_running(invocation)?;
                            let view = active_view
                                .as_ref()
                                .ok_or(AgentFailure::InvalidModelOutput)?;
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
                        replay.push(ModelReplay {
                            call_id,
                            replay: provider_replay,
                        });
                    }
                    call_index += 1;
                    transcript.push(ExpertTranscriptEntry::Capability {
                        call_id,
                        capability_id,
                        input,
                        observation: floe_agent_contract::ExpertCapabilityObservation::Success {
                            result: output,
                        },
                    });
                }
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

fn schedule_read_range(
    capability_id: &str,
    input: &str,
) -> Result<Option<(u64, u64)>, AgentFailure> {
    let range = match capability_id {
        "calendar.read" => {
            let input: CalendarRangeInput =
                serde_json::from_str(input).map_err(|_| AgentFailure::InvalidModelOutput)?;
            (input.range_start_unix_ms, input.range_end_unix_ms)
        }
        "calendar.search" => {
            let input: CalendarSearchInput =
                serde_json::from_str(input).map_err(|_| AgentFailure::InvalidModelOutput)?;
            (input.range_start_unix_ms, input.range_end_unix_ms)
        }
        "schedule.find_free_windows" => {
            let input: FreeWindowInput =
                serde_json::from_str(input).map_err(|_| AgentFailure::InvalidModelOutput)?;
            (input.range_start_unix_ms, input.range_end_unix_ms)
        }
        _ => return Err(AgentFailure::CapabilityDenied),
    };
    match range {
        (Some(start), Some(end)) if start < end && end - start <= MAX_TIMELINE_VIEW_DURATION_MS => {
            Ok(Some((start, end)))
        }
        _ => Err(AgentFailure::InvalidModelOutput),
    }
}

fn schedule_capabilities(data_class: DataClass) -> Vec<CapabilityDescriptor> {
    let range_schema = serde_json::json!({
        "type": "object",
        "properties": {
            "range_start_unix_ms": {"type": "integer", "minimum": 0},
            "range_end_unix_ms": {"type": "integer", "minimum": 1}
        },
        "required": ["range_start_unix_ms", "range_end_unix_ms"],
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
                "required": ["query", "range_start_unix_ms", "range_end_unix_ms"],
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
            "required": ["minimum_minutes", "range_start_unix_ms", "range_end_unix_ms"],
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
                && ends_at - starts_at <= MAX_TIMELINE_VIEW_DURATION_MS =>
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

#[allow(clippy::too_many_arguments)]
async fn generate_schedule_step<Model: ExpertReasoner>(
    model: &Model,
    policy: &InferencePolicyDecision,
    intent: ScheduleExecutionIntent,
    invocation: &ExpertInvocation,
    transcript: &[ExpertTranscriptEntry],
    replay: &[ModelReplay],
    capabilities: Vec<CapabilityDescriptor>,
    used_tokens: u64,
    used_cost: u64,
) -> Result<ExpertStepOutcome, AgentFailure> {
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
    let response = model
        .step(ExpertReasoningStep {
            person_id: invocation.person_id,
            invocation_id: invocation.invocation_id,
            prompt: schedule_expert_prompt(),
            policy: policy.clone(),
            context: invocation.context.clone(),
            requirement: intent.requirement(),
            transcript: transcript.to_vec(),
            capabilities,
            replay: replay.to_vec(),
            remaining_tokens,
            remaining_cost_micros,
            max_output_bytes: invocation.budget.max_output_bytes.min(4096),
            deadline: invocation.deadline,
            cancellation: invocation.cancellation.clone(),
        })
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
    if view.items.len() > MAX_TIMELINE_VIEW_ITEMS
        || serde_json::to_vec(view)
            .map_err(|_| AgentFailure::InvalidInput)?
            .len()
            > invocation
                .budget
                .max_view_bytes
                .min(MAX_TIMELINE_VIEW_BYTES)
    {
        return Err(AgentFailure::BudgetExceeded);
    }
    if view.source_handle.trim().is_empty()
        || view.source_handle.len() > 128
        || view.range_start_unix_ms >= view.range_end_unix_ms
        || view.range_end_unix_ms - view.range_start_unix_ms > MAX_TIMELINE_VIEW_DURATION_MS
        || view.coverage_complete == view.next_cursor.is_some()
        || view
            .next_cursor
            .as_ref()
            .is_some_and(|cursor| cursor.len() > 256)
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
