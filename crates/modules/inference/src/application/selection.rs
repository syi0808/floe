//! One failure-closed selection rule, independent of generation/live transport.
use crate::{LocalObservation, ModelObservationError, PrimaryObservation};
use floe_agent_contract::{AgentFailure, ProcessingBoundary};
use std::future::Future;

pub(super) async fn select_primary_or_local<T, P, L>(
    primary: P,
    local: impl FnOnce() -> L,
) -> Result<(T, ProcessingBoundary), AgentFailure>
where
    P: Future<Output = Result<PrimaryObservation<T>, ModelObservationError>>,
    L: Future<Output = Result<LocalObservation<T>, ModelObservationError>>,
{
    match primary.await.map_err(AgentFailure::from)? {
        PrimaryObservation::Available(profile) => Ok((profile, ProcessingBoundary::Gateway)),
        PrimaryObservation::Absent(_) => match local().await.map_err(|error| match error {
            ModelObservationError::InvalidInventory => AgentFailure::LocalModelInvalidOutput,
            ModelObservationError::TransportUnavailable => AgentFailure::LocalModelUnavailable,
            error => AgentFailure::from(error),
        })? {
            LocalObservation::Available(profile) => Ok((profile, ProcessingBoundary::Device)),
            LocalObservation::Unavailable(_) => Err(AgentFailure::ModelUnavailable),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LocalAvailabilityReason, PrimaryAbsence};
    use std::sync::atomic::{AtomicUsize, Ordering};

    // A non-clone, non-generation handle proves selection neither synthesizes
    // a generation request nor duplicates a prepared transport capability.
    struct LiveFixtureHandle(u8);

    #[tokio::test]
    async fn available_primary_never_observes_local_and_keeps_owned_handle() {
        let (handle, boundary) = select_primary_or_local(
            async { Ok(PrimaryObservation::Available(LiveFixtureHandle(7))) },
            || async { panic!("local observation after available Primary") },
        )
        .await
        .unwrap();
        assert_eq!(handle.0, 7);
        assert_eq!(boundary, ProcessingBoundary::Gateway);
    }

    #[tokio::test]
    async fn only_valid_absence_observes_local_exactly_once() {
        for absence in [
            PrimaryAbsence::NoGatewayConfigured,
            PrimaryAbsence::PurposeNotConfigured,
            PrimaryAbsence::PurposeDisabled,
        ] {
            let calls = AtomicUsize::new(0);
            let (handle, boundary) =
                select_primary_or_local(async { Ok(PrimaryObservation::Absent(absence)) }, || {
                    calls.fetch_add(1, Ordering::SeqCst);
                    async { Ok(LocalObservation::Available(LiveFixtureHandle(9))) }
                })
                .await
                .unwrap();
            assert_eq!(calls.load(Ordering::SeqCst), 1);
            assert_eq!(handle.0, 9);
            assert_eq!(boundary, ProcessingBoundary::Device);
        }
    }

    #[tokio::test]
    async fn every_primary_failure_prevents_local_observation() {
        for error in [
            ModelObservationError::InvalidIdentity,
            ModelObservationError::InvalidInventory,
            ModelObservationError::CapabilityUnavailable,
            ModelObservationError::CredentialRejected,
            ModelObservationError::PermissionDenied,
            ModelObservationError::Timeout,
            ModelObservationError::TransportUnavailable,
            ModelObservationError::Cancelled,
            ModelObservationError::StorageUnavailable,
            ModelObservationError::QuotaExceeded,
        ] {
            let result = select_primary_or_local::<LiveFixtureHandle, _, _>(
                async { Err(error) },
                || async { panic!("fallback must not bypass {error:?}") },
            )
            .await;
            assert!(matches!(result, Err(actual) if actual == AgentFailure::from(error)));
        }
    }

    #[tokio::test]
    async fn local_failure_and_unavailability_are_not_another_route() {
        for reason in [
            LocalAvailabilityReason::Unsupported,
            LocalAvailabilityReason::Disabled,
            LocalAvailabilityReason::NotReady,
        ] {
            let result = select_primary_or_local::<LiveFixtureHandle, _, _>(
                async {
                    Ok(PrimaryObservation::Absent(
                        PrimaryAbsence::NoGatewayConfigured,
                    ))
                },
                || async { Ok(LocalObservation::Unavailable(reason)) },
            )
            .await;
            assert!(matches!(result, Err(AgentFailure::ModelUnavailable)));
        }
        for (error, expected) in [
            (
                ModelObservationError::InvalidInventory,
                AgentFailure::LocalModelInvalidOutput,
            ),
            (
                ModelObservationError::TransportUnavailable,
                AgentFailure::LocalModelUnavailable,
            ),
            (
                ModelObservationError::PermissionDenied,
                AgentFailure::PolicyDenied,
            ),
        ] {
            let result = select_primary_or_local::<LiveFixtureHandle, _, _>(
                async { Ok(PrimaryObservation::Absent(PrimaryAbsence::PurposeDisabled)) },
                || async { Err(error) },
            )
            .await;
            assert!(matches!(result, Err(actual) if actual == expected));
        }
    }
}
