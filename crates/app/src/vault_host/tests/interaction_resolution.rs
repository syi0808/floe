use std::collections::VecDeque;
use std::num::NonZeroU64;
use std::os::unix::fs::PermissionsExt;
use std::sync::{Arc, Mutex};

use floe_agent_contract::{BoxFuture, JournalEvent, ToolCall};
use floe_kernel::RunId;

use super::*;
use crate::vault_host::interaction_resolution::{
    DriftReason, InlineOwnerMutation, LiveGrant, LiveInlineState, LiveMember, ObserveStateReader,
    RefreshInteractionCommand, RefreshOutcome, ResolveInteractionCommand, ResolveOutcome,
    refresh_interaction, resolve_interaction,
};

const NOW: i64 = 1_700_000_000_000;
const DEVICE: &str = "device";

struct Fixture {
    runs: FakeRuns,
    repo: floe_vault::VaultConversationRepository<Keys>,
    person: PersonId,
    session_id: Uuid,
    run_id: RunId,
    caller: crate::CallerContext,
    cancellation: floe_execution::Cancellation,
    _root: tempfile::TempDir,
}

impl Fixture {
    async fn open() -> Self {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let person = PersonId::new();
        let vault = EncryptedAgentVault::create(root.path(), person, Keys::default())
            .await
            .unwrap();
        vault.activate_conversation_executor().await.unwrap();
        let session = vault.create_session().await.unwrap();
        let session_id = session.id;
        let run_id = RunId::new();
        // The vault repository verifies the origin Run row independently of
        // the Runs port, so the origin must exist in both.
        let admission = vault
            .admit_conversation_turn(floe_vault::VaultConversationAdmissionRequest {
                run_id,
                command_id: floe_agent_contract::CommandId::new(),
                session_id,
                person_id: person,
                expected_session_revision: 0,
                request_digest: [7; 32],
                text: "hello".into(),
                continuation: None,
                retry_of: None,
                profile: floe_conversation::ProfileSelection::Auto,
            })
            .await
            .unwrap();
        assert!(matches!(
            admission,
            floe_vault::VaultConversationAdmission::Created { .. }
        ));
        let repo = floe_vault::VaultConversationRepository::new(Arc::new(vault));
        let receipt = floe_conversation::RunReceipt {
            run_id,
            command_id: floe_agent_contract::CommandId::new(),
            session_id,
            principal: person.to_string(),
            request_digest: [7; 32],
            state: floe_conversation::RunState::Completed,
            output: None,
            coverage: floe_agent_contract::DependencyCoverage::Independent,
            issue: None,
            session_revision: 1,
            aggregate_revision: 1,
            executor_generation: 1,
            continuation_of: None,
            continuation_executor_generation: None,
            continuation_level: 0,
            retry_of: None,
            profile: floe_conversation::ProfileSelection::Auto,
            attempt_refs: vec![],
            task_refs: vec![],
        };
        let runs = FakeRuns {
            receipt,
            journal: Mutex::new(Vec::new()),
        };
        let caller = crate::CallerContext::verified(
            crate::LocalIdentityClaim {
                person_id: person.0,
                device_id: DEVICE.into(),
            },
            1,
        )
        .unwrap();
        Self {
            runs,
            repo,
            person,
            session_id,
            run_id,
            caller,
            cancellation: floe_execution::Cancellation::default(),
            _root: root,
        }
    }

    fn principal(&self) -> String {
        self.person.to_string()
    }

    /// A fresh admitted Tool origin per seed: identical requirements under
    /// distinct origins publish distinct interactions.
    fn fresh_call(&self) -> ToolCall {
        let call = ToolCall {
            call_id: Uuid::new_v4(),
            invocation_key: floe_agent_contract::InvocationKey::new(),
            tool_id: "calendar.observe".into(),
            definition_revision: 1,
            input: "{}".into(),
        };
        let mut journal = self.runs.journal.lock().unwrap();
        let revision = journal.len() as u64 + 1;
        journal.push(floe_conversation::JournalEntry {
            revision,
            event: JournalEvent::ToolIntent { call: call.clone() },
        });
        call
    }

    async fn seed_inline(
        &self,
        target: floe_conversation::InlineObserveTarget,
    ) -> floe_conversation::ConversationInteraction {
        let call = self.fresh_call();
        let admission = floe_conversation::publish_interaction(
            &self.runs,
            &self.repo,
            floe_conversation::PublishInteractionRequest {
                principal: self.principal(),
                session_id: self.session_id,
                origin_run_id: self.run_id,
                origin: floe_conversation::InteractionOrigin::Tool {
                    call_id: call.call_id,
                },
                kind: floe_agent_contract::UserInteractionKind::SourceAccess,
                requirement: floe_conversation::InteractionRequirement {
                    kind: floe_conversation::InteractionRequirementKind::EnableObserve,
                    source_id: "floe.source.calendar".into(),
                    connection_id: Some("connection".into()),
                    consumer: "floe.builtin.schedule".into(),
                    purpose: "scheduling".into(),
                    inline: true,
                },
                target: floe_conversation::ReviewedTarget::InlineObserve(target),
            },
            NOW,
        )
        .await
        .unwrap();
        match admission {
            floe_conversation::PublishAdmission::Created(record) => record,
            floe_conversation::PublishAdmission::Existing(record) => record,
        }
    }

