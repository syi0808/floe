use std::{
    cell::RefCell,
    future::Future,
    marker::PhantomData,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll},
};

pub use floe_kernel::TraceContext;
use tracing::{Span, field};

thread_local! {
    static CURRENT_CONTEXT: RefCell<Option<TraceContext>> = const { RefCell::new(None) };
}

pub fn current_context() -> Option<TraceContext> {
    CURRENT_CONTEXT.with(|current| *current.borrow())
}

pub struct ContextGuard {
    previous: Option<TraceContext>,
    _not_send: PhantomData<Rc<()>>,
}

impl Drop for ContextGuard {
    fn drop(&mut self) {
        CURRENT_CONTEXT.with(|current| *current.borrow_mut() = self.previous.take());
    }
}

pub fn enter_context(context: TraceContext) -> ContextGuard {
    let previous = CURRENT_CONTEXT.with(|current| current.replace(Some(context)));
    ContextGuard {
        previous,
        _not_send: PhantomData,
    }
}

pub fn with_context<T>(context: TraceContext, operation: impl FnOnce() -> T) -> T {
    let _guard = enter_context(context);
    operation()
}

pub fn span(context: &TraceContext, operation: &'static str) -> Span {
    let span = tracing::info_span!(
        "floe_operation",
        operation,
        request_id = field::display(context.request_id()),
        run_id = field::display(""),
        task_id = field::display(""),
        attempt_id = field::display(""),
    );
    if let Some(run_id) = context.run_id() {
        span.record("run_id", field::display(run_id));
    }
    if let Some(task_id) = context.task_id() {
        span.record("task_id", field::display(task_id));
    }
    if let Some(attempt_id) = context.attempt_id() {
        span.record("attempt_id", field::display(attempt_id));
    }
    span
}

pub fn instrument<F>(
    future: F,
    context: TraceContext,
    operation: &'static str,
) -> impl Future<Output = F::Output>
where
    F: Future,
{
    TraceFuture {
        future: Some(Box::pin(future)),
        context,
        span: span(&context, operation),
    }
}

struct TraceFuture<F> {
    future: Option<Pin<Box<F>>>,
    context: TraceContext,
    span: Span,
}

impl<F: Future> Future for TraceFuture<F> {
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.as_mut().get_mut();
        let _context = enter_context(this.context);
        let _span = this.span.enter();
        this.future
            .as_mut()
            .expect("future is present until drop")
            .as_mut()
            .poll(cx)
    }
}

impl<F> Drop for TraceFuture<F> {
    fn drop(&mut self) {
        let _context = enter_context(self.context);
        let _span = self.span.enter();
        drop(self.future.take());
    }
}
