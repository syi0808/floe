//! Wire-level parsing and the shared identity values the app wire carries.
//!
//! The wire shape belongs here. Converting an owner's domain value to and from
//! a DTO is the binding's work, not this crate's; only values a contract owns
//! are converted here.

use chrono::{DateTime, NaiveDate, SecondsFormat, Utc};
use floe_context_contract::CalendarProvider;
use thiserror::Error;
use uuid::Uuid;

use crate::CalendarProviderDto;

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ProtocolConversionError {
    #[error("invalid {field}: {message}")]
    InvalidField {
        field: &'static str,
        message: String,
    },
    #[error("unsupported schema version {actual}; expected {expected}")]
    UnsupportedVersion { actual: u32, expected: u32 },
    #[error("{field} exceeds the protocol range")]
    OutOfRange { field: &'static str },
}

pub fn timestamp(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::AutoSi, true)
}

pub fn parse_timestamp(
    value: &str,
    field: &'static str,
) -> Result<DateTime<Utc>, ProtocolConversionError> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|error| ProtocolConversionError::InvalidField {
            field,
            message: error.to_string(),
        })
}

pub fn parse_date(value: &str, field: &'static str) -> Result<NaiveDate, ProtocolConversionError> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|error| {
        ProtocolConversionError::InvalidField {
            field,
            message: error.to_string(),
        }
    })
}

pub fn parse_uuid(value: &str, field: &'static str) -> Result<Uuid, ProtocolConversionError> {
    Uuid::parse_str(value).map_err(|error| ProtocolConversionError::InvalidField {
        field,
        message: error.to_string(),
    })
}

pub fn calendar_provider_to_dto(value: CalendarProvider) -> CalendarProviderDto {
    match value {
        CalendarProvider::Fixture => CalendarProviderDto::Fixture,
        CalendarProvider::EventKit => CalendarProviderDto::EventKit,
        CalendarProvider::Google => CalendarProviderDto::Google,
        CalendarProvider::Microsoft => CalendarProviderDto::Microsoft,
        CalendarProvider::Android => CalendarProviderDto::Android,
    }
}

pub fn calendar_provider_from_dto(value: CalendarProviderDto) -> CalendarProvider {
    match value {
        CalendarProviderDto::Fixture => CalendarProvider::Fixture,
        CalendarProviderDto::EventKit => CalendarProvider::EventKit,
        CalendarProviderDto::Google => CalendarProvider::Google,
        CalendarProviderDto::Microsoft => CalendarProvider::Microsoft,
        CalendarProviderDto::Android => CalendarProvider::Android,
    }
}

pub fn calendar_scope_to_dto(
    value: floe_context_contract::CalendarScope,
) -> crate::CalendarScopeDto {
    match value {
        floe_context_contract::CalendarScope::Selected => crate::CalendarScopeDto::Selected,
        floe_context_contract::CalendarScope::All => crate::CalendarScopeDto::All,
    }
}

pub fn calendar_scope_from_dto(
    value: crate::CalendarScopeDto,
) -> floe_context_contract::CalendarScope {
    match value {
        crate::CalendarScopeDto::Selected => floe_context_contract::CalendarScope::Selected,
        crate::CalendarScopeDto::All => floe_context_contract::CalendarScope::All,
    }
}