    async fn seed_navigation(&self) -> floe_conversation::ConversationInteraction {
        let call = self.fresh_call();
        let admission = floe_conversation::publish_interaction(
            &self.runs,
            &self.repo,
            floe_conversation::PublishInteractionRequest {
                principal: self.principal(),
                session_id: self.session_id,
                origin_run_id: self.run_id,
                origin: floe_conversation::InteractionOrigin::Tool {
                    call_id: call.call_id,
                },
                kind: floe_agent_contract::UserInteractionKind::SourceAccess,
                requirement: floe_conversation::InteractionRequirement {
                    kind: floe_conversation::InteractionRequirementKind::Reconnect,
                    source_id: "floe.source.calendar".into(),
                    connection_id: Some("connection".into()),
                    consumer: "floe.builtin.schedule".into(),
                    purpose: "scheduling".into(),
                    inline: false,
                },
                target: floe_conversation::ReviewedTarget::NavigationOnly(
                    floe_conversation::NavigationOnlyTarget {
                        destination: floe_conversation::NavigationDestination::ConnectionSettings,
                        source_id: "floe.source.calendar".into(),
                        connection_id: Some("connection".into()),
                        consumer: "floe.builtin.schedule".into(),
                        purpose: "scheduling".into(),
                    },
                ),
            },
            NOW,
        )
        .await
        .unwrap();
        match admission {
            floe_conversation::PublishAdmission::Created(record) => record,
            floe_conversation::PublishAdmission::Existing(record) => record,
        }
    }

    fn resolve_command(
        &self,
        current: &floe_conversation::ConversationInteraction,
        kind: floe_conversation::InteractionDecisionKind,
    ) -> ResolveInteractionCommand {
        ResolveInteractionCommand {
            interaction_id: current.id,
            command_id: Uuid::new_v4(),
            session_id: self.session_id,
            expected_revision: current.revision,
            kind,
            target_digest: current.target_digest,
        }
    }

    fn refresh_command(
        &self,
        current: &floe_conversation::ConversationInteraction,
    ) -> RefreshInteractionCommand {
        RefreshInteractionCommand {
            interaction_id: current.id,
            command_id: Uuid::new_v4(),
            session_id: self.session_id,
            expected_revision: current.revision,
        }
    }
}

struct FakeRuns {
    receipt: floe_conversation::RunReceipt,
    journal: Mutex<Vec<floe_conversation::JournalEntry>>,
}

impl floe_conversation::ConversationRepository for FakeRuns {
    fn find_command<'a>(
        &'a self,
        _: floe_conversation::CommandQuery,
    ) -> BoxFuture<'a, Result<Option<floe_conversation::RunReceipt>, AgentFailure>> {
        Box::pin(async { unimplemented!("resolution needs no command lookup") })
    }

    fn admit_turn<'a>(
        &'a self,
        _: floe_conversation::TurnAdmissionRequest,
    ) -> BoxFuture<'a, Result<floe_conversation::TurnAdmission, AgentFailure>> {
        Box::pin(async { unimplemented!("resolution needs no admission") })
    }

    fn admit_cancel<'a>(
        &'a self,
        _: floe_conversation::CancelRunCommand,
    ) -> BoxFuture<'a, Result<floe_conversation::CancelRunAdmission, AgentFailure>> {
        Box::pin(async { unimplemented!("resolution needs no cancellation") })
    }

    fn journal(
        &self,
        _: RunId,
    ) -> Result<Arc<dyn floe_agent_contract::ExecutionJournal>, AgentFailure> {
        unimplemented!("resolution needs no journal handle")
    }

    fn finish_run<'a>(
        &'a self,
        _: RunId,
        _: u64,
        _: floe_conversation::RunTerminal,
    ) -> BoxFuture<'a, Result<floe_conversation::RunReceipt, AgentFailure>> {
        Box::pin(async { unimplemented!("resolution never finishes runs") })
    }

    fn load_run<'a>(
        &'a self,
        _: RunId,
    ) -> BoxFuture<'a, Result<Option<floe_conversation::AdmittedTurn>, AgentFailure>> {
        Box::pin(async { unimplemented!("resolution needs no turn load") })
    }

    fn load_receipt<'a>(
        &'a self,
        run_id: RunId,
    ) -> BoxFuture<'a, Result<Option<floe_conversation::RunReceipt>, AgentFailure>> {
        let receipt = (run_id == self.receipt.run_id).then(|| self.receipt.clone());
        Box::pin(async move { Ok(receipt) })
    }

    fn recover_session<'a>(
        &'a self,
        _: floe_conversation::RecoveryRequest,
    ) -> BoxFuture<'a, Result<floe_conversation::RecoveryReceipt, AgentFailure>> {
        Box::pin(async { unimplemented!("resolution needs no recovery") })
    }

    fn load_journal<'a>(
        &'a self,
        run_id: RunId,
    ) -> BoxFuture<'a, Result<Vec<floe_conversation::JournalEntry>, AgentFailure>> {
        let journal = if run_id == self.receipt.run_id {
            self.journal.lock().unwrap().clone()
        } else {
            vec![]
        };
        Box::pin(async move { Ok(journal) })
    }
}

