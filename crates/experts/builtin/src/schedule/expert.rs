use std::collections::{BTreeMap, HashSet};
use chrono::{Local, TimeZone, Utc};
use floe_agent_contract::{AgentFailure, Artifact, DependencyCoverage};
use floe_context_contract::{CalendarContextView, CalendarViewQuery, MAX_CALENDAR_CONTEXT_ITEMS,
    MAX_CONTEXT_EVIDENCE_BYTES, validate_calendar_context_view_for_query};
use floe_experts::{ExpertFinalOutput, ExpertProgram, ExpertProgramRequest, ExpertProgramSpec,
    ExpertSourceObservation, ExpertToolObservation};
use uuid::Uuid;

use super::{ScheduleAssessment, ScheduleInsight};

#[derive(Clone, Copy, Debug, Default)]
pub struct ScheduleProgram;
const CALENDAR: &str = "floe.source.calendar";
const MAX_PAGES: usize = 8;

fn request_plan(request: &ExpertProgramRequest) -> Result<super::ScheduleRequestPlan, AgentFailure> {
    let now = Utc.timestamp_millis_opt(request.started_at_unix_ms).single().ok_or(AgentFailure::InvalidInput)?;
    super::plan_request(&request.request.message, now.with_timezone(&Local), now)
}

impl ExpertProgram for ScheduleProgram {
    fn specification(&self, request: &ExpertProgramRequest) -> Result<ExpertProgramSpec, AgentFailure> {
        let plan = request_plan(request)?;
        let mut prompt = crate::prompts::schedule_expert_prompt();
        let role = prompt.components.iter_mut().find(|part|
            part.kind == floe_agent_contract::prompts::PromptComponentKind::Role)
            .ok_or(AgentFailure::InvalidInput)?;
        role.content = role.content.replace(
            "The supplied Calendar evidence already covers the requested bounded range. Use only that evidence; do not infer events outside its coverage.",
            "Read the declared Calendar tool for its pinned requested range before answering. Follow returned next_cursor values until coverage_complete is true, for at most eight pages. Use only that admitted evidence and never infer events outside its coverage.");
        let mut spec = crate::program_support::specification(request, prompt,
            "One concise natural-language scheduling summary, at most 2048 bytes, with no model-authored artifacts.")?;
        let tool = spec.tools.iter_mut().find(|tool| tool.requirement_key == CALENDAR)
            .ok_or(AgentFailure::CapabilityDenied)?;
        let mut schema: serde_json::Value = serde_json::from_str(&tool.input_schema)
            .map_err(|_| AgentFailure::InvalidInput)?;
        schema["properties"]["range_start_unix_ms"] = serde_json::json!({"const":plan.starts_at.timestamp_millis()});
        schema["properties"]["range_end_unix_ms"] = serde_json::json!({"const":plan.ends_at.timestamp_millis()});
        tool.input_schema = serde_json::to_string(&schema).map_err(|_| AgentFailure::InvalidInput)?;
        Ok(spec)
    }

