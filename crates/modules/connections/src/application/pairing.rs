use floe_execution::Cancellation;
use floe_kernel::AgentFailure;
use tokio::time::Instant;

use crate::{
    PairingConfirmation, PairingConfirmationRequest, PairingStatus, PairingStatusRequest,
    RemoteControl,
};

#[derive(Clone)]
pub struct PairingService<Remote> {
    remote: Remote,
}

impl<Remote: RemoteControl> PairingService<Remote> {
    pub fn new(remote: Remote) -> Self {
        Self { remote }
    }

    pub async fn confirm(
        &self,
        request: PairingConfirmationRequest,
        deadline: Instant,
        cancellation: &Cancellation,
    ) -> Result<PairingConfirmation, AgentFailure> {
        validate_uuid_text(&request.pairing_id)?;
        validate_uuid_text(&request.challenge_id)?;
        validate_token_text(&request.polling_proof)?;
        if request.key_id.is_empty() || request.signature.is_empty() {
            return Err(AgentFailure::InvalidInput);
        }
        let child = cancellation.child_scope();
        let response = floe_execution::tasks::run_bounded(
            async { self.remote.confirm(request.clone(), deadline, &child).await },
            deadline,
            &child,
        )
        .await?;
        validate_confirmation(&request.pairing_id, &response)?;
        Ok(response)
    }

    pub async fn status(
        &self,
        request: PairingStatusRequest,
        deadline: Instant,
        cancellation: &Cancellation,
    ) -> Result<PairingStatus, AgentFailure> {
        validate_uuid_text(&request.pairing_id)?;
        validate_token_text(&request.polling_proof)?;
        let child = cancellation.child_scope();
        let response = floe_execution::tasks::run_bounded(
            async { self.remote.status(request.clone(), deadline, &child).await },
            deadline,
            &child,
        )
        .await?;
        validate_status(&request.pairing_id, &response)?;
        Ok(response)
    }
}

fn validate_confirmation(
    pairing_id: &str,
    response: &PairingConfirmation,
) -> Result<(), AgentFailure> {
    if response.schema_version != 1
        || response.pairing_id != pairing_id
        || !matches!(response.status.as_str(), "local_confirmed" | "approved")
    {
        return Err(AgentFailure::CapabilityUnavailable);
    }
    Ok(())
}

fn validate_status(pairing_id: &str, response: &PairingStatus) -> Result<(), AgentFailure> {
    if response.schema_version != 1
        || response.pairing_id != pairing_id
        || !matches!(
            response.status.as_str(),
            "pending" | "local_confirmed" | "approved" | "rejected" | "expired" | "repair_required"
        )
        || (response.status == "approved"
            && (response.client_id.is_none() || response.token.is_none()))
        || (response.status != "approved"
            && (response.client_id.is_some() || response.token.is_some()))
    {
        return Err(AgentFailure::CapabilityUnavailable);
    }
    Ok(())
}

fn validate_uuid_text(value: &str) -> Result<(), AgentFailure> {
    if uuid::Uuid::parse_str(value).is_ok_and(|uuid| !uuid.is_nil() && uuid.to_string() == value) {
        Ok(())
    } else {
        Err(AgentFailure::InvalidInput)
    }
}

