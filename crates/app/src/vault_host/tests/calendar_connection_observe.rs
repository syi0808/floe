use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::sync::{Arc, Mutex};

use floe_context::{NativeCalendarSubjectSource, NativeSubjectObservation, NativeSubjectRequest};
use floe_execution::Cancellation;

use super::*;

struct FixtureSubject {
    fingerprints: HashMap<Vec<String>, String>,
    default: String,
    seen: Mutex<Vec<Vec<String>>>,
}

impl NativeCalendarSubjectSource for FixtureSubject {
    async fn subject(
        &self,
        request: NativeSubjectRequest,
    ) -> Result<NativeSubjectObservation, AgentFailure> {
        let mut ids = request.calendar_ids.clone();
        ids.sort();
        self.seen.lock().unwrap().push(ids.clone());
        let before = self
            .fingerprints
            .get(&ids)
            .cloned()
            .unwrap_or_else(|| self.default.clone());
        Ok(NativeSubjectObservation {
            before,
            after: None,
        })
    }
}

struct Fixture {
    runtime: tokio::runtime::Runtime,
    core: Arc<FloeCore>,
    vault: EncryptedAgentVault<Keys>,
    person: PersonId,
    device_id: String,
    subject: FixtureSubject,
    _root: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let core = Arc::new(runtime.block_on(FloeCore::open(":memory:")).unwrap());
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let person = PersonId::new();
        let device_id = "fixture-device".to_owned();
        let vault = runtime
            .block_on(EncryptedAgentVault::create(
                root.path(),
                person,
                Keys::default(),
            ))
            .unwrap();
        runtime
            .block_on(core.source_service().establish(
                person,
                floe_context_contract::ConnectorId::try_new("calendar.event_kit").unwrap(),
                floe_context_contract::ConnectionId::try_new("fixture-connection").unwrap(),
                floe_context_contract::ExecutionOwnerId::try_new(&device_id).unwrap(),
                floe_connections::ResourceMode::Selected,
                vec![floe_connections::ConnectionResource::new(
                floe_context_contract::ResourceHandle::try_new("home").unwrap(),
                "Home".into(),
            )
            .unwrap()],
            ))
            .unwrap();
        runtime.block_on(async {
            vault
                .install_expert_bundle(
                    floe_experts::ExpertInstallOperation {
                        instance_id: vault.registry_instance_id(),
                        expected_revision: 0,
                        operation_id: Uuid::new_v4(),
                    },
                    &floe_experts_builtin::manifests(),
                    Cancellation::default(),
                )
                .await
                .unwrap();
        });
        let subject = FixtureSubject {
            fingerprints: HashMap::from([
                (vec!["home".to_owned()], "a".repeat(64)),
                (vec!["home".to_owned(), "work".to_owned()], "b".repeat(64)),
            ]),
            default: "a".repeat(64),
            seen: Mutex::new(Vec::new()),
        };
        Self {
            runtime,
            core,
            vault,
            person,
            device_id,
            subject,
            _root: root,
        }
    }

    fn connection(&self) -> floe_connections::SourceConnection {
        self.runtime
            .block_on(self.core.source_service().load(
                self.person,
                &floe_context_contract::ConnectionId::try_new("fixture-connection").unwrap(),
            ))
            .unwrap()
            .unwrap()
    }

    fn observe(
        &self,
        operation: &crate::ConnectionObserveOperation,
    ) -> Result<
        (
            Option<crate::ConnectionObserveOverview>,
            Option<crate::ConnectionObserveExpectation>,
        ),
        AgentFailure,
    > {
        self.runtime
            .block_on(super::super::calendar_access::apply_connection_observe(
                &self.core,
                &self.vault,
                &self.subject,
                self.person,
                &self.device_id,
                operation,
                Cancellation::default(),
            ))
    }

    fn reviewed_source(&self) -> floe_connections::SourceConnection {
        let current = self.connection();
        self.runtime
            .block_on(self.core.source_service().configure_reviewed_native(
                self.person,
                current.connection_id(),
                current.revision(),
                current.resource_mode(),
                current.resources().to_vec(),
                "a".repeat(64),
            ))
            .unwrap()
    }

    fn review_observe(&self) -> crate::ConnectionObserveExpectation {
        self.observe(&crate::ConnectionObserveOperation::Review {
            connector_id: "calendar.event_kit".into(),
            connection_id: "fixture-connection".into(),
        })
        .unwrap()
        .1
        .unwrap()
    }

    fn enable_observe(
        &self,
        expected: crate::ConnectionObserveExpectation,
    ) -> Result<crate::ConnectionObserveOverview, AgentFailure> {
        self.observe(&crate::ConnectionObserveOperation::SetEnabled {
            connector_id: "calendar.event_kit".into(),
            connection_id: "fixture-connection".into(),
            enabled: true,
            disconnecting: false,
            expected: Some(expected),
        })
        .map(|(overview, _)| overview.unwrap())
    }
}

