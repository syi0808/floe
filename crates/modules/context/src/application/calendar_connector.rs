//! The Calendar mirror seen as a connector observation.
//!
//! Context owns what a source projection says about freshness, state and view
//! size; Connections owns the connector shapes this returns, and Day owns the
//! mirror it reads.

use chrono::{DateTime, Utc};
use floe_context_contract::CalendarProvider;
use floe_context_contract::DataClass;
use floe_day::{CalendarFailure, CalendarMirror, CalendarSyncStatus, SourceRef};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

use floe_connections::{
    CONNECTED_CONTEXT_VERSION, CapabilityAuthority, ConnectionState, ConnectorCapabilityDescriptor,
    ConnectorConnectionSnapshot, ConnectorDescriptor, ConnectorSnapshot, ExecutionLocation,
    RetentionClass, SourceConnection, SourceFailure, SourceFailureKind, SourceState,
    ViewDescriptor, ViewSnapshot,
};

/// Rejected when the caller's device identity is not usable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectorProjectionError {
    InvalidDevice,
    InvalidObservation,
}

const CALENDAR_VIEW_ID: &str = "calendar.timeline";
const CALENDAR_FRESHNESS_MS: u64 = 5 * 60 * 1_000;
const CALENDAR_MAX_ITEMS: usize = 128;
const CALENDAR_MAX_BYTES: usize = 65_536;

/// Validate the observing device before any connector projection is produced.
pub fn validate_connector_device(device_id: &str) -> Result<(), ConnectorProjectionError> {
    if device_id.trim().is_empty() || device_id.len() > 128 {
        return Err(ConnectorProjectionError::InvalidDevice);
    }
    Ok(())
}