struct Script {
    live: LiveInlineState,
    read_failures: VecDeque<AgentFailure>,
    mutation_results: VecDeque<Result<(), AgentFailure>>,
    reads: usize,
    mutations: Vec<Uuid>,
    nav_usable: bool,
    nav_satisfied: bool,
}

#[derive(Clone)]
struct ScriptedOwners {
    script: Arc<Mutex<Script>>,
}

impl ScriptedOwners {
    fn new(live: LiveInlineState) -> Self {
        Self {
            script: Arc::new(Mutex::new(Script {
                live,
                read_failures: VecDeque::new(),
                mutation_results: VecDeque::new(),
                reads: 0,
                mutations: Vec::new(),
                nav_usable: true,
                nav_satisfied: false,
            })),
        }
    }
}

impl ObserveStateReader for ScriptedOwners {
    fn read_live_inline<'a>(
        &'a self,
        _: &'a floe_conversation::InlineObserveTarget,
        _: PersonId,
        _: &'a str,
        _: &'a floe_execution::Cancellation,
    ) -> BoxFuture<'a, Result<LiveInlineState, AgentFailure>> {
        let mut script = self.script.lock().unwrap();
        script.reads += 1;
        let result = match script.read_failures.pop_front() {
            Some(failure) => Err(failure),
            None => Ok(script.live.clone()),
        };
        Box::pin(async move { result })
    }

    fn navigation_connection_usable<'a>(
        &'a self,
        _: &'a floe_conversation::NavigationOnlyTarget,
        _: PersonId,
    ) -> BoxFuture<'a, Result<bool, AgentFailure>> {
        let usable = self.script.lock().unwrap().nav_usable;
        Box::pin(async move { Ok(usable) })
    }

    fn navigation_satisfied<'a>(
        &'a self,
        _: &'a floe_conversation::NavigationOnlyTarget,
        _: PersonId,
        _: &'a str,
        _: &'a floe_execution::Cancellation,
    ) -> BoxFuture<'a, Result<bool, AgentFailure>> {
        let satisfied = self.script.lock().unwrap().nav_satisfied;
        Box::pin(async move { Ok(satisfied) })
    }
}

impl InlineOwnerMutation for ScriptedOwners {
    fn enable_reviewed<'a>(
        &'a self,
        _: &'a floe_conversation::InlineObserveTarget,
        _: PersonId,
        _: &'a str,
        operation_id: Uuid,
        _: &'a floe_execution::Cancellation,
    ) -> BoxFuture<'a, Result<(), AgentFailure>> {
        let mut script = self.script.lock().unwrap();
        script.mutations.push(operation_id);
        let result = script.mutation_results.pop_front().unwrap_or(Ok(()));
        Box::pin(async move { result })
    }
}

fn grant_pair() -> (floe_access::GrantId, floe_access::GrantAuthority) {
    (
        floe_access::GrantId::from_uuid(Uuid::new_v4()).unwrap(),
        floe_access::GrantAuthority::from_parts(Uuid::new_v4(), NonZeroU64::new(7).unwrap())
            .unwrap(),
    )
}

fn reviewed_target() -> floe_conversation::InlineObserveTarget {
    floe_conversation::InlineObserveTarget {
        connection_id: "connection".into(),
        device_id: Some(DEVICE.into()),
        source_id: "floe.source.calendar".into(),
        connector_id: Some("calendar.macos".into()),
        consumer: "floe.builtin.schedule".into(),
        purpose: "scheduling".into(),
        connection_revision: Some(9),
        reviewed_producer_fingerprint: None,
        reviewed_native_subject: Some("subject".into()),
        members: vec![floe_conversation::ReviewedBundleMember {
            member_id: "calendar.timeline".into(),
            resource: "personal".into(),
            source_revision: None,
            expected_grant: floe_conversation::ExpectedGrantState::Absent,
            policy_authority: None,
        }],
    }
}

fn live_precondition() -> LiveInlineState {
    LiveInlineState {
        members: vec![LiveMember {
            member_id: "calendar.timeline".into(),
            resource: "personal".into(),
            source_revision: None,
            live_grants: vec![],
            policy_authority: None,
        }],
        connection_revision: Some(9),
        producer_fingerprint: None,
        native_subject: Some("subject".into()),
        connection_usable: true,
    }
}

fn live_satisfied() -> LiveInlineState {
    let (id, authority) = grant_pair();
    LiveInlineState {
        members: vec![LiveMember {
            member_id: "calendar.timeline".into(),
            resource: "personal".into(),
            source_revision: None,
            live_grants: vec![LiveGrant { id, authority }],
            policy_authority: None,
        }],
        connection_revision: Some(9),
        producer_fingerprint: None,
        native_subject: Some("subject".into()),
        connection_usable: true,
    }
}

