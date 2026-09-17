//! Granting a paired server's calendar to this device: preview it, review it,
//! activate it, pause it.
//!
//! A remote calendar is served by a producer the Person pinned, over a
//! connection this device still records, for one calendar they named. All three
//! have to hold before anything is granted, and the grant that comes out says
//! exactly what was reviewed: one calendar, read for the assistant, processed
//! nowhere but here.

use floe_context_contract::{
    CalendarProvider, ConnectionId, ConnectorId, ExecutionOwnerId, GrantAuthority, GrantConsumer,
    GrantDataCategory, GrantId, GrantOperation, GrantPurpose, GrantScope, GrantSourceBinding,
    ProcessingRestriction, ResourceHandle, SourceAuthority,
};
use floe_kernel::{AgentFailure, PersonId};

use crate::application::native_calendar::CALENDAR_EXPERT_CONSUMER as REMOTE_CALENDAR_CONSUMER;
use crate::application::remote_authority::admit_enrollment_pairing;
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
    pub consumer: String,
    pub recipient: String,
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
pub fn remote_calendar_scope(resource: &str) -> Result<GrantScope, AgentFailure> {
    let consumer = GrantConsumer::builtin(REMOTE_CALENDAR_CONSUMER)
        .map_err(|_| AgentFailure::InvalidInput)?;
    GrantScope::try_new(
        vec![ResourceHandle::try_new(resource).map_err(|_| AgentFailure::InvalidInput)?],
        vec![GrantDataCategory::Content],
        vec![GrantOperation::Read],
        vec![GrantPurpose::Assistant],
        vec![consumer],
        ProcessingRestriction::LocalOnly,
    )
    .map_err(|_| AgentFailure::InvalidInput)
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
    window: &RemoteCallWindow,
) -> Result<RemoteCalendarGrantPreview, AgentFailure> {
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
    Ok(RemoteCalendarGrantPreview {
        reference,
        producer,
        consumer: REMOTE_CALENDAR_CONSUMER.into(),
        recipient: REMOTE_CALENDAR_RECIPIENT.into(),
    })
}

/// Grant the calendar the Person reviewed.
///
/// The preview is taken again here rather than carried across the decision: a
/// producer or authority that moved while the Person was deciding is not what
/// they reviewed.
pub async fn review_and_activate_remote_calendar_grant(
    store: &impl RemoteGrantStore,
    transport: &impl RemoteGrantTransport,
    request: RemoteCalendarGrantRequest<'_>,
    connection: RemoteCalendarConnection<'_>,
    expected_producer_fingerprint: &str,
    window: &RemoteCallWindow,
) -> Result<DataAccessGrant, AgentFailure> {
    let preview =
        preview_remote_calendar_grant(store, transport, request, connection, window).await?;
    if preview.producer.fingerprint != expected_producer_fingerprint {
        return Err(AgentFailure::PolicyDenied);
    }
    let source = remote_calendar_source(request.person_id, &preview.reference)?;
    let scope = remote_calendar_scope(request.resource)?;
    store
        .activate_calendar_grant(GrantId::new(), None, source, scope)
        .await
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
