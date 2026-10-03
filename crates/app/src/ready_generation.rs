//! Concrete construction and retirement of one encrypted owner generation.
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::time::Duration;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::future::{Future, poll_fn};
use std::task::Poll;
use floe_execution::Cancellation;
use floe_kernel::{AgentFailure, OwnerActor};
use floe_vault::{EncryptedAgentVault, VaultKeyProvider};
use uuid::Uuid;
use crate::{FloeCore, LocalContextHost, ReadyOwners};

pub(crate) struct ReadyGeneration<Keys: VaultKeyProvider> {
    vault: Arc<EncryptedAgentVault<Keys>>,
    core: Arc<FloeCore>,
    owners: Arc<ReadyOwners>,
    retired: AtomicBool,
    generation: u64,
}
struct ActivationGuard<Keys: VaultKeyProvider> {
    vault: Arc<EncryptedAgentVault<Keys>>,
    core: Arc<FloeCore>,
    owners: Option<Arc<ReadyOwners>>,
    armed: bool,
    generation: Option<u64>,
}
impl<Keys: VaultKeyProvider> Drop for ActivationGuard<Keys> {
    fn drop(&mut self) {
        if self.armed {
            let _seal = RetirementSeal(self.vault.as_ref());
            if let Some(owners) = &self.owners { let _ = owners.close_admission(); }
            if let Some(generation) = self.generation {
                let _ = isolate_sync(|| self.core.product_gateway.retire(generation));
            }
        }
    }
}
struct RetirementSeal<'a, Keys: VaultKeyProvider>(&'a EncryptedAgentVault<Keys>);
impl<Keys: VaultKeyProvider> Drop for RetirementSeal<'_, Keys> {
    fn drop(&mut self) { self.0.seal(); }
}
impl<Keys: VaultKeyProvider + 'static> ReadyGeneration<Keys> {
    pub(crate) async fn activate(vault: EncryptedAgentVault<Keys>, core: Arc<FloeCore>,
        local_context: Arc<LocalContextHost>, actor: OwnerActor, operation_id: Uuid,
        cancellation: Cancellation) -> Result<Self, AgentFailure> {
        actor.validate()?;
        if vault.person_id() != actor.person_id { return Err(AgentFailure::PolicyDenied); }
        let vault = Arc::new(vault);
        let mut guard = ActivationGuard { vault: vault.clone(), core: core.clone(), owners: None, armed: true, generation: None };
        let scope = crate::host_scope(operation_id, cancellation.clone(), Duration::from_secs(55));
        // Tasks settle abandoned journals before any parent recovery. Conversation
        // subsequently reloads each exact receipt from its acknowledged intent.
        let task_repository = Arc::new(floe_vault::VaultTaskRepository::new(vault.clone()));
        let (tasks, _) = scope.run(floe_experts::TaskCoordinator::activate(floe_experts::Directory::default(),
            task_repository.clone(), floe_conversation::CONVERSATION_PURPOSE, floe_agent_contract::MAX_OUTPUT_BYTES)).await?;
        let tasks = Arc::new(tasks);
        let activation = scope.run(vault.activate_conversation_executor()).await?;
        let sources = crate::connection_observe::SourceServices::new(vault.clone(), core.clone(), local_context, actor.clone())?;
        core.product_gateway.publish(activation.executor_generation, actor.clone(),
            sources.gateway_credentials.clone(), sources.authorization_signer.clone())?;
        guard.generation = Some(activation.executor_generation);
        let model: Arc<dyn floe_agent_contract::ModelPort> = Arc::new(floe_inference::InferenceService::new(
            floe_provider_adapters::gateway::CompositeModelProvider::new(sources.gateway_credentials.as_ref().clone()),
            sources.dependency_resolver.clone(), sources.gateway_credentials.clone()));
        let memories = Arc::new(floe_vault::VaultKnowledgeRepository::new(vault.clone(), actor.clone())?);
        let learner_projection = Arc::new(floe_context::ContextLearnerProjection::new(actor.clone(),
            memories.clone(), sources.evidence_reader.clone(), sources.dependency_resolver.clone(),
            Arc::new(floe_access::SystemAccessClock))?);
        let knowledge = Arc::new(floe_knowledge::KnowledgeService::new(floe_knowledge::KnowledgeDependencies {
            actor: actor.clone(), repository: memories.clone(), learner_jobs: memories,
            journals: Arc::new(floe_vault::VaultLearnerJournalFactory::new(vault.clone(), actor.clone())?),
            model: model.clone(), projection: learner_projection,
            clock: Arc::new(floe_knowledge::SystemKnowledgeClock), learner_budget: floe_knowledge::LearnerBudget::default(),
        })?);
        let context = Arc::new(floe_context::ExpertContextDependencies {
            actor: actor.clone(), manifests: floe_experts_builtin::manifests(),
            connections: core.store.clone(), grants: vault.clone(), personal: sources.personal.clone(),
            transport: sources.transport.clone(), day: core.store.clone(),
            evidence: sources.evidence_reader.clone(), resolver: sources.dependency_resolver.clone(), leases: core.lease_registry.clone(),
        });
        let experts = Arc::new(floe_experts::ExpertsService::new(floe_experts::ExpertsDependencies {
            actor: actor.clone(),
            registry: Arc::new(floe_vault::VaultExpertRegistryRepository::new(vault.clone(), actor.clone())?),
            binding_reviews: Arc::new(floe_vault::VaultExpertBindingReviewRepository::new(vault.clone(), actor.clone())?),
            candidates: Arc::new(floe_context::ContextCandidateCatalog::new(context.clone())?),
            model: model.clone(),
            sources: Arc::new(floe_context::ContextExpertSources::new(context.clone(), knowledge.clone())?),
            projection: Arc::new(floe_context::ContextExpertProjection::new(context, knowledge.clone())?),
            clock: Arc::new(floe_experts::SystemExpertClock), tasks: tasks.clone(), programs: floe_experts_builtin::registrations(),
        })?);
        let actions = crate::action_facade::build_actions(actor.clone(), vault.clone(), core.store.clone(), core.day.clone(), task_repository)?;
        let budget = floe_conversation::AgentBudget::default();
        let conversation = Arc::new(floe_conversation::ConversationService::new(
            floe_conversation::ConversationDependencies {
                repository: Arc::new(floe_vault::VaultConversationRepository::new(vault.clone())),
                sessions: vault.clone(), experts: tasks, experts_owner: experts.clone(), knowledge: knowledge.clone(),
                model, evidence: sources.evidence_reader.clone(), resolver: sources.dependency_resolver.clone(),
                connections: sources.connections.clone(), runtime_epoch: actor.runtime_epoch,
            }, floe_conversation::ManagerConfig {
                role_spec: floe_conversation::prompts::manager_role_spec(), purpose: floe_conversation::CONVERSATION_PURPOSE.into(),
                max_iterations: budget.max_iterations.min(64), max_output_bytes: budget.max_output_bytes,
                max_run_duration: Duration::from_millis(budget.deadline_ms),
                budget: floe_execution::budget::BudgetConfig::new(budget.max_tokens,budget.max_cost_micros)
                    .with_finalization_reserve(1024,10_000.min(budget.max_cost_micros)),
            })?);
        let owners = Arc::new(ReadyOwners::new(actor.clone(), sources.connections.clone(), conversation,
            experts.clone(), knowledge.clone(), actions));
        guard.owners = Some(owners.clone());
        sources.connections.activate(&actor, &scope).await?;
        experts.activate(&scope).await?;
        owners.actions.activate(&actor, &scope).await?;
        owners.conversation.activate(&actor, &scope).await?;
        owners.knowledge.activate(&scope).await?;
        if cancellation.is_cancelled() { return Err(AgentFailure::Cancelled); }
        vault.check_access()?;
        guard.armed = false;
        Ok(Self { vault, core, owners, retired: AtomicBool::new(false), generation: activation.executor_generation })
    }
    pub(crate) fn owners(&self) -> Arc<ReadyOwners> { self.owners.clone() }
    pub(crate) fn check_access(&self) -> Result<(), AgentFailure> {
        if self.retired.load(Ordering::Acquire) { return Err(AgentFailure::VaultLocked); }
        self.vault.check_access()
    }
    pub(crate) async fn shutdown(&self, operation_id: Uuid) -> Result<(), AgentFailure> {
        let _seal = RetirementSeal(self.vault.as_ref());
        self.retired.store(true, Ordering::Release);
        let close = self.owners.close_admission();
        let gateway = isolate_sync(|| self.core.product_gateway.retire(self.generation));
        let scope = crate::host_scope(operation_id, Cancellation::new(), Duration::from_secs(34));
        let (conversation, connections, actions, experts, knowledge) = tokio::join!(
            drain_owner(|| self.owners.conversation.shutdown(&scope)),
            drain_owner(|| self.owners.connections.shutdown_and_drain(&scope)),
            drain_owner(|| self.owners.actions.shutdown_and_drain(&scope)),
            drain_owner(|| self.owners.experts.shutdown()),
            drain_owner(|| self.owners.knowledge.shutdown()),
        );
        self.vault.seal();
        close.and(gateway).and(conversation).and(connections).and(actions).and(experts).and(knowledge)
    }
}
impl<Keys: VaultKeyProvider> Drop for ReadyGeneration<Keys> {
    fn drop(&mut self) {
        let _seal = RetirementSeal(self.vault.as_ref());
        self.retired.store(true, Ordering::Release);
        let _ = self.owners.close_admission();
        let _ = isolate_sync(|| self.core.product_gateway.retire(self.generation));
    }
}

fn isolate_sync<T>(operation: impl FnOnce() -> Result<T, AgentFailure>) -> Result<T, AgentFailure> {
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(result) => result,
        Err(payload) => { std::mem::forget(payload); Err(AgentFailure::Interrupted) },
    }
}

/// A panic in one owner's drain must not cancel the other owners' drains.
async fn drain_owner<F: Future<Output = Result<(), AgentFailure>>>(start: impl FnOnce() -> F) -> Result<(), AgentFailure> {
    let future = isolate_sync(|| Ok(start()))?;
    let mut future = Box::pin(future);
    poll_fn(|context| {
        match isolate_sync(|| Ok(future.as_mut().poll(context))) {
            Ok(result) => result,
            Err(failure) => Poll::Ready(Err(failure)),
        }
    }).await
}
