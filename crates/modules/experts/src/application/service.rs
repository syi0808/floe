use std::sync::Arc;
use floe_agent_contract::{AgentFailure, ExecutionScope, ModelPort, OwnerActor};
use crate::{BindingReviewRepository, CandidateCatalog, ExpertClock, ExpertProgram,
    ExpertProjectionPort, ExpertRegistration, ExpertSourcePort, RegistryRepository,
    TaskCoordinator, TaskRepository};

pub struct ExpertsDependencies<Tasks> {
    pub actor: OwnerActor,
    pub registry: Arc<dyn RegistryRepository>,
    pub binding_reviews: Arc<dyn BindingReviewRepository>,
    pub candidates: Arc<dyn CandidateCatalog>,
    pub model: Arc<dyn ModelPort>,
    pub sources: Arc<dyn ExpertSourcePort>,
    pub projection: Arc<dyn ExpertProjectionPort>,
    pub clock: Arc<dyn ExpertClock>,
    pub tasks: Arc<TaskCoordinator<Tasks>>,
    pub programs: Vec<ExpertRegistration<Arc<dyn ExpertProgram>>>,
}

pub struct ExpertsService<Tasks> {
    pub(crate) dependencies: ExpertsDependencies<Tasks>,
}
impl<Tasks> Drop for ExpertsService<Tasks> {
    fn drop(&mut self) { self.dependencies.tasks.close_admission(); }
}

impl<Tasks: TaskRepository + 'static> ExpertsService<Tasks> {
    pub fn new(dependencies: ExpertsDependencies<Tasks>) -> Result<Self, AgentFailure> {
        dependencies.actor.validate()?;
        let mut ids = std::collections::HashSet::new();
        if dependencies.programs.len() > 64 { return Err(AgentFailure::BudgetExceeded); }
        for registration in &dependencies.programs {
            registration.manifest.validate()?;
            if !ids.insert(&registration.manifest.package.id) { return Err(AgentFailure::Conflict); }
        }
        Ok(Self { dependencies })
    }

    pub fn task_coordinator(&self) -> Arc<TaskCoordinator<Tasks>> { Arc::clone(&self.dependencies.tasks) }

    pub(crate) fn authorize(&self, actor: &OwnerActor) -> Result<(), AgentFailure> {
        actor.validate()?;
        if actor != &self.dependencies.actor { return Err(AgentFailure::PolicyDenied); }
        Ok(())
    }

    pub async fn activate(&self, scope: &ExecutionScope) -> Result<(), AgentFailure> {
        let snapshot = self.dependencies.registry.read(&self.dependencies.actor, scope).await?;
        self.republish(&snapshot)
    }

    pub(crate) fn republish(&self, snapshot: &crate::RegistrySnapshot) -> Result<(), AgentFailure> {
        let registry = crate::AgentRegistry::restore(snapshot.clone(), snapshot.instance_id)?;
        let admissions = registry.enabled_expert_admissions(self.dependencies.actor.person_id)?;
        let mut entries = Vec::new();
        for (_, admission) in admissions {
            let registration = self.dependencies.programs.iter()
                .find(|registration| registration.manifest.package == admission.package)
                .ok_or(AgentFailure::CapabilityUnavailable)?;
            let resolved = registry.resolve_admitted(self.dependencies.actor.person_id, &admission)?;
            if resolved.manifest != registration.manifest { return Err(AgentFailure::Conflict); }
            let selection = crate::ExpertExecutionSelection::from_binding(&resolved.manifest, &resolved.assignment.binding)?;
            let endpoint = crate::EngineExpertEndpoint::new(
                self.dependencies.actor.clone(), admission.clone(), selection.clone(),
                registration.manifest.clone(), Arc::clone(&registration.runner),
                Arc::clone(&self.dependencies.registry), Arc::clone(&self.dependencies.model),
                Arc::clone(&self.dependencies.sources), Arc::clone(&self.dependencies.projection),
                Arc::clone(&self.dependencies.clock),
            )?;
            entries.push((crate::DirectoryEntry {
                definition: resolved.manifest.definition,
                admission, selection, reviewed: true, enabled: true,
                admitted_principals: vec![self.dependencies.actor.person_id.to_string()],
                purposes: vec!["everyday_assistance".into()],
            }, Arc::new(endpoint) as Arc<dyn floe_agent_contract::AgentEndpoint>));
        }
        self.dependencies.tasks.directory().publish("floe.experts.registry", entries)?;
        Ok(())
    }
}
