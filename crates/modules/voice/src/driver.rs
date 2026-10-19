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
const MAX_TRACKED_LIVE_DELEGATIONS: usize = 256;
const LIVE_RESULT_POLL_INTERVAL: Duration = Duration::from_millis(200);
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
        projector: &floe_context::LiveContextProjector,
    ) -> Result<VoiceLiveCompletion, AgentFailure> {
        if call.plan().actor != self.binding.actor
            || call.plan().operation_id != self.binding.plan_id
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let mut pending_ack = None;
        let mut delegations = std::collections::VecDeque::<String>::new();
        let mut result_tick = tokio::time::interval(LIVE_RESULT_POLL_INTERVAL);
        result_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        enum Wake {
            Event(Result<LiveDelivery, AgentFailure>),
            Result,
            Shutdown,
        }
        let (evidence, mut failure) = loop {
            if self.shutdown.is_cancelled() {
                break (
                    LiveEndEvidence::Disconnected,
                    Some(AgentFailure::Interrupted),
                );
            }
            let wake = tokio::select! {
                biased;
                _=self.shutdown.cancelled()=>Wake::Shutdown,
                _=result_tick.tick(),if !delegations.is_empty()=>Wake::Result,
                result=call.receive()=>Wake::Event(result),
            };
            let delivery = match wake {
                Wake::Shutdown => {
                    break (
                        LiveEndEvidence::Disconnected,
                        Some(AgentFailure::Interrupted),
                    );
                }
                Wake::Result => {
                    let delegation = delegations.pop_front().expect("nonempty result watcher");
                    delegations.push_back(delegation.clone());
                    match self
                        .deliver_pending(&mut call, projector, &delegation)
                        .await
                    {
                        Ok(())
                        | Err(AgentFailure::StorageBusy | AgentFailure::DeadlineExceeded) => {
                            continue;
                        }
                        Err(failure) => break (LiveEndEvidence::Disconnected, Some(failure)),
                    }
                }
                Wake::Event(Ok(delivery)) => delivery,
                Wake::Event(Err(failure)) => break (LiveEndEvidence::Disconnected, Some(failure)),
            };
            if let LiveProviderEvent::Ended { evidence, .. } = delivery.event {
                pending_ack = Some(delivery.sequence);
                break (evidence, None);
            }
            match self.retain_delivery(&delivery).await {
                Ok(Some(delegation)) if !delegations.contains(&delegation) => {
                    // Correlation only; admitted Work remains canonical even if
                    // this bounded live presentation owner must stop.
                    if delegations.len() >= MAX_TRACKED_LIVE_DELEGATIONS {
                        break (
                            LiveEndEvidence::Disconnected,
                            Some(AgentFailure::BudgetExceeded),
                        );
                    }
                    delegations.push_back(delegation);
                }
                Ok(_) => {}
                Err(failure) => break (LiveEndEvidence::Disconnected, Some(failure)),
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

    async fn deliver_pending(
        &self,
        call: &mut AuthorizedLiveSession,
        projector: &floe_context::LiveContextProjector,
        delegation: &str,
    ) -> Result<(), AgentFailure> {
        call.revalidate().await?;
        let scope = call.result_read_scope();
        let Some(candidate) = scope
            .run(self.journal.pending_result(
                &self.binding.actor,
                self.binding.voice_session_id,
                delegation,
            ))
            .await?
        else {
            return Ok(());
        };
        if candidate.delegation_id != delegation
            || candidate.source.through().session_id != self.binding.surface_session_id
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let operation = crate::append_operation_id(
            self.binding.voice_session_id,
            &candidate.source,
            floe_model_contract::LiveAppendKind::PresentableResult,
            delegation,
        )?;
        let projection = projector
            .project_result(
                floe_context::LiveResultProjectionRequest {
                    plan: call.plan(),
                    voice_session_id: self.binding.voice_session_id,
                    surface_session_id: self.binding.surface_session_id,
                    projection_id: operation,
                    source: &candidate.source,
                },
                &scope,
            )
            .await?;
        let projection = match projection {
            floe_context::LiveProjectionOutcome::Ready(projection) => projection,
            floe_context::LiveProjectionOutcome::NeedsSourceReview(_) => {
                return Err(AgentFailure::PolicyDenied);
            }
            floe_context::LiveProjectionOutcome::NeedsCompaction { .. } => {
                return Err(AgentFailure::ModelInputCapacityExceeded);
            }
        };
        let receipt = self
            .deliver_result(
                call,
                floe_inference::LiveAppendRequest {
                    operation_id: operation,
                    kind: floe_model_contract::LiveAppendKind::PresentableResult,
                    delegation_id: Some(delegation.into()),
                    projection,
                },
            )
            .await?;
        match receipt.outcome {
            crate::VoiceAppendOutcome::Written => Ok(()),
            crate::VoiceAppendOutcome::NotSent { failure }
            | crate::VoiceAppendOutcome::Uncertain { failure } => Err(failure),
        }
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

    async fn retain_delivery(
        &self,
        delivery: &LiveDelivery,
    ) -> Result<Option<String>, AgentFailure> {
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
                .map(|_| None)
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
                        .map(|result| {
                            if matches!(
                                result.admission.record.outcome,
                                crate::VoiceEventOutcome::Work { .. }
                            ) {
                                if let LiveProviderEvent::DelegationRequested {
                                    delegation_id,
                                    ..
                                } = &delivery.event
                                {
                                    return Some(delegation_id.clone());
                                }
                            }
                            None
                        })
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
            | LiveProviderEvent::OutputActivity { .. } => Ok(None),
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
