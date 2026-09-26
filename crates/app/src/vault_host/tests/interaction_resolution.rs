use std::collections::VecDeque;
use std::num::NonZeroU64;
use std::os::unix::fs::PermissionsExt;
use std::sync::{Arc, Mutex};

use floe_agent_contract::{BoxFuture, JournalEvent, ToolCall};
use floe_kernel::RunId;

use super::*;
use crate::vault_host::interaction_resolution::{
    DriftReason, InlineOwnerMutation, LiveGrant, LiveInlineState, LiveMember, ObserveStateReader,
    RecipientConsentOwner, RefreshInteractionCommand, RefreshOutcome, ResolveInteractionCommand,
    ResolveOutcome, refresh_interaction, resolve_interaction,
};

const NOW: i64 = 1_700_000_000_000;
const DEVICE: &str = "device";

struct Fixture {
    runs: FakeRuns,
    vault: Arc<EncryptedAgentVault<Keys>>,
    keys: Keys,
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
        let keys = Keys::default();
        let vault = EncryptedAgentVault::create(root.path(), person, keys.clone())
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
                resume: None,
                profile: floe_conversation::ProfileSelection::Auto,
            })
            .await
            .unwrap();
        assert!(matches!(
            admission,
            floe_vault::VaultConversationAdmission::Created { .. }
        ));
        let vault = Arc::new(vault);
        let repo = floe_vault::VaultConversationRepository::new(Arc::clone(&vault));
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
            resume_of: None,
            resume_lineage: 0,
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
            vault,
            keys,
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
        source_id: &str,
        connection_id: &str,
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
                    source_id: source_id.into(),
                    connection_id: Some(connection_id.into()),
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

    fn consent_target(&self) -> floe_conversation::RecipientConsentTarget {
        floe_conversation::RecipientConsentTarget {
            recipient: "model.example".into(),
            profile_id: "server-model".into(),
            purpose: "everyday_assistance".into(),
            consumer: "conversation.root".into(),
            input_data_classes: vec![floe_agent_contract::DataClass::Personal],
            source_scopes: vec![],
            lineage: floe_agent_contract::RecipientLineage::try_new(
                self.session_id,
                self.run_id.as_uuid(),
            )
            .unwrap(),
            device_id: DEVICE.into(),
            projection_ref: Uuid::new_v4(),
            projection_revision: 1,
        }
    }

    async fn seed_consent(&self) -> floe_conversation::ConversationInteraction {
        let attempt_id = Uuid::new_v4();
        {
            let mut journal = self.runs.journal.lock().unwrap();
            let revision = journal.len() as u64 + 1;
            journal.push(floe_conversation::JournalEntry {
                revision,
                event: JournalEvent::ModelIntent {
                    attempt_id,
                    projection_ref: floe_agent_contract::ProjectionRef::new(),
                },
            });
        }
        let admission = floe_conversation::publish_interaction(
            &self.runs,
            &self.repo,
            floe_conversation::PublishInteractionRequest {
                principal: self.principal(),
                session_id: self.session_id,
                origin_run_id: self.run_id,
                origin: floe_conversation::InteractionOrigin::Model { attempt_id },
                kind: floe_agent_contract::UserInteractionKind::ProcessingRecipient,
                requirement: floe_conversation::InteractionRequirement {
                    kind: floe_conversation::InteractionRequirementKind::ApproveProcessingRecipient,
                    source_id: "model.example".into(),
                    connection_id: None,
                    consumer: "conversation.root".into(),
                    purpose: "everyday_assistance".into(),
                    inline: true,
                },
                target: floe_conversation::ReviewedTarget::RecipientConsent(self.consent_target()),
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
    expert_review_current: bool,
    live: LiveInlineState,
    read_failures: VecDeque<AgentFailure>,
    mutation_results: VecDeque<Result<(), AgentFailure>>,
    reads: usize,
    mutations: usize,
    nav_usable: bool,
    nav_satisfied: bool,
    consents: std::collections::HashMap<Uuid, floe_access::RecipientConsent>,
    grant_failures: VecDeque<AgentFailure>,
    consent_grants: usize,
    consent_checks: usize,
    consent_client: String,
}

#[derive(Clone)]
struct ScriptedOwners {
    script: Arc<Mutex<Script>>,
}

impl ScriptedOwners {
    fn new(live: LiveInlineState) -> Self {
        Self {
            script: Arc::new(Mutex::new(Script {
                expert_review_current: true,
                live,
                read_failures: VecDeque::new(),
                mutation_results: VecDeque::new(),
                reads: 0,
                mutations: 0,
                nav_usable: true,
                nav_satisfied: false,
                consents: std::collections::HashMap::new(),
                grant_failures: VecDeque::new(),
                consent_grants: 0,
                consent_checks: 0,
                consent_client: "client".into(),
            })),
        }
    }
}

impl ObserveStateReader for ScriptedOwners {
    fn expert_review_current<'a>(
        &'a self,
        _: &'a floe_conversation::ConversationInteraction,
    ) -> BoxFuture<'a, Result<bool, AgentFailure>> {
        let current = self.script.lock().unwrap().expert_review_current;
        Box::pin(async move { Ok(current) })
    }

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
        _: &'a floe_execution::Cancellation,
    ) -> BoxFuture<'a, Result<(), AgentFailure>> {
        let mut script = self.script.lock().unwrap();
        script.mutations += 1;
        let result = script.mutation_results.pop_front().unwrap_or(Ok(()));
        Box::pin(async move { result })
    }
}

