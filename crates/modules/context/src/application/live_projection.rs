//! Context-owned projection of one canonical Manager result for live delivery.
//! The resulting value is data. Inference still owns live dispatch/release fences.
use crate::{LiveResultReader, project_coverage};
use floe_context_contract::{
    ContextDependency, DataClass, DependencyCoverage, LiveContextProjection, LiveProjectionSource,
    SourceAccessBlockers, SourceProjectionReview,
};
use floe_execution::ExecutionScope;
use floe_kernel::AgentFailure;
use floe_model_contract::{LiveContextPayload, LiveModelPlan};
use std::future::Future;
use uuid::Uuid;

pub struct LiveResultProjectionRequest<'a> {
    pub plan: &'a LiveModelPlan,
    pub voice_session_id: Uuid,
    pub surface_session_id: Uuid,
    /// Stable for the same logical projection/review attempt, never regenerated
    /// just because a local read or UI response was retried.
    pub projection_id: Uuid,
    pub source: &'a LiveProjectionSource,
}

#[derive(Debug)]
pub enum LiveProjectionOutcome {
    Ready(LiveContextProjection),
    NeedsSourceReview(SourceProjectionReview),
}

/// `authorize` is the current Context source resolver, with the same error
/// semantics as history/archive projection. False means stale/withdrawn data,
/// not permission to send an older cached result instead. A direct result cannot
/// silently disappear as a historical entry might: it fails this projection.
pub async fn prepare_live_result_projection<Authorize, AuthorizationFuture>(
    reader: &dyn LiveResultReader,
    request: LiveResultProjectionRequest<'_>,
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
        || !matches!(request.source, LiveProjectionSource::SurfaceResult { .. })
    {
        return Err(AgentFailure::InvalidInput);
    }
    scope
        .run(async {
            let snapshot = reader
                .read_live_result(&request.plan.actor, request.source)
                .await?;
            snapshot.validate_for(&request.plan.actor, request.source)?;
            // Unknown lineage cannot authorize a derived Manager result, even on
            // Device. The ordinary Person-facing transcript remains unchanged.
            if snapshot.coverage == DependencyCoverage::Unknown {
                return Err(AgentFailure::PolicyDenied);
            }
            let mut classes = vec![DataClass::Personal];
            if let DependencyCoverage::Dependent { dependencies } = &snapshot.coverage {
                if dependencies
                    .iter()
                    .any(|d| d.source().connector().as_str() == floe_access::WELLBEING_CONNECTOR)
                {
                    classes.push(DataClass::HighlySensitive);
                }
            }
            // Serialize only presentable content. Internal actor/Run/Source proof,
            // provider credentials and tool execution frames never enter this text.
            let payload = LiveContextPayload::new(
                serde_json::json!({
                    "kind": "manager_result",
                    "partial": snapshot.partial,
                    "text": snapshot.text,
                })
                .to_string(),
            )
            .map_err(|_| AgentFailure::ModelInputCapacityExceeded)?;
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
                coverage: snapshot.coverage,
                // This reads the published result, not current saved memories.
                // Any separately acquired memory context carries its own revisions.
                memories: vec![],
                input_data_classes: classes,
                payload,
            };
            projection.validate_for_plan(
                request.plan,
                request.voice_session_id,
                request.surface_session_id,
            )?;
            let blockers = super::processing_projection::source_processing_requirements(
                &request.plan.actor.person_id.to_string(),
                &request.plan.actor.device_id,
                request.plan.boundary,
                &projection.coverage,
                &projection.input_data_classes,
            )?;
            if !blockers.is_empty() {
                let blockers = SourceAccessBlockers::try_new(blockers)
                    .map_err(|_| AgentFailure::InvalidInput)?;
                let review = SourceProjectionReview {
                    projection_operation_id: request.projection_id,
                    target_digest: projection.digest()?,
                    blockers,
                };
                review.validate()?;
                return Ok(LiveProjectionOutcome::NeedsSourceReview(review));
            }
            let admitted = project_coverage(&projection.coverage, &mut authorize).await?;
            if !admitted.retain_derived() {
                return Err(AgentFailure::StaleContext);
            }
            Ok(LiveProjectionOutcome::Ready(projection))
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LiveResultSnapshot;
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
    struct Reader(LiveResultSnapshot);
    impl LiveResultReader for Reader {
        fn read_live_result<'a>(
            &'a self,
            _: &'a OwnerActor,
            _: &'a LiveProjectionSource,
        ) -> BoxFuture<'a, Result<LiveResultSnapshot, AgentFailure>> {
            Box::pin(async move { Ok(self.0.clone()) })
        }
    }
    fn fixture() -> (Reader, LiveModelPlan, ExecutionScope) {
        let actor = OwnerActor {
            person_id: PersonId::new(),
            device_id: "fixture-device".into(),
            runtime_epoch: 1,
        };
        let source: LiveProjectionSource = serde_json::from_value(serde_json::json!({
            "kind":"surface_result", "through":{"session_id":Uuid::new_v4(),"sequence":1,"digest":vec![9;32]}, "surface_message_id":Uuid::new_v4()
        })).unwrap();
        let reader = Reader(LiveResultSnapshot {
            person_id: actor.person_id,
            source,
            text: "Manager's canonical result".into(),
            partial: true,
            coverage: DependencyCoverage::Independent,
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
        let ledger = BudgetLedger::new(BudgetConfig::new(1, 1), ModelUsage::default());
        let scope = ExecutionScope::root(
            Cancellation::new(),
            tokio::time::Instant::now() + Duration::from_secs(3),
            ledger.root_lease(),
            TraceContext::new(Uuid::new_v4()),
        );
        (reader, plan, scope)
    }
    fn request<'a>(reader: &'a Reader, plan: &'a LiveModelPlan) -> LiveResultProjectionRequest<'a> {
        LiveResultProjectionRequest {
            plan,
            voice_session_id: Uuid::new_v4(),
            surface_session_id: reader.0.source.through().session_id,
            projection_id: Uuid::new_v4(),
            source: &reader.0.source,
        }
    }
    fn dependency(person_id: PersonId) -> ContextDependency {
        use floe_access::*;
        let source = GrantSourceBinding::try_new(
            person_id,
            ConnectionId::try_new("fixture.connection").unwrap(),
            ConnectorId::try_new("fixture.connector").unwrap(),
            ExecutionOwnerId::try_new("fixture.owner").unwrap(),
        )
        .unwrap();
        let now = chrono::Utc::now();
        ContextDependency::try_new(
            person_id,
            GrantId::new(),
            GrantAuthority::new(),
            source,
            vec![ResourceHandle::try_new("calendar.timeline:fixture.connection").unwrap()],
            SourceAuthority::new(),
            vec![ResourceHandle::try_new("fixture.source").unwrap()],
            vec![GrantDataCategory::Content],
            GrantOperation::Read,
            GrantPurpose::Assistant,
            GrantConsumer::builtin("fixture.manager").unwrap(),
            ProcessingRestriction::DeviceOnly,
            Uuid::new_v4(),
            vec![b'x'; 16],
            Uuid::new_v4(),
            Uuid::new_v4(),
            now,
            now + chrono::Duration::minutes(5),
        )
        .unwrap()
    }
    #[tokio::test]
    async fn canonical_result_projection_carries_partial_content_without_internal_provenance() {
        let (reader, plan, scope) = fixture();
        let request = request(&reader, &plan);
        let voice = request.voice_session_id;
        let LiveProjectionOutcome::Ready(projection) =
            prepare_live_result_projection(&reader, request, &scope, |_| async {
                panic!("Independent result has no source read")
            })
            .await
            .unwrap()
        else {
            panic!("ready")
        };
        projection
            .validate_for_plan(&plan, voice, reader.0.source.through().session_id)
            .unwrap();
        let payload: serde_json::Value = serde_json::from_str(projection.payload.as_str()).unwrap();
        assert_eq!(
            payload,
            serde_json::json!({"kind":"manager_result","partial":true,"text":reader.0.text})
        );
        assert!(
            !projection
                .payload
                .as_str()
                .contains(&plan.actor.person_id.to_string())
        );
        assert!(!format!("{projection:?}").contains(&reader.0.text));
        assert!(projection.memories.is_empty());
    }
    #[tokio::test]
    async fn unknown_or_foreign_result_is_never_an_authorized_cached_append() {
        let (mut reader, plan, scope) = fixture();
        reader.0.coverage = DependencyCoverage::Unknown;
        assert!(matches!(
            prepare_live_result_projection(&reader, request(&reader, &plan), &scope, |_| async {
                Ok(true)
            })
            .await,
            Err(AgentFailure::PolicyDenied)
        ));
        reader.0.coverage = DependencyCoverage::Independent;
        reader.0.person_id = PersonId::new();
        assert!(matches!(
            prepare_live_result_projection(&reader, request(&reader, &plan), &scope, |_| async {
                Ok(true)
            })
            .await,
            Err(AgentFailure::PolicyDenied)
        ));
    }
    #[tokio::test]
    async fn result_source_review_and_current_authorization_keep_distinct_semantics() {
        let (mut reader, mut plan, scope) = fixture();
        reader.0.coverage =
            DependencyCoverage::dependent(dependency(plan.actor.person_id)).unwrap();
        assert!(matches!(
            prepare_live_result_projection(&reader, request(&reader, &plan), &scope, |_| async {
                Ok(false)
            })
            .await,
            Err(AgentFailure::StaleContext)
        ));
        assert!(matches!(
            prepare_live_result_projection(&reader, request(&reader, &plan), &scope, |_| async {
                Err(AgentFailure::StorageBusy)
            })
            .await,
            Err(AgentFailure::StorageBusy)
        ));
        plan.boundary = ProcessingBoundary::Gateway;
        let request = request(&reader, &plan);
        let projection_id = request.projection_id;
        let LiveProjectionOutcome::NeedsSourceReview(review) =
            prepare_live_result_projection(&reader, request, &scope, |_| async {
                panic!("DeviceOnly processing needs review before provider dispatch")
            })
            .await
            .unwrap()
        else {
            panic!("review")
        };
        assert_eq!(review.projection_operation_id, projection_id);
        review.validate().unwrap();
    }
    #[tokio::test]
    async fn cancellation_and_mismatched_surface_never_produce_payload() {
        let (reader, plan, scope) = fixture();
        let mut wrong = request(&reader, &plan);
        wrong.surface_session_id = Uuid::new_v4();
        assert!(matches!(
            prepare_live_result_projection(&reader, wrong, &scope, |_| async { Ok(true) }).await,
            Err(AgentFailure::InvalidInput)
        ));
        scope.cancellation().cancel();
        assert!(
            prepare_live_result_projection(&reader, request(&reader, &plan), &scope, |_| async {
                Ok(true)
            })
            .await
            .is_err()
        );
    }
}
