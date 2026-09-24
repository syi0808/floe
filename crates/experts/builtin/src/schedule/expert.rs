use floe_agent_contract::{
    AgentFailure, ExpertInsight, ExpertModelOutcome, ExpertModelRequirement,
};
use floe_context_contract::{CalendarContextView, DataClass, calendar_context_evidence};
use uuid::Uuid;

use crate::prompts::schedule_expert_prompt;
use crate::shared::{ExpertJudgment, run_expert_model};
use crate::{BuiltinExpertHost, BuiltinExpertRequest, StatefulExpertDraft, StatefulFocusProposal};

pub async fn judge<Host: BuiltinExpertHost + ?Sized>(
    host: &Host,
    request: &BuiltinExpertRequest,
    views: &[CalendarContextView],
    view_calls: u32,
    propose_focus: bool,
) -> Result<ExpertJudgment<StatefulExpertDraft>, AgentFailure> {
    let first = views.first().ok_or(AgentFailure::CapabilityUnavailable)?;
    let mut context = request.context.clone();
    context.memories.clear();
    let mut evidence = Vec::with_capacity(views.len());
    for view in views {
        evidence.push(calendar_context_evidence(view)?);
    }
    context.evidence.extend(evidence);
    let answer = match run_expert_model(
        host.model(),
        host.policy(),
        ExpertModelRequirement::Any,
        request.person_id,
        request.invocation_id,
        &request.assignment,
        request.current_time_unix_ms,
        context,
        schedule_expert_prompt(),
        40_960,
        50_000,
        request.max_output_bytes,
        request.deadline,
        &request.cancellation,
    )
    .await?
    {
        ExpertModelOutcome::Answered(answer) => answer,
        ExpertModelOutcome::Blocked(requirement) => {
            return Ok(ExpertJudgment::Blocked(requirement));
        }
    };
    let summary = answer.answer.trim();
    if summary.is_empty() || summary.len() > 2048 {
        return Err(AgentFailure::InvalidModelOutput);
    }
    let mut items: Vec<_> = views.iter().flat_map(|view| view.items.iter()).collect();
    items.sort_by_key(|item| (item.starts_at_unix_ms, item.ends_at_unix_ms));
    let mut insights = Vec::new();
    for item in items.iter().take(if propose_focus { 7 } else { 8 }) {
        insights.push(ExpertInsight::Commitment {
            evidence_handle: Uuid::new_v5(&request.invocation_id, item.evidence_handle.as_bytes()),
            untrusted_title: item.untrusted_title.clone(),
            starts_at_unix_ms: u64::try_from(item.starts_at_unix_ms)
                .map_err(|_| AgentFailure::InvalidInput)?,
            ends_at_unix_ms: u64::try_from(item.ends_at_unix_ms)
                .map_err(|_| AgentFailure::InvalidInput)?,
        });
    }
    let mut action_proposals = Vec::new();
    if propose_focus {
        if views.len() != 1 {
            return Err(AgentFailure::CapabilityUnavailable);
        }
        let duration = 60 * 60 * 1000;
        let mut cursor = first.range_start_unix_ms;
        let mut window = None;
        for item in &items {
            if item.starts_at_unix_ms.saturating_sub(cursor) >= duration {
                window = Some((cursor, cursor + duration));
                break;
            }
            cursor = cursor.max(item.ends_at_unix_ms);
        }
        if window.is_none() && first.range_end_unix_ms.saturating_sub(cursor) >= duration {
            window = Some((cursor, cursor + duration));
        }
        match window {
            Some((starts_at_unix_ms, ends_at_unix_ms)) => {
                insights.push(ExpertInsight::FocusWindow {
                    starts_at_unix_ms: u64::try_from(starts_at_unix_ms)
                        .map_err(|_| AgentFailure::InvalidInput)?,
                    ends_at_unix_ms: u64::try_from(ends_at_unix_ms)
                        .map_err(|_| AgentFailure::InvalidInput)?,
                });
                action_proposals.push(StatefulFocusProposal {
                    starts_at_unix_ms: u64::try_from(starts_at_unix_ms)
                        .map_err(|_| AgentFailure::InvalidInput)?,
                    ends_at_unix_ms: u64::try_from(ends_at_unix_ms)
                        .map_err(|_| AgentFailure::InvalidInput)?,
                });
            }
            None => insights.push(ExpertInsight::NoFocusWindow),
        }
    }
    Ok(ExpertJudgment::Decided(StatefulExpertDraft {
        source_handle: first.source_handle.clone(),
        data_class: DataClass::Personal,
        expires_at_unix_ms: u64::try_from(
            views
                .iter()
                .map(|view| view.expires_at_unix_ms)
                .min()
                .unwrap_or_default(),
        )
        .map_err(|_| AgentFailure::InvalidInput)?,
        insights,
        action_proposals,
        summary: summary.to_owned(),
        model_calls: 1,
        view_calls,
    }))
}