fn owner_operation_id(command_id: Uuid) -> Uuid {
    floe_conversation::decision_operation_id(command_id)
}

#[tokio::test]
async fn deny_records_denied_without_owner_contact() {
    let fixture = Fixture::open().await;
    let current = fixture.seed_inline(reviewed_target()).await;
    let owners = ScriptedOwners::new(live_precondition());
    let outcome = resolve_interaction(
        &fixture.runs,
        &fixture.repo,
        &owners,
        &owners,
        &fixture.caller,
        fixture.resolve_command(&current, floe_conversation::InteractionDecisionKind::Deny),
        &fixture.cancellation,
        NOW,
    )
    .await
    .unwrap();
    assert!(
        matches!(outcome, ResolveOutcome::Denied { .. }),
        "{outcome:?}"
    );
    {
        let script = owners.script.lock().unwrap();
        assert_eq!(script.reads, 0);
        assert!(script.mutations.is_empty());
    }
    let stored =
        floe_conversation::load_interaction(&fixture.repo, &fixture.principal(), current.id)
            .await
            .unwrap();
    assert!(matches!(
        stored.state,
        floe_conversation::InteractionState::Denied { .. }
    ));
}

#[tokio::test]
async fn dismiss_cancels_pending_without_owner_contact() {
    let fixture = Fixture::open().await;
    let current = fixture.seed_inline(reviewed_target()).await;
    let owners = ScriptedOwners::new(live_precondition());
    let outcome = resolve_interaction(
        &fixture.runs,
        &fixture.repo,
        &owners,
        &owners,
        &fixture.caller,
        fixture.resolve_command(
            &current,
            floe_conversation::InteractionDecisionKind::Dismiss,
        ),
        &fixture.cancellation,
        NOW,
    )
    .await
    .unwrap();
    assert!(
        matches!(outcome, ResolveOutcome::Cancelled { .. }),
        "{outcome:?}"
    );
    let script = owners.script.lock().unwrap();
    assert_eq!(script.reads, 0);
    assert!(script.mutations.is_empty());
}

#[tokio::test]
async fn approve_precondition_mutates_once_with_stable_operation_id() {
    let fixture = Fixture::open().await;
    let current = fixture.seed_inline(reviewed_target()).await;
    let owners = ScriptedOwners::new(live_precondition());
    let command = fixture.resolve_command(
        &current,
        floe_conversation::InteractionDecisionKind::Approve,
    );
    let expected_operation = owner_operation_id(command.command_id);
    let script_handle = owners.script.clone();
    // First read sees the precondition; the mutation applies and flips live;
    // post-verify sees satisfaction.
    let outcome = resolve_interaction(
        &fixture.runs,
        &fixture.repo,
        &owners,
        &FlippingMutation {
            owners: owners.clone(),
            flip_to: live_satisfied(),
        },
        &fixture.caller,
        command,
        &fixture.cancellation,
        NOW,
    )
    .await
    .unwrap();
    assert!(
        matches!(outcome, ResolveOutcome::Resolved { .. }),
        "{outcome:?}"
    );
    {
        let script = script_handle.lock().unwrap();
        assert_eq!(script.mutations, vec![expected_operation]);
    }
    let stored =
        floe_conversation::load_interaction(&fixture.repo, &fixture.principal(), current.id)
            .await
            .unwrap();
    let floe_conversation::InteractionState::Resolved { receipt } = stored.state else {
        panic!("must resolve: {:?}", stored.state);
    };
    assert_eq!(receipt.owner_operation_id, expected_operation);
    assert!(!receipt.decision_id.is_nil());
}

/// A mutation fake that flips the shared live state when the owner op
/// commits, like a real grant activation would.
struct FlippingMutation {
    owners: ScriptedOwners,
    flip_to: LiveInlineState,
}

impl InlineOwnerMutation for FlippingMutation {
    fn enable_reviewed<'a>(
        &'a self,
        _: &'a floe_conversation::InlineObserveTarget,
        _: PersonId,
        _: &'a str,
        operation_id: Uuid,
        _: &'a floe_execution::Cancellation,
    ) -> BoxFuture<'a, Result<(), AgentFailure>> {
        let mut script = self.owners.script.lock().unwrap();
        script.mutations.push(operation_id);
        script.live = self.flip_to.clone();
        Box::pin(async move { Ok(()) })
    }
}

