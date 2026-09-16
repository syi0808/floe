use std::{any::Any, sync::Once};

use floe_app::modules::diagnostics::{PanicRecord, TraceContext, panic_record};
use floe_protocol::{ErrorCodeDto, ErrorDto};
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

static INITIALIZE: Once = Once::new();

pub(crate) fn trace_context(request_id: Uuid) -> TraceContext {
    TraceContext::new(request_id)
}

pub(crate) fn instrument<F>(
    future: F,
    context: TraceContext,
    operation: &'static str,
) -> impl std::future::Future<Output = F::Output>
where
    F: std::future::Future,
{
    floe_app::modules::diagnostics::instrument(future, context, operation)
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
    let PanicRecord {
        error_id,
        panic_type,
    } = panic_record(payload);
    let error_id = error_id.to_string();
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
