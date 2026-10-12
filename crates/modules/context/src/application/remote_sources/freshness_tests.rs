// Synthetic owner-port timing tests; no live producer or signature verification.
use super::*;
use floe_access::*;
use floe_agent_contract::BoxFuture;
use floe_connections::*;
use floe_context_contract::*;
use floe_execution::Cancellation;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

#[derive(Clone, Copy)]
enum Timing {
    Fresh,
    Future,
    Expired,
    ExpiresDuringRelease,
    SlowPreview,
    CancelPreview,
    DeadlinePreview,
}

struct Fixture {
    person: PersonId,
    person_text: String,
    grant: DataAccessGrant,
    source: SourceConnection,
    reference: RemoteViewSourceReference,
    timing: Timing,
    previews: AtomicUsize,
}
impl Fixture {
    fn new(timing: Timing) -> Self {
        let person = PersonId::new();
        let connection = ConnectionId::try_new("fixture.mail").unwrap();
        let connector = ConnectorId::try_new("google.gmail").unwrap();
        let owner = ExecutionOwnerId::try_new("fixture.server").unwrap();
        let source_resources = vec![ResourceHandle::try_new("inbox").unwrap()];
        let resource = connection_view_resource("mail.communication", &connection).unwrap();
        let authority = SourceAuthority::new();
        let source = SourceConnection::establish_remote(
            person,
            connector.clone(),
            connection.clone(),
            owner.clone(),
            vec![
                ConnectionResource::new(source_resources[0].clone(), "Inbox".into(), None).unwrap(),
            ],
            authority,
        )
        .unwrap();
        let binding = GrantSourceBinding::try_new(person, connection, connector, owner).unwrap();
        let scope = GrantScope::try_new(
            vec![resource.clone()],
            vec![GrantDataCategory::Content],
            vec![GrantOperation::Read],
            vec![GrantPurpose::Assistant],
            vec![GrantConsumer::builtin("conversation.manager").unwrap()],
            ProcessingRestriction::DeviceOnly,
        )
        .unwrap();
        let mut grant =
            DataAccessGrant::new(GrantId::new(), Uuid::new_v4(), binding, scope.clone()).unwrap();
        grant.activate_review(grant.authority(), scope).unwrap();
        let reference = RemoteViewSourceReference {
            view_id: "mail.communication".into(),
            person_id: person.to_string(),
            client_id: "fixture.client".into(),
            device_id: "fixture.device".into(),
            audience: "fixture.audience".into(),
            connector_id: "google.gmail".into(),
            connection_id: "fixture.mail".into(),
            connection_revision: 1,
            execution_owner: "fixture.server".into(),
            source_authority: authority,
            resource: resource.as_str().into(),
            source_resources,
            provider_identity: "synthetic".into(),
        };
        Self {
            person,
            person_text: person.to_string(),
            grant,
            source,
            reference,
            timing,
            previews: AtomicUsize::new(0),
        }
    }
    fn pairing(&self) -> RemotePairingIdentity<'_> {
        RemotePairingIdentity {
            person_id: &self.person_text,
            client_id: "fixture.client",
            device_id: "fixture.device",
        }
    }
    fn producer(&self) -> RemoteProducerIdentity {
        RemoteProducerIdentity {
            schema_version: 1,
            instance_id: "fixture.instance".into(),
            execution_owner: self.reference.execution_owner.clone(),
            audience: self.reference.audience.clone(),
            key_id: "synthetic".into(),
            public_key: "synthetic".into(),
            fingerprint: "synthetic".into(),
        }
    }
    async fn read(
        &self,
    ) -> Result<SourceReadOutcome<(Value, Vec<AuthorizedSourceBinding>)>, AgentFailure> {
        let selected = SourceSelectionReference {
            connector_id: self.grant.source().connector().clone(),
            connection_id: self.grant.source().connection_id(),
            execution_owner_id: self.grant.source().execution_owner().clone(),
            capability_id: "mail.communication".into(),
            resource: self.grant.scope().resources()[0].clone(),
            contract_version: 1,
        };
        read_selected_remote_view(self, self, self, self, self, self.person, self.pairing(),
            "mail.communication", "conversation.manager", &[selected],
            serde_json::json!({"schema_version": floe_agent_contract::AGENT_VERSION, "query": "", "cursor": 0, "limit": 10}),
            &RemoteCallWindow { deadline: tokio::time::Instant::now()+Duration::from_secs(5), cancellation: Cancellation::new() },
            Uuid::new_v4(), &[1;32]).await
    }
    fn dependency(&self, ttl: i64) -> ContextDependency {
        let now = Utc::now().timestamp_millis();
        remote_view_dependency(
            self.person,
            &self.grant,
            self.grant.source().clone(),
            &self.reference.resource,
            self.reference.source_authority,
            &self.reference.source_resources,
            GrantConsumer::builtin("conversation.manager").unwrap(),
            vec![1; 32],
            Uuid::new_v4(),
            Uuid::new_v4(),
            now - 1000,
            now + ttl,
        )
        .unwrap()
    }
}
impl RemoteGrantTransport for Fixture {
    fn producer_identity<'a>(
        &'a self,
        _: &'a RemoteCallWindow,
    ) -> BoxFuture<'a, Result<RemoteProducerIdentity, AgentFailure>> {
        Box::pin(async { Ok(self.producer()) })
    }
    fn view_source_preview<'a>(
        &'a self,
        _: RemoteSourceQuery<'a>,
        window: &'a RemoteCallWindow,
    ) -> BoxFuture<'a, Result<SignedSourcePreview, AgentFailure>> {
        Box::pin(async move {
            let ordinal = self.previews.fetch_add(1, Ordering::SeqCst);
            match self.timing {
                Timing::ExpiresDuringRelease if ordinal > 0 => {
                    tokio::time::sleep(Duration::from_millis(250)).await
                }
                Timing::SlowPreview => tokio::time::sleep(Duration::from_millis(250)).await,
                Timing::CancelPreview => window.cancellation.cancel(),
                Timing::DeadlinePreview => {
                    tokio::time::sleep_until(window.deadline + Duration::from_millis(5)).await
                }
                _ => {}
            }
            Ok(SignedSourcePreview {
                descriptor_b64url: "synthetic".into(),
                producer_signature: "synthetic".into(),
                connection_revision: 1,
                producer: self.producer(),
            })
        })
    }
}
impl SourcePreviewVerifier for Fixture {
    fn verify<'a>(
        &'a self,
        _: &'a SignedSourcePreview,
        _: RemotePairingIdentity<'a>,
        _: RemoteSourceQuery<'a>,
    ) -> BoxFuture<'a, Result<RemoteViewSourceReference, AgentFailure>> {
        Box::pin(async { Ok(self.reference.clone()) })
    }
}
impl RemoteViewTransport for Fixture {
    fn read_admitted_view<'a>(
        &'a self,
        _: AdmittedRemoteRead<'a>,
        _: &'a RemoteCallWindow,
    ) -> BoxFuture<'a, Result<Value, AgentFailure>> {
        Box::pin(async {
            tokio::time::sleep(Duration::from_millis(10)).await;
            let now = Utc::now().timestamp_millis();
            let (observed, expires) = match self.timing {
                Timing::Fresh => (now, now + 60_000),
                Timing::Future => (now + 30_000, now + 60_000),
                Timing::Expired => (now - 60_000, now - 1),
                Timing::ExpiresDuringRelease => (now - 1000, now + 200),
                _ => (now - 1000, now + 60_000),
            };
            Ok(
                serde_json::json!({"schema_version": floe_agent_contract::AGENT_VERSION,
                "view_id": "mail.communication", "source_handle": "fixture.inbox",
                "observed_at_unix_ms": observed, "expires_at_unix_ms": expires,
                "coverage_complete": true, "items": []}),
            )
        })
    }
}

