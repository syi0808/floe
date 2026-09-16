//! Connection authorization Operations.
//!
//! Connections owns the Operation: whether an observed attempt may be trusted,
//! when the Operation continues, settles, times out or is cancelled. The client
//! only opens the authorization page and relays what it observed.

use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use floe_context_contract::ConnectionId;

/// How long an authorization Operation may stay pending before it times out.
pub const AUTHORIZATION_DEADLINE: Duration = Duration::minutes(5);
/// The interval the client is told to wait between observations.
pub const AUTHORIZATION_POLL_INTERVAL: Duration = Duration::seconds(2);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObservedConnectorStatus {
    Available,
    Connecting,
    Connected,
    Error,
    Unavailable,
}

/// One observation of an authorization attempt, exactly as the producer
/// reported it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservedAttempt {
    pub attempt_id: String,
    pub connector_id: String,
    pub connection_id: String,
    pub status: ObservedConnectorStatus,
    pub authorization_url: Option<String>,
    pub user_code: Option<String>,
    pub error_code: Option<String>,
}

/// The durable Operation this device is driving.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthorizationOperation {
    pub operation_id: Uuid,
    pub connection: ConnectionId,
    pub connector_id: String,
    pub attempt_id: String,
    /// Advances whenever the person restarts or cancels; stale observations for
    /// an earlier generation are ignored rather than applied.
    pub generation: u64,
    pub started_at: DateTime<Utc>,
    pub state: AuthorizationState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuthorizationState {
    /// Waiting for the person to finish authorization at `authorization_url`.
    Pending { authorization_url: String },
    Connected,
    Failed { code: String },
    Cancelled,
    TimedOut,
}

impl AuthorizationState {
    pub fn is_terminal(&self) -> bool {
        !matches!(self, Self::Pending { .. })
    }
}

/// What the client should do next. It never decides this for itself.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuthorizationDirective {
    /// Open this page, then observe again after the interval.
    OpenAuthorizationPage {
        authorization_url: String,
        poll_after: Duration,
    },
    /// Observe again after the interval; nothing else changed.
    ObserveAgain { poll_after: Duration },
    /// The Operation settled; refresh the connection projection.
    Settled { state: AuthorizationState },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthorizationError {
    /// The observation does not belong to this Operation.
    Mismatched,
    /// The producer reported a state this Operation cannot be in.
    InvalidTransition,
    /// The authorization page is not a page this device may open.
    UnsafeAuthorizationUrl,
}

/// Admit the first observation and start the Operation.
pub fn start_authorization(
    connection: ConnectionId,
    connector_id: String,
    observed: &ObservedAttempt,
    generation: u64,
    now: DateTime<Utc>,
) -> Result<(AuthorizationOperation, AuthorizationDirective), AuthorizationError> {
    if observed.connector_id != connector_id || observed.connection_id != connection.as_str() {
        return Err(AuthorizationError::Mismatched);
    }
    let operation = AuthorizationOperation {
        operation_id: Uuid::new_v4(),
        connection,
        connector_id,
        attempt_id: observed.attempt_id.clone(),
        generation,
        started_at: now,
        state: AuthorizationState::Pending {
            authorization_url: String::new(),
        },
    };
    match observed.status {
        ObservedConnectorStatus::Connecting => {
            let url = observed
                .authorization_url
                .as_deref()
                .ok_or(AuthorizationError::InvalidTransition)?;
            if !valid_authorization_url(url) {
                return Err(AuthorizationError::UnsafeAuthorizationUrl);
            }
            let operation = AuthorizationOperation {
                state: AuthorizationState::Pending {
                    authorization_url: url.to_owned(),
                },
                ..operation
            };
            Ok((
                operation,
                AuthorizationDirective::OpenAuthorizationPage {
                    authorization_url: url.to_owned(),
                    poll_after: AUTHORIZATION_POLL_INTERVAL,
                },
            ))
        }
        ObservedConnectorStatus::Connected => settle(operation, AuthorizationState::Connected),
        ObservedConnectorStatus::Error => settle(
            operation,
            AuthorizationState::Failed {
                code: observed
                    .error_code
                    .clone()
                    .unwrap_or_else(|| "connector_authorization_unavailable".into()),
            },
        ),
        ObservedConnectorStatus::Available | ObservedConnectorStatus::Unavailable => {
            Err(AuthorizationError::InvalidTransition)
        }
    }
}

