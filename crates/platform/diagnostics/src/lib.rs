mod context;
mod events;
mod redaction;

pub use context::{TraceContext, current_context, enter_context, instrument, span, with_context};
pub use events::{
    PanicHookGuard, PanicHookInstallError, PanicRecord, install_panic_hook, panic_record,
};
pub use redaction::{SafeCause, SafeStage};

#[cfg(test)]
mod tests {
    use std::{
        panic::{self, AssertUnwindSafe},
        sync::{
            Arc, Mutex, OnceLock,
            atomic::{AtomicBool, Ordering},
        },
    };

    use super::*;
    use tokio::sync::Barrier;
    use uuid::Uuid;

    static HOOK_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn instrument_restores_context_at_each_async_poll() {
        let barrier = Arc::new(Barrier::new(2));
        let first_context = TraceContext::new(Uuid::new_v4());
        let second_context = TraceContext::new(Uuid::new_v4());
        let first_barrier = barrier.clone();
        let second_barrier = barrier.clone();
        let first = tokio::spawn(instrument(
            async move {
                assert_eq!(current_context(), Some(first_context));
                first_barrier.wait().await;
                tokio::task::yield_now().await;
                current_context()
            },
            first_context,
            "first",
        ));
        let second = tokio::spawn(instrument(
            async move {
                assert_eq!(current_context(), Some(second_context));
                second_barrier.wait().await;
                tokio::task::yield_now().await;
                current_context()
            },
            second_context,
            "second",
        ));
        assert_eq!(first.await.unwrap(), Some(first_context));
        assert_eq!(second.await.unwrap(), Some(second_context));
        assert_eq!(current_context(), None);
    }

    #[test]
    fn cancelled_future_destructors_restore_their_own_context() {
        struct DropObserver(Arc<Mutex<Option<TraceContext>>>);

        impl Drop for DropObserver {
            fn drop(&mut self) {
                *self.0.lock().unwrap() = current_context();
            }
        }

        let observed = Arc::new(Mutex::new(None));
        let observer = DropObserver(observed.clone());
        let context = TraceContext::new(Uuid::new_v4());
        let future = instrument(
            async move {
                let _observer = observer;
                std::future::pending::<()>().await;
            },
            context,
            "cancelled",
        );
        drop(future);
        assert_eq!(*observed.lock().unwrap(), Some(context));
        assert_eq!(current_context(), None);
    }

    #[test]
    fn scoped_hook_only_suppresses_owned_context_and_preserves_replacements() {
        let _test_lock = HOOK_TEST_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap();
        let host_called = Arc::new(AtomicBool::new(false));
        let host_called_in_hook = host_called.clone();
        let previous = panic::take_hook();
        panic::set_hook(Box::new(move |_| {
            host_called_in_hook.store(true, Ordering::SeqCst);
        }));
        {
            let _guard = install_panic_hook().expect("hook is available");
            let panic = panic::catch_unwind(AssertUnwindSafe(|| panic!("host panic")))
                .expect_err("panic must be caught");
            let _ = panic_record(panic);
            assert!(host_called.load(Ordering::SeqCst));
            host_called.store(false, Ordering::SeqCst);
            let context = TraceContext::new(Uuid::new_v4());
            let panic = panic::catch_unwind(AssertUnwindSafe(|| {
                with_context(context, || panic!("secret model response"));
            }))
            .expect_err("panic must be caught");
            let record = panic_record(panic);
            assert_ne!(record.error_id, Uuid::nil());
            assert_eq!(record.panic_type, "str");
            assert!(!host_called.load(Ordering::SeqCst));

            let replacement_called = host_called.clone();
            panic::set_hook(Box::new(move |_| {
                replacement_called.store(true, Ordering::SeqCst);
            }));
        }
        let _ = panic::catch_unwind(AssertUnwindSafe(|| panic!("replacement panic")));
        assert!(host_called.load(Ordering::SeqCst));
        host_called.store(false, Ordering::SeqCst);
        {
            let _guard = install_panic_hook().expect("hook is available");
            let context = TraceContext::new(Uuid::new_v4());
            let panic = panic::catch_unwind(AssertUnwindSafe(|| {
                with_context(context, || panic!("owned panic"));
            }))
            .expect_err("panic must be caught");
            let _ = panic_record(panic);
        }
        let _ = panic::catch_unwind(AssertUnwindSafe(|| panic!("restored host panic")));
        assert!(host_called.load(Ordering::SeqCst));
        let _ = panic::take_hook();
        panic::set_hook(previous);
    }

    #[test]
    fn unwinding_guard_disables_suppression_for_future_host_panics() {
        let _test_lock = HOOK_TEST_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap();
        let host_called = Arc::new(AtomicBool::new(false));
        let host_called_in_hook = host_called.clone();
        let previous = panic::take_hook();
        panic::set_hook(Box::new(move |_| {
            host_called_in_hook.store(true, Ordering::SeqCst);
        }));
        let context = TraceContext::new(Uuid::new_v4());
        let _ = panic::catch_unwind(AssertUnwindSafe(|| {
            let _guard = install_panic_hook().expect("hook is available");
            with_context(context, || panic!("owned panic"));
        }));
        let _ = panic::catch_unwind(AssertUnwindSafe(|| panic!("future host panic")));
        assert!(host_called.load(Ordering::SeqCst));
        drop(install_panic_hook().expect("unwind does not permanently poison hook ownership"));
        let _ = panic::take_hook();
        panic::set_hook(previous);
    }
}
