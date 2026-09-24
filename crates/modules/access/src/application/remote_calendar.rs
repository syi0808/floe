//! Granting a paired server's calendar to this device: preview it, review it,
//! activate it, pause it.
//!
//! A remote calendar is served by a producer the Person pinned, over a
//! connection this device still records, for one calendar they named. All three
//! have to hold before anything is granted, and the grant that comes out says
//! exactly what was reviewed: one calendar, read for the assistant, processed
//! nowhere but here.

use floe_context_contract::{
    CalendarProvider, ConnectionId, ConnectorId, ConsumerPolicyAuthority, ContextDependency,
    ExecutionOwnerId, GrantAuthority, GrantConsumer, GrantDataCategory, GrantId, GrantOperation,
    GrantPurpose, GrantScope, GrantSourceBinding, ProcessingRestriction, ResourceHandle,
    SourceAuthority,
};
use floe_kernel::{AgentFailure, PersonId};

use crate::GrantState;
use crate::application::grants::validate_grant_expectation;
use crate::application::remote_authority::admit_enrollment_pairing;
use crate::application::remote_view::RemoteViewSourceReference;
use crate::application::remote_view::{RemoteProducerIdentity, producer_is_pinned};
use crate::data_access_grant::DataAccessGrant;
use crate::ports::remote_grants::{
    RemoteCalendarQuery, RemoteCallWindow, RemoteGrantStore, RemoteGrantTransport,
    RemotePairingIdentity,
};

/// Where a remote calendar's contents may be processed.
///
/// A calendar source read on this device never leaves it, so the recipient the
/// Person is shown is the device itself rather than any audience.
pub const REMOTE_CALENDAR_RECIPIENT: &str = "local_only";

/// One remote calendar source, as the producer signed it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoteCalendarSourceReference {
    pub person_id: String,
    pub client_id: String,
    pub device_id: String,
    pub audience: String,
    pub connector_id: String,
    pub connection_id: String,
    pub execution_owner: String,
    pub source_authority: SourceAuthority,
    pub resource: String,
    pub provider_identity: String,
}

/// What this device currently records about the calendar connection a grant
/// would name.
#[derive(Clone, Copy)]
pub struct RemoteCalendarConnection<'a> {
    pub provider: CalendarProvider,
    pub connection_id: &'a str,
    pub calendar_ids: &'a [String],
    pub disconnected: bool,
}

/// Which remote calendar is being granted, over which pairing.
#[derive(Clone, Copy)]
pub struct RemoteCalendarGrantRequest<'a> {
    pub person_id: PersonId,
    pub pairing: RemotePairingIdentity<'a>,
    pub connector_id: &'a str,
    pub connection_id: &'a str,
    pub resource: &'a str,
}

/// What the Person is shown before they grant a remote calendar.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoteCalendarGrantPreview {
    pub reference: RemoteCalendarSourceReference,
    pub producer: RemoteProducerIdentity,
    pub consumers: Vec<String>,
    pub recipient: String,
    /// The exact current grant for this source and resource, if any. All three
    /// are present or all three are absent; the review echoes them back as the
    /// reviewed expectation so a stale decision fails instead of forking the
    /// grant.
    pub grant_id: Option<GrantId>,
    pub grant_authority: Option<GrantAuthority>,
    pub consumer_policy: Option<ConsumerPolicyAuthority>,
}

/// What the Person says they already reviewed.
#[derive(Clone, Copy)]
pub struct RemoteCalendarGrantReviewExpectation<'a> {
    pub producer_fingerprint: &'a str,
    pub source_authority: SourceAuthority,
    pub grant_id: Option<GrantId>,
    pub grant_authority: Option<GrantAuthority>,
    pub consumer_policy: Option<ConsumerPolicyAuthority>,
}

/// The connector a hosted calendar provider is reached through.
///
/// A calendar this device reads for itself has no connector on a paired server,
/// and so cannot be granted this way.
pub fn hosted_calendar_connector(provider: CalendarProvider) -> Option<&'static str> {
    match provider {
        CalendarProvider::Google => Some("calendar.google"),
        CalendarProvider::Microsoft => Some("calendar.microsoft"),
        CalendarProvider::EventKit | CalendarProvider::Android | CalendarProvider::Fixture => None,
    }
}

