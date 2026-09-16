//! Correlating one host job with the trace it emits, without exposing what it
//! was carrying.
//!
//! A panic payload can hold anything the job was working on, so it is recorded
//! as an identifier and a type name and never read back out.

use std::any::Any;

use floe_diagnostics::{PanicRecord, TraceContext, panic_record};
use uuid::Uuid;

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
    floe_diagnostics::instrument(future, context, operation)
}

/// Record that a host job panicked, and return the identifier it was recorded
/// under so the caller can correlate the failure it reports.
pub(crate) fn panic_error(payload: Box<dyn Any + Send>) -> Uuid {
    let PanicRecord {
        error_id,
        panic_type,
    } = panic_record(payload);
    tracing::error!(
        error_id = %error_id,
        panic_type,
        stage = "host_job",
        "rust_core_panicked"
    );
    error_id
}
