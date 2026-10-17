//! Startup context uses a real Surface prefix, never a fabricated current User
//! turn to make a generation-only request type validate.
use crate::{LivePrefixReader, LiveProjectionOutcome, project_coverage};
use floe_agent_contract::{MAX_OUTPUT_BYTES, MessageRole};
use floe_context_contract::{
    ContextDependency, DependencyCoverage, LiveContextProjection, LiveProjectionSource,
};
use floe_execution::ExecutionScope;
use floe_kernel::AgentFailure;
use floe_model_contract::{LiveContextPayload, LiveModelPlan, MAX_LIVE_CONTEXT_BYTES};
use std::future::Future;
use uuid::Uuid;

pub struct LivePrefixProjectionRequest<'a> {
    pub plan: &'a LiveModelPlan,
    pub voice_session_id: Uuid,
    pub surface_session_id: Uuid,
    pub projection_id: Uuid,
    pub source: &'a LiveProjectionSource,
    /// Trusted role-owner instructions, not a Person message or provider hint.
    /// This parameter is internal composition, never an editable AppWire field.
    pub instructions: &'a str,
}

pub async fn prepare_live_prefix_projection<Authorize, AuthorizationFuture>(
    reader: &dyn LivePrefixReader,
    request: LivePrefixProjectionRequest<'_>,
    scope: &ExecutionScope,
    mut authorize: Authorize,
) -> Result<LiveProjectionOutcome, AgentFailure>
where
    Authorize: FnMut(ContextDependency) -> AuthorizationFuture,
    AuthorizationFuture: Future<Output = Result<bool, AgentFailure>>,
{
    request.plan.validate()?;
    request.source.validate()?;
    if request.voice_session_id.is_nil()
        || request.projection_id.is_nil()
        || request.surface_session_id != request.source.through().session_id
        || !matches!(request.source, LiveProjectionSource::SurfacePrefix { .. })
        || request.instructions.trim().is_empty()
        || request.instructions.len() > MAX_OUTPUT_BYTES
    {
        return Err(AgentFailure::InvalidInput);
    }
    let limit = request
        .plan
        .limits
        .initial_context_bytes
        .min(MAX_LIVE_CONTEXT_BYTES as u64);
    let encode = |history: &[serde_json::Value], omitted: usize| {
        serde_json::json!({
            "kind":"live_startup", "instructions":request.instructions,
            "history":history, "omitted_derived_blocks":omitted,
        })
        .to_string()
    };
    if encode(&[], 0).len() as u64 > limit {
        return Err(AgentFailure::ModelInputCapacityExceeded);
    }
    let needs_compaction = || LiveProjectionOutcome::NeedsCompaction {
        source: request.source.clone(),
        max_payload_bytes: limit,
    };
    scope
        .run(async {
            let snapshot = match reader
                .read_live_prefix(&request.plan.actor, request.source)
                .await
            {
                Err(AgentFailure::ModelInputCapacityExceeded) => return Ok(needs_compaction()),
                other => other?,
            };
            match snapshot.validate_for(&request.plan.actor, request.source) {
                Err(AgentFailure::ModelInputCapacityExceeded) => return Ok(needs_compaction()),
                other => other?,
            }
            if !snapshot.complete {
                return Ok(needs_compaction());
            }
            let mut history = Vec::new();
            let mut omitted = 0;
            let mut coverage = DependencyCoverage::Independent;
            for message in snapshot.messages {
                let admitted = project_coverage(&message.coverage, &mut authorize).await?;
                if !admitted.retain_derived() {
                    omitted += 1;
                    continue;
                }
                coverage = coverage
                    .merge(&message.coverage)
                    .map_err(|_| AgentFailure::InvalidInput)?;
                let role = if Some(message.message_id) == snapshot.summary_message_id {
                    "summary"
                } else {
                    match message.role {
                        MessageRole::User => "user",
                        MessageRole::Assistant => "assistant",
                        MessageRole::Preamble => "status",
                        _ => return Err(AgentFailure::InvalidInput),
                    }
                };
                history.push(serde_json::json!({"role":role,"text":message.text}));
            }
            let text = encode(&history, omitted);
            if text.len() as u64 > limit {
                return Ok(needs_compaction());
            }
            let projection = LiveContextProjection {
                person_id: request.plan.actor.person_id,
                device_id: request.plan.actor.device_id.clone(),
                runtime_epoch: request.plan.actor.runtime_epoch,
                voice_session_id: request.voice_session_id,
                plan_id: request.plan.operation_id,
                binding_digest: request.plan.binding_digest,
                projection_id: request.projection_id,
                projection_revision: 1,
                source: request.source.clone(),
                input_data_classes: super::live_projection::live_coverage_classes(&coverage),
                coverage,
                memories: vec![],
                payload: LiveContextPayload::new(text)
                    .map_err(|_| AgentFailure::ModelInputCapacityExceeded)?,
            };
            projection.validate_for_plan(
                request.plan,
                request.voice_session_id,
                request.surface_session_id,
            )?;
            if let Some(review) =
                super::live_projection::review_live_processing(request.plan, &projection)?
            {
                return Ok(LiveProjectionOutcome::NeedsSourceReview(review));
            }
            Ok(LiveProjectionOutcome::Ready(projection))
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LivePrefixSnapshot;
    use floe_agent_contract::AgentMessage;
    use floe_conversation_contract::SurfaceAnchor;
    use floe_execution::{
        BoxFuture, Cancellation,
        budget::{BudgetConfig, BudgetLedger, ModelUsage},
    };
    use floe_kernel::{OwnerActor, PersonId, TraceContext};
    use floe_model_contract::{
        LiveModelCapabilities, LiveTransportLimits, ModelBindingDigest, ModelSelectionCommitment,
        ProcessingBoundary,
    };
    use std::time::Duration;
    struct Reader(LivePrefixSnapshot);
    impl LivePrefixReader for Reader {
        fn read_live_prefix<'a>(
            &'a self,
            _: &'a OwnerActor,
            _: &'a LiveProjectionSource,
        ) -> BoxFuture<'a, Result<LivePrefixSnapshot, AgentFailure>> {
            Box::pin(async move { Ok(self.0.clone()) })
        }
    }
    fn fixture(messages: Vec<AgentMessage>) -> (Reader, LiveModelPlan, ExecutionScope) {
        let actor = OwnerActor {
            person_id: PersonId::new(),
            device_id: "fixture-device".into(),
            runtime_epoch: 1,
        };
        let mut through = SurfaceAnchor::empty(Uuid::new_v4()).unwrap();
        if !messages.is_empty() {
            through.sequence = messages.len() as u64;
            through.digest = [9; 32];
        }
        let reader = Reader(LivePrefixSnapshot {
            person_id: actor.person_id,
            source: LiveProjectionSource::SurfacePrefix { through },
            messages,
            summary_message_id: None,
            complete: true,
        });
        let plan = LiveModelPlan {
            operation_id: Uuid::new_v4(),
            actor,
            capabilities: LiveModelCapabilities::conversational(),
            limits: LiveTransportLimits {
                initial_context_bytes: 16384,
                append_bytes: 500,
                local_session_duration_ms: 60000,
            },
            boundary: ProcessingBoundary::Device,
            binding_digest: ModelBindingDigest([1; 32]),
            selection_commitment: ModelSelectionCommitment([2; 32]),
        };
        let budget = BudgetLedger::new(BudgetConfig::new(1, 1), ModelUsage::default());
        let scope = ExecutionScope::root(
            Cancellation::new(),
            tokio::time::Instant::now() + Duration::from_secs(2),
            budget.root_lease(),
            TraceContext::new(Uuid::new_v4()),
        );
        (reader, plan, scope)
    }
    fn message(role: MessageRole, text: &str, coverage: DependencyCoverage) -> AgentMessage {
        AgentMessage {
            message_id: Uuid::new_v4(),
            role,
            text: text.into(),
            call_id: None,
            coverage,
        }
    }
    fn request<'a>(reader: &'a Reader, plan: &'a LiveModelPlan) -> LivePrefixProjectionRequest<'a> {
        LivePrefixProjectionRequest {
            plan,
            voice_session_id: Uuid::new_v4(),
            surface_session_id: reader.0.source.through().session_id,
            projection_id: Uuid::new_v4(),
            source: &reader.0.source,
            instructions: "Synthetic trusted presentation instructions",
        }
    }
    #[tokio::test]
    async fn empty_surface_needs_no_fabricated_current_user_or_core_turn() {
        let (reader, plan, scope) = fixture(vec![]);
        let LiveProjectionOutcome::Ready(p) =
            prepare_live_prefix_projection(&reader, request(&reader, &plan), &scope, |_| async {
                panic!("no sources")
            })
            .await
            .unwrap()
        else {
            panic!("ready")
        };
        let payload: serde_json::Value = serde_json::from_str(p.payload.as_str()).unwrap();
        assert_eq!(payload["history"], serde_json::json!([]));
        assert_eq!(payload["omitted_derived_blocks"], 0);
        assert_eq!(p.coverage, DependencyCoverage::Independent);
        assert!(p.memories.is_empty());
    }
    #[tokio::test]
    async fn incomplete_or_over_budget_prefix_requests_compaction_without_silent_tail_start() {
        let (mut reader, mut plan, scope) = fixture(vec![message(
            MessageRole::User,
            &"x".repeat(1000),
            DependencyCoverage::Independent,
        )]);
        reader.0.complete = false;
        assert!(matches!(
            prepare_live_prefix_projection(&reader, request(&reader, &plan), &scope, |_| async {
                panic!("incomplete prefix cannot reach source projection")
            })
            .await
            .unwrap(),
            LiveProjectionOutcome::NeedsCompaction { .. }
        ));
        reader.0.complete = true;
        plan.limits.initial_context_bytes = 200;
        assert!(matches!(
            prepare_live_prefix_projection(&reader, request(&reader, &plan), &scope, |_| async {
                Ok(true)
            })
            .await
            .unwrap(),
            LiveProjectionOutcome::NeedsCompaction { .. }
        ));
        plan.limits.initial_context_bytes = 1;
        assert!(matches!(
            prepare_live_prefix_projection(&reader, request(&reader, &plan), &scope, |_| async {
                Ok(true)
            })
            .await,
            Err(AgentFailure::ModelInputCapacityExceeded)
        ));
    }
    #[tokio::test]
    async fn unknown_derived_history_never_erases_original_person_input_or_leaks_provenance() {
        let user = message(
            MessageRole::User,
            "Original person text",
            DependencyCoverage::Independent,
        );
        let id = user.message_id.to_string();
        let (reader, plan, scope) = fixture(vec![
            user,
            message(
                MessageRole::Assistant,
                "Unknown old answer",
                DependencyCoverage::Unknown,
            ),
        ]);
        let LiveProjectionOutcome::Ready(p) =
            prepare_live_prefix_projection(&reader, request(&reader, &plan), &scope, |_| async {
                panic!("unknown never authorizes")
            })
            .await
            .unwrap()
        else {
            panic!("ready")
        };
        let payload: serde_json::Value = serde_json::from_str(p.payload.as_str()).unwrap();
        assert_eq!(
            payload["history"],
            serde_json::json!([{"role":"user","text":"Original person text"}])
        );
        assert_eq!(payload["omitted_derived_blocks"], 1);
        assert!(!p.payload.as_str().contains(&id));
        assert!(!p.payload.as_str().contains("Unknown old answer"));
    }
    #[tokio::test]
    async fn existing_summary_is_labeled_as_summary_and_foreign_or_malformed_snapshots_fail() {
        let summary = message(
            MessageRole::Assistant,
            "Authorized earlier summary",
            DependencyCoverage::Independent,
        );
        let id = summary.message_id;
        let (mut reader, plan, scope) = fixture(vec![summary]);
        reader.0.summary_message_id = Some(id);
        let LiveProjectionOutcome::Ready(p) =
            prepare_live_prefix_projection(&reader, request(&reader, &plan), &scope, |_| async {
                Ok(true)
            })
            .await
            .unwrap()
        else {
            panic!("ready")
        };
        let payload: serde_json::Value = serde_json::from_str(p.payload.as_str()).unwrap();
        assert_eq!(payload["history"][0]["role"], "summary");
        reader.0.summary_message_id = Some(Uuid::new_v4());
        assert!(
            prepare_live_prefix_projection(&reader, request(&reader, &plan), &scope, |_| async {
                Ok(true)
            })
            .await
            .is_err()
        );
        reader.0.summary_message_id = Some(id);
        reader.0.person_id = PersonId::new();
        assert!(matches!(
            prepare_live_prefix_projection(&reader, request(&reader, &plan), &scope, |_| async {
                Ok(true)
            })
            .await,
            Err(AgentFailure::PolicyDenied)
        ));
    }
    #[tokio::test]
    async fn oversized_canonical_snapshot_requests_compaction_before_authorization() {
        let body = "x".repeat(MAX_OUTPUT_BYTES);
        let (reader, plan, scope) = fixture(
            (0..128)
                .map(|_| message(MessageRole::User, &body, DependencyCoverage::Independent))
                .collect(),
        );
        assert!(matches!(
            prepare_live_prefix_projection(&reader, request(&reader, &plan), &scope, |_| async {
                panic!("oversized history is not a ready projection")
            })
            .await
            .unwrap(),
            LiveProjectionOutcome::NeedsCompaction { .. }
        ));
    }
}