impl RecipientConsentOwner for ScriptedOwners {
    fn grant_reviewed<'a>(
        &'a self,
        target: &'a floe_conversation::RecipientConsentTarget,
        person_id: PersonId,
        device_id: &'a str,
        now_unix_ms: i64,
    ) -> BoxFuture<'a, Result<floe_access::RecipientConsent, AgentFailure>> {
        let mut script = self.script.lock().unwrap();
        script.consent_grants += 1;
        if let Some(failure) = script.grant_failures.pop_front() {
            return Box::pin(async move { Err(failure) });
        }
        let now = chrono::DateTime::from_timestamp_millis(now_unix_ms);
        let client = script.consent_client.clone();
        let result = match now {
            Some(now) => floe_access::RecipientConsent::try_new(
                person_id,
                device_id.to_owned(),
                client,
                target.recipient.clone(),
                target.profile_id.clone(),
                target.purpose.clone(),
                target.consumer.clone(),
                target.input_data_classes.clone(),
                target.source_scopes.clone(),
                target.lineage,
                target.projection_ref,
                target.projection_revision,
                now,
            )
            .map_err(|_| AgentFailure::InvalidInput)
            .map(|consent| {
                script.consents.insert(consent.id(), consent.clone());
                consent
            }),
            None => Err(AgentFailure::InvalidInput),
        };
        Box::pin(async move { result })
    }

    fn usable_consent<'a>(
        &'a self,
        target: &'a floe_conversation::RecipientConsentTarget,
        person_id: PersonId,
        device_id: &'a str,
        now_unix_ms: i64,
    ) -> BoxFuture<'a, Result<bool, AgentFailure>> {
        let mut script = self.script.lock().unwrap();
        script.consent_checks += 1;
        let Some(now) = chrono::DateTime::from_timestamp_millis(now_unix_ms) else {
            return Box::pin(async move { Err(AgentFailure::InvalidInput) });
        };
        let id = floe_access::recipient_consent_id(
            person_id,
            device_id,
            &script.consent_client,
            &target.recipient,
            &target.profile_id,
            &target.purpose,
            &target.consumer,
            &target.input_data_classes,
            &target.source_scopes,
            target.lineage,
        );
        let usable = script
            .consents
            .get(&id)
            .is_some_and(|consent| consent.is_usable_at(now));
        Box::pin(async move { Ok(usable) })
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
            policy_fingerprint: "a".repeat(64),
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
            policy_fingerprint: "a".repeat(64),
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
    let source = floe_context_contract::SourceAuthority::from_parts(
        Uuid::new_v4(),
        NonZeroU64::new(3).unwrap(),
    )
    .unwrap();
    LiveInlineState {
        members: vec![LiveMember {
            member_id: "calendar.timeline".into(),
            policy_fingerprint: "a".repeat(64),
            resource: "personal".into(),
            source_revision: None,
            live_grants: vec![LiveGrant {
                id,
                authority,
                source_authority: source,
            }],
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
    let current = fixture
        .seed_inline(reviewed_target(), "floe.source.calendar", "connection")
        .await;
    let owners = ScriptedOwners::new(live_precondition());
    let outcome = resolve_interaction(
        &fixture.runs,
        &fixture.repo,
        &owners,
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
        assert_eq!(script.mutations, 0);
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
    let current = fixture
        .seed_inline(reviewed_target(), "floe.source.calendar", "connection")
        .await;
    let owners = ScriptedOwners::new(live_precondition());
    let outcome = resolve_interaction(
        &fixture.runs,
        &fixture.repo,
        &owners,
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
    assert_eq!(script.mutations, 0);
}

#[tokio::test]
async fn approve_precondition_mutates_once_with_stable_operation_id() {
    let fixture = Fixture::open().await;
    let current = fixture
        .seed_inline(reviewed_target(), "floe.source.calendar", "connection")
        .await;
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
        &owners,
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
        assert_eq!(script.mutations, 1);
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

#[tokio::test]
async fn superseded_expert_selection_cannot_enable_reviewed_source() {
    let fixture = Fixture::open().await;
    let current = fixture
        .seed_inline(reviewed_target(), "floe.source.calendar", "connection")
        .await;
    let owners = ScriptedOwners::new(live_precondition());
    owners.script.lock().unwrap().expert_review_current = false;
    let outcome = resolve_interaction(
        &fixture.runs,
        &fixture.repo,
        &owners,
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
    assert!(matches!(
        outcome,
        ResolveOutcome::Superseded {
            reason: DriftReason::ExpertAssignmentChanged,
            replacement_id: None,
            ..
        }
    ));
    assert_eq!(owners.script.lock().unwrap().mutations, 0);
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
        _: &'a floe_execution::Cancellation,
    ) -> BoxFuture<'a, Result<(), AgentFailure>> {
        let mut script = self.owners.script.lock().unwrap();
        script.mutations += 1;
        script.live = self.flip_to.clone();
        Box::pin(async move { Ok(()) })
    }
}

#[tokio::test]
async fn double_allow_same_command_resolves_once() {
    let fixture = Fixture::open().await;
    let current = fixture
        .seed_inline(reviewed_target(), "floe.source.calendar", "connection")
        .await;
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
        &owners,
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
        &owners,
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
    assert_eq!(owners.script.lock().unwrap().mutations, 1);
    // Both attempts settle on the same stable owner operation.
    let stored =
        floe_conversation::load_interaction(&fixture.repo, &fixture.principal(), current.id)
            .await
            .unwrap();
    let floe_conversation::InteractionState::Resolved { receipt } = stored.state else {
        panic!("must resolve: {:?}", stored.state);
    };
    assert_eq!(receipt.owner_operation_id, expected_operation);
}

#[tokio::test]
async fn same_command_different_digest_conflicts_without_mutation() {
    let fixture = Fixture::open().await;
    let current = fixture
        .seed_inline(reviewed_target(), "floe.source.calendar", "connection")
        .await;
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
        &owners,
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
        &owners,
        &fixture.caller,
        command,
        &fixture.cancellation,
        NOW,
    )
    .await;
    // The digest no longer echoes the reviewed target: rejected before any
    // owner work.
    assert_eq!(outcome.unwrap_err(), AgentFailure::InvalidInput);
    assert_eq!(owners.script.lock().unwrap().mutations, 1);
}

#[tokio::test]
async fn fresh_approve_with_concurrent_grant_supersedes_with_replacement() {
    let fixture = Fixture::open().await;
    let current = fixture
        .seed_inline(reviewed_target(), "floe.source.calendar", "connection")
        .await;
    // Live already satisfies the requirement: a fresh Allow is a conflict,
    // not a silent adoption.
    let owners = ScriptedOwners::new(live_satisfied());
    let outcome = resolve_interaction(
        &fixture.runs,
        &fixture.repo,
        &owners,
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
    assert_eq!(owners.script.lock().unwrap().mutations, 0);
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
async fn fresh_approve_on_unchanged_live_grant_rereviews_and_resolves() {
    let fixture = Fixture::open().await;
    let (id, authority) = grant_pair();
    let source = floe_context_contract::SourceAuthority::from_parts(
        Uuid::new_v4(),
        NonZeroU64::new(3).unwrap(),
    )
    .unwrap();
    let mut target = reviewed_target();
    target.members[0].source_revision = Some(floe_conversation::AuthorityRevision {
        incarnation: source.incarnation(),
        epoch: source.epoch().get(),
    });
    target.members[0].expected_grant = floe_conversation::ExpectedGrantState::Active {
        grant_id: id.as_uuid(),
        authority_incarnation: authority.incarnation(),
        authority_epoch: authority.access_epoch().get(),
    };
    let current = fixture
        .seed_inline(target, "floe.source.calendar", "connection")
        .await;
    // Live matches the reviewed grant exactly: confirmation runs the
    // canonical re-review instead of superseding again.
    let live = LiveInlineState {
        members: vec![LiveMember {
            member_id: "calendar.timeline".into(),
            policy_fingerprint: "a".repeat(64),
            resource: "personal".into(),
            source_revision: Some(source),
            live_grants: vec![LiveGrant {
                id,
                authority,
                source_authority: source,
            }],
            policy_authority: None,
        }],
        connection_revision: Some(9),
        producer_fingerprint: None,
        native_subject: Some("subject".into()),
        connection_usable: true,
    };
    let owners = ScriptedOwners::new(live);
    let outcome = resolve_interaction(
        &fixture.runs,
        &fixture.repo,
        &owners,
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
        matches!(outcome, ResolveOutcome::Resolved { .. }),
        "{outcome:?}"
    );
    assert_eq!(owners.script.lock().unwrap().mutations, 1);
}

#[tokio::test]
async fn fresh_approve_with_drift_supersedes_without_mutation() {
    let fixture = Fixture::open().await;
    let current = fixture
        .seed_inline(reviewed_target(), "floe.source.calendar", "connection")
        .await;
    let mut live = live_precondition();
    live.native_subject = Some("rotated-subject".into());
    let owners = ScriptedOwners::new(live);
    let outcome = resolve_interaction(
        &fixture.runs,
        &fixture.repo,
        &owners,
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
    assert_eq!(owners.script.lock().unwrap().mutations, 0);
}

#[tokio::test]
async fn foreign_person_session_and_device_are_rejected() {
    let fixture = Fixture::open().await;
    let current = fixture
        .seed_inline(reviewed_target(), "floe.source.calendar", "connection")
        .await;
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
    assert_eq!(owners.script.lock().unwrap().mutations, 0);
}

#[tokio::test]
async fn stale_revision_conflicts_without_owner_contact() {
    let fixture = Fixture::open().await;
    let current = fixture
        .seed_inline(reviewed_target(), "floe.source.calendar", "connection")
        .await;
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
    assert_eq!(script.mutations, 0);
}

#[tokio::test]
async fn expired_interaction_persists_expired() {
    let fixture = Fixture::open().await;
    let current = fixture
        .seed_inline(reviewed_target(), "floe.source.calendar", "connection")
        .await;
    let owners = ScriptedOwners::new(live_precondition());
    let outcome = resolve_interaction(
        &fixture.runs,
        &fixture.repo,
        &owners,
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
    let satisfied_card = fixture
        .seed_inline(reviewed_target(), "floe.source.calendar", "connection")
        .await;
    let owners = ScriptedOwners::new(live_satisfied());
    let outcome = refresh_interaction(
        &fixture.runs,
        &fixture.repo,
        &owners,
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
    assert_eq!(owners.script.lock().unwrap().mutations, 0);

    // Precondition: still pending, nothing claimed.
    let pending_card = fixture
        .seed_inline(reviewed_target(), "floe.source.calendar", "connection")
        .await;
    assert_ne!(pending_card.id, satisfied_card.id);
    owners.script.lock().unwrap().live = live_precondition();
    let outcome = refresh_interaction(
        &fixture.runs,
        &fixture.repo,
        &owners,
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
    assert_eq!(owners.script.lock().unwrap().mutations, 0);
}

#[tokio::test]
async fn refresh_reconciles_resolving_by_current_truth() {
    let fixture = Fixture::open().await;
    let current = fixture
        .seed_inline(reviewed_target(), "floe.source.calendar", "connection")
        .await;
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
    assert_eq!(owners.script.lock().unwrap().mutations, 1);
}

#[tokio::test]
async fn owner_refusal_after_commit_resolves_and_other_failures_stay_resolving() {
    let fixture = Fixture::open().await;
    // The owner op refused on fresher evidence, but the grant is live: the
    // commit hid behind the refusal, so reconciliation resolves.
    let current = fixture
        .seed_inline(reviewed_target(), "floe.source.calendar", "connection")
        .await;
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
            _: &'a floe_execution::Cancellation,
        ) -> BoxFuture<'a, Result<(), AgentFailure>> {
            self.owners.script.lock().unwrap().mutations += 1;
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
        matches!(outcome, ResolveOutcome::Resolved { .. }),
        "{outcome:?}"
    );

    // A hard ambiguous failure with the precondition intact stays Resolving.
    let current = fixture
        .seed_inline(reviewed_target(), "floe.source.calendar", "connection")
        .await;
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
    let current = fixture
        .seed_inline(reviewed_target(), "floe.source.calendar", "connection")
        .await;
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
    assert_eq!(owners.script.lock().unwrap().mutations, 1);
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

use super::super::interaction_owners::HostInteractionOwners;
use super::super::review_snapshot::fixtures::{FixtureCalendarSubject, FixturePersonalInspector};

const NATIVE_FINGERPRINT: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const ROTATED_FINGERPRINT: &str =
    "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

struct HostFixture {
    base: Fixture,
    core: crate::FloeCore,
    core_dir: tempfile::TempDir,
    store: floe_provider_adapters::control::CurrentSavedConnectionStore,
}

impl HostFixture {
    async fn open() -> Self {
        let base = Fixture::open().await;
        let core_dir = tempfile::tempdir().unwrap();
        let core = crate::FloeCore::open(core_dir.path().join("core.db"))
            .await
            .unwrap();
        core.set_calendar_scope(
            base.person,
            "connection".into(),
            9,
            DEVICE.into(),
            floe_context_contract::CalendarProvider::EventKit,
            vec![floe_day::CalendarSelection {
                calendar_id: "personal".into(),
                calendar_name: "Personal".into(),
            }],
            floe_context_contract::CalendarScope::Selected,
        )
        .await
        .unwrap();
        let store = TestConnections::default().store();
        Self {
            base,
            core,
            core_dir,
            store,
        }
    }

    fn owners<'a>(
        &'a self,
        calendar: &'a FixtureCalendarSubject,
        personal: &'a FixturePersonalInspector,
    ) -> HostInteractionOwners<'a, Keys, FixtureCalendarSubject, FixturePersonalInspector> {
        HostInteractionOwners {
            core: &self.core,
            vault: &self.base.vault,
            connections: &self.store,
            calendar_subject: calendar,
            personal_subject: personal,
            probe_deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(5),
        }
    }

    /// The reviewed target an honest capture binds: live connection
    /// identity plus the probed subject fingerprint.
    async fn native_target(&self, fingerprint: &str) -> floe_conversation::InlineObserveTarget {
        let live = self
            .core
            .calendar_connection(self.base.person)
            .await
            .unwrap()
            .unwrap();
        floe_conversation::InlineObserveTarget {
            connection_id: live.connection_id.clone(),
            device_id: Some(DEVICE.into()),
            source_id: "floe.source.calendar".into(),
            connector_id: Some("calendar.event_kit".into()),
            consumer: "floe.builtin.schedule".into(),
            purpose: "scheduling".into(),
            connection_revision: Some(live.revision),
            reviewed_producer_fingerprint: None,
            reviewed_native_subject: Some(fingerprint.into()),
            members: live
                .calendars
                .iter()
                .map(|calendar| floe_conversation::ReviewedBundleMember {
                    member_id: "calendar.timeline".into(),
                    policy_fingerprint: crate::first_party_observe::member_policy_fingerprint(
                        "calendar.event_kit",
                        "calendar.timeline",
                    )
                    .unwrap(),
                    resource: calendar.calendar_id.clone(),
                    source_revision: Some(floe_conversation::AuthorityRevision {
                        incarnation: live.source_authority.incarnation(),
                        epoch: live.source_authority.epoch().get(),
                    }),
                    expected_grant: floe_conversation::ExpectedGrantState::Absent,
                    policy_authority: None,
                })
                .collect(),
        }
    }
}

#[tokio::test]
async fn native_allow_creates_exact_grant_and_resolves() {
    let host = HostFixture::open().await;
    let target = host.native_target(NATIVE_FINGERPRINT).await;
    let current = host
        .base
        .seed_inline(target, "floe.source.calendar", "connection")
        .await;
    let unrelated = host.base.seed_navigation().await;
    let calendar = FixtureCalendarSubject {
        fingerprint: NATIVE_FINGERPRINT.into(),
    };
    let personal = FixturePersonalInspector {
        fingerprint: NATIVE_FINGERPRINT.into(),
    };
    let owners = host.owners(&calendar, &personal);
    let outcome = resolve_interaction(
        &host.base.runs,
        &host.base.repo,
        &owners,
        &owners,
        &owners,
        &host.base.caller,
        host.base.resolve_command(
            &current,
            floe_conversation::InteractionDecisionKind::Approve,
        ),
        &host.base.cancellation,
        NOW,
    )
    .await
    .unwrap();
    assert!(
        matches!(outcome, ResolveOutcome::Resolved { .. }),
        "{outcome:?}"
    );
    let grants = host.base.vault.list_data_access_grants(128).await.unwrap();
    assert_eq!(grants.len(), 1);
    let grant = &grants[0];
    assert_eq!(grant.state(), floe_access::GrantState::Active);
    assert_eq!(grant.source().connector().as_str(), "calendar.event_kit");
    assert_eq!(grant.source().connection_id().as_str(), "connection");
    assert!(
        grant
            .scope()
            .resources()
            .iter()
            .any(|value| value.as_str() == "personal"),
        "grant covers the reviewed resource"
    );
    let stored =
        floe_conversation::load_interaction(&host.base.repo, &host.base.principal(), unrelated.id)
            .await
            .unwrap();
    assert!(matches!(
        stored.state,
        floe_conversation::InteractionState::Pending
    ));
    assert_eq!(stored.revision, 1);
}

#[tokio::test]
async fn native_concurrent_enable_conflicts_then_replacement_confirms_without_new_grant() {
    let host = HostFixture::open().await;
    let target = host.native_target(NATIVE_FINGERPRINT).await;
    let current = host
        .base
        .seed_inline(target.clone(), "floe.source.calendar", "connection")
        .await;
    let calendar = FixtureCalendarSubject {
        fingerprint: NATIVE_FINGERPRINT.into(),
    };
    let personal = FixturePersonalInspector {
        fingerprint: NATIVE_FINGERPRINT.into(),
    };
    let owners = host.owners(&calendar, &personal);
    // The person enables through the owning connection screen first: the
    // same canonical operation the decision would run.
    owners
        .enable_reviewed(&target, host.base.person, DEVICE, &host.base.cancellation)
        .await
        .unwrap();
    let committed = host.base.vault.list_data_access_grants(128).await.unwrap();
    assert_eq!(committed.len(), 1);
    // A fresh Allow is now a conflict, not a silent adoption.
    let outcome = resolve_interaction(
        &host.base.runs,
        &host.base.repo,
        &owners,
        &owners,
        &owners,
        &host.base.caller,
        host.base.resolve_command(
            &current,
            floe_conversation::InteractionDecisionKind::Approve,
        ),
        &host.base.cancellation,
        NOW,
    )
    .await
    .unwrap();
    let (replacement_id, replacement_target) = match outcome {
        ResolveOutcome::Superseded {
            reason,
            replacement_id,
            ..
        } => {
            assert_eq!(reason, DriftReason::ConcurrentEnablement);
            let replacement_id = replacement_id.expect("drift publishes a fresh review");
            let replacement = floe_conversation::load_interaction(
                &host.base.repo,
                &host.base.principal(),
                replacement_id,
            )
            .await
            .unwrap();
            let floe_conversation::ReviewedTarget::InlineObserve(inline) =
                replacement.target.clone()
            else {
                panic!("replacement must stay inline");
            };
            assert!(matches!(
                inline.members[0].expected_grant,
                floe_conversation::ExpectedGrantState::Active { .. }
            ));
            (replacement_id, replacement)
        }
        other => panic!("must supersede: {other:?}"),
    };
    // Confirming the fresh review re-reviews the same grant: no second
    // grant, no authority advance.
    let outcome = resolve_interaction(
        &host.base.runs,
        &host.base.repo,
        &owners,
        &owners,
        &owners,
        &host.base.caller,
        host.base.resolve_command(
            &replacement_target,
            floe_conversation::InteractionDecisionKind::Approve,
        ),
        &host.base.cancellation,
        NOW,
    )
    .await
    .unwrap();
    assert!(
        matches!(outcome, ResolveOutcome::Resolved { .. }),
        "{outcome:?}"
    );
    let grants = host.base.vault.list_data_access_grants(128).await.unwrap();
    assert_eq!(grants.len(), 1);
    assert_eq!(grants[0].id(), committed[0].id());
    assert_eq!(grants[0].authority(), committed[0].authority());
    assert_ne!(replacement_id, current.id);
}

#[tokio::test]
async fn native_stale_subject_supersedes_without_mutation() {
    let host = HostFixture::open().await;
    let target = host.native_target(NATIVE_FINGERPRINT).await;
    let current = host
        .base
        .seed_inline(target, "floe.source.calendar", "connection")
        .await;
    let calendar = FixtureCalendarSubject {
        fingerprint: ROTATED_FINGERPRINT.into(),
    };
    let personal = FixturePersonalInspector {
        fingerprint: NATIVE_FINGERPRINT.into(),
    };
    let owners = host.owners(&calendar, &personal);
    let outcome = resolve_interaction(
        &host.base.runs,
        &host.base.repo,
        &owners,
        &owners,
        &owners,
        &host.base.caller,
        host.base.resolve_command(
            &current,
            floe_conversation::InteractionDecisionKind::Approve,
        ),
        &host.base.cancellation,
        NOW,
    )
    .await
    .unwrap();
    match outcome {
        ResolveOutcome::Superseded { reason, .. } => {
            assert_eq!(reason, DriftReason::NativeSubject);
        }
        other => panic!("must supersede: {other:?}"),
    }
    let grants = host.base.vault.list_data_access_grants(128).await.unwrap();
    assert!(grants.is_empty());
}

#[tokio::test]
async fn native_commit_then_crash_reopens_and_resolves_without_second_advance() {
    let host = HostFixture::open().await;
    let target = host.native_target(NATIVE_FINGERPRINT).await;
    let current = host
        .base
        .seed_inline(target.clone(), "floe.source.calendar", "connection")
        .await;
    let calendar = FixtureCalendarSubject {
        fingerprint: NATIVE_FINGERPRINT.into(),
    };
    let personal = FixturePersonalInspector {
        fingerprint: NATIVE_FINGERPRINT.into(),
    };
    let owners = host.owners(&calendar, &personal);
    // Decide first (Resolving claimed), then commit the owner mutation,
    // then crash before the resolution is recorded.
    let command = host.base.resolve_command(
        &current,
        floe_conversation::InteractionDecisionKind::Approve,
    );
    let admitted = floe_conversation::decide_interaction(
        &host.base.repo,
        floe_conversation::DecideInteractionCommand {
            command_id: command.command_id,
            interaction_id: current.id,
            principal: host.base.principal(),
            expected_revision: current.revision,
            kind: floe_conversation::InteractionDecisionKind::Approve,
            target_digest: current.target_digest,
        },
        NOW,
    )
    .await
    .unwrap();
    let resolving = match admitted {
        floe_conversation::DecisionAdmission::Applied(current) => current,
        floe_conversation::DecisionAdmission::Rejoined(_) => panic!("fresh decision must apply"),
    };
    assert!(matches!(
        resolving.state,
        floe_conversation::InteractionState::Resolving { .. }
    ));
    owners
        .enable_reviewed(&target, host.base.person, DEVICE, &host.base.cancellation)
        .await
        .unwrap();
    let committed = host.base.vault.list_data_access_grants(128).await.unwrap();
    assert_eq!(committed.len(), 1);
    let committed_id = committed[0].id();
    let committed_authority = committed[0].authority();

    // Crash: drop the vault and repo (releasing the host lock), reopen
    // from disk with the same keys, reboot the core with the same device
    // state, and reconcile explicitly. Named bindings plus explicit
    // drops: `_` placeholders would keep the vault alive to the end of
    // the block and hold the host lock.
    let HostFixture {
        base,
        store,
        core: old_core,
        core_dir,
    } = host;
    drop(old_core);
    let Fixture {
        runs,
        repo: old_repo,
        vault: old_vault,
        keys,
        person,
        session_id,
        run_id: _run_id,
        caller,
        cancellation,
        _root,
    } = base;
    drop(old_repo);
    drop(old_vault);
    let reopened = Arc::new(
        EncryptedAgentVault::open(_root.path(), person, keys.clone())
            .await
            .unwrap(),
    );
    let repo = floe_vault::VaultConversationRepository::new(Arc::clone(&reopened));
    // The rebooted core reopens the same store: the connection record,
    // including its source authority, survives the crash.
    let core = crate::FloeCore::open(core_dir.path().join("core.db"))
        .await
        .unwrap();
    let owners = HostInteractionOwners {
        core: &core,
        vault: &reopened,
        connections: &store,
        calendar_subject: &calendar,
        personal_subject: &personal,
        probe_deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(5),
    };
    let outcome = refresh_interaction(
        &runs,
        &repo,
        &owners,
        &owners,
        &owners,
        &caller,
        RefreshInteractionCommand {
            interaction_id: resolving.id,
            command_id: Uuid::new_v4(),
            session_id,
            expected_revision: resolving.revision,
        },
        &cancellation,
        NOW,
    )
    .await
    .unwrap();
    assert!(
        matches!(outcome, RefreshOutcome::Resolved { .. }),
        "{outcome:?}"
    );
    let grants = reopened.list_data_access_grants(128).await.unwrap();
    assert_eq!(grants.len(), 1);
    assert_eq!(grants[0].id(), committed_id);
    assert_eq!(grants[0].authority(), committed_authority);
}

#[tokio::test]
async fn native_external_enable_refresh_resolves_inspect_does_not() {
    let host = HostFixture::open().await;
    let target = host.native_target(NATIVE_FINGERPRINT).await;
    let current = host
        .base
        .seed_inline(target.clone(), "floe.source.calendar", "connection")
        .await;
    let calendar = FixtureCalendarSubject {
        fingerprint: NATIVE_FINGERPRINT.into(),
    };
    let personal = FixturePersonalInspector {
        fingerprint: NATIVE_FINGERPRINT.into(),
    };
    let owners = host.owners(&calendar, &personal);
    owners
        .enable_reviewed(&target, host.base.person, DEVICE, &host.base.cancellation)
        .await
        .unwrap();
    // A read-only load never settles the card.
    let inspected =
        floe_conversation::load_interaction(&host.base.repo, &host.base.principal(), current.id)
            .await
            .unwrap();
    assert!(matches!(
        inspected.state,
        floe_conversation::InteractionState::Pending
    ));
    let outcome = refresh_interaction(
        &host.base.runs,
        &host.base.repo,
        &owners,
        &owners,
        &owners,
        &host.base.caller,
        host.base.refresh_command(&inspected),
        &host.base.cancellation,
        NOW,
    )
    .await
    .unwrap();
    assert!(
        matches!(outcome, RefreshOutcome::Resolved { .. }),
        "{outcome:?}"
    );
}

struct DeniedCalendarSubject;

impl floe_context::NativeCalendarSubjectSource for DeniedCalendarSubject {
    async fn subject(
        &self,
        _request: floe_context::NativeSubjectRequest,
    ) -> Result<floe_context::NativeSubjectObservation, AgentFailure> {
        Err(AgentFailure::CapabilityUnavailable)
    }
}

#[tokio::test]
async fn native_os_denied_and_deselected_scope_never_falsely_resolve() {
    let host = HostFixture::open().await;
    let target = host.native_target(NATIVE_FINGERPRINT).await;
    let current = host
        .base
        .seed_inline(target, "floe.source.calendar", "connection")
        .await;
    // The OS denies the subject probe: the decision is durably recorded
    // but no owner state is touched, so the card waits in Resolving for
    // explicit reconciliation once the device heals.
    let denied = DeniedCalendarSubject;
    let personal = FixturePersonalInspector {
        fingerprint: NATIVE_FINGERPRINT.into(),
    };
    let owners = HostInteractionOwners {
        core: &host.core,
        vault: &host.base.vault,
        connections: &host.store,
        calendar_subject: &denied,
        personal_subject: &personal,
        probe_deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(5),
    };
    let outcome = resolve_interaction(
        &host.base.runs,
        &host.base.repo,
        &owners,
        &owners,
        &owners,
        &host.base.caller,
        host.base.resolve_command(
            &current,
            floe_conversation::InteractionDecisionKind::Approve,
        ),
        &host.base.cancellation,
        NOW,
    )
    .await;
    assert_eq!(outcome.unwrap_err(), AgentFailure::CapabilityUnavailable);
    let stored =
        floe_conversation::load_interaction(&host.base.repo, &host.base.principal(), current.id)
            .await
            .unwrap();
    assert!(matches!(
        stored.state,
        floe_conversation::InteractionState::Resolving { .. }
    ));
    let grants = host.base.vault.list_data_access_grants(128).await.unwrap();
    assert!(grants.is_empty());
    // The device heals: explicit refresh reconciles the claimed decision
    // through the canonical operation.
    let calendar = FixtureCalendarSubject {
        fingerprint: NATIVE_FINGERPRINT.into(),
    };
    let owners = host.owners(&calendar, &personal);
    let outcome = refresh_interaction(
        &host.base.runs,
        &host.base.repo,
        &owners,
        &owners,
        &owners,
        &host.base.caller,
        host.base.refresh_command(&stored),
        &host.base.cancellation,
        NOW,
    )
    .await
    .unwrap();
    assert!(
        matches!(outcome, RefreshOutcome::Resolved { .. }),
        "{outcome:?}"
    );
}

#[tokio::test]
async fn native_deselected_scope_supersedes_without_mutation() {
    let host = HostFixture::open().await;
    let target = host.native_target(NATIVE_FINGERPRINT).await;
    let current = host
        .base
        .seed_inline(target, "floe.source.calendar", "connection")
        .await;
    // The reviewed resource leaves the selection: the review no longer
    // binds anything, so it supersedes instead of resolving.
    host.core
        .set_calendar_scope(
            host.base.person,
            "connection".into(),
            10,
            DEVICE.into(),
            floe_context_contract::CalendarProvider::EventKit,
            vec![floe_day::CalendarSelection {
                calendar_id: "work".into(),
                calendar_name: "Work".into(),
            }],
            floe_context_contract::CalendarScope::Selected,
        )
        .await
        .unwrap();
    let calendar = FixtureCalendarSubject {
        fingerprint: NATIVE_FINGERPRINT.into(),
    };
    let personal = FixturePersonalInspector {
        fingerprint: NATIVE_FINGERPRINT.into(),
    };
    let owners = host.owners(&calendar, &personal);
    let outcome = resolve_interaction(
        &host.base.runs,
        &host.base.repo,
        &owners,
        &owners,
        &owners,
        &host.base.caller,
        host.base.resolve_command(
            &current,
            floe_conversation::InteractionDecisionKind::Approve,
        ),
        &host.base.cancellation,
        NOW,
    )
    .await
    .unwrap();
    match outcome {
        ResolveOutcome::Superseded { reason, .. } => {
            assert_eq!(reason, DriftReason::ConnectionUnusable);
        }
        other => panic!("must supersede: {other:?}"),
    }
    let grants = host.base.vault.list_data_access_grants(128).await.unwrap();
    assert!(grants.is_empty());
}

#[tokio::test]
async fn personal_attention_allow_resolves() {
    let host = HostFixture::open().await;
    let target = floe_conversation::InlineObserveTarget {
        connection_id: floe_access::ATTENTION_CONNECTION.into(),
        device_id: Some(DEVICE.into()),
        source_id: "floe.source.attention".into(),
        connector_id: Some(floe_access::ATTENTION_CONNECTOR.into()),
        consumer: "floe.builtin.schedule".into(),
        purpose: "scheduling".into(),
        connection_revision: None,
        reviewed_producer_fingerprint: None,
        reviewed_native_subject: Some(NATIVE_FINGERPRINT.into()),
        members: vec![floe_conversation::ReviewedBundleMember {
            member_id: floe_access::ATTENTION_CONNECTOR.into(),
            policy_fingerprint: crate::first_party_observe::member_policy_fingerprint(
                floe_access::ATTENTION_CONNECTOR,
                floe_access::ATTENTION_CONNECTOR,
            )
            .unwrap(),
            resource: floe_access::ATTENTION_RESOURCE.into(),
            source_revision: None,
            expected_grant: floe_conversation::ExpectedGrantState::Absent,
            policy_authority: None,
        }],
    };
    let current = host
        .base
        .seed_inline(
            target,
            "floe.source.attention",
            floe_access::ATTENTION_CONNECTION,
        )
        .await;
    let calendar = FixtureCalendarSubject {
        fingerprint: NATIVE_FINGERPRINT.into(),
    };
    let personal = FixturePersonalInspector {
        fingerprint: NATIVE_FINGERPRINT.into(),
    };
    let owners = host.owners(&calendar, &personal);
    let outcome = resolve_interaction(
        &host.base.runs,
        &host.base.repo,
        &owners,
        &owners,
        &owners,
        &host.base.caller,
        host.base.resolve_command(
            &current,
            floe_conversation::InteractionDecisionKind::Approve,
        ),
        &host.base.cancellation,
        NOW,
    )
    .await
    .unwrap();
    assert!(
        matches!(outcome, ResolveOutcome::Resolved { .. }),
        "{outcome:?}"
    );
    let grants = host.base.vault.list_data_access_grants(128).await.unwrap();
    assert_eq!(grants.len(), 1);
    assert_eq!(
        grants[0].source().connector().as_str(),
        floe_access::ATTENTION_CONNECTOR
    );
}

#[tokio::test]
async fn sibling_grant_revoked_out_of_band_supersedes_with_absent_replacement() {
    let host = HostFixture::open().await;
    let target = host.native_target(NATIVE_FINGERPRINT).await;
    let current = host
        .base
        .seed_inline(target.clone(), "floe.source.calendar", "connection")
        .await;
    let calendar = FixtureCalendarSubject {
        fingerprint: NATIVE_FINGERPRINT.into(),
    };
    let personal = FixturePersonalInspector {
        fingerprint: NATIVE_FINGERPRINT.into(),
    };
    let owners = host.owners(&calendar, &personal);
    // Enable out-of-band, then confirm the fresh review so the card binds
    // the live grant as a reviewed sibling.
    owners
        .enable_reviewed(&target, host.base.person, DEVICE, &host.base.cancellation)
        .await
        .unwrap();
    let outcome = refresh_interaction(
        &host.base.runs,
        &host.base.repo,
        &owners,
        &owners,
        &owners,
        &host.base.caller,
        host.base.refresh_command(&current),
        &host.base.cancellation,
        NOW,
    )
    .await
    .unwrap();
    assert!(
        matches!(outcome, RefreshOutcome::Resolved { .. }),
        "{outcome:?}"
    );
    let grants = host.base.vault.list_data_access_grants(128).await.unwrap();
    assert_eq!(grants.len(), 1);
    // A second card reviews the live grant; revoking it out-of-band
    // invalidates that review.
    let mut sibling_target = host.native_target(NATIVE_FINGERPRINT).await;
    sibling_target.members[0].expected_grant = floe_conversation::ExpectedGrantState::Active {
        grant_id: grants[0].id().as_uuid(),
        authority_incarnation: grants[0].authority().incarnation(),
        authority_epoch: grants[0].authority().access_epoch().get(),
    };
    sibling_target.members[0].policy_authority = host
        .base
        .vault
        .calendar_grant_policy_authority(grants[0].id())
        .await
        .ok()
        .map(|authority| floe_conversation::AuthorityRevision {
            incarnation: authority.incarnation(),
            epoch: authority.epoch().get(),
        });
    let sibling = host
        .base
        .seed_inline(sibling_target, "floe.source.calendar", "connection")
        .await;
    host.base
        .vault
        .revoke_data_access_grant(grants[0].id(), grants[0].authority())
        .await
        .unwrap();
    let outcome = resolve_interaction(
        &host.base.runs,
        &host.base.repo,
        &owners,
        &owners,
        &owners,
        &host.base.caller,
        host.base.resolve_command(
            &sibling,
            floe_conversation::InteractionDecisionKind::Approve,
        ),
        &host.base.cancellation,
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
            assert!(
                matches!(reason, DriftReason::GrantState { .. }),
                "{reason:?}"
            );
            let replacement_id = replacement_id.expect("drift publishes a fresh review");
            let replacement = floe_conversation::load_interaction(
                &host.base.repo,
                &host.base.principal(),
                replacement_id,
            )
            .await
            .unwrap();
            let floe_conversation::ReviewedTarget::InlineObserve(inline) = replacement.target
            else {
                panic!("replacement must stay inline");
            };
            assert!(matches!(
                inline.members[0].expected_grant,
                floe_conversation::ExpectedGrantState::Absent
            ));
        }
        other => panic!("must supersede: {other:?}"),
    }
}

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ring::rand::SystemRandom;
use ring::signature::Ed25519KeyPair;
use sha2::Sha256;

use super::super::interaction_owners::enable_remote_reviewed;
use super::super::remote_observe::RemoteObserveContext;

const REMOTE_CLIENT_ID: &str = "paired-client";

struct ScriptedRemoteTransport {
    producer: floe_access::RemoteProducerIdentity,
    pkcs8: Vec<u8>,
    person_id: PersonId,
    authority: Mutex<floe_context_contract::SourceAuthority>,
    provider_identity: Mutex<String>,
    corrupt_signature: Mutex<bool>,
}

impl ScriptedRemoteTransport {
    fn sign(&self, descriptor: &[u8]) -> String {
        let pair = Ed25519KeyPair::from_pkcs8(&self.pkcs8).unwrap();
        let mut message = Vec::from(b"floe.remote.producer.v1\0".as_slice());
        message.extend_from_slice(descriptor);
        if *self.corrupt_signature.lock().unwrap() {
            message.extend_from_slice(b"tampered");
        }
        URL_SAFE_NO_PAD.encode(pair.sign(&message).as_ref())
    }

    fn view_preview(
        &self,
        query: floe_access::RemoteSourceQuery<'_>,
    ) -> floe_access::SignedSourcePreview {
        let authority = *self.authority.lock().unwrap();
        let descriptor = serde_json::json!({
            "v": 1,
            "operation": "remote_view_source_preview",
            "challenge_id": Uuid::new_v4().to_string(),
            "nonce": URL_SAFE_NO_PAD.encode([7u8; 32]),
            "view_id": query.view_id,
            "person_id": self.person_id.to_string(),
            "client_id": REMOTE_CLIENT_ID,
            "device_id": DEVICE,
            "audience": self.producer.audience,
            "connector_id": query.connector_id,
            "connection_id": query.connection_id,
            "connection_revision": 11u64,
            "execution_owner": self.producer.execution_owner,
            "incarnation": authority.incarnation().to_string(),
            "epoch": authority.epoch().get(),
            "resource": query.resource,
            "provider_identity": self.provider_identity.lock().unwrap().clone(),
            "issued_at_unix_ms": 1_700_000_000_000i64,
        });
        let bytes = serde_json::to_vec(&descriptor).unwrap();
        floe_access::SignedSourcePreview {
            descriptor_b64url: URL_SAFE_NO_PAD.encode(&bytes),
            producer_signature: self.sign(&bytes),
            connection_revision: 11,
            producer: self.producer.clone(),
        }
    }

    fn calendar_preview(
        &self,
        query: floe_access::RemoteCalendarQuery<'_>,
    ) -> floe_access::SignedCalendarPreview {
        let authority = *self.authority.lock().unwrap();
        let descriptor = serde_json::json!({
            "v": 1,
            "operation": "calendar_source_preview",
            "challenge_id": Uuid::new_v4().to_string(),
            "nonce": URL_SAFE_NO_PAD.encode([7u8; 32]),
            "person_id": self.person_id.to_string(),
            "client_id": REMOTE_CLIENT_ID,
            "device_id": DEVICE,
            "audience": self.producer.audience,
            "connector_id": query.connector_id,
            "connection_id": query.connection_id,
            "execution_owner": self.producer.execution_owner,
            "incarnation": authority.incarnation().to_string(),
            "epoch": authority.epoch().get(),
            "resource": query.resource,
            "provider_identity": self.provider_identity.lock().unwrap().clone(),
            "issued_at_unix_ms": 1_700_000_000_000i64,
        });
        let bytes = serde_json::to_vec(&descriptor).unwrap();
        floe_access::SignedCalendarPreview {
            descriptor_b64url: URL_SAFE_NO_PAD.encode(&bytes),
            producer_signature: self.sign(&bytes),
            producer: self.producer.clone(),
        }
    }
}

impl floe_access::RemoteGrantTransport for ScriptedRemoteTransport {
    fn producer_identity<'a>(
        &'a self,
        _window: &'a floe_access::RemoteCallWindow,
    ) -> BoxFuture<'a, Result<floe_access::RemoteProducerIdentity, AgentFailure>> {
        let producer = self.producer.clone();
        Box::pin(async move { Ok(producer) })
    }

    fn view_source_preview<'a>(
        &'a self,
        query: floe_access::RemoteSourceQuery<'a>,
        _window: &'a floe_access::RemoteCallWindow,
    ) -> BoxFuture<'a, Result<floe_access::SignedSourcePreview, AgentFailure>> {
        let preview = self.view_preview(query);
        Box::pin(async move { Ok(preview) })
    }

    fn calendar_source_preview<'a>(
        &'a self,
        query: floe_access::RemoteCalendarQuery<'a>,
        _window: &'a floe_access::RemoteCallWindow,
    ) -> BoxFuture<'a, Result<floe_access::SignedCalendarPreview, AgentFailure>> {
        let preview = self.calendar_preview(query);
        Box::pin(async move { Ok(preview) })
    }
}

