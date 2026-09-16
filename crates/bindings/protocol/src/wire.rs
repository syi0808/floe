//! Turning a raw wire field into a value, and a failure into the error the
//! wire carries.
//!
//! Parsing an identifier, checking the schema version and naming a failure are
//! wire work, so they live beside the DTOs they produce rather than inside the
//! app they are handed to. An owner never speaks these shapes internally; the
//! bodies that still do are the ones R003 07 has yet to move.

use chrono::{DateTime, NaiveDate, Utc};
use floe_agent_contract::AgentFailure;
use floe_kernel::PersonId;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use uuid::Uuid;

use crate::{ErrorCodeDto, ErrorDto, PROTOCOL_VERSION, conversion::ProtocolConversionError};

/// A request that either answered or said exactly why it could not.
pub type WireResult<T> = Result<T, ErrorDto>;

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

/// Report that an agent request could not complete, naming the failure but not
/// what the request was carrying.
pub fn agent_failure(failure: AgentFailure) -> ErrorDto {
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

/// Restate a conversion failure as the error its caller reports.
pub fn conversion_error(value: ProtocolConversionError) -> ErrorDto {
    match value {
        ProtocolConversionError::UnsupportedVersion { actual, expected } => {
            unsupported_version(actual, expected)
        }
        ProtocolConversionError::InvalidField { field, message } => invalid(field, message),
        ProtocolConversionError::OutOfRange { field } => invalid(field, "value is out of range"),
    }
}

pub fn parse_person(value: &str) -> WireResult<PersonId> {
    Uuid::parse_str(value)
        .map(PersonId)
        .map_err(|value| invalid("person_id", value.to_string()))
}

pub fn parse_id<T>(value: &str, field: &'static str, wrap: impl FnOnce(Uuid) -> T) -> WireResult<T> {
    Uuid::parse_str(value)
        .map(wrap)
        .map_err(|value| invalid(field, value.to_string()))
}

pub fn parse_time(value: &str, field: &'static str) -> WireResult<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|value| invalid(field, value.to_string()))
}

pub fn parse_date(value: &str) -> WireResult<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|value| invalid("date", value.to_string()))
}

pub fn check_version(version: u32) -> WireResult<()> {
    if version == PROTOCOL_VERSION {
        Ok(())
    } else {
        Err(unsupported_version(version, PROTOCOL_VERSION))
    }
}

/// Re-encode an owned value as the protocol shape that carries it.
pub fn protocol_payload<Output: DeserializeOwned>(value: &impl Serialize) -> WireResult<Output> {
    serde_json::to_value(value)
        .and_then(serde_json::from_value)
        .map_err(|_| {
            error(
                ErrorCodeDto::Internal,
                "protocol response conversion failed",
            )
        })
}
