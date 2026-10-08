use crate::{
    BindingReviewRepository, CandidateCatalog, ExpertClock, ExpertProgram, ExpertProjectionPort,
    ExpertRegistration, ExpertSourcePort, RegistryRepository, TaskCoordinator, TaskRepository,
};
use floe_agent_contract::{AgentFailure, ExecutionScope, ModelPort, OwnerActor};
use sha2::{Digest, Sha256};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::{collections::HashMap, sync::Weak};
use tokio::sync::Mutex;
use uuid::Uuid;

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
    pub(crate) closing: AtomicBool,
    pub(crate) operations: tokio::sync::RwLock<()>,
    command_locks: std::sync::Mutex<HashMap<floe_agent_contract::CommandId, Weak<Mutex<()>>>>,
}
impl<Tasks> Drop for ExpertsService<Tasks> {
    fn drop(&mut self) {
        self.closing.store(true, Ordering::Release);
        self.dependencies.tasks.close_admission();
    }
}

impl<Tasks: TaskRepository + 'static> ExpertsService<Tasks> {
    pub fn new(dependencies: ExpertsDependencies<Tasks>) -> Result<Self, AgentFailure> {
        dependencies.actor.validate()?;
        let mut ids = std::collections::HashSet::new();
        if dependencies.programs.len() > 64 {
            return Err(AgentFailure::BudgetExceeded);
        }
        for registration in &dependencies.programs {
            registration.manifest.validate()?;
            if !ids.insert(&registration.manifest.package.id) {
                return Err(AgentFailure::Conflict);
            }
        }
        Ok(Self {
            dependencies,
            closing: AtomicBool::new(false),
            operations: tokio::sync::RwLock::new(()),
            command_locks: std::sync::Mutex::new(HashMap::new()),
        })
    }

    pub fn task_coordinator(&self) -> Arc<TaskCoordinator<Tasks>> {
        Arc::clone(&self.dependencies.tasks)
    }

    pub(crate) fn authorize(&self, actor: &OwnerActor) -> Result<(), AgentFailure> {
        actor.validate()?;
        if actor != &self.dependencies.actor || self.closing.load(Ordering::Acquire) {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(())
    }

    pub(crate) async fn begin_operation<'a>(
        &'a self,
        actor: &OwnerActor,
        scope: &ExecutionScope,
    ) -> Result<tokio::sync::RwLockReadGuard<'a, ()>, AgentFailure> {
        self.authorize(actor)?;
        let guard = scope
            .run(async { Ok(self.operations.read().await) })
            .await?;
        self.authorize(actor)?;
        Ok(guard)
    }

    pub(crate) async fn lock_command(
        &self,
        command_id: floe_agent_contract::CommandId,
    ) -> Result<tokio::sync::OwnedMutexGuard<()>, AgentFailure> {
        if !command_id.is_valid() {
            return Err(AgentFailure::InvalidInput);
        }
        let lock = {
            let mut locks = self
                .command_locks
                .lock()
                .map_err(|_| AgentFailure::Interrupted)?;
            if locks.len() >= 64 {
                locks.retain(|_, lock| lock.strong_count() > 0);
            }
            if let Some(lock) = locks.get(&command_id).and_then(Weak::upgrade) {
                lock
            } else {
                let lock = Arc::new(Mutex::new(()));
                locks.insert(command_id, Arc::downgrade(&lock));
                lock
            }
        };
        Ok(lock.lock_owned().await)
    }

    pub async fn activate(&self, scope: &ExecutionScope) -> Result<(), AgentFailure> {
        let actor = &self.dependencies.actor;
        let _operation = self.begin_operation(actor, scope).await?;
        let mut snapshot = self.dependencies.registry.read(actor, scope).await?;
        let manifests = self
            .dependencies
            .programs
            .iter()
            .map(|program| program.manifest.clone())
            .collect::<Vec<_>>();
        if !manifests.is_empty() {
            let manifest_digest = crate::manifest_set_digest(&manifests)?;
            if !snapshot.install_receipts.iter().any(|receipt| {
                receipt.person_id == actor.person_id && receipt.manifest_digest == manifest_digest
            }) {
                let identity = serde_json::to_vec(&(
                    "floe.experts.activate.bundle.v1",
                    actor.person_id,
                    &actor.device_id,
                    snapshot.instance_id,
                    &manifest_digest,
                ))
                .map_err(|_| AgentFailure::InvalidInput)?;
                let command_id = floe_agent_contract::CommandId::from_uuid(Uuid::new_v5(
                    &snapshot.instance_id,
                    &identity,
                ))
                .ok_or(AgentFailure::InvalidInput)?;
                let request_digest: [u8; 32] = Sha256::digest(&identity).into();
                let mut registry =
                    crate::AgentRegistry::restore(snapshot.clone(), snapshot.instance_id)?;
                registry.install_bundle(
                    actor.person_id,
                    &crate::ExpertInstallOperation {
                        instance_id: snapshot.instance_id,
                        expected_revision: snapshot.revision,
                        operation_id: command_id.as_uuid(),
                    },
                    &manifests,
                )?;
                let next = registry.snapshot();
                self.authorize(actor)?;
                match self
                    .dependencies
                    .registry
                    .commit(
                        crate::RegistryCommit {
                            actor: actor.clone(),
                            command_id,
                            request_digest,
                            expected_revision: snapshot.revision,
                            next: next.clone(),
                        },
                        scope,
                    )
                    .await
                    .map_err(floe_kernel::CommandFailure::into_failure)
                {
                    Ok(receipt) => {
                        if receipt.command_id != command_id
                            || receipt.person_id != actor.person_id
                            || receipt.device_id != actor.device_id
                            || receipt.request_digest != request_digest
                            || receipt.snapshot != next
                        {
                            return Err(AgentFailure::StorageUnavailable);
                        }
                        snapshot = receipt.snapshot;
                    }
                    Err(AgentFailure::Conflict) => {
                        snapshot = self.dependencies.registry.read(actor, scope).await?;
                        if !snapshot.install_receipts.iter().any(|receipt| {
                            receipt.person_id == actor.person_id
                                && receipt.manifest_digest == manifest_digest
                        }) {
                            return Err(AgentFailure::Conflict);
                        }
                    }
                    Err(failure) => return Err(failure),
                }
            }
        }
        self.authorize(actor)?;
        self.republish(&snapshot)
    }

    pub(crate) fn republish(&self, snapshot: &crate::RegistrySnapshot) -> Result<(), AgentFailure> {
        let registry = crate::AgentRegistry::restore(snapshot.clone(), snapshot.instance_id)?;
        let admissions = registry.enabled_expert_admissions(self.dependencies.actor.person_id)?;
        let mut entries = Vec::new();
        for (_, admission) in admissions {
            let registration = self
                .dependencies
                .programs
                .iter()
                .find(|registration| registration.manifest.package == admission.package)
                .ok_or(AgentFailure::CapabilityUnavailable)?;
            let resolved =
                registry.resolve_admitted(self.dependencies.actor.person_id, &admission)?;
            if resolved.manifest != registration.manifest {
                return Err(AgentFailure::Conflict);
            }
            let selection = crate::ExpertExecutionSelection::from_binding(
                &resolved.manifest,
                &resolved.assignment.binding,
            )?;
            let endpoint = crate::EngineExpertEndpoint::new(
                self.dependencies.actor.clone(),
                admission.clone(),
                selection.clone(),
                registration.manifest.clone(),
                Arc::clone(&registration.runner),
                Arc::clone(&self.dependencies.registry),
                Arc::clone(&self.dependencies.model),
                Arc::clone(&self.dependencies.sources),
                Arc::clone(&self.dependencies.projection),
                Arc::clone(&self.dependencies.clock),
            )?;
            entries.push((
                crate::DirectoryEntry {
                    definition: resolved.manifest.definition,
                    admission,
                    selection,
                    reviewed: true,
                    enabled: true,
                    admitted_principals: vec![self.dependencies.actor.person_id.to_string()],
                    purposes: vec!["everyday_assistance".into()],
                },
                Arc::new(endpoint) as Arc<dyn floe_agent_contract::AgentEndpoint>,
            ));
        }
        self.dependencies
            .tasks
            .directory()
            .publish("floe.experts.registry", entries)?;
        Ok(())
    }
}
