use std::collections::{BTreeMap, HashSet};

use chrono::{Local, TimeZone, Utc};
use floe_agent_contract::{AgentFailure, Artifact};
use floe_context_contract::{
    CalendarContextView, CalendarViewQuery, MAX_CALENDAR_CONTEXT_ITEMS, MAX_CONTEXT_EVIDENCE_BYTES,
    SourceUnavailable, validate_calendar_context_view_for_query,
};
use serde::Serialize;

use crate::shared::ExpertJudgment;
use crate::{
    BlockedExpertStatus, BuiltinExpertHost, BuiltinExpertKind, BuiltinExpertOutput,
    BuiltinExpertRequest, RequirementReadOutcome,
};

use super::{expert, plan_request};

const MAX_PAGES: usize = 8;
const MAX_TOTAL_ITEMS: usize = 4 * MAX_CALENDAR_CONTEXT_ITEMS;
const MAX_TOTAL_BYTES: usize = MAX_CONTEXT_EVIDENCE_BYTES;

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum BlockedResult {
    Unavailable { reason: SourceUnavailable },
    NeedsUserAction,
}

pub async fn dispatch<Host: BuiltinExpertHost + ?Sized>(
    host: &Host,
    request: &BuiltinExpertRequest,
) -> Result<BuiltinExpertOutput, AgentFailure> {
    if request.assignment.trim().is_empty() || request.assignment.len() > 2048 {
        return Err(AgentFailure::InvalidInput);
    }
    let now = Utc
        .timestamp_millis_opt(request.current_time_unix_ms)
        .single()
        .ok_or(AgentFailure::InvalidInput)?;
    let plan = plan_request(&request.assignment, now.with_timezone(&Local), now)?;
    let range_start_unix_ms = plan.starts_at.timestamp_millis();
    let range_end_unix_ms = plan.ends_at.timestamp_millis();
    let mut cursor = None;
    let mut seen_cursors = HashSet::new();
    let mut pages = BTreeMap::<String, CalendarContextView>::new();
    let mut total_items = 0;
    let mut total_bytes = 0;
    for page_index in 0..MAX_PAGES {
        let query = CalendarViewQuery::try_new(
            range_start_unix_ms,
            range_end_unix_ms,
            cursor.clone(),
            MAX_CALENDAR_CONTEXT_ITEMS,
        )?;
        let outcome = crate::shared::read_declared_view::<_, Vec<CalendarContextView>>(
            host,
            request,
            "floe.source.calendar",
            serde_json::to_value(&query).map_err(|_| AgentFailure::InvalidInput)?,
        )
        .await?;
        let views = match outcome {
            RequirementReadOutcome::Ready(views) => views,
            RequirementReadOutcome::Unavailable(reason) => {
                return blocked(BlockedResult::Unavailable { reason }, vec![]);
            }
            RequirementReadOutcome::NeedsUserAction => {
                // The host publishes the requirement it captured; the report
                // proposes no requirement of its own.
                return blocked(BlockedResult::NeedsUserAction, vec![]);
            }
        };
        if views.is_empty() {
            return Err(AgentFailure::StaleContext);
        }
        let previous_sources: HashSet<_> = pages.keys().cloned().collect();
        let mut next_cursor = None;
        let mut page_sources = HashSet::new();
        for view in views {
            validate_calendar_context_view_for_query(&view, &query, Utc::now().timestamp_millis())?;
            if !page_sources.insert(view.source_handle.clone()) {
                return Err(AgentFailure::StaleContext);
            }
            total_items += view.items.len();
            total_bytes += serde_json::to_vec(&view)
                .map_err(|_| AgentFailure::InvalidInput)?
                .len();
            if total_items > MAX_TOTAL_ITEMS || total_bytes > MAX_TOTAL_BYTES {
                return Err(AgentFailure::BudgetExceeded);
            }
            if let Some(page_cursor) = &view.next_cursor {
                if next_cursor
                    .as_ref()
                    .is_some_and(|existing| existing != page_cursor)
                {
                    return Err(AgentFailure::StaleContext);
                }
                next_cursor = Some(page_cursor.clone());
            }
            match pages.get_mut(&view.source_handle) {
                Some(existing) => {
                    if existing.range_start_unix_ms != view.range_start_unix_ms
                        || existing.range_end_unix_ms != view.range_end_unix_ms
                    {
                        return Err(AgentFailure::StaleContext);
                    }
                    existing.expires_at_unix_ms =
                        existing.expires_at_unix_ms.min(view.expires_at_unix_ms);
                    if view.items.iter().any(|item| {
                        existing
                            .items
                            .iter()
                            .any(|previous| previous.evidence_handle == item.evidence_handle)
                    }) {
                        return Err(AgentFailure::StaleContext);
                    }
                    existing.items.extend(view.items);
                    existing.coverage_complete = view.coverage_complete;
                    existing.next_cursor = view.next_cursor;
                }
                None => {
                    pages.insert(view.source_handle.clone(), view);
                }
            }
        }
        if page_index > 0 && page_sources != previous_sources {
            return Err(AgentFailure::StaleContext);
        }
        if let Some(next) = next_cursor {
            if pages.values().any(|view| view.coverage_complete) {
                return Err(AgentFailure::StaleContext);
            }
            if !seen_cursors.insert(next.clone()) {
                return Err(AgentFailure::StaleContext);
            }
            cursor = Some(next);
            continue;
        }
        if pages.values().any(|view| !view.coverage_complete) {
            return Err(AgentFailure::StaleContext);
        }
        let views: Vec<_> = pages.into_values().collect();
        let draft = match expert::judge(
            host,
            request,
            &views,
            (page_index + 1) as u32,
            plan.propose_focus,
        )
        .await?
        {
            ExpertJudgment::Decided(draft) => draft,
            ExpertJudgment::Blocked(_) => {
                // The host publishes the model requirement it captured; the
                // report proposes no requirement of its own.
                return BuiltinExpertOutput::from_blocked(
                    BuiltinExpertKind::Schedule.result_artifact_name(),
                    super::RESULT_MEDIA_TYPE,
                    BlockedExpertStatus::NeedsUserAction,
                    "Model approval needs your review, so there is no schedule assessment.".into(),
                );
            }
        };
        return host.settle_stateful_result(request, draft).await;
    }
    Err(AgentFailure::BudgetExceeded)
}

fn blocked(
    result: BlockedResult,
    artifacts: Vec<Artifact>,
) -> Result<BuiltinExpertOutput, AgentFailure> {
    let result_text = match &result {
        BlockedResult::Unavailable { .. } => {
            "Calendar information is not available for this Schedule task."
        }
        BlockedResult::NeedsUserAction => {
            "Calendar access needs your review before this Schedule task can continue."
        }
    };
    BuiltinExpertOutput::from_result(
        BuiltinExpertKind::Schedule.result_artifact_name(),
        super::RESULT_MEDIA_TYPE,
        result_text.into(),
        &result,
    )
    .map(|output| output.with_artifacts(artifacts))
}