#[allow(unused_variables)]
impl GrantRepository for Fixture {
    fn find_review<'a>(
        &'a self,
        person_id: floe_kernel::PersonId,
        command_id: uuid::Uuid,
        intent_digest: [u8; 32],
    ) -> BoxFuture<'a, Result<Option<ConnectionReview>, AgentFailure>> {
        panic!("unexpected mutation or lookup: find_review")
    }
    fn read_review<'a>(
        &'a self,
        reference: ReviewRef,
    ) -> BoxFuture<'a, Result<ConnectionReview, AgentFailure>> {
        panic!("unexpected mutation or lookup: read_review")
    }
    fn store_review<'a>(
        &'a self,
        review: ConnectionReview,
    ) -> BoxFuture<'a, Result<ReviewRef, AgentFailure>> {
        panic!("unexpected mutation or lookup: store_review")
    }
    fn snapshot<'a>(
        &'a self,
        source: GrantSourceBinding,
    ) -> BoxFuture<'a, Result<GrantSnapshot, AgentFailure>> {
        Box::pin(async move {
            if source != *self.grant.source() {
                return Err(AgentFailure::PolicyDenied);
            }
            Ok(GrantSnapshot {
                authority_owner: self.grant.authority_owner(),
                source,
                grants: vec![self.grant.clone()],
            })
        })
    }
    fn commit<'a>(
        &'a self,
        command: GrantCommit,
    ) -> BoxFuture<'a, Result<GrantCommitReceipt, AgentFailure>> {
        panic!("unexpected mutation or lookup: commit")
    }
    fn receipt<'a>(
        &'a self,
        query: GrantReceiptQuery,
    ) -> BoxFuture<'a, Result<Option<GrantOperationReceipt>, AgentFailure>> {
        panic!("unexpected mutation or lookup: receipt")
    }
    fn abort<'a>(
        &'a self,
        command: GrantAbort,
    ) -> BoxFuture<'a, Result<GrantAbortOutcome, AgentFailure>> {
        panic!("unexpected mutation or lookup: abort")
    }
}