#[tokio::test]
async fn double_allow_same_command_resolves_once() {
    let fixture = Fixture::open().await;
    let current = fixture.seed_inline(reviewed_target()).await;
    let owners = ScriptedOwners::new(live_precondition());
    let command = fixture.resolve_command(
        &current,
        floe_conversation::InteractionDecisionKind::Approve,
    );
    let expected_operation = owner_operation_id(command.command_id);
    let mutation = FlippingMutation {
        owners: owners.clone(),
        flip_to: live_satisfied(),
    };
    let first = resolve_interaction(
        &fixture.runs,
        &fixture.repo,
        &owners,
        &mutation,
        &fixture.caller,
        command.clone(),
        &fixture.cancellation,
        NOW,
    )
    .await
    .unwrap();
    assert!(
        matches!(first, ResolveOutcome::Resolved { .. }),
        "{first:?}"
    );
    // Identical retry rejoins the recorded decision and settles without a
    // second mutation.
    let second = resolve_interaction(
        &fixture.runs,
        &fixture.repo,
        &owners,
        &mutation,
        &fixture.caller,
        command,
        &fixture.cancellation,
        NOW,
    )
    .await
    .unwrap();
    assert!(
        matches!(second, ResolveOutcome::Resolved { .. }),
        "{second:?}"
    );
    let script = owners.script.lock().unwrap();
    assert_eq!(script.mutations, vec![expected_operation]);
}

#[tokio::test]
async fn same_command_different_digest_conflicts_without_mutation() {
    let fixture = Fixture::open().await;
    let current = fixture.seed_inline(reviewed_target()).await;
    let owners = ScriptedOwners::new(live_precondition());
    let mut command = fixture.resolve_command(
        &current,
        floe_conversation::InteractionDecisionKind::Approve,
    );
    let mutation = FlippingMutation {
        owners: owners.clone(),
        flip_to: live_satisfied(),
    };
    let first = resolve_interaction(
        &fixture.runs,
        &fixture.repo,
        &owners,
        &mutation,
        &fixture.caller,
        command.clone(),
        &fixture.cancellation,
        NOW,
    )
    .await
    .unwrap();
    assert!(
        matches!(first, ResolveOutcome::Resolved { .. }),
        "{first:?}"
    );
    command.target_digest = [9; 32];
    command.expected_revision = current.revision;
    let outcome = resolve_interaction(
        &fixture.runs,
        &fixture.repo,
        &owners,
        &mutation,
        &fixture.caller,
        command,
        &fixture.cancellation,
        NOW,
    )
    .await;
    // The digest no longer echoes the reviewed target: rejected before any
    // owner work.
    assert_eq!(outcome.unwrap_err(), AgentFailure::InvalidInput);
    assert_eq!(owners.script.lock().unwrap().mutations.len(), 1);
}

#[tokio::test]
async fn fresh_approve_with_concurrent_grant_supersedes_with_replacement() {
    let fixture = Fixture::open().await;
    let current = fixture.seed_inline(reviewed_target()).await;
    // Live already satisfies the requirement: a fresh Allow is a conflict,
    // not a silent adoption.
    let owners = ScriptedOwners::new(live_satisfied());
    let outcome = resolve_interaction(
        &fixture.runs,
        &fixture.repo,
        &owners,
        &owners,
        &fixture.caller,
        fixture.resolve_command(
            &current,
            floe_conversation::InteractionDecisionKind::Approve,
        ),
        &fixture.cancellation,
        NOW,
    )
    .await
    .unwrap();
    let (superseded, reason, replacement) = match outcome {
        ResolveOutcome::Superseded {
            interaction,
            reason,
            replacement_id,
        } => (interaction, reason, replacement_id),
        other => panic!("must supersede: {other:?}"),
    };
    assert_eq!(reason, DriftReason::ConcurrentEnablement);
    assert!(owners.script.lock().unwrap().mutations.is_empty());
    let replacement_id = replacement.expect("drift publishes a fresh review");
    let replacement =
        floe_conversation::load_interaction(&fixture.repo, &fixture.principal(), replacement_id)
            .await
            .unwrap();
    assert!(matches!(
        replacement.state,
        floe_conversation::InteractionState::Pending
    ));
    let floe_conversation::ReviewedTarget::InlineObserve(inline) = &replacement.target else {
        panic!("replacement must stay inline");
    };
    assert!(matches!(
        inline.members[0].expected_grant,
        floe_conversation::ExpectedGrantState::Active { .. }
    ));
    assert_eq!(superseded.id, current.id);
}

#[tokio::test]
async fn fresh_approve_with_drift_supersedes_without_mutation() {
    let fixture = Fixture::open().await;
    let current = fixture.seed_inline(reviewed_target()).await;
    let mut live = live_precondition();
    live.native_subject = Some("rotated-subject".into());
    let owners = ScriptedOwners::new(live);
    let outcome = resolve_interaction(
        &fixture.runs,
        &fixture.repo,
        &owners,
        &owners,
        &fixture.caller,
        fixture.resolve_command(
            &current,
            floe_conversation::InteractionDecisionKind::Approve,
        ),
        &fixture.cancellation,
        NOW,
    )
    .await
    .unwrap();
    match outcome {
        ResolveOutcome::Superseded {
            reason,
            replacement_id,
            ..
        } => {
            assert_eq!(reason, DriftReason::NativeSubject);
            assert!(replacement_id.is_some());
        }
        other => panic!("must supersede: {other:?}"),
    }
    assert!(owners.script.lock().unwrap().mutations.is_empty());
}

