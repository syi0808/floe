//! The bundled device driver, as Context's personal source port.
//!
//! Context states what a personal read is and what it must prove. This adapter
//! carries that to the host queue in `floe-native` and hands the result to the
//! Context observation registry. It keeps no state of its own: the queue belongs
//! to the platform, the trusted observation belongs to Context.

use floe_agent_contract::{AgentFailure, BoxFuture, PersonId};
use floe_context::{
    AcquiredSource, AttentionAcquisition, AttentionAcquisitionMode, AttentionView,
    ObservationRegistry, PersonalAcquisition, PersonalDomain, PersonalSourceDriver,
    TrustedObservation,
};
use floe_execution::Cancellation;
use floe_native::{
    AttentionAcquisitionMode as NativeAttentionMode, AttentionAcquisitionRequest, AttentionBroker,
    PersonalAcquisitionRequest, PersonalBroker, PersonalDomain as NativePersonalDomain,
};
use uuid::Uuid;

pub struct NativePersonalDriver<'a> {
    pub attention: &'a AttentionBroker,
    pub personal: &'a PersonalBroker,
    pub observations: &'a ObservationRegistry,
}

/// A deadline as the acquisition queue states it.
fn deadline_unix_ms(deadline: tokio::time::Instant) -> Result<i64, AgentFailure> {
    chrono::Utc::now()
        .timestamp_millis()
        .checked_add(
            i64::try_from(
                deadline
                    .saturating_duration_since(tokio::time::Instant::now())
                    .as_millis(),
            )
            .map_err(|_| AgentFailure::DeadlineExceeded)?,
        )
        .ok_or(AgentFailure::DeadlineExceeded)
}

fn now_unix_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