#[allow(unused_variables)]
impl SourceRepository for Fixture {
    fn list_sources<'a>(
        &'a self,
        person_id: PersonId,
        limit: usize,
    ) -> BoxFuture<'a, Result<Vec<SourceConnection>, SourceRepositoryError>> {
        panic!("unexpected mutation or lookup: list_sources")
    }
    fn load<'a>(
        &'a self,
        person_id: PersonId,
        connection_id: &'a ConnectionId,
    ) -> BoxFuture<'a, Result<Option<SourceConnection>, SourceRepositoryError>> {
        Box::pin(async move {
            if person_id == self.person && connection_id == self.source.connection_id() {
                Ok(Some(self.source.clone()))
            } else {
                Ok(None)
            }
        })
    }
    fn list_current<'a>(
        &'a self,
        person_id: PersonId,
        connector_id: &'a ConnectorId,
    ) -> BoxFuture<'a, Result<Vec<SourceConnection>, SourceRepositoryError>> {
        panic!("unexpected mutation or lookup: list_current")
    }
    fn create<'a>(
        &'a self,
        source: &'a SourceConnection,
    ) -> BoxFuture<'a, Result<(), SourceRepositoryError>> {
        panic!("unexpected mutation or lookup: create")
    }
    fn update<'a>(
        &'a self,
        source: &'a SourceConnection,
        expected_revision: u64,
    ) -> BoxFuture<'a, Result<(), SourceRepositoryError>> {
        panic!("unexpected mutation or lookup: update")
    }
}