#[tokio::test]
async fn foreign_person_session_and_device_are_rejected() {
    let fixture = Fixture::open().await;
    let current = fixture.seed_inline(reviewed_target()).await;
    let owners = ScriptedOwners::new(live_precondition());
    let foreign_person = crate::CallerContext::verified(
        crate::LocalIdentityClaim {
            person_id: PersonId::new().0,
            device_id: DEVICE.into(),
        },
        1,
    )
    .unwrap();
    let outcome = resolve_interaction(
        &fixture.runs,
        &fixture.repo,
        &owners,
        &owners,
        &foreign_person,
        fixture.resolve_command(
            &current,
            floe_conversation::InteractionDecisionKind::Approve,
        ),
        &fixture.cancellation,
        NOW,
    )
    .await;
    assert_eq!(outcome.unwrap_err(), AgentFailure::NotFound);
    let mut command = fixture.resolve_command(
        &current,
        floe_conversation::InteractionDecisionKind::Approve,
    );
    command.session_id = Uuid::new_v4();
    let outcome = resolve_interaction(
        &fixture.runs,
        &fixture.repo,
        &owners,
        &owners,
        &fixture.caller,
        command,
        &fixture.cancellation,
        NOW,
    )
    .await;
    assert_eq!(outcome.unwrap_err(), AgentFailure::NotFound);
    let foreign_device = crate::CallerContext::verified(
        crate::LocalIdentityClaim {
            person_id: fixture.person.0,
            device_id: "other-device".into(),
        },
        1,
    )
    .unwrap();
    let outcome = resolve_interaction(
        &fixture.runs,
        &fixture.repo,
        &owners,
        &owners,
        &foreign_device,
        fixture.resolve_command(
            &current,
            floe_conversation::InteractionDecisionKind::Approve,
        ),
        &fixture.cancellation,
        NOW,
    )
    .await
    .unwrap();
    assert!(
        matches!(outcome, ResolveOutcome::WrongDevice { .. }),
        "{outcome:?}"
    );
    assert!(owners.script.lock().unwrap().mutations.is_empty());
}

#[tokio::test]
async fn stale_revision_conflicts_without_owner_contact() {
    let fixture = Fixture::open().await;
    let current = fixture.seed_inline(reviewed_target()).await;
    let owners = ScriptedOwners::new(live_precondition());
    let mut command = fixture.resolve_command(
        &current,
        floe_conversation::InteractionDecisionKind::Approve,
    );
    command.expected_revision = current.revision + 5;
    let outcome = resolve_interaction(
        &fixture.runs,
        &fixture.repo,
        &owners,
        &owners,
        &fixture.caller,
        command,
        &fixture.cancellation,
        NOW,
    )
    .await
    .unwrap();
    assert!(
        matches!(outcome, ResolveOutcome::Stale { .. }),
        "{outcome:?}"
    );
    let script = owners.script.lock().unwrap();
    assert_eq!(script.reads, 0);
    assert!(script.mutations.is_empty());
}

#[tokio::test]
async fn expired_interaction_persists_expired() {
    let fixture = Fixture::open().await;
    let current = fixture.seed_inline(reviewed_target()).await;
    let owners = ScriptedOwners::new(live_precondition());
    let outcome = resolve_interaction(
        &fixture.runs,
        &fixture.repo,
        &owners,
        &owners,
        &fixture.caller,
        fixture.resolve_command(
            &current,
            floe_conversation::InteractionDecisionKind::Approve,
        ),
        &fixture.cancellation,
        current.expires_at_unix_ms + 1,
    )
    .await
    .unwrap();
    assert!(
        matches!(outcome, ResolveOutcome::Expired { .. }),
        "{outcome:?}"
    );
    let stored =
        floe_conversation::load_interaction(&fixture.repo, &fixture.principal(), current.id)
            .await
            .unwrap();
    assert!(matches!(
        stored.state,
        floe_conversation::InteractionState::Expired
    ));
}

#[tokio::test]
async fn approve_on_navigation_only_is_rejected() {
    let fixture = Fixture::open().await;
    let current = fixture.seed_navigation().await;
    let owners = ScriptedOwners::new(live_precondition());
    let outcome = resolve_interaction(
        &fixture.runs,
        &fixture.repo,
        &owners,
        &owners,
        &fixture.caller,
        fixture.resolve_command(
            &current,
            floe_conversation::InteractionDecisionKind::Approve,
        ),
        &fixture.cancellation,
        NOW,
    )
    .await;
    assert_eq!(outcome.unwrap_err(), AgentFailure::InvalidInput);
    let stored =
        floe_conversation::load_interaction(&fixture.repo, &fixture.principal(), current.id)
            .await
            .unwrap();
    assert!(matches!(
        stored.state,
        floe_conversation::InteractionState::Pending
    ));
}