#[test]
fn common_observe_pauses_without_changing_calendar_source() {
    let fixture = Fixture::new();
    let source = fixture.reviewed_source();
    let (overview, _) = fixture
        .observe(&crate::ConnectionObserveOperation::Inspect {
            connector_id: "calendar.event_kit".into(),
            connection_id: "fixture-connection".into(),
        })
        .unwrap();
    assert_eq!(
        overview.unwrap().status,
        crate::ConnectionObserveStatus::NeedsReview
    );
    let active = fixture.enable_observe(fixture.review_observe()).unwrap();
    assert_eq!(active.status, crate::ConnectionObserveStatus::Active);
    let grant = fixture
        .runtime
        .block_on(fixture.vault.list_data_access_grants(16))
        .unwrap()
        .remove(0);
    let (paused, _) = fixture
        .observe(&crate::ConnectionObserveOperation::SetEnabled {
            connector_id: "calendar.event_kit".into(),
            connection_id: "fixture-connection".into(),
            enabled: false,
            disconnecting: false,
            expected: None,
        })
        .unwrap();
    assert_eq!(
        paused.unwrap().status,
        crate::ConnectionObserveStatus::Paused
    );
    assert_eq!(fixture.connection(), source);
    let paused_grant = fixture
        .runtime
        .block_on(fixture.vault.list_data_access_grants(16))
        .unwrap()
        .remove(0);
    assert_eq!(paused_grant.id(), grant.id());
}

#[test]
fn common_observe_rejects_stale_grant_source_subject_and_identity() {
    let fixture = Fixture::new();
    let source = fixture.reviewed_source();
    let reviewed = fixture.review_observe();
    let active = fixture.enable_observe(reviewed.clone()).unwrap();
    assert_eq!(active.status, crate::ConnectionObserveStatus::Active);
    assert_eq!(
        fixture.enable_observe(reviewed),
        Err(AgentFailure::AccessReviewRequired)
    );
    let current_review = fixture.review_observe();
    let changed = fixture
        .runtime
        .block_on(fixture.core.source_service().configure_reviewed_native(
            fixture.person,
            source.connection_id(),
            source.revision(),
            source.resource_mode(),
            vec![
            floe_connections::ConnectionResource::new(
                floe_context_contract::ResourceHandle::try_new("work").unwrap(),
                "Work".into(),
            )
            .unwrap(),
        ],
            "a".repeat(64),
        ))
        .unwrap();
    assert_eq!(
        fixture.enable_observe(current_review),
        Err(AgentFailure::AccessReviewRequired)
    );
    let grant = fixture
        .runtime
        .block_on(fixture.vault.list_data_access_grants(16))
        .unwrap()
        .remove(0);
    assert_eq!(grant.state(), floe_access::GrantState::Active);
    assert_eq!(
        changed.source_authority(),
        source.source_authority().advance().unwrap()
    );
    assert_eq!(
        fixture.observe(&crate::ConnectionObserveOperation::Review {
            connector_id: "calendar.event_kit".into(),
            connection_id: "wrong".into(),
        }),
        Err(AgentFailure::AccessReviewRequired)
    );
    let foreign =
        fixture
            .runtime
            .block_on(super::super::calendar_access::apply_connection_observe(
                &fixture.core,
                &fixture.vault,
                &fixture.subject,
                PersonId::new(),
                &fixture.device_id,
                &crate::ConnectionObserveOperation::Inspect {
                    connector_id: "calendar.event_kit".into(),
                    connection_id: "fixture-connection".into(),
                },
                Cancellation::default(),
            ));
    assert_eq!(foreign, Err(AgentFailure::CapabilityDenied));
}

