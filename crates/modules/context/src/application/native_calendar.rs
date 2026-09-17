//! Acquiring the subject a native calendar source would answer for.
//!
//! The device answers for its own calendar: there is no producer to sign for
//! it, so the Person's review of that subject is what a read stands on. Context
//! owns the order — read the connection, admit it, ask the device, read the
//! connection again — while Access judges every admission and the device
//! adapter only answers.

use std::future::Future;

use floe_access::{
    NativeCalendarConnection, NativeCalendarReview, RemoteCallWindow,
    admit_native_calendar_setup, admit_native_calendar_subject, is_native_calendar,
    native_calendar_connection_unchanged, reviewed_native_subject,
};
use floe_agent_contract::{AgentFailure, PersonId};
use floe_context_contract::{CalendarProvider, CalendarScope, SourceAuthority};
use floe_day::CalendarConnection;

/// The native calendar source a caller is asking about.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeCalendarSourceRequest {
    pub person_id: PersonId,
    pub provider: CalendarProvider,
    pub device_id: String,
    pub calendar_ids: Vec<String>,
    pub connection_scope: CalendarScope,
    pub source_authority: Option<SourceAuthority>,
    pub reviewed_native_subject_fingerprint: Option<String>,
    /// The connection the Person was already shown, when they named one.
    pub connection_id: Option<String>,
}

/// The connection and subject a native calendar read may now stand on.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdmittedNativeCalendarSource {
    pub provider: CalendarProvider,
    pub device_id: String,
    pub calendar_ids: Vec<String>,
    pub connection_id: String,
    pub connection_revision: u64,
    pub connection_scope: CalendarScope,
    pub source_authority: SourceAuthority,
    pub native_subject_fingerprint: String,
}

/// What the device is asked to report about its own calendar subject.
#[derive(Clone)]
pub struct NativeSubjectRequest {
    pub person_id: PersonId,
    pub device_id: String,
    pub provider: CalendarProvider,
    /// Sorted, as every side of the comparison states them.
    pub calendar_ids: Vec<String>,
    pub connection_id: String,
    pub connection_revision: u64,
    pub window: RemoteCallWindow,
}

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
    fn calendar_connection(
        &self,
    ) -> impl Future<Output = Result<Option<CalendarConnection>, AgentFailure>> + Send;
}

/// The device this Person's calendar subject is asked of.
pub trait NativeCalendarSubjectSource: Sync {
    fn subject(
        &self,
        request: NativeSubjectRequest,
    ) -> impl Future<Output = Result<NativeSubjectObservation, AgentFailure>> + Send;
}

fn evidence(connection: &CalendarConnection) -> NativeCalendarConnection<'_> {
    NativeCalendarConnection {
        connection_id: &connection.connection_id,
        device_id: &connection.device_id,
        disconnected: connection.disconnected,
        provider: connection.provider,
        scope: connection.scope,
        source_authority: connection.source_authority,
        calendar_ids: &[],
    }
}

/// The calendars a connection carries, in the shape Access compares them in.
fn connection_calendar_ids(connection: &CalendarConnection) -> Vec<String> {
    connection
        .calendars
        .iter()
        .map(|calendar| calendar.calendar_id.clone())
        .collect()
}

impl NativeCalendarSourceRequest {
    fn review<'a>(&'a self, connection_id: Option<&'a str>) -> NativeCalendarReview<'a> {
        NativeCalendarReview {
            device_id: &self.device_id,
            provider: self.provider,
            scope: self.connection_scope,
            calendar_ids: &self.calendar_ids,
            source_authority: self.source_authority,
            native_subject_fingerprint: self.reviewed_native_subject_fingerprint.as_deref(),
            connection_id,
        }
    }

    fn sorted_calendar_ids(&self) -> Vec<String> {
        let mut calendar_ids = self.calendar_ids.clone();
        calendar_ids.sort();
        calendar_ids
    }
}

/// Re-admit a native calendar source the Person already reviewed.
///
/// `None` says the request names no native source at all, which is not a
/// refusal: a hosted calendar stands on its producer, not on this device.
pub async fn admit_native_calendar_source(
    connections: &impl CalendarConnectionReader,
    device: &impl NativeCalendarSubjectSource,
    request: &NativeCalendarSourceRequest,
    window: &RemoteCallWindow,
) -> Result<Option<AdmittedNativeCalendarSource>, AgentFailure> {
    if !is_native_calendar(request.provider) {
        return Ok(None);
    }
    let admitted = acquire(
        connections,
        device,
        request,
        window,
        Acquisition {
            admit: admit_native_calendar_setup,
            compare_reviewed_subject: true,
            subject_must_hold_still: false,
        },
    )
    .await?;
    Ok(Some(admitted))
}