#[allow(unused_variables)]
impl SourceOperationRepository for Fixture {
    fn rejected_operation_command<'a>(
        &'a self,
        identity: floe_connections::ConnectionsCommandIdentity,
    ) -> BoxFuture<'a, Result<Option<floe_kernel::AgentFailure>, SourceRepositoryError>> {
        panic!("unexpected mutation or lookup: rejected_operation_command")
    }
    fn reject_unadmitted_operation_command<'a>(
        &'a self,
        identity: floe_connections::ConnectionsCommandIdentity,
        reason: floe_kernel::AgentFailure,
    ) -> BoxFuture<'a, Result<floe_connections::ConnectionsCommandResolution, SourceRepositoryError>>
    {
        panic!("unexpected mutation or lookup: reject_unadmitted_operation_command")
    }
    fn settle_presentation<'a>(
        &'a self,
        request: SourceOperationReservation,
        decision: floe_connections::SourcePresentationDecision,
    ) -> BoxFuture<'a, Result<SourceOperationRecord, SourceRepositoryError>> {
        panic!("unexpected mutation or lookup: settle_presentation")
    }
    fn reserve<'a>(
        &'a self,
        request: SourceOperationReservation,
    ) -> BoxFuture<'a, Result<SourceOperationAdmission, SourceRepositoryError>> {
        panic!("unexpected mutation or lookup: reserve")
    }
    fn load_operation<'a>(
        &'a self,
        id: Uuid,
    ) -> BoxFuture<'a, Result<Option<SourceOperationRecord>, SourceRepositoryError>> {
        panic!("unexpected mutation or lookup: load_operation")
    }
    fn compare_and_swap_operation<'a>(
        &'a self,
        change: SourceOperationChange,
    ) -> BoxFuture<'a, Result<SourceOperationRecord, SourceRepositoryError>> {
        panic!("unexpected mutation or lookup: compare_and_swap_operation")
    }
    fn list_nonterminal<'a>(
        &'a self,
        person_id: PersonId,
        limit: usize,
    ) -> BoxFuture<'a, Result<Vec<SourceOperationRecord>, SourceRepositoryError>> {
        panic!("unexpected mutation or lookup: list_nonterminal")
    }
    fn read_reservation_fence<'a>(
        &'a self,
        person_id: PersonId,
        connection_id: &'a ConnectionId,
    ) -> BoxFuture<'a, Result<SourceReservationFence, SourceRepositoryError>> {
        panic!("unexpected mutation or lookup: read_reservation_fence")
    }
    fn source_is_fenced<'a>(
        &'a self,
        person_id: PersonId,
        connection_id: &'a ConnectionId,
    ) -> BoxFuture<'a, Result<bool, SourceRepositoryError>> {
        Box::pin(async move {
            assert_eq!(person_id, self.person);
            assert_eq!(connection_id, self.source.connection_id());
            Ok(false)
        })
    }
}

#[tokio::test]
async fn a_source_observed_after_request_start_is_fresh_at_release() {
    let fixture = Fixture::new(Timing::Fresh);
    assert!(matches!(
        fixture.read().await.unwrap(),
        SourceReadOutcome::Ready(_)
    ));
}

#[tokio::test]
async fn future_and_already_expired_source_data_are_still_rejected() {
    for timing in [Timing::Future, Timing::Expired] {
        assert!(Fixture::new(timing).read().await.is_err());
    }
}

#[tokio::test]
async fn a_source_expiring_during_continuity_checks_is_not_released() {
    assert!(
        Fixture::new(Timing::ExpiresDuringRelease)
            .read()
            .await
            .is_err()
    );
}

#[tokio::test]
async fn dependency_expiring_during_preview_cannot_be_reauthorized() {
    let fixture = Fixture::new(Timing::SlowPreview);
    let dependency = fixture.dependency(200);
    let auth = DependencyAuthorization {
        deadline: tokio::time::Instant::now() + Duration::from_secs(5),
        cancellation: Cancellation::new(),
    };
    assert_eq!(
        authorize_remote_dependency(
            &fixture,
            &fixture,
            &fixture,
            &fixture,
            &fixture,
            fixture.person,
            fixture.pairing(),
            &dependency,
            &auth
        )
        .await,
        Err(AgentFailure::StaleContext)
    );
}

#[tokio::test]
async fn late_cancellation_and_deadline_are_checked_after_preview() {
    for (timing, failure) in [
        (Timing::CancelPreview, AgentFailure::Cancelled),
        (Timing::DeadlinePreview, AgentFailure::DeadlineExceeded),
    ] {
        let fixture = Fixture::new(timing);
        let dependency = fixture.dependency(60_000);
        let auth = DependencyAuthorization {
            deadline: tokio::time::Instant::now() + Duration::from_millis(30),
            cancellation: Cancellation::new(),
        };
        assert_eq!(
            authorize_remote_dependency(
                &fixture,
                &fixture,
                &fixture,
                &fixture,
                &fixture,
                fixture.person,
                fixture.pairing(),
                &dependency,
                &auth
            )
            .await,
            Err(failure)
        );
    }
}