impl floe_context::RemoteViewTransport for ScriptedRemoteTransport {
    fn read_admitted_view<'a>(
        &'a self,
        read: floe_context::AdmittedRemoteRead<'a>,
        _: &'a floe_access::RemoteCallWindow,
    ) -> BoxFuture<'a, Result<serde_json::Value, AgentFailure>> {
        Box::pin(async move {
            let now = chrono::Utc::now().timestamp_millis();
            Ok(serde_json::json!({
                "schema_version": floe_agent_contract::AGENT_VERSION,
                "view_id": read.view_id,
                "source_handle": format!("mail:{}", read.resource),
                "observed_at_unix_ms": now - 1_000,
                "expires_at_unix_ms": now + 60_000,
                "coverage_complete": true,
                "next_cursor": null,
                "items": [],
            }))
        })
    }
}

struct ProductMailReader<'a> {
    vault: &'a EncryptedAgentVault<Keys>,
    transport: &'a ScriptedRemoteTransport,
    person_id: PersonId,
}

impl floe_context::SourceReader for ProductMailReader<'_> {
    fn read<'a>(
        &'a self,
        request: &'a floe_context::SourceReadRequest,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<
                        floe_context_contract::SourceReadOutcome<floe_context::SourceRead>,
                        AgentFailure,
                    >,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            let person_text = self.person_id.to_string();
            match floe_context::read_remote_view(
                self.vault,
                self.transport,
                self.person_id,
                floe_access::RemotePairingIdentity {
                    person_id: &person_text,
                    client_id: REMOTE_CLIENT_ID,
                    device_id: DEVICE,
                },
                request.source().as_str(),
                request.consumer().identifier(),
                request.query().clone(),
                &floe_access::RemoteCallWindow {
                    deadline: request.deadline(),
                    cancellation: request.cancellation().clone(),
                },
                request.process_incarnation_id(),
                request.query_fingerprint(),
            )
            .await?
            {
                floe_context_contract::SourceReadOutcome::Ready((payload, bindings)) => {
                    Ok(floe_context_contract::SourceReadOutcome::Ready(
                        floe_context::SourceRead::with_bindings(
                            request.source().clone(),
                            payload,
                            bindings,
                        ),
                    ))
                }
                floe_context_contract::SourceReadOutcome::Unavailable(reason) => Ok(
                    floe_context_contract::SourceReadOutcome::Unavailable(reason),
                ),
                floe_context_contract::SourceReadOutcome::NeedsUserAction(blockers) => Ok(
                    floe_context_contract::SourceReadOutcome::NeedsUserAction(blockers),
                ),
            }
        })
    }
}

