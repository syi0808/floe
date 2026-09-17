//! The records and the device one personal read runs against.
//!
//! Context owns what the read is; this is what the host hands it — the Person's
//! grants as the vault holds them, and the native driver as the local context
//! bridge speaks to it. The acquisition wire shape stays here, because it is the
//! driver's, not the read's.

use floe_agent_contract::{AgentFailure, BoxFuture, PersonId};
use floe_context::{
    AcquiredSource, AttentionAcquisition, AttentionView, PersonalAcquisition, PersonalDomain,
    PersonalGrantRecords, PersonalSourceDriver,
};
use floe_execution::Cancellation;
use floe_protocol::{
    LocalContextAttentionAcquisitionModeDto, LocalContextAttentionAcquisitionRequestDto,
    LocalContextPersonalAcquisitionRequestDto, LocalContextPersonalDomainDto,
};
use floe_vault::{EncryptedAgentVault, VaultKeyProvider};
use uuid::Uuid;

use crate::local_context::LocalContextStore;

/// The Person's grants, as their vault holds them.
pub(crate) struct VaultGrantRecords<'a, Keys: VaultKeyProvider> {
    pub(crate) vault: &'a EncryptedAgentVault<Keys>,
}

impl<Keys: VaultKeyProvider> PersonalGrantRecords for VaultGrantRecords<'_, Keys> {
    fn grants<'a>(
        &'a self,
    ) -> BoxFuture<'a, Result<Vec<floe_access::DataAccessGrant>, AgentFailure>> {
        Box::pin(async move { self.vault.list_data_access_grants(128).await })
    }

    fn reviewed_subject<'a>(
        &'a self,
        grant: floe_access::GrantId,
    ) -> BoxFuture<'a, Result<String, AgentFailure>> {
        Box::pin(async move { self.vault.personal_grant_subject_fingerprint(grant).await })
    }

    fn consumer_policy<'a>(
        &'a self,
        grant: floe_access::GrantId,
    ) -> BoxFuture<'a, Result<floe_access::ConsumerPolicyAuthority, AgentFailure>> {
        Box::pin(async move { self.vault.personal_grant_consumer_policy(grant).await })
    }

    fn feasibility_query<'a>(
        &'a self,
        grant: floe_access::GrantId,
    ) -> BoxFuture<'a, Result<floe_access::FeasibilityGrantQuery, AgentFailure>> {
        Box::pin(async move { self.vault.personal_feasibility_query(grant).await })
    }
}

/// The native driver, as this host's local context bridge reaches it.
pub(crate) struct NativePersonalDriver<'a> {
    pub(crate) local_context: &'a LocalContextStore,
}

/// A deadline as the acquisition wire states it.
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

impl PersonalSourceDriver for NativePersonalDriver<'_> {
    fn personal_host_epoch(&self, person_id: PersonId) -> Result<String, AgentFailure> {
        self.local_context
            .personal_acquisition_host_epoch(person_id)
    }

    fn attention_host_epoch(&self, person_id: PersonId) -> Result<String, AgentFailure> {
        self.local_context
            .attention_acquisition_host_epoch(person_id)
    }

    fn process_incarnation(&self) -> Uuid {
        self.local_context.process_incarnation()
    }

    fn acquire<'a>(
        &'a self,
        request: PersonalAcquisition<'a>,
        cancellation: Cancellation,
    ) -> BoxFuture<'a, Result<AcquiredSource, AgentFailure>> {
        Box::pin(async move {
            let wire = LocalContextPersonalAcquisitionRequestDto {
                request_id: Uuid::new_v4().to_string(),
                host_epoch: request.host_epoch,
                person_id: request.person_id.to_string(),
                device_id: request.device_id.to_owned(),
                domain: match request.domain {
                    PersonalDomain::People => LocalContextPersonalDomainDto::People,
                    PersonalDomain::Feasibility => LocalContextPersonalDomainDto::Feasibility,
                    PersonalDomain::Wellbeing => LocalContextPersonalDomainDto::Wellbeing,
                },
                selected_handles: request.selected_handles,
                event_handle: request.feasibility.map(|query| query.event_handle.clone()),
                evidence_handles: request
                    .feasibility
                    .map(|query| query.evidence_handles.clone())
                    .unwrap_or_default(),
                destination_latitude: request
                    .feasibility
                    .map(|query| query.destination_latitude),
                destination_longitude: request
                    .feasibility
                    .map(|query| query.destination_longitude),
                event_start_unix_ms: request.feasibility.map(|query| query.event_start_unix_ms),
                event_end_unix_ms: request.feasibility.map(|query| query.event_end_unix_ms),
                travel_mode: request.feasibility.map(|query| query.travel_mode.clone()),
                deadline_unix_ms: deadline_unix_ms(request.deadline)?,
                expected_native_subject_fingerprint: Some(request.expected_subject),
            };
            let result = self.local_context.acquire_personal(wire, cancellation).await?;
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
            let wire = LocalContextAttentionAcquisitionRequestDto {
                request_id: Uuid::new_v4().to_string(),
                host_epoch: request.host_epoch,
                person_id: request.person_id.to_string(),
                device_id: request.device_id.to_owned(),
                mode: LocalContextAttentionAcquisitionModeDto::ReadProjection,
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
                expected_native_subject_fingerprint: Some(request.expected_subject),
            };
            let result = self
                .local_context
                .acquire_attention(wire, cancellation)
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
        self.local_context.commit_trusted_personal_observation(
            person_id,
            device_id,
            observation_id,
            process_incarnation_id,
            native_subject_fingerprint,
            observed_at_unix_ms,
            expires_at_unix_ms,
            query_fingerprint,
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
        self.local_context
            .commit_trusted_attention_projection_for_host(
                person_id,
                host_epoch,
                device_id,
                view,
                native_subject_fingerprint,
            )
    }
}