#[tokio::test]
async fn refresh_settles_satisfaction_replaces_drift_and_keeps_precondition() {
    let fixture = Fixture::open().await;
    // Satisfied: explicit refresh resolves without any mutation.
    let satisfied_card = fixture.seed_inline(reviewed_target()).await;
    let owners = ScriptedOwners::new(live_satisfied());
    let outcome = refresh_interaction(
        &fixture.runs,
        &fixture.repo,
        &owners,
        &owners,
        &fixture.caller,
        fixture.refresh_command(&satisfied_card),
        &fixture.cancellation,
        NOW,
    )
    .await
    .unwrap();
    assert!(
        matches!(outcome, RefreshOutcome::Resolved { .. }),
        "{outcome:?}"
    );
    assert!(owners.script.lock().unwrap().mutations.is_empty());

    // Precondition: still pending, nothing claimed.
    let pending_card = fixture.seed_inline(reviewed_target()).await;
    assert_ne!(pending_card.id, satisfied_card.id);
    owners.script.lock().unwrap().live = live_precondition();
    let outcome = refresh_interaction(
        &fixture.runs,
        &fixture.repo,
        &owners,
        &owners,
        &fixture.caller,
        fixture.refresh_command(&pending_card),
        &fixture.cancellation,
        NOW,
    )
    .await
    .unwrap();
    assert!(
        matches!(outcome, RefreshOutcome::StillPending { .. }),
        "{outcome:?}"
    );

    // Drift: replaced with a fresh review, never mutated.
    owners.script.lock().unwrap().live = {
        let mut live = live_precondition();
        live.connection_revision = Some(10);
        live
    };
    let outcome = refresh_interaction(
        &fixture.runs,
        &fixture.repo,
        &owners,
        &owners,
        &fixture.caller,
        fixture.refresh_command(&pending_card),
        &fixture.cancellation,
        NOW,
    )
    .await
    .unwrap();
    match outcome {
        RefreshOutcome::Superseded {
            reason,
            replacement_id,
            ..
        } => {
            assert_eq!(reason, DriftReason::ConnectionRevision);
            assert!(replacement_id.is_some());
        }
        other => panic!("must supersede: {other:?}"),
    }
    assert!(owners.script.lock().unwrap().mutations.is_empty());
}

#[tokio::test]
async fn refresh_reconciles_resolving_by_current_truth() {
    let fixture = Fixture::open().await;
    let current = fixture.seed_inline(reviewed_target()).await;
    let owners = ScriptedOwners::new(live_precondition());
    // Mutation reports response loss; live still shows the precondition, so
    // the card stays Resolving for explicit reconciliation.
    owners
        .script
        .lock()
        .unwrap()
        .mutation_results
        .push_back(Err(AgentFailure::DeadlineExceeded));
    let command = fixture.resolve_command(
        &current,
        floe_conversation::InteractionDecisionKind::Approve,
    );
    let outcome = resolve_interaction(
        &fixture.runs,
        &fixture.repo,
        &owners,
        &owners,
        &fixture.caller,
        command,
        &fixture.cancellation,
        NOW,
    )
    .await
    .unwrap();
    let resolving = match outcome {
        ResolveOutcome::Resolving { interaction } => interaction,
        other => panic!("must stay resolving: {other:?}"),
    };
    // The owner op actually committed: refresh settles without mutating.
    owners.script.lock().unwrap().live = live_satisfied();
    let outcome = refresh_interaction(
        &fixture.runs,
        &fixture.repo,
        &owners,
        &owners,
        &fixture.caller,
        fixture.refresh_command(&resolving),
        &fixture.cancellation,
        NOW,
    )
    .await
    .unwrap();
    assert!(
        matches!(outcome, RefreshOutcome::Resolved { .. }),
        "{outcome:?}"
    );
    assert_eq!(owners.script.lock().unwrap().mutations.len(), 1);
}

#[tokio::test]
async fn owner_refusal_after_commit_resolves_and_other_failures_stay_resolving() {
    let fixture = Fixture::open().await;
    // The owner op refused on fresher evidence, but the grant is live: the
    // commit hid behind the refusal, so reconciliation resolves.
    let current = fixture.seed_inline(reviewed_target()).await;
    let owners = ScriptedOwners::new(live_precondition());
    let flip = owners.script.clone();
    let satisfied = live_satisfied();
    struct RefusingMutation {
        owners: ScriptedOwners,
        flip: Arc<Mutex<Script>>,
        flip_to: LiveInlineState,
    }
    impl InlineOwnerMutation for RefusingMutation {
        fn enable_reviewed<'a>(
            &'a self,
            _: &'a floe_conversation::InlineObserveTarget,
            _: PersonId,
            _: &'a str,
            operation_id: Uuid,
            _: &'a floe_execution::Cancellation,
        ) -> BoxFuture<'a, Result<(), AgentFailure>> {
            self.owners
                .script
                .lock()
                .unwrap()
                .mutations
                .push(operation_id);
            self.flip.lock().unwrap().live = self.flip_to.clone();
            Box::pin(async move { Err(AgentFailure::AccessReviewRequired) })
        }
    }
    let outcome = resolve_interaction(
        &fixture.runs,
        &fixture.repo,
        &owners,
        &RefusingMutation {
            owners: owners.clone(),
            flip,
            flip_to: satisfied,
        },
        &fixture.caller,
        fixture.resolve_command(
            &current,
            floe_conversation::InteractionDecisionKind::Approve,
        ),
        &fixture.cancellation,
        NOW,
    )
    .await
    .unwrap();
    assert!(
        matches!(outcome, ResolveOutcome::Resolved { .. }),
        "{outcome:?}"
    );

    // A hard ambiguous failure with the precondition intact stays Resolving.
    let current = fixture.seed_inline(reviewed_target()).await;
    owners.script.lock().unwrap().live = live_precondition();
    owners
        .script
        .lock()
        .unwrap()
        .mutation_results
        .push_back(Err(AgentFailure::DeadlineExceeded));
    let outcome = resolve_interaction(
        &fixture.runs,
        &fixture.repo,
        &owners,
        &owners,
        &fixture.caller,
        fixture.resolve_command(
            &current,
            floe_conversation::InteractionDecisionKind::Approve,
        ),
        &fixture.cancellation,
        NOW,
    )
    .await
    .unwrap();
    assert!(
        matches!(outcome, ResolveOutcome::Resolving { .. }),
        "{outcome:?}"
    );
}

