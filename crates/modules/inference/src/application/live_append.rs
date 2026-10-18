//! Append retains all already-transmitted authorization obligations. Unknown
//! initial Device history remains its own fence and cannot erase known lineage.
use super::*;
use crate::{LiveAppendCustody, LiveAppendIntent, LiveAppendRequest, LiveAppendWrite};
use floe_context_contract::{
    ContextDependencyError, DataClass, DependencyCoverage, LiveContextProjection,
    LiveProjectionSource, MAX_CONTEXT_MEMORIES, MemoryRevisionReference,
};
use floe_execution::BoxFuture;
use floe_model_contract::{LiveAppendKind, LiveAppendPayload};

#[derive(Clone)]
pub(super) struct Sources {
    coverage: DependencyCoverage,
    memories: Vec<MemoryRevisionReference>,
    classes: Vec<DataClass>,
}
impl Sources {
    pub(super) fn initial(projection: &LiveContextProjection) -> Self {
        Self {
            // The original fence always remains in History. This accumulator
            // describes known obligations, not the original context's coverage.
            coverage: match &projection.coverage {
                DependencyCoverage::Unknown => DependencyCoverage::Independent,
                known => known.clone(),
            },
            memories: projection.memories.clone(),
            classes: projection.input_data_classes.clone(),
        }
    }
    fn extend(&self, projection: &LiveContextProjection) -> Result<Self, AgentFailure> {
        if projection.coverage == DependencyCoverage::Unknown {
            return Err(AgentFailure::PolicyDenied);
        }
        let coverage = self
            .coverage
            .merge(&projection.coverage)
            .map_err(|error| match error {
                ContextDependencyError::Conflict => AgentFailure::StaleContext,
                ContextDependencyError::TooLarge | ContextDependencyError::DependencyCount => {
                    AgentFailure::ModelInputCapacityExceeded
                }
                _ => AgentFailure::InvalidInput,
            })?;
        let mut memories = self.memories.clone();
        for incoming in &projection.memories {
            if let Some(old) = memories
                .iter()
                .find(|old| old.target_id == incoming.target_id)
            {
                if old.revision != incoming.revision {
                    // The remote call still has the old revision. It cannot be
                    // made current by relabelling or overwriting its local proof.
                    return Err(AgentFailure::StaleContext);
                }
            } else {
                memories.push(incoming.clone());
            }
        }
        if memories.len() > MAX_CONTEXT_MEMORIES {
            return Err(AgentFailure::ModelInputCapacityExceeded);
        }
        let mut classes = self.classes.clone();
        for class in &projection.input_data_classes {
            if !classes.contains(class) {
                classes.push(*class);
            }
        }
        Ok(Self {
            coverage,
            memories,
            classes,
        })
    }
}

pub(super) struct History {
    pub(super) initial: Arc<dyn LiveSourceFence>,
    pub(super) appended: Mutex<Option<Arc<dyn LiveSourceFence>>>,
}
impl LiveSourceFence for History {
    fn revalidate(&self) -> BoxFuture<'_, Result<(), AgentFailure>> {
        Box::pin(async move {
            // Never hold a synchronous lock across an authority read.
            let appended = self.appended.lock().unwrap().clone();
            self.initial.revalidate().await?;
            if let Some(fence) = appended {
                fence.revalidate().await?;
            }
            Ok(())
        })
    }
}
pub(super) trait SourceAdmission: Send + Sync {
    fn admit<'a>(
        &'a self,
        sources: &'a Sources,
        projection: &'a LiveContextProjection,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<Arc<dyn LiveSourceFence>, AgentFailure>>;
}
pub(super) struct Authority<R, A> {
    pub(super) request: ModelDispatchRequest,
    pub(super) resolver: Arc<R>,
    pub(super) authority: Arc<A>,
}
impl<R: DependencyResolver + 'static, A: GatewayAdmission + 'static> SourceAdmission
    for Authority<R, A>
{
    fn admit<'a>(
        &'a self,
        sources: &'a Sources,
        projection: &'a LiveContextProjection,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<Arc<dyn LiveSourceFence>, AgentFailure>> {
        Box::pin(async move {
            let mut request = self.request.clone();
            request.cancellation = self.request.cancellation.child_scope();
            request.deadline = scope.deadline();
            request.projection_ref = projection.projection_id;
            request.projection_revision = projection.projection_revision;
            request.coverage = sources.coverage.clone();
            request.memories = sources.memories.clone();
            request.input_data_classes = sources.classes.clone();
            let permit = admit_model_dispatch(
                request,
                self.resolver.clone(),
                self.authority.clone(),
                scope.clone(),
            )
            .await?;
            Ok(Arc::new(consume_model_dispatch(permit).await?) as Arc<dyn LiveSourceFence>)
        })
    }
}