/// That the connection this device records is the one the grant names, and
/// still carries the calendar being granted.
pub fn admits_remote_calendar_connection(
    request: &RemoteCalendarGrantRequest<'_>,
    connection: RemoteCalendarConnection<'_>,
) -> Result<(), AgentFailure> {
    let expected =
        hosted_calendar_connector(connection.provider).ok_or(AgentFailure::PolicyDenied)?;
    if connection.disconnected
        || connection.connection_id != request.connection_id
        || request.connector_id != expected
        || !connection
            .calendar_ids
            .iter()
            .any(|calendar_id| calendar_id == request.resource)
    {
        return Err(AgentFailure::StaleContext);
    }
    Ok(())
}

/// That the signed descriptor describes the producer that served it.
fn source_matches_producer(
    reference: &RemoteCalendarSourceReference,
    producer: &RemoteProducerIdentity,
) -> Result<(), AgentFailure> {
    if reference.audience != producer.audience
        || reference.execution_owner != producer.execution_owner
        || reference.provider_identity.is_empty()
    {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(())
}

/// The source binding a signed remote calendar descriptor names.
pub fn remote_calendar_source(
    person_id: PersonId,
    reference: &RemoteCalendarSourceReference,
) -> Result<GrantSourceBinding, AgentFailure> {
    GrantSourceBinding::try_new(
        person_id,
        ConnectionId::try_new(reference.connection_id.clone())
            .map_err(|_| AgentFailure::InvalidInput)?,
        ConnectorId::try_new(reference.connector_id.clone())
            .map_err(|_| AgentFailure::InvalidInput)?,
        ExecutionOwnerId::try_new(reference.execution_owner.clone())
            .map_err(|_| AgentFailure::InvalidInput)?,
        reference.source_authority,
    )
    .map_err(|_| AgentFailure::InvalidInput)
}

/// The scope one reviewed remote calendar grant carries.
pub fn remote_calendar_scope(
    resource: &str,
    consumers: &[GrantConsumer],
) -> Result<GrantScope, AgentFailure> {
    GrantScope::try_new(
        vec![ResourceHandle::try_new(resource).map_err(|_| AgentFailure::InvalidInput)?],
        vec![GrantDataCategory::Content],
        vec![GrantOperation::Read],
        vec![GrantPurpose::Assistant],
        consumers.to_vec(),
        ProcessingRestriction::LocalOnly,
    )
    .map_err(|_| AgentFailure::InvalidInput)
}

pub fn admit_remote_calendar_read(
    grant: &DataAccessGrant,
    consumer: &GrantConsumer,
    resource: &str,
) -> Result<(), AgentFailure> {
    if grant.state() != GrantState::Active
        || grant.review_required()
        || !matches!(
            grant.source().connector().as_str(),
            "calendar.google" | "calendar.microsoft"
        )
        || grant.scope().resources().len() != 1
        || grant.scope().resources()[0].as_str() != resource
        || grant.scope().categories() != [GrantDataCategory::Content]
        || !grant.scope().operations().contains(&GrantOperation::Read)
        || !grant.scope().purposes().contains(&GrantPurpose::Assistant)
        || !grant.scope().consumers().contains(consumer)
        || grant.scope().processing() != &ProcessingRestriction::LocalOnly
    {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(())
}

pub fn remote_calendar_dependency_source_admits(
    dependency: &ContextDependency,
    reference: &RemoteViewSourceReference,
    connection_revision: u64,
) -> Result<(), AgentFailure> {
    if dependency.processing() != &ProcessingRestriction::LocalOnly
        || reference.source_authority != dependency.source().source_authority()
        || reference.execution_owner != dependency.source().execution_owner().as_str()
        || reference.connection_revision != connection_revision
    {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(())
}

/// Show the Person what they would be granting.
///
/// Nothing is stored. The producer answering has to be the one they pinned, the
/// connection has to be the one this device records, and the descriptor the
/// producer signs has to name this pairing and this calendar.
pub async fn preview_remote_calendar_grant(
    store: &impl RemoteGrantStore,
    transport: &impl RemoteGrantTransport,
    request: RemoteCalendarGrantRequest<'_>,
    connection: RemoteCalendarConnection<'_>,
    consumers: &[GrantConsumer],
    window: &RemoteCallWindow,
) -> Result<RemoteCalendarGrantPreview, AgentFailure> {
    remote_calendar_scope(request.resource, consumers)?;
    admit_enrollment_pairing(request.person_id, request.pairing)?;
    let producer = transport.producer_identity(window).await?;
    producer_is_pinned(&store.pinned_producer().await?, &producer)?;
    admits_remote_calendar_connection(&request, connection)?;
    let query = RemoteCalendarQuery {
        connector_id: request.connector_id,
        connection_id: request.connection_id,
        resource: request.resource,
    };
    let preview = transport.calendar_source_preview(query, window).await?;
    let reference = store
        .verify_calendar_source_preview(&preview, request.pairing, query)
        .await?;
    source_matches_producer(&reference, &producer)?;
    let source = remote_calendar_source(request.person_id, &reference)?;
    let current = store.find_calendar_grant(&source, request.resource).await?;
    let (grant_id, grant_authority, consumer_policy) = match current {
        None => (None, None, None),
        Some(grant) => {
            let policy = store.calendar_grant_policy(grant.id()).await?;
            (Some(grant.id()), Some(grant.authority()), Some(policy))
        }
    };
    Ok(RemoteCalendarGrantPreview {
        reference,
        producer,
        consumers: consumers
            .iter()
            .map(|consumer| consumer.identifier().to_owned())
            .collect(),
        recipient: REMOTE_CALENDAR_RECIPIENT.into(),
        grant_id,
        grant_authority,
        consumer_policy,
    })
}

/// Grant the calendar the Person reviewed.
///
/// The preview is taken again here rather than carried across the decision: a
/// producer or authority that moved while the Person was deciding is not what
/// they reviewed. A repeated review of the same source and resource updates the
/// reviewed grant under CAS; it never forks an exact-resource duplicate.
pub async fn review_and_activate_remote_calendar_grant(
    store: &impl RemoteGrantStore,
    transport: &impl RemoteGrantTransport,
    request: RemoteCalendarGrantRequest<'_>,
    connection: RemoteCalendarConnection<'_>,
    consumers: &[GrantConsumer],
    expectation: RemoteCalendarGrantReviewExpectation<'_>,
    window: &RemoteCallWindow,
) -> Result<DataAccessGrant, AgentFailure> {
    if !expectation.source_authority.is_valid() {
        return Err(AgentFailure::InvalidInput);
    }
    let expected_grant =
        validate_grant_expectation(expectation.grant_id, expectation.grant_authority)?;
    match (&expected_grant, &expectation.consumer_policy) {
        (None, None) => {}
        (Some(_), Some(policy)) if policy.is_valid() => {}
        _ => return Err(AgentFailure::InvalidInput),
    }
    let preview =
        preview_remote_calendar_grant(store, transport, request, connection, consumers, window)
            .await?;
    if preview.producer.fingerprint != expectation.producer_fingerprint
        || preview.reference.source_authority != expectation.source_authority
    {
        return Err(AgentFailure::PolicyDenied);
    }
    let current = validate_grant_expectation(preview.grant_id, preview.grant_authority)?;
    if current != expected_grant || preview.consumer_policy != expectation.consumer_policy {
        return Err(AgentFailure::Conflict);
    }
    let source = remote_calendar_source(request.person_id, &preview.reference)?;
    let scope = remote_calendar_scope(request.resource, consumers)?;
    match expected_grant {
        None => {
            store
                .activate_calendar_grant(GrantId::new(), None, source, scope, None)
                .await
        }
        Some((grant_id, authority)) => {
            store
                .activate_calendar_grant(
                    grant_id,
                    Some(authority),
                    source,
                    scope,
                    expectation.consumer_policy,
                )
                .await
        }
    }
}

/// What the Person's own record says about a remote calendar grant.
pub async fn remote_calendar_grant(
    store: &impl RemoteGrantStore,
    grant_id: GrantId,
) -> Result<DataAccessGrant, AgentFailure> {
    store.calendar_grant(grant_id).await
}

/// Stop a remote calendar grant the Person no longer wants read.
pub async fn pause_remote_calendar_grant(
    store: &impl RemoteGrantStore,
    grant_id: GrantId,
    expected: GrantAuthority,
) -> Result<DataAccessGrant, AgentFailure> {
    store.pause_calendar_grant(grant_id, expected).await
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use floe_context_contract::{ConnectionId, ConnectorId, ExecutionOwnerId};
    use floe_execution::Cancellation;

    use super::*;
    use crate::application::remote_view::RemoteViewSourceReference;
    use crate::ports::remote_grants::{
        BoxFuture, RemoteGrantBinding, RemoteSourceQuery, SignedCalendarPreview,
        SignedSourcePreview,
    };

    struct Fixture {
        person_id: PersonId,
        producer: RemoteProducerIdentity,
        reference: RemoteCalendarSourceReference,
        calendars: Vec<String>,
        current: Option<DataAccessGrant>,
        policy: Option<ConsumerPolicyAuthority>,
        activated: Mutex<
            Vec<(
                GrantId,
                Option<GrantAuthority>,
                Option<ConsumerPolicyAuthority>,
            )>,
        >,
    }

    impl Fixture {
        fn new(current: Option<DataAccessGrant>, policy: Option<ConsumerPolicyAuthority>) -> Self {
            let person_id = PersonId::new();
            let authority = SourceAuthority::new();
            let producer = RemoteProducerIdentity {
                schema_version: 1,
                instance_id: "instance".into(),
                execution_owner: "server-owner".into(),
                audience: "server-audience".into(),
                key_id: "key".into(),
                public_key: "public".into(),
                fingerprint: "producer-fingerprint".into(),
            };
            Self {
                person_id,
                producer: producer.clone(),
                calendars: vec!["primary".to_owned()],
                reference: RemoteCalendarSourceReference {
                    person_id: person_id.to_string(),
                    client_id: "client".into(),
                    device_id: "device".into(),
                    audience: producer.audience.clone(),
                    connector_id: "calendar.google".into(),
                    connection_id: "connection".into(),
                    execution_owner: producer.execution_owner.clone(),
                    source_authority: authority,
                    resource: "primary".into(),
                    provider_identity: "account".into(),
                },
                current,
                policy,
                activated: Mutex::new(Vec::new()),
            }
        }

        fn grant(&self, resource: &str) -> DataAccessGrant {
            let source = GrantSourceBinding::try_new(
                self.person_id,
                ConnectionId::try_new("connection").unwrap(),
                ConnectorId::try_new("calendar.google").unwrap(),
                ExecutionOwnerId::try_new("server-owner").unwrap(),
                self.reference.source_authority,
            )
            .unwrap();
            let scope = remote_calendar_scope(
                resource,
                std::slice::from_ref(&GrantConsumer::builtin("floe.builtin.schedule").unwrap()),
            )
            .unwrap();
            let mut grant =
                DataAccessGrant::new(GrantId::new(), uuid::Uuid::new_v4(), source, scope).unwrap();
            let (source, scope) = (grant.source().clone(), grant.scope().clone());
            grant
                .activate_review(grant.authority(), source, scope)
                .unwrap();
            grant
        }

        fn request(&self) -> RemoteCalendarGrantRequest<'_> {
            RemoteCalendarGrantRequest {
                person_id: self.person_id,
                pairing: RemotePairingIdentity {
                    person_id: &self.reference.person_id,
                    client_id: &self.reference.client_id,
                    device_id: &self.reference.device_id,
                },
                connector_id: &self.reference.connector_id,
                connection_id: &self.reference.connection_id,
                resource: &self.reference.resource,
            }
        }

        fn connection(&self) -> RemoteCalendarConnection<'_> {
            RemoteCalendarConnection {
                provider: CalendarProvider::Google,
                connection_id: &self.reference.connection_id,
                calendar_ids: &self.calendars,
                disconnected: false,
            }
        }

        fn window() -> RemoteCallWindow {
            RemoteCallWindow {
                deadline: tokio::time::Instant::now() + tokio::time::Duration::from_secs(5),
                cancellation: Cancellation::default(),
            }
        }

        fn consumers() -> Vec<GrantConsumer> {
            vec![GrantConsumer::builtin("floe.builtin.schedule").unwrap()]
        }

        fn expectation(&self) -> RemoteCalendarGrantReviewExpectation<'_> {
            RemoteCalendarGrantReviewExpectation {
                producer_fingerprint: &self.producer.fingerprint,
                source_authority: self.reference.source_authority,
                grant_id: self.current.as_ref().map(|grant| grant.id()),
                grant_authority: self.current.as_ref().map(|grant| grant.authority()),
                consumer_policy: self.policy,
            }
        }
    }

    impl RemoteGrantTransport for Fixture {
        fn producer_identity<'a>(
            &'a self,
            _: &'a RemoteCallWindow,
        ) -> BoxFuture<'a, Result<RemoteProducerIdentity, AgentFailure>> {
            Box::pin(async move { Ok(self.producer.clone()) })
        }

        fn view_source_preview<'a>(
            &'a self,
            _: RemoteSourceQuery<'a>,
            _: &'a RemoteCallWindow,
        ) -> BoxFuture<'a, Result<SignedSourcePreview, AgentFailure>> {
            Box::pin(async { Err(AgentFailure::CapabilityUnavailable) })
        }

        fn calendar_source_preview<'a>(
            &'a self,
            _: RemoteCalendarQuery<'a>,
            _: &'a RemoteCallWindow,
        ) -> BoxFuture<'a, Result<SignedCalendarPreview, AgentFailure>> {
            Box::pin(async move {
                Ok(SignedCalendarPreview {
                    descriptor_b64url: String::new(),
                    producer_signature: String::new(),
                    producer: self.producer.clone(),
                })
            })
        }
    }

    impl RemoteGrantStore for Fixture {
        fn pinned_producer<'a>(
            &'a self,
        ) -> BoxFuture<'a, Result<RemoteProducerIdentity, AgentFailure>> {
            Box::pin(async move { Ok(self.producer.clone()) })
        }

        fn verify_view_source_preview<'a>(
            &'a self,
            _: &'a SignedSourcePreview,
            _: RemotePairingIdentity<'a>,
            _: RemoteSourceQuery<'a>,
        ) -> BoxFuture<'a, Result<RemoteViewSourceReference, AgentFailure>> {
            Box::pin(async { Err(AgentFailure::CapabilityUnavailable) })
        }

        fn grants<'a>(
            &'a self,
            _: usize,
        ) -> BoxFuture<'a, Result<Vec<DataAccessGrant>, AgentFailure>> {
            Box::pin(async { Err(AgentFailure::CapabilityUnavailable) })
        }

        fn find_view_grant<'a>(
            &'a self,
            _: &'a str,
            _: &'a GrantSourceBinding,
            _: &'a str,
        ) -> BoxFuture<'a, Result<Option<DataAccessGrant>, AgentFailure>> {
            Box::pin(async { Err(AgentFailure::CapabilityUnavailable) })
        }

        fn activate_view_grant<'a>(
            &'a self,
            _: &'a str,
            _: GrantId,
            _: Option<GrantAuthority>,
            _: GrantSourceBinding,
            _: GrantScope,
        ) -> BoxFuture<'a, Result<DataAccessGrant, AgentFailure>> {
            Box::pin(async { Err(AgentFailure::CapabilityUnavailable) })
        }

        fn verify_calendar_source_preview<'a>(
            &'a self,
            _: &'a SignedCalendarPreview,
            _: RemotePairingIdentity<'a>,
            _: RemoteCalendarQuery<'a>,
        ) -> BoxFuture<'a, Result<RemoteCalendarSourceReference, AgentFailure>> {
            Box::pin(async move { Ok(self.reference.clone()) })
        }

        fn activate_calendar_grant<'a>(
            &'a self,
            grant_id: GrantId,
            expected: Option<GrantAuthority>,
            _: GrantSourceBinding,
            _: GrantScope,
            expected_policy: Option<ConsumerPolicyAuthority>,
        ) -> BoxFuture<'a, Result<DataAccessGrant, AgentFailure>> {
            Box::pin(async move {
                self.activated
                    .lock()
                    .unwrap()
                    .push((grant_id, expected, expected_policy));
                match (&self.current, expected) {
                    (Some(current), Some(authority)) if current.authority() == authority => {
                        Ok(current.clone())
                    }
                    (None, None) => Ok(self.grant("primary")),
                    _ => Err(AgentFailure::Conflict),
                }
            })
        }

        fn find_calendar_grant<'a>(
            &'a self,
            _: &'a GrantSourceBinding,
            _: &'a str,
        ) -> BoxFuture<'a, Result<Option<DataAccessGrant>, AgentFailure>> {
            Box::pin(async move { Ok(self.current.clone()) })
        }

        fn calendar_grant_policy<'a>(
            &'a self,
            _: GrantId,
        ) -> BoxFuture<'a, Result<ConsumerPolicyAuthority, AgentFailure>> {
            Box::pin(async move { self.policy.ok_or(AgentFailure::VaultUnavailable) })
        }

        fn calendar_grant<'a>(
            &'a self,
            _: GrantId,
        ) -> BoxFuture<'a, Result<DataAccessGrant, AgentFailure>> {
            Box::pin(async { Err(AgentFailure::CapabilityUnavailable) })
        }

        fn pause_calendar_grant<'a>(
            &'a self,
            _: GrantId,
            _: GrantAuthority,
        ) -> BoxFuture<'a, Result<DataAccessGrant, AgentFailure>> {
            Box::pin(async { Err(AgentFailure::CapabilityUnavailable) })
        }

        fn view_grant_binding<'a>(
            &'a self,
            _: &'a str,
            _: &'a str,
            _: &'a str,
            _: SourceAuthority,
        ) -> BoxFuture<'a, Result<RemoteGrantBinding, AgentFailure>> {
            Box::pin(async { Err(AgentFailure::CapabilityUnavailable) })
        }

        fn calendar_grant_binding<'a>(
            &'a self,
            _: &'a str,
            _: &'a str,
            _: SourceAuthority,
            _: &'a str,
        ) -> BoxFuture<'a, Result<RemoteGrantBinding, AgentFailure>> {
            Box::pin(async { Err(AgentFailure::CapabilityUnavailable) })
        }
    }

    fn unseeded() -> Fixture {
        Fixture::new(None, None)
    }

    fn seeded() -> Fixture {
        let bare = Fixture::new(None, None);
        let grant = bare.grant("primary");
        Fixture::new(Some(grant), Some(ConsumerPolicyAuthority::new()))
    }

    #[tokio::test]
    async fn preview_reports_the_exact_current_grant_expectation() {
        let fixture = seeded();
        let preview = preview_remote_calendar_grant(
            &fixture,
            &fixture,
            fixture.request(),
            fixture.connection(),
            &Fixture::consumers(),
            &Fixture::window(),
        )
        .await
        .unwrap();
        let current = fixture.current.clone().unwrap();
        assert_eq!(preview.grant_id, Some(current.id()));
        assert_eq!(preview.grant_authority, Some(current.authority()));
        assert_eq!(preview.consumer_policy, fixture.policy);

        let empty = unseeded();
        let preview = preview_remote_calendar_grant(
            &empty,
            &empty,
            empty.request(),
            empty.connection(),
            &Fixture::consumers(),
            &Fixture::window(),
        )
        .await
        .unwrap();
        assert_eq!(preview.grant_id, None);
        assert_eq!(preview.grant_authority, None);
        assert_eq!(preview.consumer_policy, None);
    }

    #[tokio::test]
    async fn repeated_review_updates_the_reviewed_grant() {
        let fixture = seeded();
        let current = fixture.current.clone().unwrap();
        let grant = review_and_activate_remote_calendar_grant(
            &fixture,
            &fixture,
            fixture.request(),
            fixture.connection(),
            &Fixture::consumers(),
            fixture.expectation(),
            &Fixture::window(),
        )
        .await
        .unwrap();
        assert_eq!(grant.id(), current.id());
        let calls = fixture.activated.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, current.id());
        assert_eq!(calls[0].1, Some(current.authority()));
        assert_eq!(calls[0].2, fixture.policy);
    }

    #[tokio::test]
    async fn fresh_review_creates_when_no_grant_exists() {
        let fixture = unseeded();
        review_and_activate_remote_calendar_grant(
            &fixture,
            &fixture,
            fixture.request(),
            fixture.connection(),
            &Fixture::consumers(),
            fixture.expectation(),
            &Fixture::window(),
        )
        .await
        .unwrap();
        let calls = fixture.activated.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].1, None);
        assert_eq!(calls[0].2, None);
    }

    #[tokio::test]
    async fn review_rejects_appeared_disappeared_and_changed_grants() {
        let fixture = seeded();
        let consumers = Fixture::consumers();
        let window = Fixture::window();
        let appeared = RemoteCalendarGrantReviewExpectation {
            grant_id: None,
            grant_authority: None,
            consumer_policy: None,
            ..fixture.expectation()
        };
        assert_eq!(
            review_and_activate_remote_calendar_grant(
                &fixture,
                &fixture,
                fixture.request(),
                fixture.connection(),
                &consumers,
                appeared,
                &window,
            )
            .await,
            Err(AgentFailure::Conflict)
        );
        let stale = RemoteCalendarGrantReviewExpectation {
            grant_authority: Some(GrantAuthority::new()),
            ..fixture.expectation()
        };
        assert_eq!(
            review_and_activate_remote_calendar_grant(
                &fixture,
                &fixture,
                fixture.request(),
                fixture.connection(),
                &consumers,
                stale,
                &window,
            )
            .await,
            Err(AgentFailure::Conflict)
        );
        let rotated_policy = RemoteCalendarGrantReviewExpectation {
            consumer_policy: Some(ConsumerPolicyAuthority::new()),
            ..fixture.expectation()
        };
        assert_eq!(
            review_and_activate_remote_calendar_grant(
                &fixture,
                &fixture,
                fixture.request(),
                fixture.connection(),
                &consumers,
                rotated_policy,
                &window,
            )
            .await,
            Err(AgentFailure::Conflict)
        );

        let empty = unseeded();
        let consumers = Fixture::consumers();
        let window = Fixture::window();
        let vanished = RemoteCalendarGrantReviewExpectation {
            producer_fingerprint: &empty.producer.fingerprint,
            source_authority: empty.reference.source_authority,
            grant_id: Some(GrantId::new()),
            grant_authority: Some(GrantAuthority::new()),
            consumer_policy: Some(ConsumerPolicyAuthority::new()),
        };
        assert_eq!(
            review_and_activate_remote_calendar_grant(
                &empty,
                &empty,
                empty.request(),
                empty.connection(),
                &consumers,
                vanished,
                &window,
            )
            .await,
            Err(AgentFailure::Conflict)
        );
    }

    #[tokio::test]
    async fn review_rejects_mixed_expectation_halves() {
        let fixture = seeded();
        let consumers = Fixture::consumers();
        let window = Fixture::window();
        for expectation in [
            RemoteCalendarGrantReviewExpectation {
                grant_authority: None,
                consumer_policy: None,
                ..fixture.expectation()
            },
            RemoteCalendarGrantReviewExpectation {
                grant_id: None,
                consumer_policy: None,
                ..fixture.expectation()
            },
            RemoteCalendarGrantReviewExpectation {
                consumer_policy: None,
                ..fixture.expectation()
            },
        ] {
            assert_eq!(
                review_and_activate_remote_calendar_grant(
                    &fixture,
                    &fixture,
                    fixture.request(),
                    fixture.connection(),
                    &consumers,
                    expectation,
                    &window,
                )
                .await,
                Err(AgentFailure::InvalidInput)
            );
        }
        assert!(fixture.activated.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn review_rejects_producer_and_source_drift() {
        let fixture = seeded();
        let consumers = Fixture::consumers();
        let window = Fixture::window();
        let producer_moved = RemoteCalendarGrantReviewExpectation {
            producer_fingerprint: "another-producer",
            ..fixture.expectation()
        };
        assert_eq!(
            review_and_activate_remote_calendar_grant(
                &fixture,
                &fixture,
                fixture.request(),
                fixture.connection(),
                &consumers,
                producer_moved,
                &window,
            )
            .await,
            Err(AgentFailure::PolicyDenied)
        );
        let source_moved = RemoteCalendarGrantReviewExpectation {
            source_authority: SourceAuthority::new(),
            ..fixture.expectation()
        };
        assert_eq!(
            review_and_activate_remote_calendar_grant(
                &fixture,
                &fixture,
                fixture.request(),
                fixture.connection(),
                &consumers,
                source_moved,
                &window,
            )
            .await,
            Err(AgentFailure::PolicyDenied)
        );
        assert!(fixture.activated.lock().unwrap().is_empty());
    }
}