#[tokio::test]
async fn dismiss_cancels_resolving_without_revoking_owner_state() {
    let fixture = Fixture::open().await;
    let current = fixture.seed_inline(reviewed_target()).await;
    let owners = ScriptedOwners::new(live_precondition());
    owners
        .script
        .lock()
        .unwrap()
        .mutation_results
        .push_back(Err(AgentFailure::DeadlineExceeded));
    let approve = fixture.resolve_command(
        &current,
        floe_conversation::InteractionDecisionKind::Approve,
    );
    let outcome = resolve_interaction(
        &fixture.runs,
        &fixture.repo,
        &owners,
        &owners,
        &fixture.caller,
        approve,
        &fixture.cancellation,
        NOW,
    )
    .await
    .unwrap();
    let resolving = match outcome {
        ResolveOutcome::Resolving { interaction } => interaction,
        other => panic!("must stay resolving: {other:?}"),
    };
    let dismiss = ResolveInteractionCommand {
        interaction_id: current.id,
        command_id: Uuid::new_v4(),
        session_id: fixture.session_id,
        expected_revision: resolving.revision,
        kind: floe_conversation::InteractionDecisionKind::Dismiss,
        target_digest: current.target_digest,
    };
    let outcome = resolve_interaction(
        &fixture.runs,
        &fixture.repo,
        &owners,
        &owners,
        &fixture.caller,
        dismiss,
        &fixture.cancellation,
        NOW,
    )
    .await
    .unwrap();
    assert!(
        matches!(outcome, ResolveOutcome::Cancelled { .. }),
        "{outcome:?}"
    );
    // Dismiss records cancellation only; it never touches owner state.
    assert_eq!(owners.script.lock().unwrap().mutations.len(), 1);
}

#[tokio::test]
async fn refresh_navigation_settles_satisfaction_and_dead_connections() {
    let fixture = Fixture::open().await;
    let owners = ScriptedOwners::new(live_precondition());
    // Satisfied out-of-band: explicit refresh resolves.
    let current = fixture.seed_navigation().await;
    owners.script.lock().unwrap().nav_satisfied = true;
    let outcome = refresh_interaction(
        &fixture.runs,
        &fixture.repo,
        &owners,
        &owners,
        &fixture.caller,
        fixture.refresh_command(&current),
        &fixture.cancellation,
        NOW,
    )
    .await
    .unwrap();
    assert!(
        matches!(outcome, RefreshOutcome::Resolved { .. }),
        "{outcome:?}"
    );

    // Dead owning connection: the card's actions are invalid.
    let current = fixture.seed_navigation().await;
    owners.script.lock().unwrap().nav_satisfied = false;
    owners.script.lock().unwrap().nav_usable = false;
    let outcome = refresh_interaction(
        &fixture.runs,
        &fixture.repo,
        &owners,
        &owners,
        &fixture.caller,
        fixture.refresh_command(&current),
        &fixture.cancellation,
        NOW,
    )
    .await
    .unwrap();
    match outcome {
        RefreshOutcome::Superseded {
            reason,
            replacement_id,
            ..
        } => {
            assert_eq!(reason, DriftReason::ConnectionUnusable);
            assert_eq!(replacement_id, None);
        }
        other => panic!("must supersede: {other:?}"),
    }

    // Otherwise the navigation card stays actionable.
    let current = fixture.seed_navigation().await;
    owners.script.lock().unwrap().nav_usable = true;
    let outcome = refresh_interaction(
        &fixture.runs,
        &fixture.repo,
        &owners,
        &owners,
        &fixture.caller,
        fixture.refresh_command(&current),
        &fixture.cancellation,
        NOW,
    )
    .await
    .unwrap();
    assert!(
        matches!(outcome, RefreshOutcome::StillPending { .. }),
        "{outcome:?}"
    );
}
