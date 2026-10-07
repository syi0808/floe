//! Whether this device may still read the native calendar the Person reviewed.
//!
//! Access judges the connection and the reviewed subject only. Asking the
//! device what subject it would answer for, and re-reading the connection
//! afterwards, belong to Context and to the device adapter; neither of them
//! decides whether the read is admissible.

use floe_context_contract::{CalendarProvider, CalendarScope, SourceAuthority};
use floe_kernel::AgentFailure;

/// A Calendar source identity that the product Day surface can refresh.
/// This is deliberately narrower than the set of locally owned calendars:
/// Android support is not part of the product Calendar read contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProductCalendarBinding {
    pub connector_id: &'static str,
    pub provider: CalendarProvider,
}

/// The complete connector/provider map accepted by product Calendar reads.
/// The synthetic source is present only in Linux QA fixture builds.
pub fn supported_product_calendar_bindings() -> &'static [ProductCalendarBinding] {
    const BINDINGS: &[ProductCalendarBinding] = &[
        ProductCalendarBinding {
            connector_id: "calendar.event_kit",
            provider: CalendarProvider::EventKit,
        },
        ProductCalendarBinding {
            connector_id: "calendar.google",
            provider: CalendarProvider::Google,
        },
        ProductCalendarBinding {
            connector_id: "calendar.microsoft",
            provider: CalendarProvider::Microsoft,
        },
        #[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
        ProductCalendarBinding {
            connector_id: "calendar.fixture",
            provider: CalendarProvider::Fixture,
        },
    ];
    BINDINGS
}

pub fn supported_product_calendar_binding(
    connector_id: &str,
) -> Option<ProductCalendarBinding> {
    supported_product_calendar_bindings()
        .iter()
        .copied()
        .find(|binding| binding.connector_id == connector_id)
}

#[cfg(test)]
mod product_calendar_tests {
    use super::*;

    #[test]
    fn product_calendar_mapping_is_complete_and_excludes_unshipped_android() {
        let mut expected_connectors = vec![
            "calendar.event_kit",
            "calendar.google",
            "calendar.microsoft",
        ];
        #[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
        expected_connectors.push("calendar.fixture");
        assert_eq!(
            supported_product_calendar_bindings()
                .iter()
                .map(|binding| binding.connector_id)
                .collect::<Vec<_>>(),
            expected_connectors
        );

        for (connector, provider) in [
            ("calendar.event_kit", CalendarProvider::EventKit),
            ("calendar.google", CalendarProvider::Google),
            ("calendar.microsoft", CalendarProvider::Microsoft),
        ] {
            assert_eq!(
                supported_product_calendar_binding(connector).map(|binding| binding.provider),
                Some(provider)
            );
        }
        assert_eq!(supported_product_calendar_binding("calendar.android"), None);

        #[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
        assert_eq!(
            supported_product_calendar_binding("calendar.fixture").map(|binding| binding.provider),
            Some(CalendarProvider::Fixture)
        );
        #[cfg(not(all(feature = "qa-fixtures", target_os = "linux")))]
        assert_eq!(supported_product_calendar_binding("calendar.fixture"), None);
    }
}

/// The calendar connection this Person currently records for their device.
#[derive(Clone, Copy)]
pub struct NativeCalendarConnection<'a> {
    pub connection_id: &'a str,
    pub device_id: &'a str,
    pub disconnected: bool,
    pub provider: CalendarProvider,
    pub scope: CalendarScope,
    pub source_authority: SourceAuthority,
    pub calendar_ids: &'a [String],
}

/// The native calendar source a caller says the Person reviewed.
#[derive(Clone, Copy)]
pub struct NativeCalendarReview<'a> {
    pub device_id: &'a str,
    pub provider: CalendarProvider,
    pub scope: CalendarScope,
    pub calendar_ids: &'a [String],
    pub source_authority: Option<SourceAuthority>,
    pub native_subject_fingerprint: Option<&'a str>,
    /// The connection the Person was already shown, when they named one.
    pub connection_id: Option<&'a str>,
}

/// That the connection this device records still serves the native calendar a
/// reviewed read is bound to.
///
/// A connection that moved out from under the binding leaves the read standing
/// on something the Person never reviewed, which is stale rather than denied.
pub fn native_calendar_source_current(
    connection: NativeCalendarConnection<'_>,
    review: NativeCalendarReview<'_>,
) -> Result<SourceAuthority, AgentFailure> {
    if !binds(connection, review) || review.source_authority != Some(connection.source_authority) {
        return Err(AgentFailure::StaleContext);
    }
    review
        .source_authority
        .ok_or(AgentFailure::AccessReviewRequired)
}

/// Whether this Calendar provider is served by this device.
pub fn is_local_calendar_provider(provider: CalendarProvider) -> bool {
    match provider {
        CalendarProvider::EventKit | CalendarProvider::Android => true,
        #[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
        CalendarProvider::Fixture => true,
        _ => false,
    }
}

pub fn local_calendar_connector(provider: CalendarProvider) -> Option<&'static str> {
    match provider {
        CalendarProvider::EventKit => Some("calendar.event_kit"),
        CalendarProvider::Android => Some("calendar.android"),
        #[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
        CalendarProvider::Fixture => Some("calendar.fixture"),
        _ => None,
    }
}