    fn finalize(&self, request: &ExpertProgramRequest, observations: &[ExpertToolObservation],
        text: &str, artifacts: &[Artifact]) -> Result<ExpertFinalOutput, AgentFailure>
    {
        if !artifacts.is_empty() || text.trim().is_empty() || text.len() > 2048 {
            return Err(AgentFailure::InvalidModelOutput);
        }
        if crate::program_support::was_unavailable(observations, CALENDAR) {
            return crate::program_support::unavailable(request, observations, CALENDAR,
                crate::BuiltinExpertKind::Schedule.result_artifact_name(), super::RESULT_MEDIA_TYPE,
                "Calendar is temporarily unavailable, so there is no scheduling assessment.");
        }
        let plan = request_plan(request)?;
        let views = captured_calendars(request, observations, &plan)?;
        let first = views.first().ok_or(AgentFailure::InvalidModelOutput)?;
        let mut items = views.iter().flat_map(|view| view.items.iter()).collect::<Vec<_>>();
        items.sort_by_key(|item| (item.starts_at_unix_ms, item.ends_at_unix_ms));
        let mut insights = Vec::new();
        for item in items.iter().take(if plan.propose_focus { 7 } else { 8 }) {
            insights.push(ScheduleInsight::Commitment {
                evidence_handle: Uuid::new_v5(&request.request.invocation_key.as_uuid(), item.evidence_handle.as_bytes()),
                untrusted_title: item.untrusted_title.clone(),
                starts_at_unix_ms: u64::try_from(item.starts_at_unix_ms).map_err(|_| AgentFailure::InvalidInput)?,
                ends_at_unix_ms: u64::try_from(item.ends_at_unix_ms).map_err(|_| AgentFailure::InvalidInput)?,
            });
        }
        let mut proposal = None;
        if plan.propose_focus {
            if views.len() != 1 { return Err(AgentFailure::CapabilityUnavailable); }
            let duration = 60 * 60 * 1000;
            let mut cursor = first.range_start_unix_ms.max(request.now_unix_ms.saturating_add(60_000));
            let mut window = None;
            for item in &items {
                if item.starts_at_unix_ms.saturating_sub(cursor) >= duration {
                    window = Some((cursor, cursor + duration)); break;
                }
                cursor = cursor.max(item.ends_at_unix_ms);
            }
            if window.is_none() && first.range_end_unix_ms.saturating_sub(cursor) >= duration {
                window = Some((cursor, cursor + duration));
            }
            if let Some((start, end)) = window {
                let starts_at_unix_ms = u64::try_from(start).map_err(|_| AgentFailure::InvalidInput)?;
                let ends_at_unix_ms = u64::try_from(end).map_err(|_| AgentFailure::InvalidInput)?;
                insights.push(ScheduleInsight::FocusWindow { starts_at_unix_ms, ends_at_unix_ms });
                proposal = Some(floe_actions::ExpertCalendarProposalDraft { starts_at_unix_ms, ends_at_unix_ms });
            } else { insights.push(ScheduleInsight::NoFocusWindow); }
        }
        let assessment = ScheduleAssessment { insights };
        assessment.validate()?;
        let mut output = crate::program_support::result(request, observations,
            crate::BuiltinExpertKind::Schedule.result_artifact_name(), super::RESULT_MEDIA_TYPE,
            text.trim().to_owned(), &assessment)?;
        let (next_state, settlement) = floe_experts::prepare_expert_completion(request, &output.payload.text)?;
        if let Some(draft) = proposal {
            let captured_dependencies = match &request.coverage {
                DependencyCoverage::Dependent { dependencies } => dependencies.clone(),
                _ => return Err(AgentFailure::PolicyDenied),
            };
            let admitted_selections = request.selection.requirements.iter()
                .filter(|requirement| requirement.capability == "calendar.timeline")
                .flat_map(|requirement| requirement.selected.iter().cloned()).collect();
            output.payload.artifacts.push(floe_actions::seal_expert_calendar_proposal(floe_actions::ExpertProposalContext {
                person_id: request.actor.person_id, instance_id: request.admission.registry_instance_id,
                assignment_id: request.admission.assignment_id, package: request.admission.package.clone(),
                task_id: request.request.task_id, invocation_id: request.request.invocation_key.as_uuid(),
                next_state_revision: next_state.revision, data_class: request.data_class,
                captured_dependencies, admitted_selections, now_unix_ms: request.now_unix_ms,
            }, draft)?);
        }
        output.settlement = Some(settlement);
        Ok(output)
    }
}