struct RemoteFixture {
    base: Fixture,
    core: crate::FloeCore,
    store: floe_provider_adapters::control::CurrentSavedConnectionStore,
    transport: ScriptedRemoteTransport,
    connection_id: String,
}

impl RemoteFixture {
    async fn open() -> Self {
        let base = Fixture::open().await;
        let core = crate::FloeCore::open(":memory:").await.unwrap();
        let connection_id = Uuid::new_v4().to_string();
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
        let pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
        let public = pair.public_key().as_ref().to_vec();
        let fingerprint = Sha256::digest(&public)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let instance_id = Uuid::new_v4().to_string();
        let producer = floe_access::RemoteProducerIdentity {
            schema_version: 1,
            audience: format!("floe.server:{instance_id}"),
            instance_id,
            execution_owner: Uuid::new_v4().to_string(),
            key_id: Uuid::new_v4().to_string(),
            public_key: URL_SAFE_NO_PAD.encode(&public),
            fingerprint,
        };
        base.vault
            .remote_pin_producer(producer.clone())
            .await
            .unwrap();
        let transport = ScriptedRemoteTransport {
            producer,
            pkcs8: pkcs8.as_ref().to_vec(),
            person_id: base.person,
            authority: Mutex::new(floe_context_contract::SourceAuthority::new()),
            provider_identity: Mutex::new("google:subject-a".into()),
            corrupt_signature: Mutex::new(false),
        };
        let store = TestConnections::default().store();
        Self {
            base,
            core,
            store,
            transport,
            connection_id,
        }
    }