#[test]
fn calendar_connection_observe_conformance_eleven_to_twelve() {
    let fixture = Fixture::new();
    let calendar_ids = (0..11)
        .map(|index| format!("calendar-{index:02}"))
        .collect::<Vec<_>>();
    let initial = fixture.connection();
    let configured = fixture
        .runtime
        .block_on(
            fixture.core.source_service().configure_reviewed_native(
                fixture.person,
                initial.connection_id(),
                initial.revision(),
                floe_connections::ResourceMode::Selected,
                calendar_ids
                    .iter()
                    .map(|calendar_id| {
                        floe_connections::ConnectionResource::new(
                            floe_context_contract::ResourceHandle::try_new(calendar_id).unwrap(),
                            calendar_id.clone(),
                        )
                        .unwrap()
                    })
                    .collect(),
                "a".repeat(64),
            ),
        )
        .unwrap();
    let candidates =
        floe_context::discover_source_candidates(floe_context::SourceCandidateRequest {
            person_id: fixture.person,
            device_id: &fixture.device_id,
            capability: "calendar.timeline",
            contract_version: 1,
            remote_connections: &[],
            remote_execution_owner: None,
            source_connections: std::slice::from_ref(&configured),
        })
        .unwrap();
    assert_eq!(candidates.len(), 1);
    let candidate = &candidates[0];
    assert_eq!(
        candidate.reference.resource.as_str(),
        "calendar.timeline:fixture-connection"
    );
    let registry = fixture
        .runtime
        .block_on(fixture.vault.expert_registry())
        .unwrap()
        .unwrap();
    let schedule = registry
        .installations
        .iter()
        .find(|installation| installation.package.id == "floe.builtin.schedule")
        .unwrap();
    let assignment = registry
        .assignments
        .iter()
        .find(|assignment| assignment.installation_id == schedule.id)
        .unwrap();
    let binding = fixture
        .runtime
        .block_on(fixture.vault.replace_expert_binding(
            Uuid::new_v4(),
            floe_experts::ExpertBindingCommand {
                assignment_id: assignment.id,
                package: schedule.package.clone(),
                definition_revision: 1,
                requirement_key: "floe.source.calendar".into(),
                expected_binding_revision: assignment.binding.revision,
                selected: vec![candidate.reference.clone()],
            },
        ))
        .unwrap();
    let mut extension = floe_experts_builtin::manifests()
        .into_iter()
        .find(|manifest| manifest.package.id == "floe.builtin.schedule")
        .unwrap();
    extension.package.id = "example.calendar.extension".into();
    extension.definition.card.id = extension.package.id.clone();
    extension.publisher = "example".into();
    extension.validate().unwrap();
    let current_registry = fixture
        .runtime
        .block_on(fixture.vault.expert_registry())
        .unwrap()
        .unwrap();
    fixture
        .runtime
        .block_on(fixture.vault.install_expert_bundle(
            floe_experts::ExpertInstallOperation {
                instance_id: fixture.vault.registry_instance_id(),
                expected_revision: current_registry.revision,
                operation_id: Uuid::new_v4(),
            },
            &[extension],
            Cancellation::default(),
        ))
        .unwrap();
    let policy = crate::first_party_observe::calendar_policy().unwrap();
    assert_eq!(
        crate::first_party_observe::member_policy_digest("calendar.event_kit", "calendar.timeline")
            .unwrap(),
        crate::first_party_observe::policy_digest(&policy).unwrap()
    );
    assert!(
        !policy
            .consumers
            .iter()
            .any(|consumer| consumer.identifier() == "example.calendar.extension")
    );
    let inspect = crate::ConnectionObserveOperation::Inspect {
        connector_id: "calendar.event_kit".into(),
        connection_id: configured.connection_id().as_str().into(),
    };
    let review = crate::ConnectionObserveOperation::Review {
        connector_id: "calendar.event_kit".into(),
        connection_id: configured.connection_id().as_str().into(),
    };
    let (overview, _) = fixture.observe(&inspect).unwrap();
    let overview = overview.unwrap();
    assert_eq!(overview.status, crate::ConnectionObserveStatus::NeedsReview);
    assert_eq!(overview.source_resources, calendar_ids);
    let (_, expected) = fixture.observe(&review).unwrap();
    let expected = expected.unwrap();
    assert_eq!(expected.members.len(), 1);
    assert_eq!(expected.members[0].view_id, "calendar.timeline");
    assert_eq!(
        expected.members[0].resource,
        "calendar.timeline:fixture-connection"
    );
    assert_eq!(
        fixture.subject.seen.lock().unwrap().last().unwrap(),
        &calendar_ids
    );
    let enable = |expected| crate::ConnectionObserveOperation::SetEnabled {
        connector_id: "calendar.event_kit".into(),
        connection_id: "fixture-connection".into(),
        enabled: true,
        disconnecting: false,
        expected: Some(expected),
    };
    let (active, _) = fixture.observe(&enable(expected.clone())).unwrap();
    assert_eq!(
        active.unwrap().status,
        crate::ConnectionObserveStatus::Active
    );
    let grants = fixture
        .runtime
        .block_on(fixture.vault.list_data_access_grants(16))
        .unwrap();
    assert_eq!(grants.len(), 1);
    let before_grant = grants[0].clone();
    assert_eq!(before_grant.scope().resources().len(), 1);
    assert_eq!(
        before_grant.scope().resources()[0].as_str(),
        candidate.reference.resource.as_str()
    );
    assert_eq!(before_grant.scope().consumers(), policy.consumers);
    let before_source = fixture.connection();
    let twelve = (0..12)
        .map(|index| format!("calendar-{index:02}"))
        .collect::<Vec<_>>();
    let changed = fixture
        .runtime
        .block_on(
            fixture.core.source_service().configure_reviewed_native(
                fixture.person,
                before_source.connection_id(),
                before_source.revision(),
                floe_connections::ResourceMode::Selected,
                twelve
                    .iter()
                    .map(|calendar_id| {
                        floe_connections::ConnectionResource::new(
                            floe_context_contract::ResourceHandle::try_new(calendar_id).unwrap(),
                            calendar_id.clone(),
                        )
                        .unwrap()
                    })
                    .collect(),
                "a".repeat(64),
            ),
        )
        .unwrap();
    assert_eq!(
        changed.source_authority(),
        before_source.source_authority().advance().unwrap()
    );
    let (active_after_edit, _) = fixture.observe(&inspect).unwrap();
    assert_eq!(
        active_after_edit.unwrap().status,
        crate::ConnectionObserveStatus::Active
    );
    assert_eq!(
        fixture.observe(&enable(expected)),
        Err(AgentFailure::AccessReviewRequired)
    );
    let grants_after = fixture
        .runtime
        .block_on(fixture.vault.list_data_access_grants(16))
        .unwrap();
    assert_eq!(grants_after[0].id(), before_grant.id());
    assert_eq!(grants_after[0].authority(), before_grant.authority());
    let candidates_after =
        floe_context::discover_source_candidates(floe_context::SourceCandidateRequest {
            person_id: fixture.person,
            device_id: &fixture.device_id,
            capability: "calendar.timeline",
            contract_version: 1,
            remote_connections: &[],
            remote_execution_owner: None,
            source_connections: std::slice::from_ref(&changed),
        })
        .unwrap();
    assert_eq!(candidates_after.len(), 1);
    assert_eq!(candidates_after[0].candidate_id, candidate.candidate_id);
    let registry_after = fixture
        .runtime
        .block_on(fixture.vault.expert_registry())
        .unwrap()
        .unwrap();
    let assignment_after = registry_after
        .assignments
        .iter()
        .find(|current| current.id == assignment.id)
        .unwrap();
    assert_eq!(assignment_after.binding.revision, binding.revision);
    let (_, current_review) = fixture.observe(&review).unwrap();
    let (enabled, _) = fixture.observe(&enable(current_review.unwrap())).unwrap();
    assert_eq!(
        enabled.unwrap().status,
        crate::ConnectionObserveStatus::Active
    );
    assert_eq!(
        fixture.subject.seen.lock().unwrap().last().unwrap(),
        &twelve
    );
}