/// The native provider a connector identity grants bind, when it is native.
pub fn local_calendar_provider(connector_id: &str) -> Option<CalendarProvider> {
    match connector_id {
        "calendar.event_kit" => Some(CalendarProvider::EventKit),
        "calendar.android" => Some(CalendarProvider::Android),
        #[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
        "calendar.fixture" => Some(CalendarProvider::Fixture),
        _ => None,
    }
}

/// The execution owner for an admitted local Calendar connector. This is the
/// source identity contract shared by Connections, Access, Context and the
/// native acquisition adapter; connector names are never inferred from an
/// owner string or from the host platform.
pub fn local_calendar_execution_owner_for_connector(
    connector_id: &str,
    device_id: &str,
) -> Option<String> {
    match local_calendar_provider(connector_id)? {
        CalendarProvider::EventKit => Some(crate::apple_execution_owner(device_id)),
        CalendarProvider::Android => Some(device_id.to_owned()),
        #[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
        CalendarProvider::Fixture => Some(fixture_calendar_execution_owner(device_id)),
        _ => None,
    }
}

/// Stable, device-scoped identity for the opt-in synthetic Calendar source.
#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
pub fn fixture_calendar_execution_owner(device_id: &str) -> String {
    format!("fixture:{device_id}")
}

/// The execution owner corresponding to a local Calendar provider.
pub fn local_calendar_execution_owner(
    provider: CalendarProvider,
    device_id: &str,
) -> Option<String> {
    local_calendar_connector(provider)
        .and_then(|connector| local_calendar_execution_owner_for_connector(connector, device_id))
}

/// Stable connection identifiers whose local providers are owned here.
/// Android remains adapter-defined until its native setup contract is shipped.
pub fn local_calendar_connection_id(provider: CalendarProvider) -> Option<&'static str> {
    match provider {
        CalendarProvider::EventKit => Some("calendar.event_kit.local"),
        #[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
        CalendarProvider::Fixture => Some("calendar.fixture.local"),
        _ => None,
    }
}

pub fn local_calendar_connection_id_for_connector(connector_id: &str) -> Option<&'static str> {
    local_calendar_provider(connector_id).and_then(local_calendar_connection_id)
}

fn binds(connection: NativeCalendarConnection<'_>, review: NativeCalendarReview<'_>) -> bool {
    !connection.disconnected
        && local_calendar_connection_id(connection.provider)
            .is_none_or(|expected| connection.connection_id == expected)
        && connection.device_id == review.device_id
        && connection.provider == review.provider
        && connection.scope == review.scope
        && review
            .connection_id
            .is_none_or(|expected| connection.connection_id == expected)
        && review
            .calendar_ids
            .iter()
            .all(|identifier| connection.calendar_ids.contains(identifier))
}

/// The authority an Expert setup on a native calendar may be installed under.
///
/// A connection that does not bind what the setup names is the Person's own
/// registry disagreeing with their device, which is a conflict rather than a
/// review; an authority or subject they never reviewed is a review they owe.
pub fn admit_native_calendar_setup(
    connection: NativeCalendarConnection<'_>,
    review: NativeCalendarReview<'_>,
) -> Result<SourceAuthority, AgentFailure> {
    if !binds(connection, review) {
        return Err(AgentFailure::Conflict);
    }
    reviewed_authority(connection, review)
}

/// The authority a device may be asked for its calendar subject under.
///
/// Nothing is installed by a subject preview, so a connection that no longer
/// binds what the Person is being shown is a review rather than a conflict.
pub fn admit_native_calendar_subject(
    connection: NativeCalendarConnection<'_>,
    review: NativeCalendarReview<'_>,
) -> Result<SourceAuthority, AgentFailure> {
    if !binds(connection, review) {
        return Err(AgentFailure::AccessReviewRequired);
    }
    reviewed_authority(connection, review)
}

fn reviewed_authority(
    connection: NativeCalendarConnection<'_>,
    review: NativeCalendarReview<'_>,
) -> Result<SourceAuthority, AgentFailure> {
    if !connection.source_authority.is_valid() {
        return Err(AgentFailure::AccessReviewRequired);
    }
    let reviewed = review
        .source_authority
        .ok_or(AgentFailure::AccessReviewRequired)?;
    if reviewed != connection.source_authority {
        return Err(AgentFailure::AccessReviewRequired);
    }
    Ok(reviewed)
}

/// That the connection a native read was admitted against is still the one
/// answering once the device has reported its subject.
pub fn native_calendar_connection_unchanged(
    connection: NativeCalendarConnection<'_>,
    expected_connection_id: &str,
    review: NativeCalendarReview<'_>,
    authority: SourceAuthority,
) -> Result<(), AgentFailure> {
    if !binds(
        connection,
        NativeCalendarReview {
            connection_id: Some(expected_connection_id),
            ..review
        },
    ) || connection.source_authority != authority
    {
        return Err(AgentFailure::AccessReviewRequired);
    }
    Ok(())
}

/// The subject the Person reviewed, as the caller must already hold it.
pub fn reviewed_native_subject<'a>(
    review: NativeCalendarReview<'a>,
) -> Result<&'a str, AgentFailure> {
    review
        .native_subject_fingerprint
        .ok_or(AgentFailure::AccessReviewRequired)
}
