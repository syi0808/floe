use std::{future::Future, sync::Mutex, time::SystemTime};

use floe_domain::PersonId;
use serde::{Deserialize, Serialize};
use tokio::time::Instant;
use uuid::Uuid;

use crate::{
    AGENT_VERSION, AgentFailure, AgentRegistry, Cancellation, DataClass, ExpertRule,
    PackageImplementation, PackageRef,
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

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExpertInput {
    Briefing { focus_minutes: u16 },
    ProposeFocus { focus_minutes: u16 },
}

#[derive(Clone, Copy)]
pub struct ExpertBudget {
    pub max_view_calls: u32,
    pub max_view_bytes: usize,
    pub max_output_bytes: usize,
    pub max_insights: usize,
}

impl Default for ExpertBudget {
    fn default() -> Self {
        Self {
            max_view_calls: 1,
            max_view_bytes: 16384,
            max_output_bytes: 16384,
            max_insights: 8,
        }
    }
}

pub struct ExpertInvocation {
    pub schema_version: u32,
    pub invocation_id: Uuid,
    pub instance_id: Uuid,
    pub person_id: PersonId,
    pub assignment_id: Uuid,
    pub expected_registry_revision: u64,
    pub granted_view_handles: Vec<Uuid>,
    pub allowed_data_classes: Vec<DataClass>,
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
    pub state_revision: u64,
    pub view_calls: u32,
}

pub struct ExpertHost<'host, Views> {
    pub registry: &'host Mutex<AgentRegistry>,
    pub views: &'host Views,
}

impl<Views: ExpertViews> ExpertHost<'_, Views> {
    pub async fn invoke(
        &self,
        mut invocation: ExpertInvocation,
    ) -> Result<ExpertResult, AgentFailure> {
        if invocation.schema_version != AGENT_VERSION {
            return Err(AgentFailure::UnsupportedVersion);
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
        let focus_minutes = match invocation.input {
            ExpertInput::Briefing { focus_minutes }
            | ExpertInput::ProposeFocus { focus_minutes } => focus_minutes,
        };
        if !(1..=240).contains(&focus_minutes) {
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
        let view = tokio::select! {
            biased;
            _ = invocation.cancellation.cancelled() => return Err(AgentFailure::Cancelled),
            result = tokio::time::timeout_at(invocation.deadline, self.views.timeline(read)) =>
                result.map_err(|_| AgentFailure::DeadlineExceeded)??,
        };
        check_running(&invocation)?;
        validate_view(&view, &invocation, resolved.data_class)?;
        let minimum = match &resolved.package.implementation {
            PackageImplementation::Schedule => focus_minutes,
            PackageImplementation::Declarative { rules } => match rules.as_slice() {
                [ExpertRule::FindFocusWindow { minimum_minutes }] => {
                    focus_minutes.max(*minimum_minutes)
                }
                _ => return Err(AgentFailure::CapabilityDenied),
            },
            _ => return Err(AgentFailure::CapabilityDenied),
        };
        let insights = analyze_schedule(&view, minimum);
        if insights.len() > invocation.budget.max_insights.min(8) {
            return Err(AgentFailure::BudgetExceeded);
        }
        let action_proposals = if matches!(invocation.input, ExpertInput::ProposeFocus { .. }) {
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
        || view.range_end_unix_ms - view.range_start_unix_ms > 86_400_000
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