    fn owners<'a>(
        &'a self,
        calendar: &'a FixtureCalendarSubject,
        personal: &'a FixturePersonalInspector,
    ) -> RemoteTestOwners<'a> {
        RemoteTestOwners {
            core: &self.core,
            vault: &self.base.vault,
            store: &self.store,
            calendar,
            personal,
            transport: &self.transport,
            deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(5),
        }
    }

    /// The reviewed target an honest capture binds for the canonical
    /// gmail bundle: live producer pin plus the previewed authority.
    fn gmail_target(&self) -> floe_conversation::InlineObserveTarget {
        let policies = crate::first_party_observe::remote_policies("gmail").unwrap();
        assert_eq!(policies.len(), 2);
        let authority = *self.transport.authority.lock().unwrap();
        let mut members: Vec<floe_conversation::ReviewedBundleMember> = policies
            .iter()
            .map(|policy| floe_conversation::ReviewedBundleMember {
                member_id: policy.view_id.to_owned(),
                policy_fingerprint: crate::first_party_observe::policy_fingerprint(policy).unwrap(),
                resource: floe_context::remote_view_resource(policy.view_id, &self.connection_id),
                source_revision: Some(floe_conversation::AuthorityRevision {
                    incarnation: authority.incarnation(),
                    epoch: authority.epoch().get(),
                }),
                expected_grant: floe_conversation::ExpectedGrantState::Absent,
                policy_authority: None,
            })
            .collect();
        members.sort_by(|left, right| {
            left.member_id
                .cmp(&right.member_id)
                .then_with(|| left.resource.cmp(&right.resource))
        });
        floe_conversation::InlineObserveTarget {
            connection_id: self.connection_id.clone(),
            device_id: Some(DEVICE.into()),
            source_id: "floe.source.gmail".into(),
            connector_id: Some("gmail".into()),
            consumer: "floe.builtin.schedule".into(),
            purpose: "scheduling".into(),
            connection_revision: None,
            reviewed_producer_fingerprint: Some(self.transport.producer.fingerprint.clone()),
            reviewed_native_subject: None,
            members,
        }
    }
}

struct RemoteTestOwners<'a> {
    core: &'a crate::FloeCore,
    vault: &'a EncryptedAgentVault<Keys>,
    store: &'a floe_provider_adapters::control::CurrentSavedConnectionStore,
    calendar: &'a FixtureCalendarSubject,
    personal: &'a FixturePersonalInspector,
    transport: &'a ScriptedRemoteTransport,
    deadline: tokio::time::Instant,
}

impl RemoteTestOwners<'_> {
    fn host(
        &self,
    ) -> HostInteractionOwners<'_, Keys, FixtureCalendarSubject, FixturePersonalInspector> {
        HostInteractionOwners {
            core: self.core,
            vault: self.vault,
            connections: self.store,
            calendar_subject: self.calendar,
            personal_subject: self.personal,
            probe_deadline: self.deadline,
        }
    }
}

impl ObserveStateReader for RemoteTestOwners<'_> {
    fn expert_review_current<'a>(
        &'a self,
        interaction: &'a floe_conversation::ConversationInteraction,
    ) -> BoxFuture<'a, Result<bool, AgentFailure>> {
        Box::pin(async move { self.host().expert_review_current(interaction).await })
    }

    fn read_live_inline<'a>(
        &'a self,
        target: &'a floe_conversation::InlineObserveTarget,
        person_id: PersonId,
        device_id: &'a str,
        cancellation: &'a floe_execution::Cancellation,
    ) -> BoxFuture<'a, Result<LiveInlineState, AgentFailure>> {
        Box::pin(async move {
            self.host()
                .read_live_inline(target, person_id, device_id, cancellation)
                .await
        })
    }

    fn navigation_connection_usable<'a>(
        &'a self,
        target: &'a floe_conversation::NavigationOnlyTarget,
        person_id: PersonId,
    ) -> BoxFuture<'a, Result<bool, AgentFailure>> {
        Box::pin(async move {
            self.host()
                .navigation_connection_usable(target, person_id)
                .await
        })
    }

    fn navigation_satisfied<'a>(
        &'a self,
        target: &'a floe_conversation::NavigationOnlyTarget,
        person_id: PersonId,
        device_id: &'a str,
        cancellation: &'a floe_execution::Cancellation,
    ) -> BoxFuture<'a, Result<bool, AgentFailure>> {
        Box::pin(async move {
            self.host()
                .navigation_satisfied(target, person_id, device_id, cancellation)
                .await
        })
    }
}

