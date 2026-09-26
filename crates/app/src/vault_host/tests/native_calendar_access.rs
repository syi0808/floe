use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;

use floe_context::{NativeCalendarSubjectSource, NativeSubjectObservation, NativeSubjectRequest};
use floe_execution::Cancellation;

use super::super::calendar_access::apply_calendar_access;
use super::*;
use crate::{CalendarAccessChange, CalendarAccessOverview, CalendarAccessState, CalendarSelection};

struct FixtureSubject {
    fingerprints: HashMap<Vec<String>, String>,
    default: String,
}

impl NativeCalendarSubjectSource for FixtureSubject {
    async fn subject(
        &self,
        request: NativeSubjectRequest,
    ) -> Result<NativeSubjectObservation, AgentFailure> {
        let mut ids = request.calendar_ids.clone();
        ids.sort();
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
            .block_on(core.set_calendar_scope(
                person,
                "fixture-connection".into(),
                1,
                device_id.clone(),
                floe_context_contract::CalendarProvider::EventKit,
                vec![CalendarSelection {
                    calendar_id: "home".into(),
                    calendar_name: "Home".into(),
                }],
                floe_context_contract::CalendarScope::Selected,
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
            let registry = vault.expert_registry().await.unwrap().unwrap();
            for assignment in &registry.assignments {
                let installation = registry
                    .installations
                    .iter()
                    .find(|entry| entry.id == assignment.installation_id)
                    .unwrap();
                let manifest = registry
                    .manifests
                    .iter()
                    .find(|entry| entry.package == installation.package)
                    .unwrap();
                let Some(requirement) = manifest
                    .source_requirements
                    .iter()
                    .find(|requirement| requirement.capability == "calendar.timeline")
                else {
                    continue;
                };
                vault
                    .replace_expert_binding(
                        Uuid::new_v4(),
                        floe_experts::ExpertBindingCommand {
                            assignment_id: assignment.id,
                            package: installation.package.clone(),
                            definition_revision: manifest.definition.definition_revision,
                            requirement_key: requirement.key.clone(),
                            expected_binding_revision: 1,
                            selected: vec![floe_context_contract::SourceSelectionReference {
                                connector_id: floe_context_contract::ConnectorId::try_new(
                                    "calendar.event_kit",
                                )
                                .unwrap(),
                                connection_id: floe_context_contract::ConnectionId::try_new(
                                    "fixture-connection",
                                )
                                .unwrap(),
                                execution_owner_id:
                                    floe_context_contract::ExecutionOwnerId::try_new(&device_id)
                                        .unwrap(),
                                capability_id: "calendar.timeline".into(),
                                resource: floe_context_contract::ResourceHandle::try_new("home")
                                    .unwrap(),
                                contract_version: 1,
                            }],
                        },
                    )
                    .await
                    .unwrap();
            }
        });
        let subject = FixtureSubject {
            fingerprints: HashMap::from([
                (vec!["home".to_owned()], "a".repeat(64)),
                (vec!["home".to_owned(), "work".to_owned()], "b".repeat(64)),
            ]),
            default: "a".repeat(64),
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

    fn connection(&self) -> floe_day::CalendarConnection {
        self.runtime
            .block_on(self.core.calendar_connection(self.person))
            .unwrap()
            .unwrap()
    }

    fn apply(&self, change: CalendarAccessChange) -> Result<CalendarAccessOverview, AgentFailure> {
        self.runtime.block_on(apply_calendar_access(
            &self.core,
            &self.vault,
            &self.subject,
            self.person,
            self.device_id.clone(),
            change,
            Cancellation::default(),
        ))
    }

    fn review(
        &self,
        overview: &CalendarAccessOverview,
        calendars: &[String],
        fingerprint: &str,
    ) -> Result<CalendarAccessOverview, AgentFailure> {
        self.apply(CalendarAccessChange::Review {
            connection_id: overview.connection_id.clone(),
            calendar_ids: calendars.to_vec(),
            expected_source_authority: overview.source_authority,
            expected_native_subject_fingerprint: fingerprint.to_owned(),
            expected_grant_id: overview.grant_id,
            expected_grant_authority: overview.grant_authority,
        })
    }
}

#[test]
fn connection_projection_inspection_never_creates_a_grant() {
    let fixture = Fixture::new();
    for _ in 0..2 {
        let raw = fixture.apply(CalendarAccessChange::Inspect).unwrap();
        let projected = crate::ConnectionObserveOverview::from_calendar(raw);
        assert_eq!(
            projected.status,
            crate::ConnectionObserveStatus::NeedsReview
        );
        assert!(projected.members.is_empty());
    }
    assert!(
        fixture
            .runtime
            .block_on(fixture.vault.list_data_access_grants(16))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn calendar_review_and_pause_use_current_grant_authority() {
    let fixture = Fixture::new();
    let inspected = fixture.apply(CalendarAccessChange::Inspect).unwrap();
    let active = fixture
        .review(&inspected, &["home".to_owned()], &"a".repeat(64))
        .unwrap();
    assert_eq!(active.state, CalendarAccessState::Active);
    assert_eq!(
        fixture.apply(CalendarAccessChange::Pause {
            grant_id: active.grant_id.unwrap(),
            expected_grant_authority: floe_access::GrantAuthority::new(),
        }),
        Err(AgentFailure::Conflict)
    );
    let paused = fixture
        .apply(CalendarAccessChange::Pause {
            grant_id: active.grant_id.unwrap(),
            expected_grant_authority: active.grant_authority.unwrap(),
        })
        .unwrap();
    assert_eq!(paused.state, CalendarAccessState::Paused);
}

#[test]
fn inspect_connected_calendar_without_grant_needs_review() {
    let fixture = Fixture::new();
    let overview = fixture.apply(CalendarAccessChange::Inspect).unwrap();
    assert_eq!(overview.person_id, fixture.person);
    assert_eq!(overview.connection_id, "fixture-connection");
    assert_eq!(overview.selected_resources, vec!["home".to_owned()]);
    assert!(overview.granted_resources.is_empty());
    assert_eq!(overview.grant_id, None);
    assert_eq!(overview.state, CalendarAccessState::NeedsReview);
    assert!(overview.review_required);
}

#[test]
fn fresh_review_creates_an_active_grant() {
    let fixture = Fixture::new();
    let inspected = fixture.apply(CalendarAccessChange::Inspect).unwrap();
    let overview = fixture
        .review(&inspected, &["home".to_owned()], &"a".repeat(64))
        .unwrap();
    assert_eq!(overview.state, CalendarAccessState::Active);
    assert!(!overview.review_required);
    assert_eq!(overview.granted_resources, vec!["home".to_owned()]);
    assert!(overview.grant_id.is_some());
    assert!(overview.grant_authority.is_some());
    assert!(overview.consumer_policy.is_some());
    let reread = fixture.apply(CalendarAccessChange::Inspect).unwrap();
    assert_eq!(reread.state, CalendarAccessState::Active);
    assert_eq!(reread.grant_id, overview.grant_id);
}

#[test]
fn stale_grant_expectations_conflict() {
    let fixture = Fixture::new();
    let inspected = fixture.apply(CalendarAccessChange::Inspect).unwrap();
    let active = fixture
        .review(&inspected, &["home".to_owned()], &"a".repeat(64))
        .unwrap();
    let paused = fixture
        .apply(CalendarAccessChange::Pause {
            grant_id: active.grant_id.unwrap(),
            expected_grant_authority: active.grant_authority.unwrap(),
        })
        .unwrap();
    assert_eq!(paused.state, CalendarAccessState::Paused);
    assert_eq!(
        fixture.review(&active, &["home".to_owned()], &"a".repeat(64)),
        Err(AgentFailure::Conflict)
    );
    assert_eq!(
        fixture.apply(CalendarAccessChange::Pause {
            grant_id: active.grant_id.unwrap(),
            expected_grant_authority: active.grant_authority.unwrap(),
        }),
        Err(AgentFailure::Conflict)
    );
    assert_eq!(
        fixture.apply(CalendarAccessChange::Remove {
            grant_id: active.grant_id.unwrap(),
            expected_grant_authority: active.grant_authority.unwrap(),
        }),
        Err(AgentFailure::Conflict)
    );
    let reactivated = fixture
        .review(&paused, &["home".to_owned()], &"a".repeat(64))
        .unwrap();
    assert_eq!(reactivated.state, CalendarAccessState::Active);
    assert_eq!(reactivated.grant_id, active.grant_id);
}

#[test]
fn pause_and_remove_leave_the_connection_intact() {
    let fixture = Fixture::new();
    let inspected = fixture.apply(CalendarAccessChange::Inspect).unwrap();
    let before = fixture.connection();
    let active = fixture
        .review(&inspected, &["home".to_owned()], &"a".repeat(64))
        .unwrap();
    let paused = fixture
        .apply(CalendarAccessChange::Pause {
            grant_id: active.grant_id.unwrap(),
            expected_grant_authority: active.grant_authority.unwrap(),
        })
        .unwrap();
    let removed = fixture
        .apply(CalendarAccessChange::Remove {
            grant_id: paused.grant_id.unwrap(),
            expected_grant_authority: paused.grant_authority.unwrap(),
        })
        .unwrap();
    assert_eq!(removed.state, CalendarAccessState::Revoked);
    let after = fixture.connection();
    assert_eq!(after.connection_id, before.connection_id);
    assert_eq!(after.revision, before.revision);
    assert_eq!(after.source_authority, before.source_authority);
    assert!(!after.disconnected);
    assert_eq!(after.calendars, before.calendars);
}

#[test]
fn selection_change_requires_a_fresh_subject_and_authority() {
    let fixture = Fixture::new();
    let inspected = fixture.apply(CalendarAccessChange::Inspect).unwrap();
    let active = fixture
        .review(&inspected, &["home".to_owned()], &"a".repeat(64))
        .unwrap();
    fixture
        .runtime
        .block_on(fixture.core.set_calendar_scope(
            fixture.person,
            "fixture-connection".into(),
            2,
            fixture.device_id.clone(),
            floe_context_contract::CalendarProvider::EventKit,
            vec![
                CalendarSelection {
                    calendar_id: "home".into(),
                    calendar_name: "Home".into(),
                },
                CalendarSelection {
                    calendar_id: "work".into(),
                    calendar_name: "Work".into(),
                },
            ],
            floe_context_contract::CalendarScope::Selected,
        ))
        .unwrap();
    // The old authority and the old subject no longer describe the review.
    assert_eq!(
        fixture.review(
            &active,
            &["home".to_owned(), "work".to_owned()],
            &"a".repeat(64)
        ),
        Err(AgentFailure::StaleContext)
    );
    let current = fixture.apply(CalendarAccessChange::Inspect).unwrap();
    assert_ne!(current.source_authority, active.source_authority);
    assert_eq!(
        current.state,
        CalendarAccessState::NeedsReview,
        "rotated authority selects no grant"
    );
    assert_eq!(
        fixture.review(
            &current,
            &["home".to_owned(), "work".to_owned()],
            &"b".repeat(64)
        ),
        Err(AgentFailure::InvalidInput),
        "a newly added Calendar is not automatically included in existing Expert bindings"
    );
    fixture.runtime.block_on(async {
        let registry = fixture.vault.expert_registry().await.unwrap().unwrap();
        for assignment in &registry.assignments {
            let installation = registry
                .installations
                .iter()
                .find(|entry| entry.id == assignment.installation_id)
                .unwrap();
            let manifest = registry
                .manifests
                .iter()
                .find(|entry| entry.package == installation.package)
                .unwrap();
            let Some(requirement) = manifest
                .source_requirements
                .iter()
                .find(|entry| entry.capability == "calendar.timeline")
            else {
                continue;
            };
            let selected = ["home", "work"]
                .into_iter()
                .map(|resource| floe_context_contract::SourceSelectionReference {
                    connector_id: floe_context_contract::ConnectorId::try_new("calendar.event_kit")
                        .unwrap(),
                    connection_id: floe_context_contract::ConnectionId::try_new(
                        "fixture-connection",
                    )
                    .unwrap(),
                    execution_owner_id: floe_context_contract::ExecutionOwnerId::try_new(
                        &fixture.device_id,
                    )
                    .unwrap(),
                    capability_id: "calendar.timeline".into(),
                    resource: floe_context_contract::ResourceHandle::try_new(resource).unwrap(),
                    contract_version: 1,
                })
                .collect();
            fixture
                .vault
                .replace_expert_binding(
                    Uuid::new_v4(),
                    floe_experts::ExpertBindingCommand {
                        assignment_id: assignment.id,
                        package: installation.package.clone(),
                        definition_revision: manifest.definition.definition_revision,
                        requirement_key: requirement.key.clone(),
                        expected_binding_revision: assignment.binding.revision,
                        selected,
                    },
                )
                .await
                .unwrap();
        }
    });
    let reviewed = fixture
        .review(
            &current,
            &["home".to_owned(), "work".to_owned()],
            &"b".repeat(64),
        )
        .unwrap();
    assert_eq!(reviewed.state, CalendarAccessState::Active);
    assert_eq!(
        reviewed.granted_resources,
        vec!["home".to_owned(), "work".to_owned()]
    );
}

#[test]
fn review_rejects_a_stale_subject_before_mutation() {
    let fixture = Fixture::new();
    let inspected = fixture.apply(CalendarAccessChange::Inspect).unwrap();
    assert_eq!(
        fixture.review(&inspected, &["home".to_owned()], &"c".repeat(64)),
        Err(AgentFailure::AccessReviewRequired)
    );
    let still = fixture.apply(CalendarAccessChange::Inspect).unwrap();
    assert_eq!(still.state, CalendarAccessState::NeedsReview);
}

#[test]
fn review_rejects_malformed_and_mixed_expectations_without_device_io() {
    let fixture = Fixture::new();
    let inspected = fixture.apply(CalendarAccessChange::Inspect).unwrap();
    assert_eq!(
        fixture.apply(CalendarAccessChange::Review {
            connection_id: inspected.connection_id.clone(),
            calendar_ids: vec!["home".to_owned()],
            expected_source_authority: inspected.source_authority,
            expected_native_subject_fingerprint: "not-a-fingerprint".to_owned(),
            expected_grant_id: None,
            expected_grant_authority: None,
        }),
        Err(AgentFailure::InvalidInput)
    );
    assert_eq!(
        fixture.apply(CalendarAccessChange::Review {
            connection_id: inspected.connection_id.clone(),
            calendar_ids: vec!["home".to_owned()],
            expected_source_authority: inspected.source_authority,
            expected_native_subject_fingerprint: "a".repeat(64),
            expected_grant_id: Some(floe_access::GrantId::new()),
            expected_grant_authority: None,
        }),
        Err(AgentFailure::InvalidInput)
    );
    assert_eq!(
        fixture.apply(CalendarAccessChange::Review {
            connection_id: "another-connection".to_owned(),
            calendar_ids: vec!["home".to_owned()],
            expected_source_authority: inspected.source_authority,
            expected_native_subject_fingerprint: "a".repeat(64),
            expected_grant_id: None,
            expected_grant_authority: None,
        }),
        Err(AgentFailure::StaleContext)
    );
}

#[test]
fn registry_revision_is_unchanged_across_calendar_access() {
    let fixture = Fixture::new();
    let expected_revision = fixture
        .runtime
        .block_on(fixture.vault.registry_overview())
        .unwrap()
        .unwrap()
        .revision;
    let inspected = fixture.apply(CalendarAccessChange::Inspect).unwrap();
    let active = fixture
        .review(&inspected, &["home".to_owned()], &"a".repeat(64))
        .unwrap();
    let paused = fixture
        .apply(CalendarAccessChange::Pause {
            grant_id: active.grant_id.unwrap(),
            expected_grant_authority: active.grant_authority.unwrap(),
        })
        .unwrap();
    fixture
        .review(&paused, &["home".to_owned()], &"a".repeat(64))
        .unwrap();
    let overview = fixture
        .runtime
        .block_on(fixture.vault.registry_overview())
        .unwrap()
        .unwrap();
    assert_eq!(overview.revision, expected_revision);
}

#[test]
fn worker_inspect_serves_needs_review_without_device_io() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("vaults");
    let person = PersonId::new();
    let keys = Keys::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let core = Arc::new(runtime.block_on(FloeCore::open(":memory:")).unwrap());
    runtime
        .block_on(core.set_calendar_scope(
            person,
            "fixture-connection".into(),
            1,
            "fixture-device".to_owned(),
            floe_context_contract::CalendarProvider::EventKit,
            vec![CalendarSelection {
                calendar_id: "home".into(),
                calendar_name: "Home".into(),
            }],
            floe_context_contract::CalendarScope::Selected,
        ))
        .unwrap();
    let worker = Worker::with_core_and_connection_store(
        root,
        keys,
        core,
        Arc::new(LocalContextHost::default()),
        Arc::new(crate::events::AppEventBuffer::default()),
        floe_provider_adapters::control::CurrentSavedConnectionStore::fixed(None),
    )
    .unwrap();
    assert_eq!(perform(&worker, person, WorkerAction::Create).failure, None);
    let caller = remote_caller(person, "fixture-device");
    let operation_id = Uuid::new_v4();
    worker
        .local_request(
            &caller,
            operation_id,
            Some(
                crate::local_operations::LocalOperationIntent::LocalAccessInspection(
                    crate::LocalAccessInspection::CalendarAccess,
                ),
            ),
            crate::local_operations::LocalOperationOwner::Access,
            false,
        )
        .unwrap();
    let mut inspected = None;
    for _ in 0..100 {
        let polled = worker
            .local_request(
                &caller,
                operation_id,
                None,
                crate::local_operations::LocalOperationOwner::Access,
                false,
            )
            .unwrap();
        if polled.done {
            inspected = Some(polled);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let inspected = inspected.expect("calendar inspect completes");
    assert_eq!(inspected.failure, None);
    assert_eq!(inspected.stage, "calendar_access");
    let overview = inspected.calendar_access.unwrap();
    assert_eq!(overview.state, CalendarAccessState::NeedsReview);
    assert_eq!(overview.connection_id, "fixture-connection");
    worker
        .local_request(
            &caller,
            operation_id,
            None,
            crate::local_operations::LocalOperationOwner::Access,
            true,
        )
        .unwrap();
    let malformed = perform(
        &worker,
        person,
        WorkerAction::CalendarAccess {
            change: Box::new(crate::CalendarAccessConfiguration {
                device_id: "fixture-device".into(),
                change: CalendarAccessChange::Review {
                    connection_id: "fixture-connection".into(),
                    calendar_ids: vec!["home".into()],
                    expected_source_authority: overview.source_authority,
                    expected_native_subject_fingerprint: "not-a-fingerprint".into(),
                    expected_grant_id: None,
                    expected_grant_authority: None,
                },
            }),
        },
    );
    assert_eq!(malformed.failure, Some(AgentFailure::InvalidInput));
}
