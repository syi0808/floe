use floe_agent_contract::{
    A2A_PROTOCOL_VERSION, AGENT_VERSION, AgentCard, AgentDefinition, AgentFailure, DataClass,
    ModelPlacement,
};
use floe_experts::{
    A2AMessageRole, A2ASendMessageRequest, A2ATask, A2ATaskState, AgentRegistry, ContractRef,
    EXPERT_MANIFEST_SCHEMA_VERSION, ExpertInstallOperation, ExpertManifest,
    ExpertSourceRequirement, InProcessAgent, PackageKind, PackageRef, RegistrySnapshot,
};
use floe_kernel::PersonId;
use std::sync::Mutex;
use uuid::Uuid;

const TEST_PACKAGE_ID: &str = "example.test.registry-expert";

pub(crate) fn generic_manifest(package_id: &str) -> ExpertManifest {
    let manifest = ExpertManifest {
        schema_version: EXPERT_MANIFEST_SCHEMA_VERSION,
        package: PackageRef {
            kind: PackageKind::Expert,
            id: package_id.into(),
            version: "1.0.0".into(),
        },
        publisher: "example.test".into(),
        definition: AgentDefinition {
            card: AgentCard {
                schema_version: AGENT_VERSION,
                protocol_version: A2A_PROTOCOL_VERSION.into(),
                id: package_id.into(),
                version: "1.0.0".into(),
                name: "Test Registry Expert".into(),
                description: "Exercises generic Expert Registry and Task semantics.".into(),
                domain_tags: vec![],
                skills: vec![],
                supported_placements: vec![ModelPlacement::DeviceLocal],
            },
            definition_revision: 1,
        },
        data_class: DataClass::Personal,
        prompt_contract: ContractRef {
            id: "example.test.registry-prompt".into(),
            revision: 1,
        },
        result_contracts: vec![],
        source_requirements: vec![ExpertSourceRequirement {
            key: "selected_calendar".into(),
            capability: "calendar.timeline".into(),
            contract_version: 1,
            minimum_sources: 1,
            maximum_sources: 1,
        }],
        capability_requirements: vec![],
        state_schema_version: 1,
    };
    manifest.validate().unwrap();
    manifest
}

pub(crate) struct TestExpertRegistryHost {
    person_id: PersonId,
    assignment_id: Uuid,
    registry: Mutex<AgentRegistry>,
}

impl TestExpertRegistryHost {
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
        registry.install_bundle(
            person_id,
            &ExpertInstallOperation {
                instance_id,
                expected_revision: 0,
                operation_id: Uuid::new_v4(),
            },
            &[generic_manifest(TEST_PACKAGE_ID)],
        )?;
        Self::from_snapshot(person_id, registry.snapshot())
    }

    fn from_snapshot(
        person_id: PersonId,
        snapshot: RegistrySnapshot,
    ) -> Result<Self, AgentFailure> {
        let instance_id = snapshot.instance_id;
        let registry = AgentRegistry::restore(snapshot, instance_id)?;
        let snapshot = registry.snapshot();
        let installations: Vec<_> = snapshot
            .installations
            .iter()
            .filter(|entry| entry.package.id == TEST_PACKAGE_ID)
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
        Ok(Self {
            person_id,
            assignment_id: assignment.id,
            registry: Mutex::new(registry),
        })
    }
}

impl InProcessAgent for TestExpertRegistryHost {
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
            || request.agent_id != TEST_PACKAGE_ID
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
            &generic_manifest(TEST_PACKAGE_ID).package,
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
            result: Some("Generic Expert registry result.".into()),
            failure: None,
            settlement: None,
        })
    }
}
