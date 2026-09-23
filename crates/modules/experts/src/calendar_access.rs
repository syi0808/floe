//! Installing the Schedule Expert on a calendar, and changing what it may read.
//!
//! Which view a setup names, whether a change needs the source admitted again,
//! and which connection the registry is written against are all the registry's
//! own judgments. Re-admitting a native source belongs to whoever can ask the
//! device; persisting the registry belongs to the store. Neither of them
//! decides what the change means.

use floe_agent_contract::{AgentFailure, CalendarProvider, SourceAuthority};
use uuid::Uuid;

use crate::registry::{
    CalendarAccessChange, CalendarAccessConfiguration, CalendarExpertOverview, CalendarExpertSetup,
    CalendarExpertSetupReceipt, CalendarViewBinding,
};

pub type BoxFuture<'a, T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

/// A native calendar source that has just been admitted for this change.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdmittedCalendarSource {
    pub connection_id: String,
    pub source_authority: SourceAuthority,
    pub native_subject_fingerprint: String,
}

/// Which connection a registry write is made against.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CalendarAccessSource {
    /// The setup names no native source; the registry stands on its own.
    Registry,
    /// The change stands on a native connection that was admitted for it.
    Native { connection_id: String },
}

/// Who can say whether this device may still read the calendar a setup names.
///
/// `None` is not a refusal: it says the setup names no native source, and so
/// there is nothing for a device to admit.
pub trait CalendarSourceAdmission: Sync {
    fn admit<'a>(
        &'a self,
        request: &'a CalendarExpertSetup,
    ) -> BoxFuture<'a, Result<Option<AdmittedCalendarSource>, AgentFailure>>;
}

/// The Person's own Expert registry, as a calendar change reads and writes it.
pub trait CalendarSetupStore: Sync {
    fn overview<'a>(&'a self) -> BoxFuture<'a, Result<CalendarExpertOverview, AgentFailure>>;

    fn install<'a>(
        &'a self,
        request: CalendarExpertSetup,
        connection_id: String,
    ) -> BoxFuture<'a, Result<(), AgentFailure>>;

    fn configure<'a>(
        &'a self,
        configuration: CalendarAccessConfiguration,
        source: CalendarAccessSource,
    ) -> BoxFuture<'a, Result<CalendarExpertOverview, AgentFailure>>;
}

/// Install the Schedule Expert against one calendar, and report the result.
///
/// A native calendar is installed under the authority and subject the device
/// just admitted, never under whatever the request happened to carry.
pub async fn install_calendar_expert(
    store: &impl CalendarSetupStore,
    admission: &impl CalendarSourceAdmission,
    mut request: CalendarExpertSetup,
) -> Result<CalendarExpertOverview, AgentFailure> {
    let admitted = admission.admit(&request).await?;
    let connection_id = match admitted {
        Some(admitted) => {
            request.source_authority = Some(admitted.source_authority);
            request.reviewed_native_subject_fingerprint = Some(admitted.native_subject_fingerprint);
            admitted.connection_id
        }
        None => request.setup_id.to_string(),
    };
    store.install(request, connection_id).await?;
    store.overview().await
}

/// Apply one change to what an installed calendar Expert may read.
pub async fn apply_calendar_access(
    store: &impl CalendarSetupStore,
    admission: &impl CalendarSourceAdmission,
    mut configuration: CalendarAccessConfiguration,
) -> Result<CalendarExpertOverview, AgentFailure> {
    let overview = store.overview().await?;
    if let Some((setup, view)) = native_setup(&overview, configuration.setup_id) {
        // Access finds grants by source identity, never by setup. Every native
        // change re-admits the live source so the write stands on a current
        // connection; there is no stored setup-to-connection link.
        let request = setup_request(&configuration, setup, view);
        let source = CalendarAccessSource::Native {
            connection_id: admission
                .admit(&request)
                .await?
                .ok_or(AgentFailure::AccessReviewRequired)?
                .connection_id,
        };
        return store.configure(configuration, source).await;
    }
    // A scope change may be what first binds this setup to a native calendar,
    // in which case the device has to admit it before the registry records it.
    if matches!(configuration.change, CalendarAccessChange::SetScope { .. })
        && let Some(admitted) = admission.admit(&scope_request(&configuration)).await?
    {
        let connection_id = admitted.connection_id.clone();
        record_admission(&mut configuration.change, admitted);
        return store
            .configure(
                configuration,
                CalendarAccessSource::Native { connection_id },
            )
            .await;
    }
    store
        .configure(configuration, CalendarAccessSource::Registry)
        .await
}