impl RecipientConsentOwner for RemoteTestOwners<'_> {
    fn grant_reviewed<'a>(
        &'a self,
        target: &'a floe_conversation::RecipientConsentTarget,
        person_id: PersonId,
        device_id: &'a str,
        now_unix_ms: i64,
    ) -> BoxFuture<'a, Result<floe_access::RecipientConsent, AgentFailure>> {
        Box::pin(async move {
            self.host()
                .grant_reviewed(target, person_id, device_id, now_unix_ms)
                .await
        })
    }

    fn usable_consent<'a>(
        &'a self,
        target: &'a floe_conversation::RecipientConsentTarget,
        person_id: PersonId,
        device_id: &'a str,
        now_unix_ms: i64,
    ) -> BoxFuture<'a, Result<bool, AgentFailure>> {
        Box::pin(async move {
            self.host()
                .usable_consent(target, person_id, device_id, now_unix_ms)
                .await
        })
    }
}

impl InlineOwnerMutation for RemoteTestOwners<'_> {
    fn enable_reviewed<'a>(
        &'a self,
        target: &'a floe_conversation::InlineObserveTarget,
        person_id: PersonId,
        device_id: &'a str,
        cancellation: &'a floe_execution::Cancellation,
    ) -> BoxFuture<'a, Result<(), AgentFailure>> {
        Box::pin(async move {
            let connector = target.connector_id.as_deref().unwrap_or("");
            let policies = crate::first_party_observe::remote_policies(connector)?;
            if policies.is_empty() {
                let host = self.host();
                return host
                    .enable_reviewed(target, person_id, device_id, cancellation)
                    .await;
            }
            let calendar = policies.len() == 1 && policies[0].view_id == "calendar.timeline";
            let resource = if calendar {
                Some(
                    target
                        .members
                        .first()
                        .ok_or(AgentFailure::InvalidInput)?
                        .resource
                        .as_str(),
                )
            } else {
                None
            };
            let person_text = person_id.to_string();
            let window = floe_access::RemoteCallWindow {
                deadline: self.deadline,
                cancellation: cancellation.clone(),
            };
            let pairing = floe_access::RemotePairingIdentity {
                person_id: &person_text,
                device_id,
                client_id: REMOTE_CLIENT_ID,
            };
            let ctx = RemoteObserveContext {
                core: self.core,
                vault: self.vault,
                person_id,
                pairing,
                connector_id: connector,
                connection_id: target.connection_id.as_str(),
                resource,
                window: &window,
            };
            enable_remote_reviewed(&ctx, self.transport, target).await
        })
    }
}

#[tokio::test]
async fn gmail_views_allow_enables_bundle_atomically_and_resolves() {
    let host = RemoteFixture::open().await;
    let target = host.gmail_target();
    let current = host
        .base
        .seed_inline(target, "floe.source.gmail", host.connection_id.as_str())
        .await;
    let calendar = FixtureCalendarSubject {
        fingerprint: NATIVE_FINGERPRINT.into(),
    };
    let personal = FixturePersonalInspector {
        fingerprint: NATIVE_FINGERPRINT.into(),
    };
    let owners = host.owners(&calendar, &personal);
    let outcome = resolve_interaction(
        &host.base.runs,
        &host.base.repo,
        &owners,
        &owners,
        &owners,
        &host.base.caller,
        host.base.resolve_command(
            &current,
            floe_conversation::InteractionDecisionKind::Approve,
        ),
        &host.base.cancellation,
        NOW,
    )
    .await
    .unwrap();
    assert!(
        matches!(outcome, ResolveOutcome::Resolved { .. }),
        "{outcome:?}"
    );
    let grants = host.base.vault.list_data_access_grants(128).await.unwrap();
    assert_eq!(grants.len(), 2);
    for grant in &grants {
        assert_eq!(grant.state(), floe_access::GrantState::Active);
        assert_eq!(grant.source().connector().as_str(), "gmail");
        assert_eq!(grant.source().connection_id().as_str(), host.connection_id);
    }
    let mut resources: Vec<&str> = grants
        .iter()
        .flat_map(|grant| grant.scope().resources().iter().map(|value| value.as_str()))
        .collect();
    resources.sort();
    resources.dedup();
    assert_eq!(resources.len(), 2);
}

