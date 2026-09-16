//! The app handle this ABI hands back to its caller, and the app failures it
//! restates on the wire.
//!
//! Naming a failure on the wire is the binding's own work: the app reports its
//! own error, and this is where that error becomes the shape the caller reads.

use floe_app::{AppComposition, AppHost, AppOpenError, CoreError, ErrorCode, HostError};
use floe_protocol::{
    ErrorCodeDto, ErrorDto,
    wire::{error, invalid},
};

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

pub fn open_error(value: AppOpenError) -> ErrorDto {
    match value {
        AppOpenError::Host(host) => host_error(host),
        AppOpenError::Runtime(message) | AppOpenError::Store(message) => {
            error(ErrorCodeDto::Internal, message)
        }
    }
}
