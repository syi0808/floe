//! Canonical retained model-event -> Voice/Conversation custody loop. Native
//! media effects and UI observation remain separate from this owned driver.
use crate::custody::{busy, retry_local};
use crate::{VoiceController, VoiceMutation, VoiceMutationKind};
use floe_execution::budget::ModelAttemptReceipt;
use floe_inference::{AuthorizedLiveSession, LiveDelivery};
use floe_kernel::{AgentFailure, CommandFailure};
use floe_model_contract::{LiveEndEvidence, LiveProviderEvent, LiveUsageObservation};
use std::time::Duration;
use tokio::time::Instant;
use uuid::Uuid;
const DELIVERY_ACK_RETRY_WINDOW: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug)]
pub struct VoiceLiveCompletion {
    pub usage: ModelAttemptReceipt,
    pub failure: Option<AgentFailure>,
}
impl VoiceController {
    pub async fn consume_live(
        &self,
        mut call: AuthorizedLiveSession,
    ) -> Result<VoiceLiveCompletion, AgentFailure> {
        if call.plan().actor != self.binding.actor
            || call.plan().operation_id != self.binding.plan_id
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let mut pending_ack = None;
        let (evidence, mut failure) = loop {
            if self.shutdown.is_cancelled() {
                break (
                    LiveEndEvidence::Disconnected,
                    Some(AgentFailure::Interrupted),
                );
            }
            let delivery = match call.receive().await {
                Ok(delivery) => delivery,
                Err(failure) => break (LiveEndEvidence::Disconnected, Some(failure)),
            };
            if let LiveProviderEvent::Ended { evidence, .. } = delivery.event {
                pending_ack = Some(delivery.sequence);
                break (evidence, None);
            }
            if let Err(failure) = self.retain_delivery(&delivery).await {
                break (LiveEndEvidence::Disconnected, Some(failure));
            }
            // The same exact local event/command is never reprocessed solely
            // because the transport ACK was lost. No later event is read yet.
            if let Err(failure) = self.ack_delivery(&mut call, delivery.sequence).await {
                pending_ack = Some(delivery.sequence);
                break (LiveEndEvidence::Disconnected, Some(failure));
            }
        };
        // Fence native/transport immediately, before any potentially slow DB.
        let settled = call.settle_usage(LiveUsageObservation::default())?;
        failure = failure.or(settled.failure);
        // Try both known facts even if one write fails. A receipt is never
        // replaced by zero/unknown just because another cleanup phase failed.
        let usage = self
            .settle_usage(settled.receipt)
            .await
            .map_err(CommandFailure::into_failure);
        let remote = self
            .record_control(
                Uuid::new_v5(&self.binding.voice_session_id, b"floe.voice.controller-end"),
                VoiceMutationKind::RemoteEnd {
                    binding: self.binding.clone(),
                    evidence,
                },
            )
            .await;
        let usage = usage?;
        remote?;
        if let Some(sequence) = pending_ack {
            if let Err(error) = call.acknowledge(sequence).await {
                failure = failure.or(Some(error));
            }
        }
        Ok(VoiceLiveCompletion { usage, failure })
    }

    async fn ack_delivery(
        &self,
        call: &mut AuthorizedLiveSession,
        sequence: u64,
    ) -> Result<(), AgentFailure> {
        let deadline = Instant::now() + DELIVERY_ACK_RETRY_WINDOW;
        let mut delay = Duration::from_millis(20);
        loop {
            let result = tokio::select! {
                biased;
                _ = self.shutdown.cancelled() => return Err(AgentFailure::Interrupted),
                result = tokio::time::timeout_at(deadline, call.acknowledge(sequence)) => {
                    result.map_err(|_| AgentFailure::DeadlineExceeded)?
                }
            };
            match result {
                Err(error @ (AgentFailure::ServerModelUnavailable | AgentFailure::StorageBusy))
                    if Instant::now() < deadline =>
                {
                    tokio::select! {
                        biased;
                        _ = self.shutdown.cancelled() => return Err(AgentFailure::Interrupted),
                        _ = tokio::time::sleep_until((Instant::now() + delay).min(deadline)) => {},
                    }
                    if Instant::now() >= deadline {
                        return Err(error);
                    }
                    delay = (delay * 2).min(Duration::from_millis(100));
                }
                result => return result,
            }
        }
    }

    async fn retain_delivery(&self, delivery: &LiveDelivery) -> Result<(), AgentFailure> {
        match &delivery.event {
            LiveProviderEvent::Ready {} => {
                let mut identity = b"floe.voice.ready:".to_vec();
                identity.extend_from_slice(&delivery.sequence.to_be_bytes());
                self.record_control(
                    Uuid::new_v5(&self.binding.voice_session_id, &identity),
                    VoiceMutationKind::RemoteReady {
                        binding: self.binding.clone(),
                    },
                )
                .await
            }
            LiveProviderEvent::PersonUtteranceFinal { .. }
            | LiveProviderEvent::AssistantUtteranceFinal { .. }
            | LiveProviderEvent::DelegationRequested { .. } => {
                if self.shutdown.is_cancelled()
                    && matches!(
                        delivery.event,
                        LiveProviderEvent::DelegationRequested { .. }
                    )
                {
                    return Err(AgentFailure::Interrupted);
                }
                retry_local(
                    || async {
                        crate::service::ingest_owned(
                            self.journal.as_ref(),
                            self.conversation.as_ref(),
                            &self.binding.actor,
                            self.binding.voice_session_id,
                            delivery.event.clone(),
                        )
                        .await
                        .map(|_| ())
                        .map_err(CommandFailure::into_failure)
                    },
                    busy,
                )
                .await
            }
            // Ephemeral captions/activity never manufacture a canonical input
            // or claim UI/native presentation. Those subscribers are separate.
            LiveProviderEvent::TranscriptDelta { .. }
            | LiveProviderEvent::InputActivity { .. }
            | LiveProviderEvent::OutputActivity { .. } => Ok(()),
            LiveProviderEvent::Ended { .. } => Err(AgentFailure::InvalidInput),
        }
    }
    async fn record_control(
        &self,
        command_id: Uuid,
        kind: VoiceMutationKind,
    ) -> Result<(), AgentFailure> {
        let record = retry_local(
            || {
                self.journal
                    .read_call(&self.binding.actor, self.binding.voice_session_id)
            },
            busy,
        )
        .await?
        .ok_or(AgentFailure::NotFound)?;
        if record.intent.binding != self.binding {
            return Err(AgentFailure::PolicyDenied);
        }
        let request = VoiceMutation {
            voice_session_id: self.binding.voice_session_id,
            command_id,
            expected_revision: record.revision,
            kind,
        };
        retry_local(
            || async {
                self.journal
                    .mutate(&self.binding.actor, request.clone())
                    .await
                    .map(|_| ())
                    .map_err(CommandFailure::into_failure)
            },
            busy,
        )
        .await
    }
}
