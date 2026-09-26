use floe_agent_contract::AgentFailure;
use floe_experts::{
    A2AMessageRole, A2ASendMessageRequest, A2ATask, A2ATaskState, AgentCard,
    AgentRegistry, ExpertInstallOperation, InProcessAgent, PackageKind, PackageRef,
    RegistrySnapshot,
};
use floe_kernel::PersonId;
use std::sync::Mutex;
use uuid::Uuid;

pub(crate) struct TestScheduleHost {
    person_id: PersonId,
    assignment_id: Uuid,
    registry: Mutex<AgentRegistry>,
}

impl TestScheduleHost {
    #[cfg(unix)]
    pub(crate) fn snapshot(&self) -> Result<RegistrySnapshot, AgentFailure> {
        Ok(self
            .registry
            .lock()
            .map_err(|_| AgentFailure::StorageUnavailable)?
            .snapshot())
    }

    pub(crate) fn new_with_instance(
        person_id: PersonId,
        instance_id: Uuid,
    ) -> Result<Self, AgentFailure> {
        let mut registry = AgentRegistry::new(instance_id);
        let mut manifest = floe_experts_builtin::manifests()
            .into_iter()
            .find(|manifest| manifest.package.id == "floe.builtin.schedule")
            .ok_or(AgentFailure::NotFound)?;
        manifest.package.id = "floe.schedule".into();
        manifest.definition.card.id = manifest.package.id.clone();
        registry.install_bundle(person_id, &ExpertInstallOperation {
            instance_id,
            expected_revision: 0,
            operation_id: Uuid::new_v4(),
        }, &[manifest])?;
        Self::from_snapshot(person_id, registry.snapshot())
    }

    pub(crate) fn from_snapshot(
        person_id: PersonId,
        snapshot: RegistrySnapshot,
    ) -> Result<Self, AgentFailure> {
        let instance_id = snapshot.instance_id;
        let registry = AgentRegistry::restore(snapshot, instance_id)?;
        let snapshot = registry.snapshot();
        let installations: Vec<_> = snapshot
            .installations
            .iter()
            .filter(|entry| {
                entry.package.kind == PackageKind::Expert
                    && entry.package.id == "floe.schedule"
                    && entry.package.version == "1.0.0"
            })
            .map(|entry| entry.id)
            .collect();
        let assignments: Vec<_> = snapshot
            .assignments
            .iter()
            .filter(|entry| {
                entry.person_id == person_id && installations.contains(&entry.installation_id)
            })
            .collect();
        let [assignment] = assignments.as_slice() else {
            return Err(AgentFailure::Conflict);
        };
        let assignment_id = assignment.id;
        Ok(Self {
            person_id,
            assignment_id,
            registry: Mutex::new(registry),
        })
    }

}

impl InProcessAgent for TestScheduleHost {
    fn agent_cards(&self, person_id: PersonId) -> Vec<AgentCard> {
        if person_id != self.person_id {
            return vec![];
        }
        self.registry
            .lock()
            .ok()
            .and_then(|registry| registry.enabled_expert_cards(person_id).ok())
            .unwrap_or_default()
    }

    async fn handle_message(
        &self,
        request: A2ASendMessageRequest,
    ) -> Result<A2ATask, AgentFailure> {
        if request.person_id != self.person_id
            || request.agent_id != "floe.schedule"
            || request.message.role != A2AMessageRole::User
        {
            return Err(AgentFailure::CapabilityDenied);
        }
        let task_id = request.message.task_id.ok_or(AgentFailure::InvalidInput)?;
        request.message.text()?;
        let mut registry = self
            .registry
            .lock()
            .map_err(|_| AgentFailure::CapabilityUnavailable)?;
        let resolved = registry.resolve_assignment(
            registry.instance_id(),
            self.person_id,
            self.assignment_id,
            &PackageRef { kind: PackageKind::Expert, id: "floe.schedule".into(), version: "1.0.0".into() },
            1,
        )?;
        registry.complete(&resolved, task_id)?;
        Ok(A2ATask {
            id: task_id,
            context_id: request.message.context_id,
            agent_id: request.agent_id,
            state: A2ATaskState::Completed,
            history: vec![request.message],
            artifacts: vec![],
            result: Some("The synthetic sample contains a commitment and an available window.".into()),
            failure: None,
            settlement: None,
        })
    }
}