impl AuthorizedLiveSession {
    /// The registered Voice owner must retain this operation through response
    /// loss. Before-I/O errors are not sent; once I/O starts, failures are
    /// explicitly uncertain and fence the call rather than authorizing replay.
    pub async fn append(
        &mut self,
        request: LiveAppendRequest,
        custody: &dyn LiveAppendCustody,
    ) -> Result<LiveAppendWrite, AgentFailure> {
        if self.attempt.is_none() || self.terminal_usage.is_some() {
            return Err(AgentFailure::Conflict);
        }
        self.revalidate().await?;
        request.projection.validate_for_plan(
            &self.plan,
            self.voice_session_id,
            self.surface_session_id,
        )?;
        if !matches!(
            request.projection.source,
            LiveProjectionSource::SurfaceResult { .. }
        ) || matches!(request.kind, LiveAppendKind::Instruction)
        {
            return Err(AgentFailure::InvalidInput);
        }
        let payload = LiveAppendPayload {
            operation_id: request.operation_id,
            kind: request.kind,
            delegation_id: request.delegation_id,
            text: request.projection.payload.as_str().to_owned(),
        };
        payload.validate().map_err(|error| match error {
            floe_model_contract::ModelContractError::Bounds => {
                AgentFailure::ModelInputCapacityExceeded
            }
            _ => AgentFailure::InvalidInput,
        })?;
        if payload.text.len() as u64 > self.plan.limits.append_bytes {
            return Err(AgentFailure::ModelInputCapacityExceeded);
        }
        let sources = self.append_sources.extend(&request.projection)?;
        self.transport.validate_append(&payload)?;
        let authorization = self.scope.child_scope(self.scope.deadline(), 0, 0, None);
        let fence = authorization
            .run(async {
                tokio::time::timeout(
                    REVALIDATE_TIMEOUT,
                    self.append_authority
                        .admit(&sources, &request.projection, &authorization),
                )
                .await
                .map_err(|_| AgentFailure::DeadlineExceeded)?
            })
            .await?;
        let intent = LiveAppendIntent {
            actor: self.plan.actor.clone(),
            voice_session_id: self.voice_session_id,
            surface_session_id: self.surface_session_id,
            plan_id: self.plan.operation_id,
            source: request.projection.source.clone(),
            projection_digest: request.projection.digest()?,
            payload,
        };
        intent.validate()?;
        if !custody.claim_append(&intent, &self.scope).await? {
            return Err(AgentFailure::Conflict);
        }
        // Recheck both old and new obligations after durable intent. A source
        // change while waiting for custody cannot cross the provider handoff.
        self.revalidate().await?;
        if let Err(error) = bounded_revalidate(&fence, &self.scope).await {
            return Err(self.fail(error));
        }
        *self.history.appended.lock().unwrap() = Some(fence);
        self.append_sources = sources;
        // Install cumulative obligations before the first possible write. Even a
        // partial batch keeps these obligations until this call is retired.
        let write = self.scope.child_scope(self.scope.deadline(), 0, 0, None);
        let mut inflight = InFlight {
            call: self,
            completed: false,
        };
        let result = write
            .run(inflight.call.transport.append(&intent.payload, &write))
            .await;
        inflight.completed = true;
        match result {
            Ok(()) => Ok(LiveAppendWrite::Written),
            Err(error) => Ok(LiveAppendWrite::Uncertain(self.fail(error))),
        }
    }
}

// Durable intent remains uncertain if the owning task unexpectedly drops during
// a write. This is a terminal fence, not a retry permission or remote close ACK.
struct InFlight<'a> {
    call: &'a AuthorizedLiveSession,
    completed: bool,
}
impl Drop for InFlight<'_> {
    fn drop(&mut self) {
        if !self.completed {
            self.call.fail(AgentFailure::Interrupted);
        }
    }
}