/// The installed setup this change names, when it reads a native calendar.
fn native_setup(
    overview: &CalendarExpertOverview,
    setup_id: Uuid,
) -> Option<(&CalendarExpertSetupReceipt, &CalendarViewBinding)> {
    let setup = overview
        .setups
        .iter()
        .find(|setup| setup.setup_id == setup_id)?;
    let view = overview
        .views
        .iter()
        .find(|view| view.handle == setup.view_handle)
        .filter(|view| {
            matches!(
                view.provider,
                CalendarProvider::EventKit | CalendarProvider::Android
            )
        })?;
    Some((setup, view))
}

/// The source an installed setup would be re-admitted under.
fn setup_request(
    configuration: &CalendarAccessConfiguration,
    setup: &CalendarExpertSetupReceipt,
    view: &CalendarViewBinding,
) -> CalendarExpertSetup {
    match &configuration.change {
        CalendarAccessChange::SetScope {
            provider,
            device_id,
            calendar_ids,
            connection_scope,
            connection_revision,
            source_authority,
            reviewed_native_subject_fingerprint,
        } => CalendarExpertSetup {
            instance_id: configuration.instance_id,
            expected_revision: configuration.expected_revision,
            setup_id: setup.setup_id,
            provider: *provider,
            device_id: device_id.clone(),
            calendar_ids: calendar_ids.clone(),
            connection_scope: *connection_scope,
            connection_revision: *connection_revision,
            source_authority: *source_authority,
            reviewed_native_subject_fingerprint: reviewed_native_subject_fingerprint.clone(),
        },
        _ => CalendarExpertSetup {
            instance_id: configuration.instance_id,
            expected_revision: configuration.expected_revision,
            setup_id: setup.setup_id,
            provider: view.provider,
            device_id: view.device_id.clone(),
            calendar_ids: view.calendar_ids.clone(),
            connection_scope: view.connection_scope,
            connection_revision: view.connection_revision,
            source_authority: setup.source_authority.or(view.source_authority),
            reviewed_native_subject_fingerprint: setup.reviewed_native_subject_fingerprint.clone(),
        },
    }
}

/// The source a scope change states on its own, with no installed view behind it.
fn scope_request(configuration: &CalendarAccessConfiguration) -> CalendarExpertSetup {
    match &configuration.change {
        CalendarAccessChange::SetScope {
            provider,
            device_id,
            calendar_ids,
            connection_scope,
            connection_revision,
            source_authority,
            reviewed_native_subject_fingerprint,
        } => CalendarExpertSetup {
            instance_id: configuration.instance_id,
            expected_revision: configuration.expected_revision,
            setup_id: configuration.setup_id,
            provider: *provider,
            device_id: device_id.clone(),
            calendar_ids: calendar_ids.clone(),
            connection_scope: *connection_scope,
            connection_revision: *connection_revision,
            source_authority: *source_authority,
            reviewed_native_subject_fingerprint: reviewed_native_subject_fingerprint.clone(),
        },
        _ => unreachable!("only a scope change states its own source"),
    }
}

/// Record what the device admitted on the change itself, so the registry stores
/// the authority and subject that were just checked.
fn record_admission(change: &mut CalendarAccessChange, admitted: AdmittedCalendarSource) {
    if let CalendarAccessChange::SetScope {
        source_authority,
        reviewed_native_subject_fingerprint,
        ..
    } = change
    {
        *source_authority = Some(admitted.source_authority);
        *reviewed_native_subject_fingerprint = Some(admitted.native_subject_fingerprint);
    }
}
