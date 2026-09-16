//! Turning one host request into an answer the caller's boundary can carry.
//!
//! Parsing an identifier, checking the protocol version and naming a failure
//! are the same wherever a request arrives from, so the host does them once
//! here and every entry point reports the same shape.

use chrono::{DateTime, NaiveDate, Utc};
use floe_kernel::PersonId;
use floe_protocol::{ErrorCodeDto, ErrorDto, PROTOCOL_VERSION};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use uuid::Uuid;

use crate::{AppComposition, AppHost, CoreError, ErrorCode, HostError};

/// A host request that either answered or said exactly why it could not.
pub type BridgeResult<T> = Result<T, ErrorDto>;

/// One opened app, held for as long as its caller holds it.
pub struct FloeHandle {
    app: AppHost<AppComposition>,
}

impl FloeHandle {
    pub fn new(app: AppHost<AppComposition>) -> Self {
        Self { app }
    }

    pub fn services(&self) -> &AppComposition {
        self.app.legacy_services()
    }

    pub fn app(&self) -> &AppHost<AppComposition> {
        &self.app
    }
}

pub fn error(code: ErrorCodeDto, message: impl Into<String>) -> ErrorDto {
    ErrorDto {
        code,
        message: message.into(),
        field: None,
        metadata: Default::default(),
    }
}

pub fn invalid(field: &'static str, message: impl Into<String>) -> ErrorDto {
    ErrorDto {
        code: ErrorCodeDto::Validation,
        message: message.into(),
        field: Some(field.into()),
        metadata: Default::default(),
    }
}

pub fn host_error(value: HostError) -> ErrorDto {
    match value {
        HostError::InvalidIdentity | HostError::InvalidRequest => {
            invalid("host", "invalid local host identity")
        }
        HostError::IdentityUnavailable => {
            error(ErrorCodeDto::Internal, "local host identity is unavailable")
        }
        HostError::Closing | HostError::UnsupportedCaller | HostError::Shutdown => {
            error(ErrorCodeDto::Internal, "host is unavailable")
        }
    }
}

pub fn core_error(value: CoreError) -> ErrorDto {
    ErrorDto {
        code: match value.code {
            ErrorCode::Validation => ErrorCodeDto::Validation,
            ErrorCode::NotFound => ErrorCodeDto::NotFound,
            ErrorCode::Conflict => ErrorCodeDto::Conflict,
            ErrorCode::Storage => ErrorCodeDto::Storage,
            ErrorCode::NoFocusSlot => ErrorCodeDto::NoFocusSlot,
        },
        message: value.message,
        field: None,
        metadata: value.metadata,
    }
}

/// Report that an agent request could not complete, naming the failure but not
/// what the request was carrying.
pub fn agent_failure(failure: floe_agent_contract::AgentFailure) -> ErrorDto {
    use floe_agent_contract::AgentFailure;

    let code = match failure {
        AgentFailure::NotFound => ErrorCodeDto::NotFound,
        AgentFailure::Conflict => ErrorCodeDto::Conflict,
        AgentFailure::StorageUnavailable => ErrorCodeDto::Storage,
        AgentFailure::UnsupportedVersion => ErrorCodeDto::UnsupportedVersion,
        _ => ErrorCodeDto::Validation,
    };
    let mut result = error(code, "Agent request could not complete");
    if let Ok(Value::String(reason)) = serde_json::to_value(failure) {
        result.metadata.insert("agent_failure".into(), reason);
    }
    result
}

pub fn unsupported_version(actual: u32, expected: u32) -> ErrorDto {
    let mut value = error(
        ErrorCodeDto::UnsupportedVersion,
        format!("unsupported schema version {actual}; expected {expected}"),
    );
    value.metadata.insert("actual".into(), actual.to_string());
    value
        .metadata
        .insert("expected".into(), expected.to_string());
    value
}

pub fn parse_person(value: &str) -> BridgeResult<PersonId> {
    Uuid::parse_str(value)
        .map(PersonId)
        .map_err(|value| invalid("person_id", value.to_string()))
}

pub fn parse_id<T>(value: &str, field: &'static str, wrap: impl FnOnce(Uuid) -> T) -> BridgeResult<T> {
    Uuid::parse_str(value)
        .map(wrap)
        .map_err(|value| invalid(field, value.to_string()))
}

pub fn parse_time(value: &str, field: &'static str) -> BridgeResult<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|value| invalid(field, value.to_string()))
}

pub fn parse_date(value: &str) -> BridgeResult<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|value| invalid("date", value.to_string()))
}

pub fn check_version(version: u32) -> BridgeResult<()> {
    if version == PROTOCOL_VERSION {
        Ok(())
    } else {
        Err(unsupported_version(version, PROTOCOL_VERSION))
    }
}

/// Re-encode an owned value as the protocol shape that carries it.
pub fn protocol_payload<Output: DeserializeOwned>(value: &impl Serialize) -> BridgeResult<Output> {
    serde_json::to_value(value)
        .and_then(serde_json::from_value)
        .map_err(|_| {
            error(
                ErrorCodeDto::Internal,
                "protocol response conversion failed",
            )
        })
}

/// Restate an Action failure as the error its caller reports.
pub fn action_error(value: floe_actions::ActionError) -> ErrorDto {
    use floe_actions::ActionErrorCode;

    ErrorDto {
        code: match value.code {
            ActionErrorCode::Validation => ErrorCodeDto::Validation,
            ActionErrorCode::NotFound => ErrorCodeDto::NotFound,
            ActionErrorCode::Conflict => ErrorCodeDto::Conflict,
            ActionErrorCode::Storage => ErrorCodeDto::Storage,
        },
        message: value.message,
        field: None,
        metadata: value.metadata,
    }
}

/// Restate a protocol conversion failure as the error its caller reports.
pub fn conversion_error(value: floe_protocol::conversion::ProtocolConversionError) -> ErrorDto {
    use floe_protocol::conversion::ProtocolConversionError;

    match value {
        ProtocolConversionError::UnsupportedVersion { actual, expected } => {
            unsupported_version(actual, expected)
        }
        ProtocolConversionError::InvalidField { field, message } => invalid(field, message),
        ProtocolConversionError::OutOfRange { field } => invalid(field, "value is out of range"),
    }
}