/// Ask the device what subject it would answer for, so the Person can review it.
///
/// Nothing is granted here. The subject has to hold still while the device
/// looks, and the connection has to be the same one afterwards, or what the
/// Person would be shown is already stale.
pub async fn preview_native_calendar_subject(
    connections: &impl CalendarConnectionReader,
    device: &impl NativeCalendarSubjectSource,
    request: &NativeCalendarSourceRequest,
    window: &RemoteCallWindow,
) -> Result<AdmittedNativeCalendarSource, AgentFailure> {
    if window.cancellation.is_cancelled() {
        return Err(AgentFailure::Cancelled);
    }
    let calendar_ids = request.sorted_calendar_ids();
    if calendar_ids.is_empty()
        || calendar_ids.len() > 4
        || calendar_ids.windows(2).any(|pair| pair[0] == pair[1])
        || calendar_ids
            .iter()
            .any(|identifier| identifier.trim().is_empty() || identifier.len() > 512)
    {
        return Err(AgentFailure::InvalidInput);
    }
    if !is_native_calendar(request.provider) {
        return Err(AgentFailure::CapabilityUnavailable);
    }
    acquire(
        connections,
        device,
        request,
        window,
        Acquisition {
            admit: admit_native_calendar_subject,
            compare_reviewed_subject: false,
            subject_must_hold_still: true,
        },
    )
    .await
}

/// How one acquisition treats the subject the device reports.
struct Acquisition {
    admit: fn(
        NativeCalendarConnection<'_>,
        NativeCalendarReview<'_>,
    ) -> Result<SourceAuthority, AgentFailure>,
    /// The Person already reviewed a subject, and it has to be the one that
    /// answers. A preview is what produces that subject in the first place, so
    /// it has nothing to compare against.
    compare_reviewed_subject: bool,
    /// The subject has to be the same before and after the device looks.
    subject_must_hold_still: bool,
}

async fn acquire(
    connections: &impl CalendarConnectionReader,
    device: &impl NativeCalendarSubjectSource,
    request: &NativeCalendarSourceRequest,
    window: &RemoteCallWindow,
    acquisition: Acquisition,
) -> Result<AdmittedNativeCalendarSource, AgentFailure> {
    let connection = connections
        .calendar_connection()
        .await?
        .ok_or(AgentFailure::AccessReviewRequired)?;
    let calendar_ids = connection_calendar_ids(&connection);
    let review = request.review(request.connection_id.as_deref());
    let authority = (acquisition.admit)(
        NativeCalendarConnection {
            calendar_ids: &calendar_ids,
            ..evidence(&connection)
        },
        review,
    )?;
    let reviewed = if acquisition.compare_reviewed_subject {
        Some(reviewed_native_subject(review)?.to_owned())
    } else {
        None
    };
    let observed = device
        .subject(NativeSubjectRequest {
            person_id: request.person_id,
            device_id: request.device_id.clone(),
            provider: request.provider,
            calendar_ids: request.sorted_calendar_ids(),
            connection_id: connection.connection_id.clone(),
            connection_revision: connection.revision,
            window: window.clone(),
        })
        .await?;
    if acquisition.subject_must_hold_still
        && observed
            .after
            .as_deref()
            .is_some_and(|after| after != observed.before)
    {
        return Err(AgentFailure::StaleContext);
    }
    if reviewed.is_some_and(|reviewed| observed.before != reviewed) {
        return Err(AgentFailure::AccessReviewRequired);
    }
    let refreshed = connections
        .calendar_connection()
        .await?
        .ok_or(AgentFailure::AccessReviewRequired)?;
    let refreshed_ids = connection_calendar_ids(&refreshed);
    native_calendar_connection_unchanged(
        NativeCalendarConnection {
            calendar_ids: &refreshed_ids,
            ..evidence(&refreshed)
        },
        &connection.connection_id,
        review,
        authority,
    )?;
    Ok(AdmittedNativeCalendarSource {
        provider: request.provider,
        device_id: request.device_id.clone(),
        calendar_ids: request.sorted_calendar_ids(),
        connection_id: refreshed.connection_id,
        connection_revision: refreshed.revision,
        connection_scope: refreshed.scope,
        source_authority: authority,
        native_subject_fingerprint: observed.before,
    })
}