#[tokio::test]
async fn gmail_reviewed_absence_policy_fingerprint_drift_supersedes() {
    let host = RemoteFixture::open().await;
    let mut target = host.gmail_target();
    assert!(
        target
            .members
            .iter()
            .all(|member| member.policy_authority.is_none())
    );
    target.members[0].policy_fingerprint = "b".repeat(64);
    let current = host
        .base
        .seed_inline(target, "floe.source.gmail", host.connection_id.as_str())
        .await;
    let calendar = FixtureCalendarSubject {
        fingerprint: NATIVE_FINGERPRINT.into(),
    };
    let personal = FixturePersonalInspector {
        fingerprint: NATIVE_FINGERPRINT.into(),
    };
    let owners = host.owners(&calendar, &personal);
    let outcome = resolve_interaction(
        &host.base.runs,
        &host.base.repo,
        &owners,
        &owners,
        &owners,
        &host.base.caller,
        host.base.resolve_command(
            &current,
            floe_conversation::InteractionDecisionKind::Approve,
        ),
        &host.base.cancellation,
        NOW,
    )
    .await
    .unwrap();
    assert!(
        matches!(
            outcome,
            ResolveOutcome::Superseded {
                reason: DriftReason::PolicyFingerprint { .. },
                ..
            }
        ),
        "{outcome:?}"
    );
    assert!(
        host.base
            .vault
            .list_data_access_grants(128)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn gmail_connection_review_rejects_changed_policy_before_enable() {
    let host = RemoteFixture::open().await;
    let person_text = host.base.person.to_string();
    let window = floe_access::RemoteCallWindow {
        deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(5),
        cancellation: host.base.cancellation.clone(),
    };
    let ctx = RemoteObserveContext {
        core: &host.core,
        vault: &host.base.vault,
        person_id: host.base.person,
        pairing: floe_access::RemotePairingIdentity {
            person_id: &person_text,
            device_id: DEVICE,
            client_id: REMOTE_CLIENT_ID,
        },
        connector_id: "gmail",
        connection_id: &host.connection_id,
        resource: None,
        window: &window,
    };
    let mut review = super::super::remote_observe::review_bundle(&ctx, &host.transport)
        .await
        .unwrap();
    review.members[0].policy_fingerprint = "b".repeat(64);
    assert!(matches!(
        super::super::remote_observe::enable_bundle(&ctx, &host.transport, &review).await,
        Err(AgentFailure::AccessReviewRequired)
    ));
    assert!(
        host.base
            .vault
            .list_data_access_grants(128)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn manager_mail_read_requires_assistant_in_reviewed_product_policy() {
    let host = RemoteFixture::open().await;
    let person_text = host.base.person.to_string();
    let window = floe_access::RemoteCallWindow {
        deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(5),
        cancellation: host.base.cancellation.clone(),
    };
    let ctx = RemoteObserveContext {
        core: &host.core,
        vault: &host.base.vault,
        person_id: host.base.person,
        pairing: floe_access::RemotePairingIdentity {
            person_id: &person_text,
            device_id: DEVICE,
            client_id: REMOTE_CLIENT_ID,
        },
        connector_id: "microsoft.mail",
        connection_id: &host.connection_id,
        resource: None,
        window: &window,
    };
    let review = super::super::remote_observe::review_bundle(&ctx, &host.transport)
        .await
        .unwrap();
    assert_eq!(review.members.len(), 1);
    super::super::remote_observe::enable_bundle(&ctx, &host.transport, &review)
        .await
        .unwrap();
    let grants = host.base.vault.list_data_access_grants(128).await.unwrap();
    assert_eq!(grants.len(), 1);
    assert!(
        grants[0]
            .scope()
            .consumers()
            .iter()
            .any(|consumer| consumer.identifier() == "assistant")
    );
    let direct = floe_context::read_remote_view(
        host.base.vault.as_ref(),
        &host.transport,
        host.base.person,
        floe_access::RemotePairingIdentity { person_id: &person_text, client_id: REMOTE_CLIENT_ID, device_id: DEVICE },
        floe_context::MAIL_VIEW,
        "assistant",
        serde_json::json!({"schema_version": floe_agent_contract::AGENT_VERSION, "query": "", "cursor": 0, "limit": 25}),
        &window,
        Uuid::new_v4(),
        &[7; 32],
    ).await.unwrap();
    assert!(
        matches!(direct, floe_context_contract::SourceReadOutcome::Ready(_)),
        "{direct:?}"
    );
    let local_context = crate::local_context::LocalContextHost::default();
    let tools = floe_context::ContextToolService::new(
        host.base.person,
        DEVICE,
        floe_vault::VaultGrantRecords::new(&host.base.vault),
        crate::vault_host::personal_grants::native_driver(&local_context),
        Some(ProductMailReader {
            vault: &host.base.vault,
            transport: &host.transport,
            person_id: host.base.person,
        }),
    )
    .unwrap();
    let ledger = floe_execution::budget::BudgetLedger::new(
        floe_execution::budget::BudgetConfig::new(100, 100),
        Default::default(),
    );
    let scope = floe_execution::ExecutionScope::root(
        floe_execution::Cancellation::default(),
        tokio::time::Instant::now() + std::time::Duration::from_secs(30),
        ledger.work_lease(),
        floe_agent_contract::TraceContext::new(Uuid::new_v4()),
    );
    let outcome = tools
        .invoke_outcome(
            &floe_agent_contract::ToolCall {
                call_id: Uuid::new_v4(),
                invocation_key: floe_agent_contract::InvocationKey::new(),
                tool_id: floe_context::MAIL_COMMUNICATION_READ.into(),
                definition_revision: floe_context::MANAGER_TOOL_DEFINITION_REVISION,
                input: r#"{"query":""}"#.into(),
            },
            &scope,
        )
        .await
        .unwrap();
    let floe_context_contract::SourceReadOutcome::Ready(result) = outcome else {
        panic!("manager mail read must be Ready");
    };
    let floe_agent_contract::DependencyCoverage::Dependent { dependencies } = result.coverage
    else {
        panic!("mail read must retain coverage");
    };
    assert_eq!(dependencies.len(), 1);
    assert_eq!(dependencies[0].consumer().identifier(), "assistant");
    assert_eq!(
        dependencies[0].operation(),
        floe_context_contract::GrantOperation::Read
    );
    assert_eq!(
        dependencies[0].resources()[0].as_str(),
        floe_context::remote_view_resource(floe_context::MAIL_VIEW, &host.connection_id)
    );
}

#[tokio::test]
async fn gmail_authority_rotation_after_review_supersedes_without_mutation() {
    let host = RemoteFixture::open().await;
    let target = host.gmail_target();
    let current = host
        .base
        .seed_inline(target, "floe.source.gmail", host.connection_id.as_str())
        .await;
    // The server rotates its source authority after the review. The local
    // re-read cannot observe it, but the enable's fresh bundle can, so the
    // owner refuses and the card supersedes instead of widening.
    *host.transport.authority.lock().unwrap() = floe_context_contract::SourceAuthority::new();
    let calendar = FixtureCalendarSubject {
        fingerprint: NATIVE_FINGERPRINT.into(),
    };
    let personal = FixturePersonalInspector {
        fingerprint: NATIVE_FINGERPRINT.into(),
    };
    let owners = host.owners(&calendar, &personal);
    let outcome = resolve_interaction(
        &host.base.runs,
        &host.base.repo,
        &owners,
        &owners,
        &owners,
        &host.base.caller,
        host.base.resolve_command(
            &current,
            floe_conversation::InteractionDecisionKind::Approve,
        ),
        &host.base.cancellation,
        NOW,
    )
    .await
    .unwrap();
    match outcome {
        ResolveOutcome::Superseded { reason, .. } => {
            assert_eq!(reason, DriftReason::OwnerObservedDrift);
        }
        other => panic!("must supersede: {other:?}"),
    }
    let grants = host.base.vault.list_data_access_grants(128).await.unwrap();
    assert!(grants.is_empty());
}

#[tokio::test]
async fn gmail_bad_signature_never_mutates_nor_resolves() {
    let host = RemoteFixture::open().await;
    let target = host.gmail_target();
    let current = host
        .base
        .seed_inline(target, "floe.source.gmail", host.connection_id.as_str())
        .await;
    *host.transport.corrupt_signature.lock().unwrap() = true;
    let calendar = FixtureCalendarSubject {
        fingerprint: NATIVE_FINGERPRINT.into(),
    };
    let personal = FixturePersonalInspector {
        fingerprint: NATIVE_FINGERPRINT.into(),
    };
    let owners = host.owners(&calendar, &personal);
    let outcome = resolve_interaction(
        &host.base.runs,
        &host.base.repo,
        &owners,
        &owners,
        &owners,
        &host.base.caller,
        host.base.resolve_command(
            &current,
            floe_conversation::InteractionDecisionKind::Approve,
        ),
        &host.base.cancellation,
        NOW,
    )
    .await
    .unwrap();
    // The previews fail verification: the claimed decision waits in
    // Resolving for recovery, with no mutation and no false resolution.
    let resolving = match outcome {
        ResolveOutcome::Resolving { interaction } => interaction,
        other => panic!("must stay resolving: {other:?}"),
    };
    assert!(matches!(
        resolving.state,
        floe_conversation::InteractionState::Resolving { .. }
    ));
    let grants = host.base.vault.list_data_access_grants(128).await.unwrap();
    assert!(grants.is_empty());
}

#[tokio::test]
async fn remote_calendar_allow_resolves_through_hosted_connection() {
    let host = RemoteFixture::open().await;
    host.core
        .set_calendar_scope(
            host.base.person,
            host.connection_id.clone(),
            9,
            DEVICE.into(),
            floe_context_contract::CalendarProvider::Google,
            vec![floe_day::CalendarSelection {
                calendar_id: "primary".into(),
                calendar_name: "Primary".into(),
            }],
            floe_context_contract::CalendarScope::Selected,
        )
        .await
        .unwrap();
    let authority = *host.transport.authority.lock().unwrap();
    let target = floe_conversation::InlineObserveTarget {
        connection_id: host.connection_id.clone(),
        device_id: Some(DEVICE.into()),
        source_id: "floe.source.calendar".into(),
        connector_id: Some("calendar.google".into()),
        consumer: "floe.builtin.schedule".into(),
        purpose: "scheduling".into(),
        connection_revision: Some(9),
        reviewed_producer_fingerprint: Some(host.transport.producer.fingerprint.clone()),
        reviewed_native_subject: None,
        members: vec![floe_conversation::ReviewedBundleMember {
            member_id: "calendar.timeline".into(),
            policy_fingerprint: crate::first_party_observe::member_policy_fingerprint(
                "calendar.google",
                "calendar.timeline",
            )
            .unwrap(),
            resource: "primary".into(),
            source_revision: Some(floe_conversation::AuthorityRevision {
                incarnation: authority.incarnation(),
                epoch: authority.epoch().get(),
            }),
            expected_grant: floe_conversation::ExpectedGrantState::Absent,
            policy_authority: None,
        }],
    };
    let current = host
        .base
        .seed_inline(target, "floe.source.calendar", host.connection_id.as_str())
        .await;
    let calendar = FixtureCalendarSubject {
        fingerprint: NATIVE_FINGERPRINT.into(),
    };
    let personal = FixturePersonalInspector {
        fingerprint: NATIVE_FINGERPRINT.into(),
    };
    let owners = host.owners(&calendar, &personal);
    let outcome = resolve_interaction(
        &host.base.runs,
        &host.base.repo,
        &owners,
        &owners,
        &owners,
        &host.base.caller,
        host.base.resolve_command(
            &current,
            floe_conversation::InteractionDecisionKind::Approve,
        ),
        &host.base.cancellation,
        NOW,
    )
    .await
    .unwrap();
    assert!(
        matches!(outcome, ResolveOutcome::Resolved { .. }),
        "{outcome:?}"
    );
    let grants = host.base.vault.list_data_access_grants(128).await.unwrap();
    assert_eq!(grants.len(), 1);
    assert_eq!(grants[0].source().connector().as_str(), "calendar.google");
    assert_eq!(
        grants[0].source().connection_id().as_str(),
        host.connection_id
    );
}

#[tokio::test]
async fn gmail_reviewed_subset_supersedes_on_canonical_extras() {
    let host = RemoteFixture::open().await;
    let mut target = host.gmail_target();
    // The review covers only one of the two canonical views: the live
    // bundle is wider than reviewed, so the card supersedes instead of
    // widening.
    target.members.pop();
    assert_eq!(target.members.len(), 1);
    let current = host
        .base
        .seed_inline(target, "floe.source.gmail", host.connection_id.as_str())
        .await;
    let calendar = FixtureCalendarSubject {
        fingerprint: NATIVE_FINGERPRINT.into(),
    };
    let personal = FixturePersonalInspector {
        fingerprint: NATIVE_FINGERPRINT.into(),
    };
    let owners = host.owners(&calendar, &personal);
    let outcome = resolve_interaction(
        &host.base.runs,
        &host.base.repo,
        &owners,
        &owners,
        &owners,
        &host.base.caller,
        host.base.resolve_command(
            &current,
            floe_conversation::InteractionDecisionKind::Approve,
        ),
        &host.base.cancellation,
        NOW,
    )
    .await
    .unwrap();
    match outcome {
        ResolveOutcome::Superseded { reason, .. } => {
            assert_eq!(reason, DriftReason::MemberSet);
        }
        other => panic!("must supersede: {other:?}"),
    }
    let grants = host.base.vault.list_data_access_grants(128).await.unwrap();
    assert!(grants.is_empty());
}

#[tokio::test]
async fn native_grant_paused_out_of_band_supersedes() {
    let host = HostFixture::open().await;
    let target = host.native_target(NATIVE_FINGERPRINT).await;
    let current = host
        .base
        .seed_inline(target.clone(), "floe.source.calendar", "connection")
        .await;
    let calendar = FixtureCalendarSubject {
        fingerprint: NATIVE_FINGERPRINT.into(),
    };
    let personal = FixturePersonalInspector {
        fingerprint: NATIVE_FINGERPRINT.into(),
    };
    let owners = host.owners(&calendar, &personal);
    owners
        .enable_reviewed(&target, host.base.person, DEVICE, &host.base.cancellation)
        .await
        .unwrap();
    let first = host.base.vault.list_data_access_grants(128).await.unwrap();
    assert_eq!(first.len(), 1);
    // A second card reviews the live grant; pausing it out-of-band
    // invalidates that review, because a paused grant authorizes nothing.
    let mut paused_target = host.native_target(NATIVE_FINGERPRINT).await;
    paused_target.members[0].expected_grant = floe_conversation::ExpectedGrantState::Active {
        grant_id: first[0].id().as_uuid(),
        authority_incarnation: first[0].authority().incarnation(),
        authority_epoch: first[0].authority().access_epoch().get(),
    };
    let paused = host
        .base
        .seed_inline(paused_target, "floe.source.calendar", "connection")
        .await;
    host.base
        .vault
        .pause_data_access_grant(first[0].id(), first[0].authority())
        .await
        .unwrap();
    let outcome = resolve_interaction(
        &host.base.runs,
        &host.base.repo,
        &owners,
        &owners,
        &owners,
        &host.base.caller,
        host.base
            .resolve_command(&paused, floe_conversation::InteractionDecisionKind::Approve),
        &host.base.cancellation,
        NOW,
    )
    .await
    .unwrap();
    match outcome {
        ResolveOutcome::Superseded { reason, .. } => {
            assert!(
                matches!(reason, DriftReason::GrantState { .. }),
                "{reason:?}"
            );
        }
        other => panic!("must supersede: {other:?}"),
    }
    assert_ne!(paused.id, current.id);
}

#[tokio::test]
async fn same_command_different_kind_conflicts() {
    let fixture = Fixture::open().await;
    let current = fixture
        .seed_inline(reviewed_target(), "floe.source.calendar", "connection")
        .await;
    let owners = ScriptedOwners::new(live_precondition());
    let mut command =
        fixture.resolve_command(&current, floe_conversation::InteractionDecisionKind::Deny);
    let outcome = resolve_interaction(
        &fixture.runs,
        &fixture.repo,
        &owners,
        &owners,
        &owners,
        &fixture.caller,
        command.clone(),
        &fixture.cancellation,
        NOW,
    )
    .await
    .unwrap();
    assert!(
        matches!(outcome, ResolveOutcome::Denied { .. }),
        "{outcome:?}"
    );
    // The same command with a different decision conflicts: terminal
    // states never reopen.
    command.kind = floe_conversation::InteractionDecisionKind::Approve;
    command.expected_revision = current.revision;
    let outcome = resolve_interaction(
        &fixture.runs,
        &fixture.repo,
        &owners,
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
        matches!(
            outcome,
            ResolveOutcome::Stale { .. } | ResolveOutcome::Terminal { .. }
        ),
        "{outcome:?}"
    );
}

#[tokio::test]
async fn gmail_commit_then_crash_reopens_and_resolves_without_second_mutation() {
    let host = RemoteFixture::open().await;
    let target = host.gmail_target();
    let current = host
        .base
        .seed_inline(
            target.clone(),
            "floe.source.gmail",
            host.connection_id.as_str(),
        )
        .await;
    let calendar = FixtureCalendarSubject {
        fingerprint: NATIVE_FINGERPRINT.into(),
    };
    let personal = FixturePersonalInspector {
        fingerprint: NATIVE_FINGERPRINT.into(),
    };
    let owners = host.owners(&calendar, &personal);
    // Decide first (Resolving claimed), then commit the bundle through
    // the canonical enable, then crash before resolution is recorded.
    let command = host.base.resolve_command(
        &current,
        floe_conversation::InteractionDecisionKind::Approve,
    );
    let admitted = floe_conversation::decide_interaction(
        &host.base.repo,
        floe_conversation::DecideInteractionCommand {
            command_id: command.command_id,
            interaction_id: current.id,
            principal: host.base.principal(),
            expected_revision: current.revision,
            kind: floe_conversation::InteractionDecisionKind::Approve,
            target_digest: current.target_digest,
        },
        NOW,
    )
    .await
    .unwrap();
    let resolving = match admitted {
        floe_conversation::DecisionAdmission::Applied(current) => current,
        floe_conversation::DecisionAdmission::Rejoined(_) => panic!("fresh decision must apply"),
    };
    owners
        .enable_reviewed(&target, host.base.person, DEVICE, &host.base.cancellation)
        .await
        .unwrap();
    let committed = host.base.vault.list_data_access_grants(128).await.unwrap();
    assert_eq!(committed.len(), 2);

    // Crash: drop the vault and repo, reopen from disk, and reconcile.
    // The server side (scripted transport) survives unchanged.
    let RemoteFixture {
        base,
        core: old_core,
        store,
        transport,
        connection_id: _,
    } = host;
    drop(old_core);
    let Fixture {
        runs,
        repo: old_repo,
        vault: old_vault,
        keys,
        person,
        session_id,
        run_id: _run_id,
        caller,
        cancellation,
        _root,
    } = base;
    drop(old_repo);
    drop(old_vault);
    let reopened = Arc::new(
        EncryptedAgentVault::open(_root.path(), person, keys.clone())
            .await
            .unwrap(),
    );
    let repo = floe_vault::VaultConversationRepository::new(Arc::clone(&reopened));
    let core = crate::FloeCore::open(":memory:").await.unwrap();
    let owners = RemoteTestOwners {
        core: &core,
        vault: &reopened,
        store: &store,
        calendar: &calendar,
        personal: &personal,
        transport: &transport,
        deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(5),
    };
    let outcome = refresh_interaction(
        &runs,
        &repo,
        &owners,
        &owners,
        &owners,
        &caller,
        RefreshInteractionCommand {
            interaction_id: resolving.id,
            command_id: Uuid::new_v4(),
            session_id,
            expected_revision: resolving.revision,
        },
        &cancellation,
        NOW,
    )
    .await
    .unwrap();
    assert!(
        matches!(outcome, RefreshOutcome::Resolved { .. }),
        "{outcome:?}"
    );
    let grants = reopened.list_data_access_grants(128).await.unwrap();
    assert_eq!(grants.len(), 2);
    for grant in &grants {
        assert!(
            committed
                .iter()
                .any(|before| before.id() == grant.id() && before.authority() == grant.authority()),
            "no second mutation: {grant:?}"
        );
    }
}

#[tokio::test]
async fn observer_cancellation_never_revokes_a_recorded_decision() {
    let fixture = Fixture::open().await;
    let current = fixture
        .seed_inline(reviewed_target(), "floe.source.calendar", "connection")
        .await;
    // Observer timeout cancels the watch, not the person's explicit
    // decision: the decision still records and the owner operation still
    // runs through reconciliation.
    let cancelled = floe_execution::Cancellation::default();
    cancelled.cancel();
    let owners = ScriptedOwners::new(live_precondition());
    let mutation = FlippingMutation {
        owners: owners.clone(),
        flip_to: live_satisfied(),
    };
    let outcome = resolve_interaction(
        &fixture.runs,
        &fixture.repo,
        &owners,
        &mutation,
        &owners,
        &fixture.caller,
        fixture.resolve_command(
            &current,
            floe_conversation::InteractionDecisionKind::Approve,
        ),
        &cancelled,
        NOW,
    )
    .await
    .unwrap();
    assert!(
        matches!(outcome, ResolveOutcome::Resolved { .. }),
        "{outcome:?}"
    );
}

#[tokio::test]
async fn owner_refusal_on_matching_precondition_supersedes() {
    let fixture = Fixture::open().await;
    let current = fixture
        .seed_inline(reviewed_target(), "floe.source.calendar", "connection")
        .await;
    // The owner refuses even though the local re-read matches: it judged
    // fresher evidence (remote rotation the local read cannot see), so
    // the review is stale by definition.
    let owners = ScriptedOwners::new(live_precondition());
    owners
        .script
        .lock()
        .unwrap()
        .mutation_results
        .push_back(Err(AgentFailure::AccessReviewRequired));
    let outcome = resolve_interaction(
        &fixture.runs,
        &fixture.repo,
        &owners,
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
        ResolveOutcome::Superseded { reason, .. } => {
            assert_eq!(reason, DriftReason::OwnerObservedDrift);
        }
        other => panic!("must supersede: {other:?}"),
    }
}

#[tokio::test]
async fn consent_approve_grants_exact_review_and_resolves() {
    let fixture = Fixture::open().await;
    let owners = ScriptedOwners::new(live_precondition());
    let current = fixture.seed_consent().await;
    let command = fixture.resolve_command(
        &current,
        floe_conversation::InteractionDecisionKind::Approve,
    );
    let outcome = resolve_interaction(
        &fixture.runs,
        &fixture.repo,
        &owners,
        &owners,
        &owners,
        &fixture.caller,
        command,
        &fixture.cancellation,
        NOW,
    )
    .await
    .unwrap();
    let resolved = match outcome {
        ResolveOutcome::Resolved { interaction } => interaction,
        other => panic!("must resolve: {other:?}"),
    };
    assert!(matches!(
        resolved.state,
        floe_conversation::InteractionState::Resolved { .. }
    ));
    let script = owners.script.lock().unwrap();
    assert_eq!(script.consent_grants, 1);
    assert_eq!(script.consents.len(), 1);
    let granted = script.consents.values().next().unwrap();
    let floe_conversation::ReviewedTarget::RecipientConsent(target) = &current.target else {
        panic!("seeded consent target");
    };
    assert_eq!(granted.person_id(), fixture.person);
    assert_eq!(granted.device_id(), DEVICE);
    assert_eq!(granted.recipient(), target.recipient);
    assert_eq!(granted.profile_id(), target.profile_id);
    assert_eq!(granted.purpose(), target.purpose);
    assert_eq!(granted.consumer(), target.consumer);
    assert_eq!(granted.input_data_classes(), target.input_data_classes);
    assert_eq!(granted.lineage(), target.lineage);
    assert_eq!(granted.revision(), 1);
}

#[tokio::test]
async fn consent_rejoined_command_rejoins_same_consent() {
    let fixture = Fixture::open().await;
    let owners = ScriptedOwners::new(live_precondition());
    let current = fixture.seed_consent().await;
    // A crash between decision and grant leaves Resolving; the same command
    // rejoins the claimed operation instead of granting twice.
    let decision = floe_conversation::DecideInteractionCommand {
        command_id: Uuid::new_v4(),
        interaction_id: current.id,
        principal: fixture.principal(),
        expected_revision: current.revision,
        kind: floe_conversation::InteractionDecisionKind::Approve,
        target_digest: current.target_digest,
    };
    let floe_conversation::DecisionAdmission::Applied(_) =
        floe_conversation::decide_interaction(&fixture.repo, decision.clone(), NOW)
            .await
            .unwrap()
    else {
        panic!("decision must apply");
    };
    let command = ResolveInteractionCommand {
        interaction_id: current.id,
        command_id: decision.command_id,
        session_id: fixture.session_id,
        expected_revision: current.revision,
        kind: floe_conversation::InteractionDecisionKind::Approve,
        target_digest: current.target_digest,
    };
    for _ in 0..2 {
        let outcome = resolve_interaction(
            &fixture.runs,
            &fixture.repo,
            &owners,
            &owners,
            &owners,
            &fixture.caller,
            command.clone(),
            &fixture.cancellation,
            NOW,
        )
        .await
        .unwrap();
        assert!(matches!(outcome, ResolveOutcome::Resolved { .. }));
    }
    let script = owners.script.lock().unwrap();
    assert_eq!(script.consent_grants, 1);
    assert_eq!(script.consents.len(), 1);
}

#[tokio::test]
async fn consent_deny_and_dismiss_mutate_nothing() {
    for kind in [
        floe_conversation::InteractionDecisionKind::Deny,
        floe_conversation::InteractionDecisionKind::Dismiss,
    ] {
        let fixture = Fixture::open().await;
        let owners = ScriptedOwners::new(live_precondition());
        let current = fixture.seed_consent().await;
        let outcome = resolve_interaction(
            &fixture.runs,
            &fixture.repo,
            &owners,
            &owners,
            &owners,
            &fixture.caller,
            fixture.resolve_command(&current, kind),
            &fixture.cancellation,
            NOW,
        )
        .await
        .unwrap();
        assert!(matches!(
            outcome,
            ResolveOutcome::Denied { .. } | ResolveOutcome::Cancelled { .. }
        ));
        let script = owners.script.lock().unwrap();
        assert_eq!(script.consent_grants, 0);
        assert!(script.consents.is_empty());
    }
}

#[tokio::test]
async fn consent_grant_failure_fails_closed_and_refresh_reconciles() {
    let fixture = Fixture::open().await;
    let owners = ScriptedOwners::new(live_precondition());
    owners
        .script
        .lock()
        .unwrap()
        .grant_failures
        .push_back(AgentFailure::PolicyDenied);
    let current = fixture.seed_consent().await;
    let outcome = resolve_interaction(
        &fixture.runs,
        &fixture.repo,
        &owners,
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
    assert!(matches!(outcome, Err(AgentFailure::PolicyDenied)));
    // No resolution without the consent: the card stays Resolving.
    let stored = floe_conversation::InteractionRepository::get_interaction(
        &fixture.repo,
        fixture.person,
        current.id,
    )
    .await
    .unwrap()
    .unwrap();
    assert!(matches!(
        stored.state,
        floe_conversation::InteractionState::Resolving { .. }
    ));
    // Once the owner succeeds, refresh reconciles the claimed operation.
    let outcome = refresh_interaction(
        &fixture.runs,
        &fixture.repo,
        &owners,
        &owners,
        &owners,
        &fixture.caller,
        fixture.refresh_command(&stored),
        &fixture.cancellation,
        NOW,
    )
    .await
    .unwrap();
    assert!(matches!(outcome, RefreshOutcome::Resolved { .. }));
    let script = owners.script.lock().unwrap();
    assert_eq!(script.consent_grants, 2);
    assert_eq!(script.consents.len(), 1);
}

#[tokio::test]
async fn consent_refresh_settles_existing_usable_consent_without_new_grant() {
    let fixture = Fixture::open().await;
    let owners = ScriptedOwners::new(live_precondition());
    let current = fixture.seed_consent().await;
    // An identical parallel review granted first: refresh settles without
    // granting again.
    let floe_conversation::ReviewedTarget::RecipientConsent(target) = &current.target else {
        panic!("seeded consent target");
    };
    owners
        .grant_reviewed(target, fixture.person, DEVICE, NOW)
        .await
        .unwrap();
    let outcome = refresh_interaction(
        &fixture.runs,
        &fixture.repo,
        &owners,
        &owners,
        &owners,
        &fixture.caller,
        fixture.refresh_command(&current),
        &fixture.cancellation,
        NOW,
    )
    .await
    .unwrap();
    assert!(matches!(outcome, RefreshOutcome::Resolved { .. }));
    let script = owners.script.lock().unwrap();
    assert_eq!(script.consent_grants, 1);
    assert!(script.consent_checks >= 1);
}

#[tokio::test]
async fn consent_wrong_device_never_grants() {
    let fixture = Fixture::open().await;
    let owners = ScriptedOwners::new(live_precondition());
    let current = fixture.seed_consent().await;
    let foreign_device = crate::CallerContext::verified(
        crate::LocalIdentityClaim {
            person_id: fixture.person.0,
            device_id: "foreign-device".into(),
        },
        1,
    )
    .unwrap();
    let outcome = resolve_interaction(
        &fixture.runs,
        &fixture.repo,
        &owners,
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
    assert!(matches!(outcome, ResolveOutcome::WrongDevice { .. }));
    let script = owners.script.lock().unwrap();
    assert_eq!(script.consent_grants, 0);
    assert!(script.consents.is_empty());
}
