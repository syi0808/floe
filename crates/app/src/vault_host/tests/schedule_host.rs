use floe_agent_contract::{AgentFailure, DataClass};
use floe_experts::{
    A2AMessageRole, A2ASendMessageRequest, A2ATask, A2ATaskState, AgentCard, AgentId,
    AgentPackage, AgentRegistry, ExpertMetadata, InProcessAgent, PackageImplementation,
    PackageKind, PackageRef, RegistrySnapshot,
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
        let tool = PackageRef {
            kind: PackageKind::Tool,
            id: "floe.timeline.read".into(),
            version: "1.0.0".into(),
        };
        let expert = PackageRef {
            kind: PackageKind::Expert,
            id: "floe.schedule".into(),
            version: "1.0.0".into(),
        };
        registry.register(
            registry.revision(),
            AgentPackage {
                schema_version: 1,
                reference: tool.clone(),
                publisher: "floe".into(),
                implementation: PackageImplementation::TimelineRead {
                    data_class: DataClass::Synthetic,
                },
                expert_metadata: None,
                required_tools: vec![],
                state_schema_version: 1,
            },
        )?;
        registry.register(
            registry.revision(),
            AgentPackage {
                schema_version: 1,
                reference: expert.clone(),
                publisher: "floe".into(),
                implementation: PackageImplementation::Builtin {
                    expert: AgentId::try_new("floe.schedule").expect("fixture ids are valid"),
                },
                expert_metadata: Some(ExpertMetadata {
                    name: "Schedule Expert".into(),
                    description: "Reviews calendars, availability, conflicts, and the realism of plans from a scheduling perspective.".into(),
                    domain_tags: vec!["schedule".into(), "calendar".into()],
                    skills: vec!["Provide independent scheduling judgment".into()],
                    supported_placements: floe_experts_builtin::BuiltinExpertKind::Schedule
                        .declaration()
                        .supported_placements,
                }),
                required_tools: vec![tool.clone()],
                state_schema_version: 1,
            },
        )?;
        let tool_installation = registry.install(registry.revision(), &tool)?;
        let expert_installation = registry.install(registry.revision(), &expert)?;
        let tool_assignment =
            registry.assign(registry.revision(), person_id, tool_installation, vec![])?;
        let assignment_id = registry.assign(
            registry.revision(),
            person_id,
            expert_installation,
            vec![tool_assignment],
        )?;
        for installation in [tool_installation, expert_installation] {
            registry.set_installation_enabled(registry.revision(), installation, true)?;
        }
        for assignment in [tool_assignment, assignment_id] {
            registry.set_assignment_enabled(registry.revision(), person_id, assignment, true)?;
        }
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
            .map(|registry| registry.enabled_expert_cards(person_id))
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
        let expected = AgentId::try_new("floe.schedule").unwrap();
        let resolved = registry.resolve_builtin(
            registry.instance_id(),
            self.person_id,
            self.assignment_id,
            registry.revision(),
            &expected,
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
