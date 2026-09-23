use std::collections::{BTreeMap, HashSet};

use chrono::{Local, TimeZone, Utc};
use floe_agent_contract::{
    AgentFailure, Artifact, ArtifactPart, DependencyCoverage,
};
use floe_context_contract::{
    CalendarContextView, CalendarViewQuery, SourceReadOutcome, SourceUnavailable,
    validate_calendar_context_view_for_query, MAX_CALENDAR_CONTEXT_ITEMS,
    MAX_CONTEXT_EVIDENCE_BYTES,
};
use serde::Serialize;
use uuid::Uuid;

use crate::{BuiltinExpertHost, BuiltinExpertKind, BuiltinExpertOutput, BuiltinExpertRequest};

use super::{expert, plan_request};

const MAX_PAGES: usize = 8;
const MAX_TOTAL_ITEMS: usize = 4 * MAX_CALENDAR_CONTEXT_ITEMS;
const MAX_TOTAL_BYTES: usize = MAX_CONTEXT_EVIDENCE_BYTES;
pub const SOURCE_ACCESS_REQUIREMENT_MEDIA_TYPE: &str =
    "application/vnd.floe.source-access-requirement+json;version=1";

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
        let outcome = host.calendar_views(request, query.clone()).await?;
        let views = match outcome {
            SourceReadOutcome::Ready(views) => views,
            SourceReadOutcome::Unavailable(reason) => {
                return blocked(BlockedResult::Unavailable { reason }, vec![]);
            }
            SourceReadOutcome::NeedsUserAction(requirement) => {
                requirement.validate().map_err(|_| AgentFailure::StaleContext)?;
                let artifact = Artifact {
                    artifact_id: Uuid::new_v4(),
                    name: "Calendar access requirement".into(),
                    parts: vec![ArtifactPart::Data {
                        media_type: SOURCE_ACCESS_REQUIREMENT_MEDIA_TYPE.into(),
                        data: serde_json::to_string(&requirement)
                            .map_err(|_| AgentFailure::InvalidInput)?,
                    }],
                    coverage: DependencyCoverage::Independent,
                };
                return blocked(BlockedResult::NeedsUserAction, vec![artifact]);
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
                if next_cursor.as_ref().is_some_and(|existing| existing != page_cursor) {
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
        let draft = expert::judge(host, request, &views, (page_index + 1) as u32, plan.propose_focus)
            .await?;
        return host.settle_stateful_result(request, draft).await;
    }
    Err(AgentFailure::BudgetExceeded)
}

fn blocked(
    result: BlockedResult,
    artifacts: Vec<Artifact>,
) -> Result<BuiltinExpertOutput, AgentFailure> {
    BuiltinExpertOutput::from_result(
        BuiltinExpertKind::Schedule.result_artifact_name(),
        "Calendar information is not available for this Schedule task.".into(),
        &result,
    )
    .map(|output| output.with_artifacts(artifacts))
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    use chrono::Duration;
    use floe_agent_contract::{
        AgentContext, BoxFuture, ExpertModel, ExpertModelAnswer, ExpertModelCall, PersonId,
    };
    use floe_context_contract::{
        AttentionView, AuthorizedRead, CalendarContextItem, ConfirmedInteractionView,
        ContextDependency, GrantConsumer, GrantOperation, GrantPurpose, GrantScope,
        MemoryContextSnapshot, NativeContextView, PeopleView, SourceAccessRequirement,
        SourceAccessRequirementKind, SourceGrant, WellbeingView, WorkContextView,
    };
    use floe_execution::Cancellation;
    use tokio::time::Instant;

    use super::*;
    use crate::{Acquiring, BuiltinContextSource};

    struct UnusedRead;

    impl floe_context_contract::HeldGrant for UnusedRead {
        fn scope(&self) -> &GrantScope {
            panic!("unused source read")
        }

        fn dependency(&self) -> &ContextDependency {
            panic!("unused source read")
        }
    }

    impl AuthorizedRead for UnusedRead {
        fn payload(&self) -> &serde_json::Value {
            panic!("unused source read")
        }

        fn is_fresh(&self) -> bool {
            panic!("unused source read")
        }
    }

    struct Model(AtomicUsize);

    impl ExpertModel for Model {
        fn answer<'a>(
            &'a self,
            _: ExpertModelCall,
        ) -> BoxFuture<'a, Result<ExpertModelAnswer, AgentFailure>> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Box::pin(async {
                Ok(ExpertModelAnswer {
                    schema_version: floe_agent_contract::AGENT_VERSION,
                    answer: "The calendar has one appointment.".into(),
                    used_tokens: 10,
                    cost_micros: 0,
                })
            })
        }
    }

    enum Scenario {
        Complete,
        Paginated,
        Cycle,
        Unavailable,
        NeedsUserAction,
        Failure,
    }

    struct Host {
        model: Model,
        policy: floe_agent_contract::InferencePolicyDecision,
        scenario: Scenario,
        queries: Mutex<Vec<CalendarViewQuery>>,
    }

    impl Host {
        fn new(scenario: Scenario) -> Self {
            Self {
                model: Model(AtomicUsize::new(0)),
                policy: super::super::run_policy(floe_context_contract::DataClass::Personal),
                scenario,
                queries: Mutex::new(Vec::new()),
            }
        }
    }

    impl BuiltinExpertHost for Host {
        type Model = Model;
        type SourceRead = UnusedRead;

        fn model(&self) -> &Self::Model {
            &self.model
        }

        fn policy(&self) -> &floe_agent_contract::InferencePolicyDecision {
            &self.policy
        }

        fn source_grant(&self, _: &str, _: BuiltinContextSource) -> SourceGrant {
            panic!("Schedule must not use setup grants")
        }

        fn read_source_view<'a>(
            &'a self,
            _: &'a BuiltinExpertRequest,
            _: &'a str,
            _: serde_json::Value,
        ) -> Acquiring<'a, Self::SourceRead> {
            panic!("Schedule must use Calendar Context")
        }

        fn record_dependency(
            &self,
            _: Uuid,
            _: Uuid,
            _: ContextDependency,
        ) -> Result<(), AgentFailure> {
            panic!("Calendar Context records dependencies")
        }

        fn calendar_views<'a>(
            &'a self,
            _: &'a BuiltinExpertRequest,
            query: CalendarViewQuery,
        ) -> Acquiring<'a, SourceReadOutcome<Vec<CalendarContextView>>> {
            Box::pin(async move {
                self.queries.lock().unwrap().push(query.clone());
                match self.scenario {
                    Scenario::Unavailable => Ok(SourceReadOutcome::Unavailable(
                        SourceUnavailable::TemporarilyUnavailable,
                    )),
                    Scenario::NeedsUserAction => Ok(SourceReadOutcome::NeedsUserAction(
                        SourceAccessRequirement::try_new(
                            "floe.source.calendar",
                            None,
                            None,
                            GrantOperation::Read,
                            GrantConsumer::builtin("floe.builtin.schedule").unwrap(),
                            GrantPurpose::Assistant,
                            vec![],
                            None,
                            SourceAccessRequirementKind::SelectResource,
                            None,
                            false,
                        )
                        .unwrap(),
                    )),
                    Scenario::Failure => Err(AgentFailure::StorageUnavailable),
                    _ => {
                        let now = Utc::now();
                        let first = query.cursor().is_none();
                        let next_cursor = match self.scenario {
                            Scenario::Paginated if first => Some("next".into()),
                            Scenario::Cycle => Some("next".into()),
                            _ => None,
                        };
                        Ok(SourceReadOutcome::Ready(vec![CalendarContextView {
                            schema_version: floe_agent_contract::AGENT_VERSION,
                            view_id: floe_context_contract::CALENDAR_CONTEXT_VIEW_ID.into(),
                            source_handle: "calendar:test".into(),
                            observed_at_unix_ms: (now - Duration::seconds(1)).timestamp_millis(),
                            expires_at_unix_ms: (now + Duration::minutes(2)).timestamp_millis(),
                            range_start_unix_ms: query.range_start_unix_ms(),
                            range_end_unix_ms: query.range_end_unix_ms(),
                            coverage_complete: next_cursor.is_none(),
                            next_cursor,
                            items: vec![CalendarContextItem {
                                evidence_handle: if first { "event:first" } else { "event:next" }
                                    .into(),
                                untrusted_title: "Appointment".into(),
                                starts_at_unix_ms: query.range_start_unix_ms() + 60_000,
                                ends_at_unix_ms: query.range_start_unix_ms() + 120_000,
                                all_day: false,
                            }],
                        }]))
                    }
                }
            })
        }

        fn settle_stateful_result<'a>(
            &'a self,
            _: &'a BuiltinExpertRequest,
            draft: crate::StatefulExpertDraft,
        ) -> Acquiring<'a, BuiltinExpertOutput> {
            Box::pin(async move {
                BuiltinExpertOutput::from_result(
                    BuiltinExpertKind::Schedule.result_artifact_name(),
                    draft.summary.clone(),
                    &draft,
                )
            })
        }

        fn work_context_views<'a>(&'a self, _: &'a BuiltinExpertRequest) -> Acquiring<'a, Vec<WorkContextView>> {
            panic!("unused")
        }

        fn people_view<'a>(&'a self, _: &'a BuiltinExpertRequest) -> Acquiring<'a, PeopleView> {
            panic!("unused")
        }

        fn confirmed_interaction_views<'a>(
            &'a self,
            _: &'a BuiltinExpertRequest,
            _: &'a PeopleView,
        ) -> Acquiring<'a, Vec<ConfirmedInteractionView>> {
            panic!("unused")
        }

        fn wellbeing_view<'a>(&'a self, _: &'a BuiltinExpertRequest) -> Acquiring<'a, WellbeingView> {
            panic!("unused")
        }

        fn attention_view<'a>(
            &'a self,
            _: &'a BuiltinExpertRequest,
        ) -> Acquiring<'a, (AttentionView, ContextDependency)> {
            panic!("unused")
        }

        fn conversation_context_available(&self) -> bool {
            false
        }

        fn memory_context<'a>(&'a self) -> Acquiring<'a, MemoryContextSnapshot> {
            panic!("unused")
        }

        fn task_view<'a>(&'a self) -> Acquiring<'a, NativeContextView> {
            panic!("unused")
        }

        fn staged_task_views(&self) -> &[NativeContextView] {
            &[]
        }
    }

    fn request() -> BuiltinExpertRequest {
        BuiltinExpertRequest {
            agent_id: BuiltinExpertKind::Schedule.package_id().into(),
            person_id: PersonId(Uuid::new_v4()),
            invocation_id: Uuid::new_v4(),
            assignment: "today".into(),
            current_time_unix_ms: Utc::now().timestamp_millis(),
            context: AgentContext {
                projection_version: 1,
                persona: None,
                memories: vec![],
                optional_context_issues: vec![],
                evidence: vec![],
            },
            max_output_bytes: 16_384,
            deadline: Instant::now() + std::time::Duration::from_secs(30),
            cancellation: Cancellation::default(),
        }
    }

    #[tokio::test]
    async fn complete_and_paginated_evidence_precedes_judgment() {
        for scenario in [Scenario::Complete, Scenario::Paginated] {
            let host = Host::new(scenario);
            let output = dispatch(&host, &request()).await.unwrap();
            assert!(output.summary.contains("appointment"));
            assert_eq!(host.model.0.load(Ordering::SeqCst), 1);
            let queries = host.queries.lock().unwrap();
            assert!(queries.iter().all(|query| {
                query.range_start_unix_ms() == queries[0].range_start_unix_ms()
                    && query.range_end_unix_ms() == queries[0].range_end_unix_ms()
            }));
        }
    }

    #[tokio::test]
    async fn blocked_and_failed_reads_never_call_the_model() {
        for scenario in [
            Scenario::Cycle,
            Scenario::Unavailable,
            Scenario::NeedsUserAction,
            Scenario::Failure,
        ] {
            let host = Host::new(scenario);
            let result = dispatch(&host, &request()).await;
            assert_eq!(host.model.0.load(Ordering::SeqCst), 0);
            match host.scenario {
                Scenario::Unavailable => assert!(result.unwrap().data.contains("unavailable")),
                Scenario::NeedsUserAction => {
                    let output = result.unwrap();
                    assert_eq!(output.artifacts.len(), 1);
                    assert!(output.artifacts[0].parts.iter().any(|part| matches!(part, ArtifactPart::Data { media_type, .. } if media_type == SOURCE_ACCESS_REQUIREMENT_MEDIA_TYPE)));
                }
                _ => assert!(result.is_err()),
            }
        }
    }
}