pub fn project_calendar_connector(
    source: &SourceConnection,
    mirror: Option<&CalendarMirror>,
    device_id: &str,
    now: DateTime<Utc>,
) -> Result<ConnectorSnapshot, ConnectorProjectionError> {
    validate_connector_device(device_id)?;
    if source.execution_owner_id().as_str() != device_id {
        return Err(ConnectorProjectionError::InvalidObservation);
    }
    let connector_id = source.connector_id().as_str();
    let provider =
        provider_for_connector(connector_id).ok_or(ConnectorProjectionError::InvalidObservation)?;
    let mirror = mirror.filter(|mirror| {
        mirror.connection.connection_id == source.connection_id().as_str()
            && mirror.connection.provider == provider
    });
    let now_unix_ms = milliseconds(now)?;
    let statuses: BTreeMap<_, _> = source
        .resources()
        .iter()
        .map(|calendar| {
            let calendar_id = calendar.handle().as_str().to_owned();
            let status = mirror
                .and_then(|mirror| mirror.connection.source_statuses.get(&calendar_id))
                .cloned()
                .unwrap_or_else(|| CalendarSyncStatus {
                    last_success_at: mirror
                        .filter(|mirror| mirror.connection.source_statuses.is_empty())
                        .and_then(|mirror| mirror.connection.last_success_at),
                    last_range: mirror
                        .filter(|mirror| mirror.connection.source_statuses.is_empty())
                        .and_then(|mirror| mirror.connection.last_range.clone()),
                    error: mirror
                        .filter(|mirror| mirror.connection.source_statuses.is_empty())
                        .and_then(|mirror| mirror.connection.error),
                    error_at: mirror
                        .filter(|mirror| mirror.connection.source_statuses.is_empty())
                        .and_then(|mirror| mirror.connection.error_at),
                });
            (calendar_id, status)
        })
        .collect();
    let healthy_source_exists = statuses.values().any(|status| {
        status.error.is_none()
            && status.last_success_at.is_some_and(|success| {
                success <= now
                    && success
                        .checked_add_signed(chrono::Duration::minutes(5))
                        .is_some_and(|expiry| expiry > now)
            })
    });
    let failed_source_exists = statuses.values().any(|status| status.error.is_some());
    let stale_source_exists = statuses.values().any(|status| {
        status.error.is_none()
            && status.last_success_at.is_some_and(|success| {
                success > now
                    || success
                        .checked_add_signed(chrono::Duration::minutes(5))
                        .is_none_or(|expiry| expiry <= now)
            })
    });
    let pending_source_exists = statuses
        .values()
        .any(|status| status.error.is_none() && status.last_success_at.is_none());
    let state = if source.state() == SourceState::Disconnected {
        ConnectionState::Disconnected
    } else if source.state() == SourceState::Pending {
        ConnectionState::Pending
    } else if healthy_source_exists
        && (failed_source_exists || stale_source_exists || pending_source_exists)
    {
        ConnectionState::Degraded
    } else if healthy_source_exists {
        ConnectionState::Ready
    } else if failed_source_exists || stale_source_exists {
        ConnectionState::Unavailable
    } else {
        ConnectionState::Pending
    };
    let last_failure = if let Some(failure) = mirror.and_then(|mirror| mirror.connection.error) {
        Some(SourceFailure {
            kind: source_failure_kind(failure),
            observed_at_unix_ms: mirror
                .and_then(|mirror| mirror.connection.error_at)
                .map(milliseconds)
                .transpose()?
                .unwrap_or(now_unix_ms),
        })
    } else if stale_source_exists {
        Some(SourceFailure {
            kind: SourceFailureKind::Stale,
            observed_at_unix_ms: now_unix_ms,
        })
    } else if healthy_source_exists && pending_source_exists {
        Some(SourceFailure {
            kind: SourceFailureKind::NoData,
            observed_at_unix_ms: now_unix_ms,
        })
    } else {
        None
    };
    let last_success_at = statuses
        .values()
        .filter_map(|status| status.last_success_at)
        .max();
    let granted_scopes = if source.state() == SourceState::Disconnected {
        Vec::new()
    } else {
        vec!["calendar.events.read".into()]
    };
    let mut views = Vec::new();
    if source.is_serving() {
        for (calendar_id, status) in statuses {
            if status.error.is_some() {
                continue;
            }
            let Some(success) = status.last_success_at else {
                continue;
            };
            let source_events: Vec<_> = mirror
                .into_iter()
                .flat_map(|mirror| mirror.events.iter())
                .filter(|event| {
                    matches!(&event.source, SourceRef::Calendar(source)
                        if source.provider == provider
                            && source.calendar_id == calendar_id)
                })
                .collect();
            let projected_items: Vec<_> = source_events
                .iter()
                .map(|event| (&event.id, &event.title, &event.schedule))
                .collect();
            let byte_count = serde_json::to_vec(&projected_items)
                .map_err(|_| ConnectorProjectionError::InvalidObservation)?
                .len();
            views.push(ViewSnapshot {
                schema_version: CONNECTED_CONTEXT_VERSION,
                view_id: CALENDAR_VIEW_ID.into(),
                source_handle: source_handle(provider, &calendar_id),
                observed_at_unix_ms: milliseconds(success)?,
                expires_at_unix_ms: milliseconds(
                    success
                        .checked_add_signed(chrono::Duration::minutes(5))
                        .ok_or_else(|| ConnectorProjectionError::InvalidObservation)?,
                )?,
                item_count: source_events.len(),
                byte_count,
                provenance_count: source_events.len(),
            });
        }
    }
    views.sort_by(|left, right| left.source_handle.cmp(&right.source_handle));

    Ok(ConnectorSnapshot {
        descriptor: ConnectorDescriptor {
            schema_version: CONNECTED_CONTEXT_VERSION,
            id: connector_id.into(),
            version: "1.0.0".into(),
            provider: provider_name(provider).into(),
            execution: ExecutionLocation::Device {
                device_id: device_id.into(),
            },
            capabilities: vec![
                ConnectorCapabilityDescriptor {
                    schema_version: CONNECTED_CONTEXT_VERSION,
                    id: "calendar.events.read".into(),
                    version: "1.0.0".into(),
                    authority: CapabilityAuthority::Observe,
                    required_scopes: vec!["calendar.events.read".into()],
                    output_view_id: Some(CALENDAR_VIEW_ID.into()),
                },
                ConnectorCapabilityDescriptor {
                    schema_version: CONNECTED_CONTEXT_VERSION,
                    id: "calendar.events.create".into(),
                    version: "1.0.0".into(),
                    authority: CapabilityAuthority::Act,
                    required_scopes: vec!["calendar.events.write".into()],
                    output_view_id: None,
                },
            ],
            views: vec![ViewDescriptor {
                schema_version: CONNECTED_CONTEXT_VERSION,
                id: CALENDAR_VIEW_ID.into(),
                version: "1.0.0".into(),
                data_class: match provider {
                    CalendarProvider::Fixture => DataClass::Synthetic,
                    CalendarProvider::EventKit
                    | CalendarProvider::Google
                    | CalendarProvider::Microsoft
                    | CalendarProvider::Android => DataClass::Personal,
                },
                retention: RetentionClass::Mirror,
                freshness_ttl_ms: CALENDAR_FRESHNESS_MS,
                max_items: CALENDAR_MAX_ITEMS,
                max_bytes: CALENDAR_MAX_BYTES,
                provenance_required: true,
            }],
        },
        connection: ConnectorConnectionSnapshot {
            schema_version: CONNECTED_CONTEXT_VERSION,
            connector_id: connector_id.into(),
            connection_id: Some(source.connection_id().as_str().to_owned()),
            person_id: Some(source.person_id().to_string()),
            device_binding: None,
            state,
            granted_scopes,
            observed_at_unix_ms: now_unix_ms,
            last_success_at_unix_ms: last_success_at.map(milliseconds).transpose()?,
            last_failure,
        },
        views,
    })
}

fn provider_for_connector(connector_id: &str) -> Option<CalendarProvider> {
    match connector_id {
        "calendar.fixture" => Some(CalendarProvider::Fixture),
        "calendar.event_kit" => Some(CalendarProvider::EventKit),
        "calendar.google" => Some(CalendarProvider::Google),
        "calendar.microsoft" => Some(CalendarProvider::Microsoft),
        "calendar.android" => Some(CalendarProvider::Android),
        _ => None,
    }
}

