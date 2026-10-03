use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use chrono::{DateTime, Utc};
use floe_agent_contract::{AgentFailure, BoxFuture, CommandId, ExecutionScope, ModelPort, OwnerActor};
use floe_execution::Cancellation;
use uuid::Uuid;

pub trait KnowledgeClock: Send + Sync { fn now(&self) -> DateTime<Utc>; }
pub struct SystemKnowledgeClock;
impl KnowledgeClock for SystemKnowledgeClock { fn now(&self) -> DateTime<Utc> { Utc::now() } }

pub trait KnowledgeRead: Send + Sync {
    fn read_context<'a>(&'a self, actor: &'a OwnerActor, scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<crate::MemoryContextSnapshot, AgentFailure>>;
}

pub trait KnowledgeOwner: KnowledgeRead {
    fn activate<'a>(&'a self, scope: &'a ExecutionScope) -> BoxFuture<'a, Result<(), AgentFailure>>;
    fn foreground_lease(&self) -> Result<crate::KnowledgeForegroundLease, AgentFailure>;
    fn close_admission(&self);
    fn shutdown<'a>(&'a self) -> BoxFuture<'a, Result<(), AgentFailure>>;
    fn overview<'a>(&'a self, actor: &'a OwnerActor, limit: usize, scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<crate::MemoryOverviewSnapshot, AgentFailure>>;
    fn review<'a>(&'a self, actor: &'a OwnerActor, scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<crate::MemoryReviewSnapshot, AgentFailure>>;
    fn decide<'a>(&'a self, actor: &'a OwnerActor, command_id: CommandId, candidate_id: Uuid,
        kind: crate::KnowledgeDecisionKind, scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<crate::KnowledgeDecisionResult, AgentFailure>>;
    fn run_next<'a>(&'a self, cancellation: Cancellation) -> BoxFuture<'a, Result<bool, AgentFailure>>;
}

pub struct KnowledgeDependencies {
    pub actor: OwnerActor,
    pub repository: Arc<dyn crate::KnowledgeRepository>,
    pub learner_jobs: Arc<dyn crate::LearnerJobRepository>,
    pub journals: Arc<dyn crate::LearnerJournalFactory>,
    pub model: Arc<dyn ModelPort>,
    pub projection: Arc<dyn crate::LearnerProjectionPort>,
    pub clock: Arc<dyn KnowledgeClock>,
    pub learner_budget: crate::LearnerBudget,
}

pub struct KnowledgeService {
    actor: OwnerActor,
    repository: Arc<dyn crate::KnowledgeRepository>,
    learner: Arc<crate::LearnerService>,
    scheduling: crate::LearnerScheduling,
    background: tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
    clock: Arc<dyn KnowledgeClock>,
    closing: AtomicBool,
}
impl KnowledgeService {
    pub fn new(dependencies: KnowledgeDependencies) -> Result<Self, AgentFailure> {
        dependencies.actor.validate()?;
        let learner = crate::LearnerService::new(dependencies.actor.clone(), Arc::clone(&dependencies.repository),
            dependencies.learner_jobs, dependencies.journals, dependencies.model, dependencies.projection,
            Arc::clone(&dependencies.clock), dependencies.learner_budget)?;
        Ok(Self { actor: dependencies.actor, repository: dependencies.repository,
            learner: Arc::new(learner), clock: dependencies.clock, closing: AtomicBool::new(false),
            scheduling: crate::LearnerScheduling::default(), background: tokio::sync::Mutex::new(None) })
    }
    fn authorize(&self, actor: &OwnerActor) -> Result<(), AgentFailure> {
        actor.validate()?;
        if actor != &self.actor || self.closing.load(Ordering::Acquire) { return Err(AgentFailure::PolicyDenied); }
        Ok(())
    }
}
impl KnowledgeRead for KnowledgeService {
    fn read_context<'a>(&'a self, actor: &'a OwnerActor, scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<crate::MemoryContextSnapshot, AgentFailure>>
    { Box::pin(async move { self.authorize(actor)?; self.repository.read_context(actor, self.clock.now(), scope).await }) }
}
impl KnowledgeOwner for KnowledgeService {
    fn activate<'a>(&'a self, scope: &'a ExecutionScope) -> BoxFuture<'a, Result<(), AgentFailure>> {
        Box::pin(async move {
            self.authorize(&self.actor)?;
            let mut background = scope.run(async { Ok(self.background.lock().await) }).await?;
            self.authorize(&self.actor)?;
            if let Some(handle) = background.as_ref() {
                return if handle.is_finished() { Err(AgentFailure::Interrupted) } else { Ok(()) };
            }
            let learner = Arc::clone(&self.learner);
            let scheduling = self.scheduling.clone();
            *background = Some(tokio::spawn(async move { learner.drive_background(scheduling).await }));
            Ok(())
        })
    }
    fn foreground_lease(&self) -> Result<crate::KnowledgeForegroundLease, AgentFailure> {
        self.authorize(&self.actor)?;
        crate::KnowledgeForegroundLease::acquire(self.scheduling.clone())
    }
    fn close_admission(&self) {
        self.closing.store(true, Ordering::Release);
        self.scheduling.close();
        self.learner.close_admission();
    }
    fn shutdown<'a>(&'a self) -> BoxFuture<'a, Result<(), AgentFailure>> {
        Box::pin(async move {
            self.close_admission();
            let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
            tokio::time::timeout_at(deadline, self.learner.shutdown()).await
                .map_err(|_| AgentFailure::DeadlineExceeded)??;
            let mut background = tokio::time::timeout_at(deadline, self.background.lock()).await
                .map_err(|_| AgentFailure::DeadlineExceeded)?;
            if let Some(handle) = background.as_mut() {
                tokio::time::timeout_at(deadline, handle).await
                    .map_err(|_| AgentFailure::DeadlineExceeded)?
                    .map_err(|_| AgentFailure::Interrupted)?;
                *background = None;
            }
            Ok(())
        })
    }
    fn overview<'a>(&'a self, actor: &'a OwnerActor, limit: usize, scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<crate::MemoryOverviewSnapshot, AgentFailure>>
    { Box::pin(async move { self.authorize(actor)?; crate::validate_memory_overview_limit(limit)?;
        self.repository.overview(actor, limit, scope).await }) }
    fn review<'a>(&'a self, actor: &'a OwnerActor, scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<crate::MemoryReviewSnapshot, AgentFailure>>
    { Box::pin(async move { self.authorize(actor)?; self.repository.review(actor, scope).await }) }
    fn decide<'a>(&'a self, actor: &'a OwnerActor, command_id: CommandId, candidate_id: Uuid,
        kind: crate::KnowledgeDecisionKind, scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<crate::KnowledgeDecisionResult, AgentFailure>>
    { Box::pin(async move { self.authorize(actor)?;
        if candidate_id.is_nil() || command_id.as_uuid().is_nil() { return Err(AgentFailure::InvalidInput); }
        self.repository.decide(actor, crate::MemoryDecisionRequest { command_id, candidate_id,
            kind, decided_at: self.clock.now() }, scope).await }) }
    fn run_next<'a>(&'a self, cancellation: Cancellation) -> BoxFuture<'a, Result<bool, AgentFailure>> {
        Box::pin(async move { self.authorize(&self.actor)?; self.learner.run_next(cancellation).await })
    }
}
impl Drop for KnowledgeService { fn drop(&mut self) { self.scheduling.close(); self.learner.close_admission(); } }
