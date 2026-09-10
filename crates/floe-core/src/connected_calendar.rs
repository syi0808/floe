use chrono::{DateTime, Utc};
use floe_agent::{
    CONNECTED_CONTEXT_VERSION, CapabilityAuthority, ConnectionState, ConnectorCapabilityDescriptor,
    ConnectorConnectionSnapshot, ConnectorDescriptor, ConnectorSnapshot, DataClass,
    ExecutionLocation, RetentionClass, SourceFailure, SourceFailureKind, ViewDescriptor,
    ViewSnapshot,
};
use floe_domain::{
    CalendarFailure, CalendarMirror, CalendarProvider, CalendarSyncStatus, PersonId,
};
use sha2::{Digest, Sha256};

use crate::{CoreError, ErrorCode, FloeCore};

const CALENDAR_VIEW_ID: &str = "calendar.timeline";
const CALENDAR_FRESHNESS_MS: u64 = 5 * 60 * 1_000;
const CALENDAR_MAX_ITEMS: usize = 128;
const CALENDAR_MAX_BYTES: usize = 65_536;

impl FloeCore {
    pub async fn calendar_connector_snapshot(
        &self,
        person_id: PersonId,
        device_id: &str,
        now: DateTime<Utc>,
    ) -> Result<Option<ConnectorSnapshot>, CoreError> {
        if device_id.trim().is_empty() || device_id.len() > 128 {
            return Err(CoreError::new(
                ErrorCode::Validation,
                "device identity must not be empty",
            ));
        }
        let Some(mirror) = self.store.calendar_mirror(person_id).await? else {
            return Ok(None);
        };
        Ok(Some(project_calendar_connector(&mirror, device_id, now)?))
    }
}

fn project_calendar_connector(
    mirror: &CalendarMirror,
    device_id: &str,
    now: DateTime<Utc>,
) -> Result<ConnectorSnapshot, CoreError> {
    let connection = &mirror.connection;
    let connector_id = connector_id(connection.provider);
    let now_unix_ms = milliseconds(now)?;
    let statuses = if connection.source_statuses.is_empty() && !connection.disconnected {
        connection
            .selected_calendars()
            .into_iter()
            .map(|calendar| {
                (
                    calendar.calendar_id,
                    CalendarSyncStatus {
                        last_success_at: connection.last_success_at,
                        last_range: connection.last_range.clone(),
                        error: connection.error,
                        error_at: connection.error_at,
                    },
                )
            })
            .collect()
    } else {
        connection.source_statuses.clone()
    };
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
    let state = if connection.disconnected {
        ConnectionState::Disconnected
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
    let last_failure = if let Some(failure) = connection.error {
        Some(SourceFailure {
            kind: source_failure_kind(failure),
            observed_at_unix_ms: connection
                .error_at
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
    let granted_scopes = if connection.disconnected {
        Vec::new()
    } else {
        vec!["calendar.events.read".into()]
    };
    let mut views = Vec::new();
    if !connection.disconnected {
        for (calendar_id, status) in statuses {
            if status.error.is_some() {
                continue;
            }
            let Some(success) = status.last_success_at else {
                continue;
            };
            let source_events: Vec<_> = mirror
                .events
                .iter()
                .filter(|event| {
                    matches!(&event.source, floe_domain::SourceRef::Calendar(source)
                        if source.provider == connection.provider
                            && source.calendar_id == calendar_id)
                })
                .collect();
            let projected_items: Vec<_> = source_events
                .iter()
                .map(|event| (&event.id, &event.title, &event.schedule))
                .collect();
            let byte_count = serde_json::to_vec(&projected_items)
                .map_err(|_| CoreError::new(ErrorCode::Storage, "calendar view is invalid"))?
                .len();
            views.push(ViewSnapshot {
                schema_version: CONNECTED_CONTEXT_VERSION,
                view_id: CALENDAR_VIEW_ID.into(),
                source_handle: source_handle(connection.provider, &calendar_id),
                observed_at_unix_ms: milliseconds(success)?,
                expires_at_unix_ms: milliseconds(
                    success
                        .checked_add_signed(chrono::Duration::minutes(5))
                        .ok_or_else(|| {
                            CoreError::new(ErrorCode::Validation, "calendar time is invalid")
                        })?,
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
            provider: provider_name(connection.provider).into(),
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
                data_class: match connection.provider {
                    CalendarProvider::Fixture => DataClass::Synthetic,
                    CalendarProvider::EventKit => DataClass::Personal,
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
            state,
            granted_scopes,
            observed_at_unix_ms: now_unix_ms,
            last_success_at_unix_ms: last_success_at.map(milliseconds).transpose()?,
            last_failure,
        },
        views,
    })
}

fn connector_id(provider: CalendarProvider) -> &'static str {
    match provider {
        CalendarProvider::Fixture => "calendar.fixture",
        CalendarProvider::EventKit => "calendar.event_kit",
    }
}

fn provider_name(provider: CalendarProvider) -> &'static str {
    match provider {
        CalendarProvider::Fixture => "fixture",
        CalendarProvider::EventKit => "apple_event_kit",
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

fn milliseconds(time: DateTime<Utc>) -> Result<u64, CoreError> {
    u64::try_from(time.timestamp_millis())
        .map_err(|_| CoreError::new(ErrorCode::Validation, "calendar time is invalid"))
}