/// Apply one observation to a pending Operation.
///
/// An observation for another attempt or an earlier generation is rejected, not
/// silently applied. The deadline is enforced here, never by the client.
pub fn observe_authorization(
    operation: &AuthorizationOperation,
    observed: &ObservedAttempt,
    generation: u64,
    now: DateTime<Utc>,
) -> Result<(AuthorizationOperation, AuthorizationDirective), AuthorizationError> {
    if operation.generation != generation
        || operation.attempt_id != observed.attempt_id
        || operation.connector_id != observed.connector_id
        || operation.connection.as_str() != observed.connection_id
    {
        return Err(AuthorizationError::Mismatched);
    }
    if operation.state.is_terminal() {
        return Err(AuthorizationError::InvalidTransition);
    }
    if now.signed_duration_since(operation.started_at) >= AUTHORIZATION_DEADLINE {
        return settle(operation.clone(), AuthorizationState::TimedOut);
    }
    match observed.status {
        ObservedConnectorStatus::Connecting => Ok((
            operation.clone(),
            AuthorizationDirective::ObserveAgain {
                poll_after: AUTHORIZATION_POLL_INTERVAL,
            },
        )),
        ObservedConnectorStatus::Connected => {
            settle(operation.clone(), AuthorizationState::Connected)
        }
        ObservedConnectorStatus::Error => settle(
            operation.clone(),
            AuthorizationState::Failed {
                code: observed
                    .error_code
                    .clone()
                    .unwrap_or_else(|| "connector_authorization_unavailable".into()),
            },
        ),
        ObservedConnectorStatus::Available | ObservedConnectorStatus::Unavailable => settle(
            operation.clone(),
            AuthorizationState::Failed {
                code: "connection_changed".into(),
            },
        ),
    }
}

/// Cancel a pending Operation. The generation advances so that an observation
/// already in flight can no longer settle it.
pub fn cancel_authorization(
    operation: &AuthorizationOperation,
) -> Result<AuthorizationOperation, AuthorizationError> {
    if operation.state.is_terminal() {
        return Err(AuthorizationError::InvalidTransition);
    }
    Ok(AuthorizationOperation {
        generation: operation.generation.saturating_add(1),
        state: AuthorizationState::Cancelled,
        ..operation.clone()
    })
}

fn settle(
    operation: AuthorizationOperation,
    state: AuthorizationState,
) -> Result<(AuthorizationOperation, AuthorizationDirective), AuthorizationError> {
    let settled = AuthorizationOperation {
        state: state.clone(),
        ..operation
    };
    Ok((settled, AuthorizationDirective::Settled { state }))
}

/// Only an HTTPS page without embedded credentials may be opened.
pub fn valid_authorization_url(value: &str) -> bool {
    let Some(rest) = value.strip_prefix("https://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    !authority.is_empty() && !authority.contains('@')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connection() -> ConnectionId {
        ConnectionId::try_new("connection-1").unwrap()
    }

    fn observed(status: ObservedConnectorStatus) -> ObservedAttempt {
        ObservedAttempt {
            attempt_id: "attempt-1".into(),
            connector_id: "calendar.google".into(),
            connection_id: "connection-1".into(),
            status,
            authorization_url: Some("https://provider.example/authorize".into()),
            user_code: None,
            error_code: None,
        }
    }

    #[test]
    fn an_authorization_page_must_be_https_without_embedded_credentials() {
        assert!(valid_authorization_url("https://provider.example/authorize"));
        assert!(!valid_authorization_url("http://provider.example/authorize"));
        assert!(!valid_authorization_url("https://user:pass@provider.example/a"));
        assert!(!valid_authorization_url("https:///authorize"));
    }

    #[test]
    fn an_observation_for_another_generation_cannot_settle_the_operation() {
        let now = Utc::now();
        let (operation, _) = start_authorization(
            connection(),
            "calendar.google".into(),
            &observed(ObservedConnectorStatus::Connecting),
            1,
            now,
        )
        .unwrap();
        assert_eq!(
            observe_authorization(
                &operation,
                &observed(ObservedConnectorStatus::Connected),
                2,
                now
            )
            .err(),
            Some(AuthorizationError::Mismatched)
        );
    }

    #[test]
    fn a_pending_operation_times_out_at_the_deadline_rather_than_polling_forever() {
        let now = Utc::now();
        let (operation, _) = start_authorization(
            connection(),
            "calendar.google".into(),
            &observed(ObservedConnectorStatus::Connecting),
            1,
            now,
        )
        .unwrap();
        let (settled, directive) = observe_authorization(
            &operation,
            &observed(ObservedConnectorStatus::Connecting),
            1,
            now + AUTHORIZATION_DEADLINE,
        )
        .unwrap();
        assert_eq!(settled.state, AuthorizationState::TimedOut);
        assert_eq!(
            directive,
            AuthorizationDirective::Settled {
                state: AuthorizationState::TimedOut
            }
        );
    }

    #[test]
    fn a_cancelled_operation_cannot_be_settled_by_an_observation_in_flight() {
        let now = Utc::now();
        let (operation, _) = start_authorization(
            connection(),
            "calendar.google".into(),
            &observed(ObservedConnectorStatus::Connecting),
            1,
            now,
        )
        .unwrap();
        let cancelled = cancel_authorization(&operation).unwrap();
        assert_eq!(cancelled.state, AuthorizationState::Cancelled);
        assert_eq!(
            observe_authorization(
                &cancelled,
                &observed(ObservedConnectorStatus::Connected),
                cancelled.generation,
                now
            )
            .err(),
            Some(AuthorizationError::InvalidTransition)
        );
    }
}