fn provider_name(provider: CalendarProvider) -> &'static str {
    match provider {
        CalendarProvider::Fixture => "fixture",
        CalendarProvider::EventKit => "apple_event_kit",
        CalendarProvider::Google => "google_calendar",
        CalendarProvider::Microsoft => "microsoft_calendar",
        CalendarProvider::Android => "android_calendar",
    }
}

fn source_failure_kind(failure: CalendarFailure) -> SourceFailureKind {
    match failure {
        CalendarFailure::PermissionDenied => SourceFailureKind::PermissionDenied,
        CalendarFailure::CalendarUnavailable | CalendarFailure::ProviderUnavailable => {
            SourceFailureKind::Unavailable
        }
    }
}

fn source_handle(provider: CalendarProvider, calendar_id: &str) -> String {
    let digest = Sha256::digest(format!("{}:{calendar_id}", provider_name(provider)).as_bytes());
    format!("calendar.timeline:{digest:x}")
}

fn milliseconds(time: DateTime<Utc>) -> Result<u64, ConnectorProjectionError> {
    u64::try_from(time.timestamp_millis()).map_err(|_| ConnectorProjectionError::InvalidObservation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use floe_connections::{ConnectionResource, ResourceMode};
    use floe_context_contract::{ConnectionId, ConnectorId, ExecutionOwnerId, ResourceHandle};
    use floe_kernel::PersonId;

    #[test]
    fn source_identity_projects_without_a_mirror() {
        let person = PersonId::new();
        let source = SourceConnection::establish(
            person,
            ConnectorId::try_new("calendar.event_kit").unwrap(),
            ConnectionId::try_new("calendar-source").unwrap(),
            ExecutionOwnerId::try_new("device").unwrap(),
            ResourceMode::Selected,
            vec![
                ConnectionResource::new(ResourceHandle::try_new("home").unwrap(), "Home".into())
                    .unwrap(),
            ],
        )
        .unwrap();
        let before = source.clone();
        let snapshot = project_calendar_connector(&source, None, "device", Utc::now()).unwrap();
        assert_eq!(snapshot.connection.state, ConnectionState::Pending);
        assert_eq!(
            snapshot.connection.connection_id.as_deref(),
            Some("calendar-source")
        );
        assert_eq!(
            snapshot.connection.person_id.as_deref(),
            Some(person.to_string().as_str())
        );
        assert!(snapshot.views.is_empty());
        assert_eq!(source, before);
    }

    #[test]
    fn foreign_device_cannot_project_calendar_source() {
        let source = SourceConnection::establish(
            PersonId::new(),
            ConnectorId::try_new("calendar.event_kit").unwrap(),
            ConnectionId::try_new("calendar-source").unwrap(),
            ExecutionOwnerId::try_new("device-a").unwrap(),
            ResourceMode::Selected,
            vec![
                ConnectionResource::new(ResourceHandle::try_new("home").unwrap(), "Home".into())
                    .unwrap(),
            ],
        )
        .unwrap();
        assert_eq!(
            project_calendar_connector(&source, None, "device-b", Utc::now()),
            Err(ConnectorProjectionError::InvalidObservation)
        );
    }

    #[test]
    fn stale_mirror_status_cannot_expand_source_resources() {
        let person = PersonId::new();
        let mut source = SourceConnection::establish(
            person,
            ConnectorId::try_new("calendar.event_kit").unwrap(),
            ConnectionId::try_new("calendar-source").unwrap(),
            ExecutionOwnerId::try_new("device").unwrap(),
            ResourceMode::Selected,
            vec![
                ConnectionResource::new(ResourceHandle::try_new("home").unwrap(), "Home".into())
                    .unwrap(),
            ],
        )
        .unwrap();
        source
            .update_native_subject(source.revision(), "a".repeat(64))
            .unwrap();
        let now = Utc::now();
        let status = CalendarSyncStatus {
            last_success_at: Some(now),
            last_range: None,
            error: None,
            error_at: None,
        };
        let mirror = CalendarMirror {
            connection: floe_day::CalendarConnection {
                connection_id: "calendar-source".into(),
                device_id: "device".into(),
                disconnected: false,
                scope: floe_context_contract::CalendarScope::Selected,
                provider: CalendarProvider::EventKit,
                calendars: vec![],
                revision: 1,
                source_authority: floe_context_contract::SourceAuthority::new(),
                last_success_at: Some(now),
                last_range: None,
                error: None,
                error_at: None,
                source_statuses: BTreeMap::from([
                    ("home".into(), status.clone()),
                    ("work".into(), status),
                ]),
            },
            events: vec![],
        };
        let snapshot = project_calendar_connector(&source, Some(&mirror), "device", now).unwrap();
        assert_eq!(snapshot.connection.state, ConnectionState::Ready);
        assert_eq!(snapshot.views.len(), 1);
    }
}
