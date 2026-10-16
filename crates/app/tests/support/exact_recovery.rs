//! Test clients recover an uncertain response with the exact same request.
//! This does not restart a workflow or retry a model/provider operation.
use std::{future::Future, time::Duration};

pub async fn observe_exact_request<R, T, E, F, Fut>(
    request: R,
    mut submit: F,
    retryable: impl Fn(&E) -> bool,
) -> Result<T, E>
where
    R: Clone,
    F: FnMut(R) -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    let mut retries = 0;
    loop {
        match submit(request.clone()).await {
            Err(failure) if retries < 3 && retryable(&failure) => {
                tokio::time::sleep(Duration::from_millis(25 << retries)).await;
                retries += 1;
            }
            result => return result,
        }
    }
}

pub fn connections_busy(failure: &floe_connections::ConnectionsCommandFailure) -> bool {
    use floe_connections::ConnectionsCommandFailure as F;
    use floe_kernel::AgentFailure::StorageBusy;
    matches!(
        failure,
        F::NotAdmitted(StorageBusy) | F::Indeterminate(StorageBusy) | F::Admitted(StorageBusy)
    )
}

pub fn observe_exact_request_sync<R, T, E>(
    request: R,
    mut submit: impl FnMut(R) -> Result<T, E>,
    retryable: impl Fn(&E) -> bool,
) -> Result<T, E>
where
    R: Clone,
{
    let mut retries = 0;
    loop {
        match submit(request.clone()) {
            Err(failure) if retries < 3 && retryable(&failure) => {
                std::thread::sleep(Duration::from_millis(25 << retries));
                retries += 1;
            }
            result => return result,
        }
    }
}
