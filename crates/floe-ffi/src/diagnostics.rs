use std::{any::Any, cell::RefCell, sync::Once};

use floe_protocol::{ErrorCodeDto, ErrorDto};
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

static INITIALIZE: Once = Once::new();

thread_local! {
    static REQUEST_ID: RefCell<Option<String>> = const { RefCell::new(None) };
}

pub(crate) struct RequestGuard(Option<String>);

impl Drop for RequestGuard {
    fn drop(&mut self) {
        REQUEST_ID.with(|current| *current.borrow_mut() = self.0.take());
    }
}

pub(crate) fn enter_request(request_id: impl Into<String>) -> RequestGuard {
    let request_id = request_id.into();
    let previous = REQUEST_ID.with(|current| current.replace(Some(request_id)));
    RequestGuard(previous)
}

pub(crate) fn request_id() -> Option<String> {
    REQUEST_ID.with(|current| current.borrow().clone())
}

pub(crate) fn initialize() {
    INITIALIZE.call_once(|| {
        let filter = std::env::var("FLOE_LOG")
            .or_else(|_| std::env::var("RUST_LOG"))
            .unwrap_or_else(|_| "warn,floe_ffi=info".into());
        let subscriber = tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::new(filter))
            .with_ansi(false)
            .json()
            .flatten_event(true)
            .with_current_span(true)
            .with_span_list(true)
            .finish();
        let _ = tracing::subscriber::set_global_default(subscriber);
    });
}

pub(crate) fn panic_error(payload: Box<dyn Any + Send>) -> ErrorDto {
    let error_id = Uuid::new_v4().to_string();
    let panic_type = if payload.is::<&str>() {
        "str"
    } else if payload.is::<String>() {
        "string"
    } else {
        "unknown"
    };
    tracing::error!(
        error_id,
        panic_type,
        stage = "ffi_boundary",
        "rust_core_panicked"
    );
    ErrorDto {
        code: ErrorCodeDto::Internal,
        message: "Rust core panicked".into(),
        field: None,
        metadata: [("error_id".into(), error_id)].into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panic_error_is_correlated_without_exposing_payload() {
        let error = panic_error(Box::new(String::from("private model response")));

        assert_eq!(error.code, ErrorCodeDto::Internal);
        assert_eq!(error.message, "Rust core panicked");
        assert!(error.metadata["error_id"].parse::<Uuid>().is_ok());
        assert!(!format!("{error:?}").contains("private model response"));
    }
}