impl PersonalSourceDriver for NativePersonalDriver<'_> {
    fn personal_host_epoch(&self, person_id: PersonId) -> Result<String, AgentFailure> {
        self.personal.host_epoch(person_id)
    }

    fn attention_host_epoch(&self, person_id: PersonId) -> Result<String, AgentFailure> {
        self.attention.host_epoch(person_id)
    }

    fn process_incarnation(&self) -> Uuid {
        self.observations.process_incarnation()
    }

    fn acquire<'a>(
        &'a self,
        request: PersonalAcquisition<'a>,
        cancellation: Cancellation,
    ) -> BoxFuture<'a, Result<AcquiredSource, AgentFailure>> {
        Box::pin(async move {
            let command = PersonalAcquisitionRequest {
                request_id: Uuid::new_v4(),
                host_epoch: request.host_epoch,
                person_id: request.person_id,
                device_id: request.device_id.to_owned(),
                domain: match request.domain {
                    PersonalDomain::People => NativePersonalDomain::People,
                    PersonalDomain::Feasibility => NativePersonalDomain::Feasibility,
                    PersonalDomain::Wellbeing => NativePersonalDomain::Wellbeing,
                },
                selected_handles: request.selected_handles,
                event_handle: request.feasibility.map(|query| query.event_handle.clone()),
                evidence_handles: request
                    .feasibility
                    .map(|query| query.evidence_handles.clone())
                    .unwrap_or_default(),
                destination_latitude: request.feasibility.map(|query| query.destination_latitude),
                destination_longitude: request.feasibility.map(|query| query.destination_longitude),
                event_start_unix_ms: request.feasibility.map(|query| query.event_start_unix_ms),
                event_end_unix_ms: request.feasibility.map(|query| query.event_end_unix_ms),
                travel_mode: request.feasibility.map(|query| query.travel_mode.clone()),
                deadline_unix_ms: deadline_unix_ms(request.deadline)?,
                expected_native_subject_fingerprint: Some(request.expected_subject),
            };
            let result = self
                .personal
                .submit(command, now_unix_ms(), cancellation)
                .await?;
            Ok(AcquiredSource {
                view: result.view,
                subject_before: result.native_subject_fingerprint_before,
                subject_after: result.native_subject_fingerprint_after,
            })
        })
    }

    fn acquire_attention<'a>(
        &'a self,
        request: AttentionAcquisition<'a>,
        cancellation: Cancellation,
    ) -> BoxFuture<'a, Result<AcquiredSource, AgentFailure>> {
        Box::pin(async move {
            let command = AttentionAcquisitionRequest {
                request_id: Uuid::new_v4(),
                host_epoch: request.host_epoch,
                person_id: request.person_id,
                device_id: request.device_id.to_owned(),
                mode: match request.mode {
                    AttentionAcquisitionMode::ReadProjection => NativeAttentionMode::ReadProjection,
                    AttentionAcquisitionMode::InspectSubject => NativeAttentionMode::InspectSubject,
                },
                // An attention read never waits longer than half a minute.
                deadline_unix_ms: chrono::Utc::now()
                    .timestamp_millis()
                    .checked_add(
                        i64::try_from(
                            request
                                .deadline
                                .saturating_duration_since(tokio::time::Instant::now())
                                .min(std::time::Duration::from_secs(30))
                                .as_millis(),
                        )
                        .map_err(|_| AgentFailure::DeadlineExceeded)?,
                    )
                    .ok_or(AgentFailure::DeadlineExceeded)?,
                expected_native_subject_fingerprint: request.expected_subject,
            };
            let result = self
                .attention
                .submit(command, now_unix_ms(), cancellation)
                .await?;
            Ok(AcquiredSource {
                view: result.view,
                subject_before: result.native_subject_fingerprint_before,
                subject_after: result.native_subject_fingerprint_after,
            })
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn commit_personal_observation(
        &self,
        person_id: PersonId,
        device_id: &str,
        observation_id: Uuid,
        process_incarnation_id: Uuid,
        native_subject_fingerprint: &str,
        observed_at_unix_ms: i64,
        expires_at_unix_ms: i64,
        query_fingerprint: Vec<u8>,
    ) -> Result<(), AgentFailure> {
        // The observation only counts under the host epoch that produced it.
        let host_epoch = self.personal.host_epoch(person_id)?;
        self.observations.commit_trusted_personal_observation(
            person_id,
            device_id,
            host_epoch,
            observation_id,
            process_incarnation_id,
            native_subject_fingerprint,
            observed_at_unix_ms,
            expires_at_unix_ms,
            query_fingerprint,
        )
    }

    fn trusted_personal_observation(
        &self,
        person_id: PersonId,
        device_id: &str,
        observation_id: Uuid,
        process_incarnation_id: Uuid,
    ) -> Result<TrustedObservation, AgentFailure> {
        let host_epoch = self.personal.host_epoch(person_id)?;
        let observation = self.observations.trusted_personal_observation(
            person_id,
            device_id,
            &host_epoch,
            observation_id,
            process_incarnation_id,
            now_unix_ms(),
        )?;
        Ok(TrustedObservation {
            native_subject_fingerprint: observation.native_subject_fingerprint,
            observed_at_unix_ms: observation.observed_at_unix_ms,
            expires_at_unix_ms: observation.expires_at_unix_ms,
            query_fingerprint: observation.query_fingerprint,
        })
    }

    fn trusted_attention_observation(
        &self,
        person_id: PersonId,
        device_id: &str,
        observation_id: Uuid,
        process_incarnation_id: Uuid,
    ) -> Result<(AttentionView, String), AgentFailure> {
        // Without a live attention host there is nothing to vouch for the
        // projection, whatever is still cached.
        let host_epoch = self
            .attention
            .host_epoch(person_id)
            .map_err(|_| AgentFailure::PolicyDenied)?;
        self.observations.trusted_attention_observation(
            person_id,
            device_id,
            &host_epoch,
            observation_id,
            process_incarnation_id,
            now_unix_ms(),
        )
    }

    fn commit_attention_projection(
        &self,
        person_id: PersonId,
        host_epoch: &str,
        device_id: &str,
        view: &AttentionView,
        native_subject_fingerprint: &str,
    ) -> Result<(Uuid, Uuid), AgentFailure> {
        if !host_epoch.is_empty() {
            // A projection is only trusted under the host epoch that is live now.
            let live = self.attention.host_epoch(person_id)?;
            if live != host_epoch {
                return Err(AgentFailure::CapabilityUnavailable);
            }
        }
        self.observations.commit_trusted_attention_projection(
            person_id,
            host_epoch,
            device_id,
            view,
            native_subject_fingerprint,
            now_unix_ms(),
        )
    }
}

/// The device, as Access's personal subject inspector.
///
/// An inspection never reads the source: it only asks which subject the device
/// would answer for, so the Person can be shown what they are about to grant.
impl floe_access::PersonalSubjectInspector for NativePersonalDriver<'_> {
    fn inspect<'a>(
        &'a self,
        person_id: PersonId,
        device_id: &'a str,
        probe: floe_access::PersonalSubjectProbe<'a>,
        expected_native_subject_fingerprint: Option<String>,
        deadline: Option<tokio::time::Instant>,
        cancellation: Cancellation,
    ) -> BoxFuture<'a, Result<floe_access::PersonalSubjectEvidence, AgentFailure>> {
        Box::pin(async move {
            let evidence = match probe {
                floe_access::PersonalSubjectProbe::Attention => {
                    // Attention is inspected through the attention host, which
                    // keeps its own epoch, and never waits past half a minute.
                    let remaining = deadline
                        .map(|deadline| {
                            deadline.saturating_duration_since(tokio::time::Instant::now())
                        })
                        .unwrap_or_else(|| std::time::Duration::from_secs(30))
                        .min(std::time::Duration::from_secs(30));
                    let request = AttentionAcquisitionRequest {
                        request_id: Uuid::new_v4(),
                        host_epoch: self.attention.host_epoch(person_id)?,
                        person_id,
                        device_id: device_id.to_owned(),
                        mode: NativeAttentionMode::InspectSubject,
                        deadline_unix_ms: now_unix_ms()
                            .checked_add(
                                i64::try_from(remaining.as_millis())
                                    .map_err(|_| AgentFailure::DeadlineExceeded)?,
                            )
                            .ok_or(AgentFailure::DeadlineExceeded)?,
                        expected_native_subject_fingerprint,
                    };
                    let result = self
                        .attention
                        .submit(request, now_unix_ms(), cancellation)
                        .await?;
                    (
                        result.native_subject_fingerprint_before,
                        result.native_subject_fingerprint_after,
                    )
                }
                probe => {
                    let (domain, selected_handles, query) = match probe {
                        floe_access::PersonalSubjectProbe::People { selected_handles } => {
                            (NativePersonalDomain::People, selected_handles, None)
                        }
                        floe_access::PersonalSubjectProbe::Feasibility { query } => {
                            (NativePersonalDomain::Feasibility, Vec::new(), Some(query))
                        }
                        floe_access::PersonalSubjectProbe::Wellbeing => {
                            (NativePersonalDomain::Wellbeing, Vec::new(), None)
                        }
                        floe_access::PersonalSubjectProbe::Attention => unreachable!(),
                    };
                    let request = PersonalAcquisitionRequest {
                        request_id: Uuid::new_v4(),
                        host_epoch: self.personal.host_epoch(person_id)?,
                        person_id,
                        device_id: device_id.to_owned(),
                        domain,
                        selected_handles,
                        event_handle: query.map(|query| query.event_handle.clone()),
                        evidence_handles: query
                            .map(|query| query.evidence_handles.clone())
                            .unwrap_or_default(),
                        destination_latitude: query.map(|query| query.destination_latitude),
                        destination_longitude: query.map(|query| query.destination_longitude),
                        event_start_unix_ms: query.map(|query| query.event_start_unix_ms),
                        event_end_unix_ms: query.map(|query| query.event_end_unix_ms),
                        travel_mode: query.map(|query| query.travel_mode.clone()),
                        deadline_unix_ms: now_unix_ms().saturating_add(30_000),
                        expected_native_subject_fingerprint,
                    };
                    let result = self
                        .personal
                        .submit(request, now_unix_ms(), cancellation)
                        .await?;
                    (
                        result.native_subject_fingerprint_before,
                        result.native_subject_fingerprint_after,
                    )
                }
            };
            Ok(floe_access::PersonalSubjectEvidence {
                before: evidence.0,
                after: evidence.1,
            })
        })
    }

    fn attention_presence(&self, person_id: PersonId, device_id: &str) -> Option<Uuid> {
        self.observations
            .attention_observation(person_id, device_id, now_unix_ms())
            .ok()
            .map(|(_, _, process_incarnation)| process_incarnation)
    }
}