fn validate_token_text(value: &str) -> Result<(), AgentFailure> {
    if !value.is_empty()
        && value.len() <= 256
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        Ok(())
    } else {
        Err(AgentFailure::InvalidInput)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct HangingRemote {
        calls: std::sync::atomic::AtomicUsize,
        cancellation: std::sync::Mutex<Option<Cancellation>>,
    }

    impl RemoteControl for HangingRemote {
        async fn confirm(
            &self,
            _: PairingConfirmationRequest,
            _: Instant,
            _: &Cancellation,
        ) -> Result<PairingConfirmation, AgentFailure> {
            unreachable!()
        }

        async fn status(
            &self,
            _: PairingStatusRequest,
            _: Instant,
            cancellation: &Cancellation,
        ) -> Result<PairingStatus, AgentFailure> {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            *self.cancellation.lock().unwrap() = Some(cancellation.clone());
            std::future::pending().await
        }
    }

    fn poll_request() -> PairingStatusRequest {
        PairingStatusRequest {
            pairing_id: pair_id(),
            polling_proof: "polling-proof".into(),
        }
    }

    #[tokio::test]
    async fn service_fences_expired_and_cancelled_calls_before_remote_invocation() {
        let service = PairingService::new(HangingRemote::default());
        let parent = Cancellation::default();
        assert_eq!(
            service
                .status(poll_request(), Instant::now(), &parent)
                .await,
            Err(AgentFailure::DeadlineExceeded)
        );
        assert!(!parent.is_cancelled());
        parent.cancel();
        assert_eq!(
            service
                .status(
                    poll_request(),
                    Instant::now() + std::time::Duration::from_secs(5),
                    &parent
                )
                .await,
            Err(AgentFailure::Cancelled)
        );
        assert_eq!(
            service
                .remote
                .calls
                .load(std::sync::atomic::Ordering::SeqCst),
            0
        );
    }

    #[tokio::test]
    async fn service_bounds_uncooperative_remote_and_cancels_only_its_child() {
        let service = PairingService::new(HangingRemote::default());
        let parent = Cancellation::default();
        assert_eq!(
            service
                .status(
                    poll_request(),
                    Instant::now() + std::time::Duration::from_millis(20),
                    &parent
                )
                .await,
            Err(AgentFailure::DeadlineExceeded)
        );
        assert!(!parent.is_cancelled());
        assert_eq!(
            service
                .remote
                .cancellation
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .reason(),
            Some(floe_execution::CancelReason::Deadline)
        );
        let mut call = Box::pin(service.status(
            poll_request(),
            Instant::now() + std::time::Duration::from_secs(5),
            &parent,
        ));
        std::future::poll_fn(|context| {
            assert!(std::future::Future::poll(call.as_mut(), context).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        drop(call);
        assert!(!parent.is_cancelled());
        assert_eq!(
            service
                .remote
                .cancellation
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .reason(),
            Some(floe_execution::CancelReason::OwnerDropped)
        );
    }

    #[test]
    fn status_credentials_are_reserved_for_approved() {
        let mut response = valid_status();
        for status in [
            "pending",
            "local_confirmed",
            "rejected",
            "expired",
            "repair_required",
        ] {
            response.status = status.into();
            assert_eq!(
                validate_status(&pair_id(), &response),
                Err(AgentFailure::CapabilityUnavailable)
            );
        }
        response.status = "approved".into();
        response.token = None;
        assert_eq!(
            validate_status(&pair_id(), &response),
            Err(AgentFailure::CapabilityUnavailable)
        );
        response.client_id = None;
        response.status = "unknown_status".into();
        assert_eq!(
            validate_status(&pair_id(), &response),
            Err(AgentFailure::CapabilityUnavailable)
        );
    }

    #[derive(Clone)]
    struct Remote {
        confirmation: PairingConfirmation,
        status: PairingStatus,
    }

    impl RemoteControl for Remote {
        async fn confirm(
            &self,
            _: PairingConfirmationRequest,
            _: Instant,
            _: &Cancellation,
        ) -> Result<PairingConfirmation, AgentFailure> {
            Ok(self.confirmation.clone())
        }

        async fn status(
            &self,
            _: PairingStatusRequest,
            _: Instant,
            _: &Cancellation,
        ) -> Result<PairingStatus, AgentFailure> {
            Ok(self.status.clone())
        }
    }

    fn pair_id() -> String {
        "00000000-0000-4000-8000-000000000001".into()
    }

    fn service(status: PairingStatus) -> PairingService<Remote> {
        PairingService::new(Remote {
            confirmation: PairingConfirmation {
                schema_version: 1,
                pairing_id: pair_id(),
                status: "local_confirmed".into(),
            },
            status,
        })
    }

    fn valid_status() -> PairingStatus {
        PairingStatus {
            schema_version: 1,
            pairing_id: pair_id(),
            status: "approved".into(),
            person_id: "person".into(),
            device_id: "device".into(),
            producer: None,
            issuer: None,
            issuer_fingerprint: None,
            client_id: Some(pair_id()),
            token: Some("token".into()),
        }
    }

    #[tokio::test]
    async fn validates_pairing_request_and_response_at_service_boundary() {
        let service = service(valid_status());
        let cancellation = Cancellation::default();
        assert_eq!(
            service
                .confirm(
                    PairingConfirmationRequest {
                        pairing_id: pair_id(),
                        polling_proof: "polling-proof".into(),
                        challenge_id: pair_id(),
                        key_id: "key".into(),
                        signature: "signature".into(),
                    },
                    Instant::now() + tokio::time::Duration::from_secs(5),
                    &cancellation,
                )
                .await
                .unwrap()
                .status,
            "local_confirmed"
        );
        assert_eq!(
            service
                .status(
                    PairingStatusRequest {
                        pairing_id: pair_id(),
                        polling_proof: "polling-proof".into(),
                    },
                    Instant::now() + tokio::time::Duration::from_secs(5),
                    &cancellation,
                )
                .await
                .unwrap()
                .status,
            "approved"
        );
        assert_eq!(
            service
                .status(
                    PairingStatusRequest {
                        pairing_id: "00000000-0000-4000-8000-000000000002".into(),
                        polling_proof: "polling-proof".into(),
                    },
                    Instant::now() + tokio::time::Duration::from_secs(5),
                    &cancellation,
                )
                .await,
            Err(AgentFailure::CapabilityUnavailable)
        );
    }
}

/// The durable pairing Operation this device is driving.
///
/// Connections owns when the Operation continues, settles or expires, and which
/// observed identity may be accepted. The client only displays the code and
/// relays what the producer reported.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PairingOperation {
    pub pairing_id: String,
    pub challenge_id: String,
    pub polling_proof: String,
    /// Advances on restart or cancel; an observation for an earlier generation
    /// can no longer settle the Operation.
    pub generation: u64,
    pub expires_at_unix_ms: i64,
    pub state: PairingOperationState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PairingOperationState {
    AwaitingApproval,
    Approved { client_id: String, token: String },
    Rejected,
    Expired,
    RepairRequired,
}

impl PairingOperationState {
    pub fn is_terminal(&self) -> bool {
        !matches!(self, Self::AwaitingApproval)
    }
}

/// What the client should do next for a pairing Operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PairingDirective {
    ObserveAgain { poll_after_ms: i64 },
    Settled { state: PairingOperationState },
}

/// The interval the client is told to wait between observations.
pub const PAIRING_POLL_INTERVAL_MS: i64 = 2_000;

/// Apply one observed pairing status to the Operation.
///
/// Expiry is enforced here rather than by the client's own clock loop, and an
/// approved status is accepted only when the producer's identity matches the
/// pairing it belongs to.
pub fn observe_pairing(
    operation: &PairingOperation,
    observed: &PairingStatus,
    generation: u64,
    now_unix_ms: i64,
) -> Result<(PairingOperation, PairingDirective), AgentFailure> {
    if operation.generation != generation || operation.pairing_id != observed.pairing_id {
        return Err(AgentFailure::Conflict);
    }
    if operation.state.is_terminal() {
        return Err(AgentFailure::Conflict);
    }
    if now_unix_ms >= operation.expires_at_unix_ms {
        return Ok(settle_pairing(operation, PairingOperationState::Expired));
    }
    let state = match observed.status.as_str() {
        "pending" | "local_confirmed" => {
            return Ok((
                operation.clone(),
                PairingDirective::ObserveAgain {
                    poll_after_ms: PAIRING_POLL_INTERVAL_MS,
                },
            ));
        }
        "rejected" => PairingOperationState::Rejected,
        "expired" => PairingOperationState::Expired,
        "repair_required" => PairingOperationState::RepairRequired,
        "approved" => {
            let client_id = observed
                .client_id
                .as_deref()
                .ok_or(AgentFailure::CapabilityUnavailable)?;
            let token = observed
                .token
                .as_deref()
                .ok_or(AgentFailure::CapabilityUnavailable)?;
            if client_id != observed.pairing_id {
                return Err(AgentFailure::CapabilityUnavailable);
            }
            validate_token_text(token)?;
            PairingOperationState::Approved {
                client_id: client_id.to_owned(),
                token: token.to_owned(),
            }
        }
        _ => return Err(AgentFailure::CapabilityUnavailable),
    };
    Ok(settle_pairing(operation, state))
}

/// Cancel a pending pairing Operation and advance its generation.
pub fn cancel_pairing(operation: &PairingOperation) -> Result<PairingOperation, AgentFailure> {
    if operation.state.is_terminal() {
        return Err(AgentFailure::Conflict);
    }
    Ok(PairingOperation {
        generation: operation.generation.saturating_add(1),
        state: PairingOperationState::Expired,
        ..operation.clone()
    })
}

fn settle_pairing(
    operation: &PairingOperation,
    state: PairingOperationState,
) -> (PairingOperation, PairingDirective) {
    (
        PairingOperation {
            state: state.clone(),
            ..operation.clone()
        },
        PairingDirective::Settled { state },
    )
}
