use floe_agent_contract::{AgentFailure, DataClass};
use floe_context::AgentContext;
use floe_experts::{
    A2AArtifact, A2AMessageRole, A2APart, A2ASendMessageRequest, A2ATask, A2ATaskState, AgentCard,
    AgentId, AgentPackage, AgentRegistry, EXPERT_RESULT_MEDIA_TYPE, ExpertBudget, ExpertInput,
    ExpertInvocation, ExpertMetadata, InProcessAgent, PackageImplementation, PackageKind,
    PackageRef, RegistrySnapshot,
};
use floe_experts_builtin::schedule::{
    ExpertHost, ExpertTimelineView, ExpertViews, TimelineViewItem, TimelineViewRead,
};
use floe_kernel::PersonId;
use std::sync::Mutex;
use uuid::Uuid;

pub(crate) struct TestScheduleHost {
    person_id: PersonId,
    instance_id: Uuid,
    assignment_id: Uuid,
    registry: Mutex<AgentRegistry>,
    view: ExpertTimelineView,
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
        let handle = Uuid::new_v4();
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
                    expert: AgentId::try_new(
                        floe_experts_builtin::BuiltinExpertKind::Schedule.package_id(),
                    )
                    .expect("builtin expert ids are valid"),
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
        let tool_assignment = registry.assign(
            registry.revision(),
            person_id,
            tool_installation,
            vec![],
            vec![handle],
        )?;
        let assignment_id = registry.assign(
            registry.revision(),
            person_id,
            expert_installation,
            vec![tool_assignment],
            vec![handle],
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
        let [handle] = assignment.granted_view_handles.as_slice() else {
            return Err(AgentFailure::CapabilityDenied);
        };
        let assignment_id = assignment.id;
        let handle = *handle;
        Ok(Self {
            person_id,
            instance_id,
            assignment_id,
            registry: Mutex::new(registry),
            view: ExpertTimelineView {
                schema_version: 1,
                handle,
                person_id,
                data_class: DataClass::Synthetic,
                source_handle: "fixture.synthetic.timeline".into(),
                range_start_unix_ms: 36_000_000,
                range_end_unix_ms: 43_200_000,
                expires_at_unix_ms: u64::MAX,
                coverage_complete: true,
                next_cursor: None,
                items: vec![TimelineViewItem {
                    evidence_handle: Uuid::from_u128(handle.as_u128() ^ 1),
                    untrusted_title: "Design review".into(),
                    starts_at_unix_ms: 36_000_000,
                    ends_at_unix_ms: 39_600_000,
                }],
            },
        })
    }

    #[cfg(unix)]
    pub(super) fn ensure_snapshot(
        person_id: PersonId,
        mut snapshot: RegistrySnapshot,
    ) -> Result<RegistrySnapshot, AgentFailure> {
        if snapshot.packages.iter().any(|package| {
            matches!(
                package.reference.id.as_str(),
                "floe.timeline.read" | "floe.schedule"
            )
        }) {
            return Ok(snapshot);
        }
        let sample = Self::new_with_instance(person_id, snapshot.instance_id)?.snapshot()?;
        snapshot.packages.extend(sample.packages);
        snapshot.installations.extend(sample.installations);
        snapshot.assignments.extend(sample.assignments);
        snapshot.revision = snapshot
            .revision
            .checked_add(1)
            .ok_or(AgentFailure::BudgetExceeded)?;
        let instance_id = snapshot.instance_id;
        Ok(AgentRegistry::restore(snapshot, instance_id)?.snapshot())
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
            .and_then(|registry| {
                registry
                    .expert_card(
                        person_id,
                        self.assignment_id,
                        registry.revision(),
                        self.view.handle,
                    )
                    .ok()
            })
            .into_iter()
            .collect()
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
        let assignment = request.message.text()?.to_owned();
        let expected_registry_revision = self
            .registry
            .lock()
            .map_err(|_| AgentFailure::CapabilityUnavailable)?
            .revision();
        let assignments = floe_experts::RegistryAssignments::new(&self.registry);
        let result = ExpertHost {
            assignments: &assignments,
            views: self,
        }
        .invoke(ExpertInvocation {
            capabilities: std::sync::Arc::new(NoCapabilityJournal),
            context: AgentContext {
                projection_version: 1,
                persona: None,
                optional_context_issues: vec![],
                memories: vec![],
                evidence: vec![],
            },
            schema_version: request.schema_version,
            invocation_id: task_id,
            instance_id: self.instance_id,
            person_id: request.person_id,
            assignment_id: self.assignment_id,
            expected_registry_revision,
            granted_view_handles: vec![self.view.handle],
            allowed_data_classes: vec![DataClass::Synthetic],
            current_time_unix_ms: self.view.range_start_unix_ms,
            timezone_offset_seconds: 0,
            suggested_range_start_unix_ms: Some(self.view.range_start_unix_ms),
            suggested_range_end_unix_ms: Some(self.view.range_end_unix_ms),
            input: ExpertInput::Analyze {
                request: assignment,
                focus_minutes: Some(60),
            },
            budget: ExpertBudget {
                max_output_bytes: request.max_output_bytes,
                ..ExpertBudget::default()
            },
            deadline: request.deadline,
            cancellation: request.cancellation,
        })
        .await?;
        let data = serde_json::to_string(&result).map_err(|_| AgentFailure::InvalidModelOutput)?;
        Ok(A2ATask {
            id: task_id,
            context_id: request.message.context_id,
            agent_id: request.agent_id,
            state: A2ATaskState::Completed,
            history: vec![request.message],
            artifacts: vec![A2AArtifact {
                artifact_id: Uuid::new_v4(),
                name: "Synthetic schedule result".into(),
                parts: vec![
                    A2APart::Text {
                        text: "The synthetic sample contains a commitment and an available window."
                            .into(),
                    },
                    A2APart::Data {
                        media_type: EXPERT_RESULT_MEDIA_TYPE.into(),
                        data,
                    },
                ],
            }],
            failure: None,
            settlement: None,
        })
    }
}

impl ExpertViews for TestScheduleHost {
    async fn timeline(
        &self,
        request: TimelineViewRead,
    ) -> Result<ExpertTimelineView, AgentFailure> {
        if request.person_id != self.person_id || request.handle != self.view.handle {
            return Err(AgentFailure::CapabilityDenied);
        }
        if self.view.items.len() > request.max_items
            || serde_json::to_vec(&self.view)
                .map_err(|_| AgentFailure::InvalidInput)?
                .len()
                > request.max_bytes
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        Ok(self.view.clone())
    }
}

struct NoCapabilityJournal;

impl floe_agent_contract::CapabilityJournal for NoCapabilityJournal {
    fn record<'a>(
        &'a self,
        _record: floe_agent_contract::CapabilityExecution,
    ) -> floe_agent_contract::BoxFuture<'a, Result<(), AgentFailure>> {
        Box::pin(async { Ok(()) })
    }
}
