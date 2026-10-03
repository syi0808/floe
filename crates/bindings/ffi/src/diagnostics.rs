use std::{any::Any, sync::Once};

use floe_app::{PanicRecord, panic_record};
use floe_protocol::{ErrorCodeDto, ErrorDto};
use tracing_subscriber::EnvFilter;

static INITIALIZE: Once = Once::new();

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
