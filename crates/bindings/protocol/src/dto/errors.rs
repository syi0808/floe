use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::APP_WIRE_VERSION;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppWireErrorCodeDto {
    Validation,
    UnsupportedVersion,
    CommandIdConflict,
    SessionBusy,
    Conflict,
    NotFound,
    AccessDenied,
    Unavailable,
    Internal,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AppWireErrorDto {
    pub code: AppWireErrorCodeDto,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_failure: Option<OwnerFailureDto>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AppResponseDto<T> {
    pub schema_version: u32,
    pub request_id: Uuid,
    #[serde(flatten)]
    pub outcome: AppResponseOutcomeDto<T>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AppResponseOutcomeDto<T> {
    Ok {
        result: T,
    },
    Error {
        error: AppWireErrorDto,
    },
    CommandError {
        disposition: AppCommandDispositionDto,
        error: AppWireErrorDto,
    },
}

/// Admission evidence for this delivery of the exact command.
/// A later rejection does not erase an earlier indeterminate delivery.
/// Error categories and recovery hints never supply this evidence.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppCommandDispositionDto {
    NotAdmitted,
    Admitted,
    Indeterminate,
}

impl<T> AppResponseDto<T> {
    pub fn ok(request_id: Uuid, result: T) -> Self {
        Self {
            schema_version: APP_WIRE_VERSION,
            request_id,
            outcome: AppResponseOutcomeDto::Ok { result },
        }
    }

    pub fn error(request_id: Uuid, error: AppWireErrorDto) -> Self {
        Self {
            schema_version: APP_WIRE_VERSION,
            request_id,
            outcome: AppResponseOutcomeDto::Error { error },
        }
    }

    pub fn command_error(
        request_id: Uuid,
        disposition: AppCommandDispositionDto,
        error: AppWireErrorDto,
    ) -> Self {
        Self {
            schema_version: APP_WIRE_VERSION,
            request_id,
            outcome: AppResponseOutcomeDto::CommandError { disposition, error },
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnerRecoveryDto {
    None,
    Reobserve,
    Reconcile,
    Unlock,
    Reopen,
    NewReview,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerFailureDto {
    pub domain: floe_kernel::AgentFailureDomain,
    pub category: floe_kernel::AgentFailureCategory,
    pub reason: floe_kernel::AgentFailure,
    pub incident_id: super::UuidRefDto,
    pub correlation_id: super::UuidRefDto,
    pub reload_required: bool,
    pub seal_session: bool,
    pub recovery: OwnerRecoveryDto,
    pub safe_actions: Vec<floe_kernel::AgentFailureSafeAction>,
}