fn captured_calendars(request: &ExpertProgramRequest, observations: &[ExpertToolObservation],
    plan: &super::ScheduleRequestPlan) -> Result<Vec<CalendarContextView>, AgentFailure>
{
    let reads = observations.iter().filter(|observation| observation.requirement_key == CALENDAR).collect::<Vec<_>>();
    if reads.is_empty() || reads.len() > MAX_PAGES { return Err(AgentFailure::InvalidModelOutput); }
    let mut pages = BTreeMap::<String, CalendarContextView>::new();
    let mut cursor = None;
    let mut seen_cursors = HashSet::new();
    let mut total_items = 0usize;
    let mut total_bytes = 0usize;
    let mut completed = false;
    for observation in reads {
        if completed { return Err(AgentFailure::InvalidModelOutput); }
        let query: CalendarViewQuery = serde_json::from_str(&observation.call.input).map_err(|_| AgentFailure::InvalidModelOutput)?;
        query.validate()?;
        if query.range_start_unix_ms() != plan.starts_at.timestamp_millis()
            || query.range_end_unix_ms() != plan.ends_at.timestamp_millis()
            || query.cursor() != cursor.as_deref()
        { return Err(AgentFailure::InvalidModelOutput); }
        let ExpertSourceObservation::Ready { payload, .. } = &observation.outcome
            else { return Err(AgentFailure::InvalidModelOutput); };
        let views: Vec<CalendarContextView> = serde_json::from_value(payload.clone()).map_err(|_| AgentFailure::InvalidModelOutput)?;
        if views.is_empty() { return Err(AgentFailure::InvalidModelOutput); }
        let previous_sources = pages.keys().cloned().collect::<HashSet<_>>();
        let mut page_sources = HashSet::new();
        let mut next_cursor = None;
        for view in views {
            validate_calendar_context_view_for_query(&view, &query, request.now_unix_ms)?;
            if !page_sources.insert(view.source_handle.clone()) { return Err(AgentFailure::InvalidModelOutput); }
            total_items = total_items.checked_add(view.items.len()).ok_or(AgentFailure::BudgetExceeded)?;
            total_bytes = total_bytes.checked_add(serde_json::to_vec(&view).map_err(|_| AgentFailure::InvalidInput)?.len())
                .ok_or(AgentFailure::BudgetExceeded)?;
            if total_items > 4 * MAX_CALENDAR_CONTEXT_ITEMS || total_bytes > MAX_CONTEXT_EVIDENCE_BYTES {
                return Err(AgentFailure::BudgetExceeded);
            }
            if let Some(value) = &view.next_cursor {
                if next_cursor.as_ref().is_some_and(|expected| expected != value) { return Err(AgentFailure::StaleContext); }
                next_cursor = Some(value.clone());
            }
            match pages.get_mut(&view.source_handle) {
                Some(existing) => {
                    if existing.range_start_unix_ms != view.range_start_unix_ms || existing.range_end_unix_ms != view.range_end_unix_ms
                        || view.items.iter().any(|item| existing.items.iter().any(|previous| previous.evidence_handle == item.evidence_handle))
                    { return Err(AgentFailure::StaleContext); }
                    existing.expires_at_unix_ms = existing.expires_at_unix_ms.min(view.expires_at_unix_ms);
                    existing.items.extend(view.items);
                    existing.coverage_complete = view.coverage_complete;
                    existing.next_cursor = view.next_cursor;
                }
                None => { pages.insert(view.source_handle.clone(), view); }
            }
        }
        if !previous_sources.is_empty() && page_sources != previous_sources { return Err(AgentFailure::StaleContext); }
        if let Some(next) = next_cursor {
            if pages.values().any(|view| view.coverage_complete) || !seen_cursors.insert(next.clone()) {
                return Err(AgentFailure::StaleContext);
            }
            cursor = Some(next);
        } else {
            if pages.values().any(|view| !view.coverage_complete) { return Err(AgentFailure::StaleContext); }
            completed = true;
        }
    }
    if !completed { return Err(AgentFailure::InvalidModelOutput); }
    Ok(pages.into_values().collect())
}
