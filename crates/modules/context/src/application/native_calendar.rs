//! Acquiring the subject a native calendar source would answer for.
//!
//! The device answers for its own calendar: there is no producer to sign for
//! it, so the Person's review of that subject is what a read stands on. Context
//! owns the order — read the connection, admit it, ask the device, read the
//! connection again — while Access judges every admission and the device
//! adapter only answers.

use std::future::Future;

use floe_access::{
    CalendarReadAccessAdmission, CalendarReadAccessRequest, ReadAuthorityEvidence,
    ReadAuthorityIdentity, RemoteCallWindow, admits_native_calendar_read, validate_read_authority,
};
use floe_agent_contract::{AgentFailure, PersonId};
use floe_connections::SourceConnection;
use floe_context_contract::{CalendarProvider, CalendarReadAccessStamp};

use crate::CalendarSource;

/// The subject the device answered for.
///
/// A device that can look twice reports what it saw after the read as well, so
/// that a subject which moved underneath it is never mistaken for a stable one.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeSubjectObservation {
    pub before: String,
    pub after: Option<String>,
}

/// The Person's own record of which calendar this device is connected to.
pub trait CalendarConnectionReader: Sync {
    fn source_is_fenced(
        &self,
        person_id: PersonId,
        connection_id: &floe_context_contract::ConnectionId,
    ) -> impl Future<Output = Result<bool, AgentFailure>> + Send;
    fn calendar_connection(
        &self,
    ) -> impl Future<Output = Result<Option<SourceConnection>, AgentFailure>> + Send;
}

pub trait NativeCalendarGrantReader: Sync {
    fn admit(
        &self,
        connection: &SourceConnection,
        person_id: PersonId,
        consumer: &str,
    ) -> impl Future<Output = Result<CalendarReadAccessAdmission, AgentFailure>> + Send;
}

pub struct AdmittedNativeCalendarRead {
    pub connection: SourceConnection,
    pub stamp: CalendarReadAccessStamp,
    pub admission: CalendarReadAccessAdmission,
}

pub async fn admit_current_native_calendar_read(
    connections: &impl CalendarConnectionReader,
    source: &impl CalendarSource,
    grants: &impl NativeCalendarGrantReader,
    person_id: PersonId,
    device_id: &str,
    consumer: &str,
    window: &RemoteCallWindow,
) -> Result<AdmittedNativeCalendarRead, AgentFailure> {
    check_window(window)?;
    let connection = connections
        .calendar_connection()
        .await?
        .ok_or(AgentFailure::AccessReviewRequired)?;
    if connections
        .source_is_fenced(person_id, connection.connection_id())
        .await?
    {
        return Err(AgentFailure::PolicyDenied);
    }
    let provider = native_provider(&connection)?;
    if !floe_access::is_local_calendar_provider(provider) {
        return Err(AgentFailure::CapabilityUnavailable);
    }
    let execution_owner = floe_access::local_calendar_execution_owner(provider, device_id)
        .ok_or(AgentFailure::CapabilityUnavailable)?;
    if !connection.is_serving()
        || connection.person_id() != person_id
        || connection.execution_owner_id().as_str() != execution_owner
        || connection.revision() == 0
        || !connection.source_authority().is_valid()
        || connection.native_subject_fingerprint().is_none()
    {
        return Err(AgentFailure::AccessReviewRequired);
    }
    let mut calendar_ids = connection_calendar_ids(&connection);
    calendar_ids.sort();
    if calendar_ids.is_empty()
        || calendar_ids.windows(2).any(|pair| pair[0] == pair[1])
        || calendar_ids.iter().any(|identifier| {
            identifier.trim().is_empty()
                || identifier.len() > 512
                || identifier.chars().any(char::is_control)
        })
    {
        return Err(AgentFailure::AccessReviewRequired);
    }
    let consumer_identity = floe_access::GrantConsumer::builtin(consumer)
        .map_err(|_| AgentFailure::CapabilityDenied)?;
    let mut stamp = source
        .check(CalendarReadAccessRequest {
            person_id,
            device_id: device_id.to_owned(),
            provider,
            calendar_ids: calendar_ids.clone(),
            expected_native_subject_fingerprint: None,
            deadline: window.deadline,
            cancellation: window.cancellation.clone(),
        })
        .await?;
    check_window(window)?;
    stamp.calendar_ids.sort();
    validate_read_authority(
        &ReadAuthorityIdentity {
            person_id,
            device_id,
            provider: &provider,
            resource_ids: &calendar_ids,
        },
        &ReadAuthorityEvidence {
            schema_version: stamp.schema_version,
            identity: ReadAuthorityIdentity {
                person_id: stamp.person_id,
                device_id: &stamp.device_id,
                provider: &stamp.provider,
                resource_ids: &stamp.calendar_ids,
            },
            subject_fingerprint: &stamp.native_subject_fingerprint,
            generation: &stamp.generation,
        },
    )?;
    if connection.native_subject_fingerprint() != Some(stamp.native_subject_fingerprint.as_str()) {
        return Err(AgentFailure::AccessReviewRequired);
    }
    let admission = grants.admit(&connection, person_id, consumer).await?;
    admits_native_calendar_read(
        &admission,
        person_id,
        connection.connection_id().as_str(),
        provider,
        device_id,
        connection.source_authority(),
        &consumer_identity,
    )?;
    check_window(window)?;
    let refreshed = connections
        .calendar_connection()
        .await?
        .ok_or(AgentFailure::AccessReviewRequired)?;
    if connections
        .source_is_fenced(person_id, connection.connection_id())
        .await?
    {
        return Err(AgentFailure::PolicyDenied);
    }
    if refreshed != connection {
        return Err(AgentFailure::StaleContext);
    }
    check_window(window)?;
    Ok(AdmittedNativeCalendarRead {
        connection,
        stamp,
        admission,
    })
}

fn check_window(window: &RemoteCallWindow) -> Result<(), AgentFailure> {
    if window.cancellation.is_cancelled() {
        return Err(AgentFailure::Cancelled);
    }
    if tokio::time::Instant::now() >= window.deadline {
        return Err(AgentFailure::DeadlineExceeded);
    }
    Ok(())
}

/// The calendars a connection carries, in the shape Access compares them in.
fn connection_calendar_ids(connection: &SourceConnection) -> Vec<String> {
    connection
        .resources()
        .iter()
        .map(|calendar| calendar.handle().as_str().to_owned())
        .collect()
}

fn native_provider(connection: &SourceConnection) -> Result<CalendarProvider, AgentFailure> {
    floe_access::local_calendar_provider(connection.connector_id().as_str())
        .ok_or(AgentFailure::CapabilityUnavailable)
}
