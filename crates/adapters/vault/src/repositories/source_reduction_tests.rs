//! Real encrypted Connections + Access stores with synthetic unavailable external ports.
use crate::{EncryptedAgentVault, ProductStoreIdentity, RootKey, TursoStore, VaultKeyProvider};
use floe_access::*;
use floe_connections::*;
use floe_execution::{
    BoxFuture, Cancellation, ExecutionScope,
    budget::{BudgetConfig, BudgetLedger, ModelUsage},
};
use floe_kernel::{AgentFailure, OwnerActor, PersonId, TraceContext};
use std::{
    collections::HashMap,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};
use uuid::Uuid;

struct NoExternal;
impl GatewayPairingPort for NoExternal {
    fn prepare_start<'a>(
        &'a self,
        _request: PairingStartRequest,
        _scope: &'a OperationScope,
    ) -> BoxFuture<'a, Result<PreparedPairingStart, PairingError>> {
        Box::pin(async { panic!("unrelated external port called during local reduction") })
    }
    fn start<'a>(
        &'a self,
        _prepared: PreparedPairingStart,
        _scope: &'a OperationScope,
    ) -> BoxFuture<'a, Result<StartedPairing, PairingError>> {
        Box::pin(async { panic!("unrelated external port called during local reduction") })
    }
    fn confirm<'a>(
        &'a self,
        _request: PairingConfirmation,
        _scope: &'a OperationScope,
    ) -> BoxFuture<'a, Result<PairingObservation, PairingError>> {
        Box::pin(async { panic!("unrelated external port called during local reduction") })
    }
    fn observe<'a>(
        &'a self,
        _pairing: &'a PairingHandle,
        _scope: &'a OperationScope,
    ) -> BoxFuture<'a, Result<PairingObservation, PairingError>> {
        Box::pin(async { panic!("unrelated external port called during local reduction") })
    }
    fn cancel<'a>(
        &'a self,
        _pairing: &'a PairingHandle,
        _scope: &'a OperationScope,
    ) -> BoxFuture<'a, Result<PairingObservation, PairingError>> {
        Box::pin(async { panic!("unrelated external port called during local reduction") })
    }
}
impl RemoteIntegrationPort for NoExternal {
    fn list<'a>(
        &'a self,
        _query: IntegrationCatalogQuery,
        _scope: &'a OperationScope,
    ) -> BoxFuture<'a, Result<IntegrationCatalog, IntegrationError>> {
        Box::pin(async { panic!("unrelated external port called during local reduction") })
    }
    fn begin<'a>(
        &'a self,
        _command: BeginIntegration,
        _scope: &'a OperationScope,
    ) -> BoxFuture<'a, Result<IntegrationOperation, IntegrationError>> {
        Box::pin(async { panic!("unrelated external port called during local reduction") })
    }
    fn observe<'a>(
        &'a self,
        _operation: &'a IntegrationOperationRef,
        _scope: &'a OperationScope,
    ) -> BoxFuture<'a, Result<IntegrationOperation, IntegrationError>> {
        Box::pin(async { panic!("unrelated external port called during local reduction") })
    }
    fn cancel<'a>(
        &'a self,
        _command: CancelIntegration,
        _scope: &'a OperationScope,
    ) -> BoxFuture<'a, Result<IntegrationOperation, IntegrationError>> {
        Box::pin(async { panic!("unrelated external port called during local reduction") })
    }
    fn configure<'a>(
        &'a self,
        _command: ConfigureIntegration,
        _scope: &'a OperationScope,
    ) -> BoxFuture<'a, Result<IntegrationSnapshot, IntegrationError>> {
        Box::pin(async { panic!("unrelated external port called during local reduction") })
    }
    fn disconnect<'a>(
        &'a self,
        _command: DisconnectIntegration,
        _scope: &'a OperationScope,
    ) -> BoxFuture<'a, Result<IntegrationSnapshot, IntegrationError>> {
        Box::pin(async { panic!("unrelated external port called during local reduction") })
    }
    fn management_launch<'a>(
        &'a self,
        _request: ManagementLaunchRequest,
        _scope: &'a OperationScope,
    ) -> BoxFuture<'a, Result<ValidatedManagementLaunch, IntegrationError>> {
        Box::pin(async { panic!("unrelated external port called during local reduction") })
    }
}
impl SourceCatalogPort for NoExternal {
    fn inspect<'a>(
        &'a self,
        _actor: &'a floe_kernel::OwnerActor,
        _source: &'a SourceConnection,
        _scope: &'a OperationScope,
    ) -> BoxFuture<'a, Result<SourceCatalogObservation, floe_kernel::AgentFailure>> {
        Box::pin(async { panic!("unrelated external port called during local reduction") })
    }
}
impl NativeSourceSetupPort for NoExternal {
    fn availability<'a>(
        &'a self,
        _actor: &'a floe_kernel::OwnerActor,
        _connector_id: &'a ConnectorId,
        _scope: &'a OperationScope,
    ) -> BoxFuture<'a, Result<NativeAvailabilityObservation, floe_kernel::AgentFailure>> {
        Box::pin(async { panic!("unrelated external port called during local reduction") })
    }
    fn request_permission<'a>(
        &'a self,
        _actor: &'a floe_kernel::OwnerActor,
        _request: NativeSetupRequest,
        _scope: &'a OperationScope,
    ) -> BoxFuture<'a, Result<NativeSetupObservation, floe_kernel::AgentFailure>> {
        Box::pin(async { panic!("unrelated external port called during local reduction") })
    }
}
#[derive(Clone, Default)]
struct Keys(Arc<Mutex<HashMap<(PersonId, Uuid), [u8; 32]>>>);
impl VaultKeyProvider for Keys {
    fn load(&self, person: PersonId, vault: Uuid) -> Result<RootKey, AgentFailure> {
        self.0
            .lock()
            .unwrap()
            .get(&(person, vault))
            .copied()
            .map(RootKey::from_bytes)
            .ok_or(AgentFailure::VaultUnavailable)
    }
    fn insert(&self, person: PersonId, vault: Uuid, key: &RootKey) -> Result<(), AgentFailure> {
        self.0
            .lock()
            .unwrap()
            .insert((person, vault), *key.as_bytes());
        Ok(())
    }
}
struct Root(PathBuf);
impl Root {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("floe-source-reduction-{}", Uuid::new_v4()));
        std::fs::create_dir(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        Self(path)
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
struct Catalog(Vec<TrustedConsumerRegistration>);
impl TrustedConsumerCatalog for Catalog {
    fn registrations(&self) -> &[TrustedConsumerRegistration] {
        &self.0
    }
}
struct Evidence {
    result: Mutex<Result<SourceExpectation, AgentFailure>>,
    calls: AtomicUsize,
}
impl SourceReviewEvidence for Evidence {
    fn observe<'a>(
        &'a self,
        _: &'a OwnerActor,
        _: &'a SourceConnection,
        _: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<SourceExpectation, AgentFailure>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { self.result.lock().unwrap().clone() })
    }
    fn inspect_selection<'a>(
        &'a self,
        _: &'a OwnerActor,
        _: &'a SourceConnection,
        _: &'a [ConnectionResource],
        _: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<String, AgentFailure>> {
        Box::pin(async { panic!("no resource selection in reduction") })
    }
}
#[derive(Default)]
struct Cleanup {
    calls: Mutex<Vec<(Uuid, SourceCleanupTarget)>>,
    uncertain: std::sync::atomic::AtomicBool,
}
impl SourceCleanup for Cleanup {
    fn disconnect<'a>(
        &'a self,
        operation: &'a SourceOperationRecord,
        receipt: &'a GrantCommitReceipt,
        _: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<SourceCleanupOutcome, AgentFailure>> {
        Box::pin(async move {
            operation.validate().unwrap();
            receipt.validate().unwrap();
            assert_eq!(receipt.kind, GrantCommitKind::Disconnect);
            assert!(operation.matches_reservation(&receipt.reservation));
            let target = operation
                .cleanup_target
                .clone()
                .expect("durable target before effect");
            self.calls
                .lock()
                .unwrap()
                .push((operation.operation_id, target));
            Ok(if self.uncertain.load(Ordering::SeqCst) {
                SourceCleanupOutcome::Uncertain
            } else {
                SourceCleanupOutcome::Completed
            })
        })
    }
}
fn scope() -> ExecutionScope {
    ExecutionScope::root(
        Cancellation::new(),
        tokio::time::Instant::now() + std::time::Duration::from_secs(60),
        BudgetLedger::new(BudgetConfig::new(0, 0), ModelUsage::default()).root_lease(),
        TraceContext::new(Uuid::new_v4()),
    )
}
struct Fixture {
    vault: Option<Arc<EncryptedAgentVault<Keys>>>,
    store: Option<Arc<TursoStore>>,
    evidence: Arc<Evidence>,
    cleanup: Arc<Cleanup>,
    actor: OwnerActor,
    source: SourceConnection,
    target: SourceExpectation,
    keys: Keys,
    root: Root,
}
impl Fixture {
    async fn new() -> Self {
        let root = Root::new();
        let keys = Keys::default();
        let actor = OwnerActor {
            person_id: PersonId::new(),
            device_id: "reduction.device".into(),
            runtime_epoch: 1,
        };
        std::fs::create_dir(root.0.join("vault")).unwrap();
        std::fs::set_permissions(root.0.join("vault"), std::fs::Permissions::from_mode(0o700))
            .unwrap();
        let vault = Arc::new(
            EncryptedAgentVault::create(&root.0.join("vault"), actor.person_id, keys.clone())
                .await
                .unwrap(),
        );
        let store = Arc::new(
            TursoStore::create_new(
                root.0.join("product.db"),
                ProductStoreIdentity::new(actor.person_id, actor.device_id.clone()).unwrap(),
            )
            .await
            .unwrap(),
        );
        let source = SourceConnection::establish_remote(
            actor.person_id,
            ConnectorId::try_new("gmail").unwrap(),
            ConnectionId::try_new("fixture.mail").unwrap(),
            ExecutionOwnerId::try_new("fixture.server").unwrap(),
            vec![
                ConnectionResource::new(
                    ResourceHandle::try_new("inbox").unwrap(),
                    "Inbox".into(),
                    None,
                )
                .unwrap(),
            ],
            SourceAuthority::new(),
        )
        .unwrap();
        store.create(&source).await.unwrap();
        let target = SourceExpectation {
            source: GrantSourceBinding::try_new(
                actor.person_id,
                source.connection_id().clone(),
                source.connector_id().clone(),
                source.execution_owner_id().clone(),
            )
            .unwrap(),
            revision: Some(1),
            provider_revision: Some(23),
            authority: source.source_authority(),
            physical_resources: vec![ResourceHandle::try_new("inbox").unwrap()],
            subject_fingerprint: "synthetic.subject".into(),
            gateway: Some(VerifiedGatewayBinding {
                person_id: actor.person_id.to_string(),
                device_id: actor.device_id.clone(),
                client_id: "fixture.client".into(),
                producer_instance: "fixture.instance".into(),
                producer_key_fingerprint: "b".repeat(64),
                producer_audience: "fixture.audience".into(),
                enrollment_id: "fixture.enrollment".into(),
                credential_generation: 1,
            }),
        };
        let fixture = Self {
            vault: Some(vault),
            store: Some(store),
            evidence: Arc::new(Evidence {
                result: Mutex::new(Err(AgentFailure::CapabilityUnavailable)),
                calls: AtomicUsize::new(0),
            }),
            cleanup: Arc::new(Cleanup::default()),
            actor,
            source,
            target,
            keys,
            root,
        };
        let access = fixture.access();
        let review = access
            .prepare_review(
                &fixture.actor,
                PrepareConnectionReview {
                    command_id: Uuid::new_v4(),
                    source: fixture.target.clone(),
                    views: vec![ViewReviewRequest {
                        view_id: "mail.communication".into(),
                        expected: ExpectedGrant::Absent {
                            resource: floe_context_contract::connection_view_resource(
                                "mail.communication",
                                fixture.source.connection_id(),
                            )
                            .unwrap(),
                        },
                        consumers: vec![GrantConsumer::builtin("fixture.reader").unwrap()],
                        purpose: GrantPurpose::Assistant,
                        categories: vec![GrantDataCategory::Content],
                        processing: ProcessingRestriction::DeviceOnly,
                    }],
                },
                &scope(),
            )
            .await
            .unwrap();
        access
            .apply_review(
                &fixture.actor,
                review.reference,
                SourceReservationEvidence {
                    operation_id: Uuid::new_v4(),
                    command_id: Uuid::new_v4(),
                    device_id: fixture.actor.device_id.clone(),
                    request_digest: [1; 32],
                    reservation_id: Uuid::new_v4(),
                    reservation_generation: 1,
                    source: fixture.target.clone(),
                },
                &scope(),
            )
            .await
            .unwrap();
        fixture
    }
    fn access(&self) -> Arc<AccessService> {
        Arc::new(AccessService::new(
            self.vault.as_ref().unwrap().clone(),
            Arc::new(Catalog(vec![TrustedConsumerRegistration {
                package_identity: "fixture.reader".into(),
                manifest_revision: 1,
                consumer_identity: GrantConsumer::builtin("fixture.reader").unwrap(),
                declared_view_capabilities: vec![TrustedViewCapability {
                    view_id: "mail.communication".into(),
                    data_class: floe_access::trusted_view_capability("mail.communication")
                        .unwrap()
                        .data_class,
                    categories: vec![GrantDataCategory::Content],
                    purposes: vec![GrantPurpose::Assistant],
                }],
            }])),
            Arc::new(SystemAccessClock),
        ))
    }
    fn service(&self) -> ConnectionsService {
        let vault = self.vault.as_ref().unwrap();
        let no = Arc::new(NoExternal);
        ConnectionsService::new(ConnectionsDependencies {
            sources: self.store.as_ref().unwrap().clone(),
            access: self.access(),
            evidence: self.evidence.clone(),
            cleanup: self.cleanup.clone(),
            pairing: Arc::new(GatewayPairingService::new(vault.clone(), no.clone())),
            gateways: vault.clone(),
            remote_integrations: no.clone(),
            products: vault.clone(),
            source_catalog: no.clone(),
            native_setup: no,
        })
    }
    async fn state(&self) -> GrantState {
        let grants = self
            .vault
            .as_ref()
            .unwrap()
            .snapshot(self.target.source.clone())
            .await
            .unwrap()
            .grants;
        assert_eq!(grants.len(), 1);
        grants[0].state()
    }
    async fn reopen(&mut self) {
        drop(self.vault.take());
        drop(self.store.take());
        self.store = Some(Arc::new(
            TursoStore::open_existing(
                self.root.0.join("product.db"),
                ProductStoreIdentity::new(self.actor.person_id, self.actor.device_id.clone())
                    .unwrap(),
            )
            .await
            .unwrap(),
        ));
        self.vault = Some(Arc::new(
            EncryptedAgentVault::open(
                &self.root.0.join("vault"),
                self.actor.person_id,
                self.keys.clone(),
            )
            .await
            .unwrap(),
        ));
    }
    async fn reserve_interrupted(&self, kind: SourceOperationKind) -> SourceOperationRecord {
        let record = SourceOperationRecord {
            device_id: self.actor.device_id.clone(),
            operation_id: Uuid::new_v4(),
            command_id: Uuid::new_v4(),
            request_digest: [5; 32],
            reservation_id: Uuid::new_v4(),
            reservation_generation: 1,
            expected: SourceOperationExpectation {
                source: self.target.source.clone(),
                revision: Some(1),
                authority: self.source.source_authority(),
            },
            review: None,
            kind,
            phase: SourceOperationPhase::Reserved,
            revision: 1,
            cleanup_target: None,
        };
        self.store
            .as_ref()
            .unwrap()
            .reserve(SourceOperationReservation {
                record: record.clone(),
            })
            .await
            .unwrap();
        record
    }
}
#[tokio::test]
async fn offline_pause_commits_and_exact_replay_survives_reopen() {
    let mut f = Fixture::new().await;
    let command = Uuid::new_v4();
    let result = f
        .service()
        .pause_observe_operation(&f.actor, command, f.source.connection_id(), 1, &scope())
        .await
        .unwrap();
    assert!(matches!(
        result.phase,
        SourceOperationPhase::Completed { .. }
    ));
    assert_eq!(f.state().await, GrantState::Paused);
    assert_eq!(f.evidence.calls.load(Ordering::SeqCst), 0);
    f.reopen().await;
    assert_eq!(
        f.service()
            .pause_observe_operation(&f.actor, command, f.source.connection_id(), 1, &scope())
            .await
            .unwrap(),
        result
    );
    assert_eq!(f.state().await, GrantState::Paused);
    assert_eq!(f.evidence.calls.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn offline_disconnect_revokes_before_cleanup_then_replays_exact_target_after_reopen() {
    let mut f = Fixture::new().await;
    let command = Uuid::new_v4();
    assert!(matches!(
        f.service()
            .disconnect_source_operation(&f.actor, command, f.source.connection_id(), 1, &scope())
            .await,
        Err(ConnectionsCommandFailure::Admitted(
            AgentFailure::CapabilityUnavailable
        ))
    ));
    assert_eq!(f.state().await, GrantState::Revoked);
    assert!(f.cleanup.calls.lock().unwrap().is_empty());
    assert!(
        f.store
            .as_ref()
            .unwrap()
            .source_is_fenced(f.actor.person_id, f.source.connection_id())
            .await
            .unwrap()
    );
    f.reopen().await;
    *f.evidence.result.lock().unwrap() = Ok(f.target.clone());
    f.cleanup.uncertain.store(true, Ordering::SeqCst);
    let pending = f
        .service()
        .disconnect_source_operation(&f.actor, command, f.source.connection_id(), 1, &scope())
        .await
        .unwrap();
    assert!(matches!(
        pending.phase,
        SourceOperationPhase::RepairRequired {
            reason: SourceRepairReason::CleanupUncertain,
            ..
        }
    ));
    assert_eq!(
        pending.cleanup_target,
        Some(SourceCleanupTarget {
            gateway: f.target.gateway.clone().unwrap(),
            provider_revision: 23
        })
    );
    f.reopen().await;
    *f.evidence.result.lock().unwrap() = Err(AgentFailure::CapabilityUnavailable);
    f.cleanup.uncertain.store(false, Ordering::SeqCst);
    let before = f.evidence.calls.load(Ordering::SeqCst);
    let done = f
        .service()
        .disconnect_source_operation(&f.actor, command, f.source.connection_id(), 1, &scope())
        .await
        .unwrap();
    assert!(matches!(done.phase, SourceOperationPhase::Completed { .. }));
    assert_eq!(f.evidence.calls.load(Ordering::SeqCst), before);
    assert_eq!(f.state().await, GrantState::Revoked);
    let calls = f.cleanup.calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0], calls[1]);
    assert_eq!(
        calls[0].1,
        SourceCleanupTarget {
            gateway: f.target.gateway.clone().unwrap(),
            provider_revision: 23
        }
    );
    assert!(
        !f.store
            .as_ref()
            .unwrap()
            .source_is_fenced(f.actor.person_id, f.source.connection_id())
            .await
            .unwrap()
    );
}
#[tokio::test]
async fn interrupted_reduction_reservation_resumes_without_provider_observation() {
    let mut f = Fixture::new().await;
    let reserved = f
        .reserve_interrupted(SourceOperationKind::ConnectionObservePause)
        .await;
    assert!(
        f.access()
            .receipt(
                &f.actor,
                GrantReceiptQuery {
                    identity: reserved.identity()
                },
                &scope()
            )
            .await
            .unwrap()
            .is_none()
    );
    f.reopen().await;
    let settled = f
        .service()
        .reconcile(&f.actor, reserved.operation_id, None, &scope())
        .await
        .unwrap();
    assert!(matches!(
        settled.phase,
        SourceOperationPhase::Completed { .. }
    ));
    assert_eq!(f.state().await, GrantState::Paused);
    assert_eq!(f.evidence.calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        f.service()
            .reconcile(&f.actor, reserved.operation_id, None, &scope())
            .await
            .unwrap(),
        settled
    );
}
#[tokio::test]
async fn abort_tombstone_excludes_recovered_reduction() {
    let mut f = Fixture::new().await;
    let reserved = f
        .reserve_interrupted(SourceOperationKind::ConnectionObservePause)
        .await;
    f.access()
        .abort(
            &f.actor,
            GrantAbort {
                identity: reserved.identity(),
            },
            &scope(),
        )
        .await
        .unwrap();
    f.reopen().await;
    let done = f
        .service()
        .reconcile(&f.actor, reserved.operation_id, None, &scope())
        .await
        .unwrap();
    assert!(matches!(done.phase, SourceOperationPhase::Aborted { .. }));
    assert_eq!(f.state().await, GrantState::Active);
    assert_eq!(f.evidence.calls.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn interrupted_disconnect_recovers_local_revocation_before_offline_cleanup() {
    let mut f = Fixture::new().await;
    let reserved = f
        .reserve_interrupted(SourceOperationKind::ConnectionDisconnect)
        .await;
    f.reopen().await;
    assert_eq!(
        f.service()
            .reconcile(&f.actor, reserved.operation_id, None, &scope())
            .await,
        Err(AgentFailure::CapabilityUnavailable)
    );
    assert_eq!(f.state().await, GrantState::Revoked);
    let receipt = f
        .access()
        .receipt(
            &f.actor,
            GrantReceiptQuery {
                identity: reserved.identity(),
            },
            &scope(),
        )
        .await
        .unwrap()
        .unwrap();
    assert!(
        matches!(&receipt,GrantOperationReceipt::Committed(value) if value.kind==GrantCommitKind::Disconnect)
    );
    f.reopen().await;
    assert_eq!(
        f.service()
            .reconcile(&f.actor, reserved.operation_id, None, &scope())
            .await,
        Err(AgentFailure::CapabilityUnavailable)
    );
    assert_eq!(
        f.access()
            .receipt(
                &f.actor,
                GrantReceiptQuery {
                    identity: reserved.identity()
                },
                &scope()
            )
            .await
            .unwrap()
            .unwrap(),
        receipt
    );
    assert!(f.cleanup.calls.lock().unwrap().is_empty());
}
#[tokio::test]
async fn minimal_pause_commit_cannot_reactivate_a_grant() {
    let f = Fixture::new().await;
    f.service()
        .pause_observe_operation(
            &f.actor,
            Uuid::new_v4(),
            f.source.connection_id(),
            1,
            &scope(),
        )
        .await
        .unwrap();
    let reserved = f
        .reserve_interrupted(SourceOperationKind::ConnectionObservePause)
        .await;
    let grant = f
        .vault
        .as_ref()
        .unwrap()
        .snapshot(f.target.source.clone())
        .await
        .unwrap()
        .grants
        .remove(0);
    let expected = ExpectedGrant::Present {
        grant_id: grant.id(),
        authority: grant.authority(),
        resource: grant.scope().resources()[0].clone(),
    };
    let mut activated = grant.clone();
    activated
        .activate_review(activated.authority(), activated.scope().clone())
        .unwrap();
    assert!(
        f.vault
            .as_ref()
            .unwrap()
            .commit(GrantCommit {
                reservation: GrantReservation::Reduction(reserved.identity()),
                kind: GrantCommitKind::PauseObserve,
                mutations: vec![GrantMutation {
                    expected,
                    successor: activated
                }]
            })
            .await
            .is_err()
    );
    assert_eq!(f.state().await, GrantState::Paused);
    assert!(
        f.access()
            .receipt(
                &f.actor,
                GrantReceiptQuery {
                    identity: reserved.identity()
                },
                &scope()
            )
            .await
            .unwrap()
            .is_none()
    );
}
#[tokio::test]
async fn foreign_device_cannot_resume_another_devices_reduction() {
    let f = Fixture::new().await;
    let reserved = f
        .reserve_interrupted(SourceOperationKind::ConnectionObservePause)
        .await;
    let mut foreign = f.actor.clone();
    foreign.device_id = "other.device".into();
    assert_eq!(
        f.service()
            .reconcile(&foreign, reserved.operation_id, None, &scope())
            .await,
        Err(AgentFailure::PolicyDenied)
    );
    assert_eq!(f.state().await, GrantState::Active);
    assert_eq!(f.evidence.calls.load(Ordering::SeqCst), 0);
}
