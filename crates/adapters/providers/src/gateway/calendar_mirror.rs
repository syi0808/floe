//! Strict, proof-carrying private Gateway transport for product Calendar reads.
//! It returns only normalized Day batches and never creates an Access permit.
use std::collections::{BTreeMap, HashSet};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, NaiveDate, Utc};
use floe_access::{
    AuthorizationProofVerifier, AuthorizationSignature, AuthorizationSigningCommand,
    ProductCalendarChallenge, ProductCalendarClaims,
    ProductCalendarPageQuery, ProductCalendarPermission, ProductCalendarReadPermit,
    ProductCalendarResultKind, ProductCalendarSigningCommand, ProductCalendarSourceClaims,
    ProductCalendarSourcePreview, CalendarProductWirePurpose, ProductSourceObservation,
    RemoteProducerIdentity, SourceExpectation,
};
use floe_connections::SourceConnection;
use floe_context::{CalendarProductPage, CalendarProductPageOutcome};
use floe_context_contract::{CalendarProvider, ResourceHandle};
use floe_day::{
    CalendarExternalRevision, CalendarFailure, CalendarRecord, CalendarResourceOutcome,
    EventSchedule,
};
use floe_execution::ExecutionScope;
use floe_kernel::{AgentFailure, OwnerActor};
use reqwest::Method;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::value::RawValue;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::{
    http::GatewayHttpTransport, json::strict_json_bytes, proof, product_lease::ProductGatewayLease,
    views::{access_producer_identity, ProducerIdentityResponse},
};

const MAX_PREVIEW_BYTES: usize = 65_536;
const MAX_PAGE_BYTES: usize = 1 << 20;
const MAX_CURSOR_BYTES: usize = 4096;
const MAX_EXTERNAL_ID_BYTES: usize = 512;
const MAX_TITLE_BYTES: usize = 4096;
const MAX_PROVIDER_TEXT_BYTES: usize = 128;
const MAX_HTTP_CALL: std::time::Duration = std::time::Duration::from_secs(30);

/// Client retained for one complete source acquisition. The lease cannot be
/// silently replaced while a page stream is in flight.
pub struct GatewayCalendarMirrorClient {
    lease: ProductGatewayLease,
}

#[derive(Serialize)]
struct SourcePreviewRequest<'a> {
    schema_version: u32,
    connector_id: &'a str,
    connection_id: &'a str,
    local_revision: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourcePreviewResponse {
    schema_version: u32,
    descriptor_b64url: String,
    producer_signature: String,
    producer: ProducerIdentityResponse,
}

#[derive(Serialize)]
struct AdmissionRequest<'a> {
    schema_version: u32,
    claims: &'a ProductCalendarClaims,
    expires_at_unix_ms: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChallengeResponse {
    schema_version: u32,
    challenge_b64url: String,
    producer_signature: String,
    producer: ProducerIdentityResponse,
}

#[derive(Serialize)]
struct ProofRequest<'a> {
    schema_version: u32,
    proof: Proof<'a>,
}

#[derive(Serialize)]
struct Proof<'a> {
    challenge_id: &'a str,
    key_id: &'a str,
    signature: &'a str,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ErrorEnvelope {
    error: ErrorCode,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ErrorCode {
    code: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawReleaseResponse {
    schema_version: u32,
    page: Box<RawValue>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCalendarPage {
    schema_version: u32,
    result_kind: ProductCalendarResultKind,
    refresh_operation_id: Uuid,
    read_operation_id: Uuid,
    page_id: Uuid,
    person_id: String,
    device_id: String,
    source: ProductCalendarSourceClaims,
    calendar_id: String,
    range_start_unix_ms: i64,
    range_end_unix_ms: i64,
    observed_at_unix_ms: i64,
    expires_at_unix_ms: i64,
    outcome: RawCalendarPageOutcome,
}

#[derive(Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
enum RawCalendarPageOutcome {
    Complete { records: Vec<RawCalendarRecord> },
    More { records: Vec<RawCalendarRecord>, cursor: String },
    Failed { reason: CalendarFailure },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCalendarRecord {
    can_modify: bool,
    calendar_id: String,
    external_id: String,
    external_revision: CalendarExternalRevision,
    title: String,
    schedule: RawCalendarSchedule,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum RawCalendarSchedule {
    Timed {
        starts_at: String,
        ends_at: String,
        timezone: String,
    },
    AllDay {
        start_date: String,
        end_date_exclusive: String,
    },
}

struct PageResult {
    outcome: CalendarProductPageOutcome,
    observed_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    received_records: usize,
    received_bytes: usize,
}

impl GatewayCalendarMirrorClient {
    pub fn new(lease: ProductGatewayLease) -> Self {
        Self { lease }
    }

    /// Obtain a signed metadata-only source preview. The local revision is an
    /// echo in the signed descriptor; all remote revisions and resources are
    /// read from that descriptor.
    pub async fn observe_source(
        &self,
        actor: &OwnerActor,
        source: &SourceConnection,
        scope: &ExecutionScope,
    ) -> Result<ProductSourceObservation, AgentFailure> {
        actor.validate()?;
        source.validate().map_err(|_| AgentFailure::InvalidInput)?;
        if actor.person_id != source.person_id()
            || self.lease.actor() != actor
            || !source.is_serving()
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let provider = provider_for(source)?;
        let (handles, resource_ids) = configured_resources(source)?;
        let request = SourcePreviewRequest {
            schema_version: 1,
            connector_id: source.connector_id().as_str(),
            connection_id: source.connection_id().as_str(),
            local_revision: source.revision(),
        };
        let response: SourcePreviewResponse = self
            .post_json("/v1/calendar/mirror/source-preview", &request, scope, None)
            .await?;
        if response.schema_version != 1 {
            return Err(AgentFailure::CapabilityUnavailable);
        }
        let identity = pinned_producer(
            &response.producer,
            self.lease.credentials().binding(),
            source.execution_owner_id().as_str(),
        )?;
        let descriptor_bytes = proof::decode_canonical(&response.descriptor_b64url, MAX_PREVIEW_BYTES)?;
        strict_json_bytes(&descriptor_bytes, MAX_PREVIEW_BYTES)?;
        let descriptor: ProductCalendarSourcePreview =
            serde_json::from_slice(&descriptor_bytes).map_err(|_| AgentFailure::InvalidInput)?;
        let signature = proof::decode_exact(&response.producer_signature, 64)?;
        proof::verify_signature(&identity, &descriptor_bytes, &signature)?;
        validate_preview(
            &descriptor,
            actor,
            source,
            provider,
            &handles,
            &resource_ids,
            self.lease.credentials().binding(),
            &identity,
        )?;
        self.lease.ensure_current()?;
        self.lease.revalidate().await?;

        let expectation = SourceExpectation {
            source: floe_context_contract::GrantSourceBinding::try_new(
                actor.person_id,
                source.connection_id().clone(),
                source.connector_id().clone(),
                source.execution_owner_id().clone(),
            )
            .map_err(|_| AgentFailure::InvalidInput)?,
            revision: Some(descriptor.source.local_revision),
            provider_revision: Some(descriptor.source.provider_revision),
            authority: source.source_authority(),
            physical_resources: handles,
            subject_fingerprint: descriptor.source.provider_identity,
            gateway: Some(self.lease.credentials().binding().clone()),
        };
        expectation.validate()?;
        Ok(ProductSourceObservation {
            expectation,
            provider,
            permission: ProductCalendarPermission::ProviderRead {
                identity_generation: descriptor.source.identity_generation,
                gateway_runtime_generation: self.lease.generation(),
            },
            observed_at: Utc::now(),
        })
    }

    /// Read all exact configured physical calendars under the retained permit.
    /// Any incomplete remote resource becomes one Failed batch and discards
    /// that resource's staged records.
    pub async fn acquire(
        &self,
        permit: &ProductCalendarReadPermit,
        scope: &ExecutionScope,
    ) -> Result<floe_context::CalendarProductReadResult, AgentFailure> {
        self.lease.ensure_current()?;
        self.lease.revalidate().await?;
        permit.revalidate(scope).await?;
        let request = permit.request();
        let observation = permit.observation();
        let gateway = observation
            .expectation
            .gateway
            .as_ref()
            .ok_or(AgentFailure::PolicyDenied)?;
        let ProductCalendarPermission::ProviderRead {
            identity_generation,
            gateway_runtime_generation,
        } = observation.permission
        else {
            return Err(AgentFailure::PolicyDenied);
        };
        if gateway_runtime_generation != self.lease.generation()
            || gateway != self.lease.credentials().binding()
            || observation.provider != provider_for_connector(request.source.connector().as_str())?
        {
            return Err(AgentFailure::StaleContext);
        }
        let source_claims = source_claims(&observation.expectation, identity_generation)?;
        let resources = observation
            .expectation
            .physical_resources
            .iter()
            .map(|resource| resource.as_str().to_owned())
            .collect::<Vec<_>>();
        if resources.is_empty()
            || resources.len() > floe_context_contract::MAX_SOURCE_RESOURCES
            || resources.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(AgentFailure::StaleContext);
        }

        let mut batches = Vec::with_capacity(resources.len());
        let mut received_total = 0usize;
        let mut successful_total = 0usize;
        let mut received_total_bytes = 0usize;
        let mut expires_at = permit.expires_at();
        for calendar_id in &resources {
            self.lease.ensure_current()?;
            permit.revalidate(scope).await?;
            let result = self
                .fetch_resource(
                    permit,
                    scope,
                    &source_claims,
                    &resources,
                    calendar_id,
                    received_total,
                    received_total_bytes,
                )
                .await?;
            received_total_bytes = received_total_bytes.checked_add(result.received_bytes).filter(|bytes| *bytes <= request.limits.max_bytes as usize).ok_or(AgentFailure::BudgetExceeded)?;
            received_total = received_total
                .checked_add(result.received_records)
                .ok_or(AgentFailure::BudgetExceeded)?;
            if received_total > request.limits.max_records as usize {
                return Err(AgentFailure::BudgetExceeded);
            }
            expires_at = expires_at.min(result.expires_at);
            let outcome = match result.outcome {
                CalendarProductPageOutcome::Complete { records } => {
                    successful_total = successful_total
                        .checked_add(records.len())
                        .ok_or(AgentFailure::BudgetExceeded)?;
                    CalendarResourceOutcome::Complete {
                        calendar_id: calendar_id.clone(),
                        records,
                        observed_at: result.observed_at,
                    }
                }
                CalendarProductPageOutcome::Failed { reason } => CalendarResourceOutcome::Failed {
                    calendar_id: calendar_id.clone(),
                    reason,
                    observed_at: result.observed_at,
                },
                CalendarProductPageOutcome::More { .. } => {
                    return Err(AgentFailure::InvalidInput)
                }
            };
            batches.push(outcome);
        }

        let bytes = serde_json::to_vec(&batches).map_err(|_| AgentFailure::InvalidInput)?;
        let observed_at = Utc::now();
        if bytes.len() > request.limits.max_bytes as usize
            || received_total > request.limits.max_records as usize
            || observed_at < observation.observed_at
            || expires_at <= observed_at
            || expires_at > permit.expires_at()
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        self.lease.ensure_current()?;
        permit.revalidate(scope).await?;
        Ok(floe_context::CalendarProductReadResult {
            consumed_records: u32::try_from(received_total).map_err(|_| AgentFailure::BudgetExceeded)?,
            consumed_bytes: u32::try_from(received_total_bytes.max(bytes.len())).map_err(|_| AgentFailure::BudgetExceeded)?,
            batches,
            binding: floe_access::ProductCalendarResultBinding {
                read_operation_id: request.read_operation_id,
                source: observation.expectation.clone(),
                payload_digest: Sha256::digest(&bytes).into(),
                record_count: u32::try_from(successful_total).map_err(|_| AgentFailure::BudgetExceeded)?,
                byte_count: u32::try_from(bytes.len()).map_err(|_| AgentFailure::BudgetExceeded)?,
                observed_at,
                expires_at,
            },
        })
    }

    async fn fetch_resource(
        &self,
        permit: &ProductCalendarReadPermit,
        scope: &ExecutionScope,
        source: &ProductCalendarSourceClaims,
        resources: &[String],
        calendar_id: &str,
        total_before: usize,
        bytes_before: usize,
    ) -> Result<PageResult, AgentFailure> {
        let request = permit.request();
        let mut cursor = String::new();
        let mut seen_cursors = HashSet::new();
        seen_cursors.insert(cursor.clone());
        let mut records_by_id = BTreeMap::<String, CalendarRecord>::new();
        let mut latest_observed = permit.observation().observed_at;
        let mut earliest_expiry = permit.expires_at();
        let mut received_records = 0usize;
        let mut received_bytes = 0usize;

        loop {
            self.lease.ensure_current()?;
            permit.revalidate(scope).await?;
            let remaining_records = (request.limits.max_records as usize)
                .checked_sub(total_before.saturating_add(received_records))
                .ok_or(AgentFailure::BudgetExceeded)?;
            if remaining_records == 0 {
                return Ok(failed_page_result(
                    CalendarFailure::BudgetExceeded,
                    latest_observed,
                    earliest_expiry,
                    received_records,
                    received_bytes,
                ));
            }
            let limit = request
                .limits
                .max_page_records
                .min(u32::try_from(remaining_records).unwrap_or(u32::MAX));
            if limit == 0 {
                return Err(AgentFailure::BudgetExceeded);
            }
            let query = ProductCalendarPageQuery {
                calendar_id: calendar_id.to_owned(),
                range_start_unix_ms: request.range_start.timestamp_millis(),
                range_end_unix_ms: request.range_end.timestamp_millis(),
                cursor: cursor.clone(),
                limit,
            };
            let query_bytes = serde_json::to_vec(&query).map_err(|_| AgentFailure::InvalidInput)?;
            let claims = ProductCalendarClaims {
                person_id: request.actor.person_id.to_string(),
                client_id: self.lease.credentials().binding().client_id.clone(),
                device_id: request.actor.device_id.clone(),
                audience: self.lease.credentials().binding().producer_audience.clone(),
                producer_instance: self.lease.credentials().binding().producer_instance.clone(),
                producer_key_fingerprint: self.lease.credentials().binding().producer_key_fingerprint.clone(),
                enrollment_id: self.lease.credentials().binding().enrollment_id.clone(),
                credential_generation: self.lease.credentials().binding().credential_generation,
                purpose: CalendarProductWirePurpose::DayRefresh,
                result_kind: ProductCalendarResultKind::CalendarMirror,
                refresh_operation_id: request.refresh_operation_id,
                read_operation_id: request.read_operation_id,
                page_id: Uuid::new_v4(),
                source: source.clone(),
                resources: resources.to_vec(),
                query,
                query_sha256: hex_sha256(&query_bytes),
                limits: request.limits,
            };
            claims.validate_permit(permit)?;
            let admission = self
                .admit_and_sign(permit, scope, &claims)
                .await?;
            self.lease.ensure_current()?;
            permit.revalidate(scope).await?;
            let release = self
                .read_and_verify(permit, scope, &claims, &admission)
                .await?;
            self.lease.ensure_current()?;
            permit.revalidate(scope).await?;
            let signature = self
                .sign_challenge(permit, scope, &claims, release, ChallengeKind::Release {
                    admission_id: admission.challenge.challenge_id(),
                })
                .await?;
            self.lease.ensure_current()?;
            permit.revalidate(scope).await?;
            let proof = ProofRequest {
                schema_version: 1,
                proof: Proof {
                    challenge_id: &signature.challenge_id,
                    key_id: &signature.signature.key_id,
                    signature: &signature.signature.signature,
                },
            };
            let body = serde_json::to_vec(&proof).map_err(|_| AgentFailure::InvalidInput)?;
            let raw_response = self
                .post_bytes(
                    "/v1/calendar/mirror/release",
                    body,
                    scope,
                    Some(permit),
                )
                .await?;
            // Deserialize only the outer envelope first. RawValue preserves the
            // exact staged page bytes so the signed digest is checked before the
            // strict semantic page decoder runs.
            if raw_response.len() > MAX_PAGE_BYTES + 1024 {
                return Err(AgentFailure::ServerModelInvalidOutput);
            }
            let release_response: RawReleaseResponse =
                serde_json::from_slice(&raw_response).map_err(|_| AgentFailure::InvalidInput)?;
            if release_response.schema_version != 1
                || hex_sha256(release_response.page.get().as_bytes())
                    != signature.result_sha256
            {
                return Err(AgentFailure::PolicyDenied);
            }
            let raw_page_bytes = release_response.page.get().as_bytes();
            if raw_page_bytes.len() > request.limits.max_page_bytes as usize { return Err(AgentFailure::BudgetExceeded); }
            received_bytes = received_bytes.checked_add(raw_page_bytes.len()).ok_or(AgentFailure::BudgetExceeded)?;
            if bytes_before.checked_add(received_bytes).is_none_or(|bytes| bytes > request.limits.max_bytes as usize) { return Err(AgentFailure::BudgetExceeded); }
            strict_json_bytes(raw_page_bytes, MAX_PAGE_BYTES)?;
            let raw_page: RawCalendarPage = serde_json::from_str(release_response.page.get())
                .map_err(|_| AgentFailure::InvalidInput)?;
            validate_raw_page_schedules(&raw_page)?;
            let page: CalendarProductPage = serde_json::from_str(release_response.page.get())
                .map_err(|_| AgentFailure::InvalidInput)?;
            self.lease.ensure_current()?;
            self.lease.revalidate().await?;
            permit.revalidate(scope).await?;
            let (page_observed, page_expires) =
                validate_page(&page, &claims, permit, calendar_id)?;
            latest_observed = latest_observed.max(page_observed);
            earliest_expiry = earliest_expiry.min(page_expires);

            let page_records = match &page.outcome {
                CalendarProductPageOutcome::Complete { records }
                | CalendarProductPageOutcome::More { records, .. } => records,
                CalendarProductPageOutcome::Failed { reason } => {
                    return Ok(PageResult {
                        outcome: CalendarProductPageOutcome::Failed { reason: *reason },
                        observed_at: page_observed,
                        expires_at: page_expires,
                        received_records,
                        received_bytes,
                    });
                }
            };
            if page_records.len() > limit as usize {
                return Err(AgentFailure::BudgetExceeded);
            }
            let record_bytes = serde_json::to_vec(page_records).map_err(|_| AgentFailure::InvalidInput)?;
            if record_bytes.len() > request.limits.max_page_bytes as usize
                || record_bytes.len() > MAX_PAGE_BYTES
            {
                return Err(AgentFailure::BudgetExceeded);
            }
            received_records = received_records
                .checked_add(page_records.len())
                .ok_or(AgentFailure::BudgetExceeded)?;
            if bytes_before.saturating_add(received_bytes) > request.limits.max_bytes as usize
                || total_before.saturating_add(received_records)
                    > request.limits.max_records as usize
            {
                return Ok(failed_page_result(
                    CalendarFailure::BudgetExceeded,
                    page_observed,
                    page_expires,
                    received_records,
                    received_bytes,
                ));
            }
            for record in page_records {
                validate_record(record, calendar_id)?;
                match records_by_id.get(&record.external_id) {
                    Some(existing) if existing != record => {
                        return Ok(failed_page_result(
                            CalendarFailure::SourceChanged,
                            page_observed,
                            page_expires,
                            received_records,
                            received_bytes,
                        ));
                    }
                    Some(_) => {}
                    None => {
                        records_by_id.insert(record.external_id.clone(), record.clone());
                    }
                }
            }
            match page.outcome {
                CalendarProductPageOutcome::Complete { .. } => {
                    return Ok(PageResult {
                        outcome: CalendarProductPageOutcome::Complete {
                            records: records_by_id.into_values().collect(),
                        },
                        observed_at: latest_observed,
                        expires_at: earliest_expiry,
                        received_records,
                        received_bytes,
                    });
                }
                CalendarProductPageOutcome::More { cursor: next, .. } => {
                    if next.is_empty()
                        || next.len() > MAX_CURSOR_BYTES
                        || next.chars().any(char::is_control)
                    {
                        return Err(AgentFailure::InvalidInput);
                    }
                    if !seen_cursors.insert(next.clone()) {
                        return Ok(failed_page_result(
                            CalendarFailure::ProviderUnavailable,
                            page_observed,
                            page_expires,
                            received_records,
                            received_bytes,
                        ));
                    }
                    cursor = next;
                }
                CalendarProductPageOutcome::Failed { .. } => unreachable!("handled above"),
            }
        }
    }

    async fn admit_and_sign(
        &self,
        permit: &ProductCalendarReadPermit,
        scope: &ExecutionScope,
        claims: &ProductCalendarClaims,
    ) -> Result<SignedChallenge, AgentFailure> {
        self.lease.ensure_current()?;
        self.lease.revalidate().await?;
        permit.revalidate(scope).await?;
        let request = AdmissionRequest {
            schema_version: 1,
            claims,
            expires_at_unix_ms: permit.expires_at().timestamp_millis(),
        };
        let response: ChallengeResponse = self
            .post_json("/v1/calendar/mirror/admit", &request, scope, Some(permit))
            .await?;
        self.sign_response(permit, scope, claims, response, ChallengeKind::Admission)
            .await
    }

    async fn read_and_verify(
        &self,
        permit: &ProductCalendarReadPermit,
        scope: &ExecutionScope,
        claims: &ProductCalendarClaims,
        admission: &SignedChallenge,
    ) -> Result<ChallengeResponse, AgentFailure> {
        self.lease.ensure_current()?;
        self.lease.revalidate().await?;
        permit.revalidate(scope).await?;
        let request = ProofRequest {
            schema_version: 1,
            proof: Proof {
                challenge_id: &admission.challenge_id,
                key_id: &admission.signature.key_id,
                signature: &admission.signature.signature,
            },
        };
        let body = serde_json::to_vec(&request).map_err(|_| AgentFailure::InvalidInput)?;
        let response: ChallengeResponse = self
            .post_bytes_json("/v1/calendar/mirror/read", body, scope, Some(permit))
            .await?;
        let verified = self
            .verify_response(permit, scope, claims, response, ChallengeKind::Release {
                admission_id: admission.challenge.challenge_id(),
            })
            .await?;
        match &verified.challenge {
            ProductCalendarChallenge::Release { .. } => Ok(verified.response),
            _ => Err(AgentFailure::PolicyDenied),
        }
    }

    async fn sign_challenge(
        &self,
        permit: &ProductCalendarReadPermit,
        scope: &ExecutionScope,
        claims: &ProductCalendarClaims,
        response: ChallengeResponse,
        kind: ChallengeKind,
    ) -> Result<SignedChallenge, AgentFailure> {
        let verified = self.verify_response(permit, scope, claims, response, kind).await?;
        self.lease.ensure_current()?;
        self.lease.revalidate().await?;
        permit.revalidate(scope).await?;
        let command = ProductCalendarSigningCommand {
            permit,
            scope,
            producer: verified.producer,
            expected: verified.challenge.clone(),
            canonical_bytes: verified.canonical_bytes,
            producer_signature: verified.producer_signature,
        };
        let checked = super::proof::GatewayProofVerifier.verify_product(&command)?;
        if checked != verified.challenge {
            return Err(AgentFailure::PolicyDenied);
        }
        let signature = self
            .lease
            .signer()
            .sign_authorization(AuthorizationSigningCommand::DayCalendarRefresh(command))
            .await?;
        if signature.key_id != verified.challenge.key_id() {
            return Err(AgentFailure::PolicyDenied);
        }
        proof::decode_exact(&signature.signature, 64)?;
        self.lease.ensure_current()?;
        permit.revalidate(scope).await?;
        let challenge_id = verified.challenge.challenge_id().to_string();
        let result_sha256 = match &verified.challenge {
            ProductCalendarChallenge::Release { result_sha256, .. } => result_sha256.clone(),
            ProductCalendarChallenge::Admission { .. } => String::new(),
        };
        Ok(SignedChallenge {
            challenge: verified.challenge,
            challenge_id,
            signature,
            result_sha256,
        })
    }

    async fn sign_response(
        &self,
        permit: &ProductCalendarReadPermit,
        scope: &ExecutionScope,
        claims: &ProductCalendarClaims,
        response: ChallengeResponse,
        kind: ChallengeKind,
    ) -> Result<SignedChallenge, AgentFailure> {
        self.sign_challenge(permit, scope, claims, response, kind).await
    }

    async fn verify_response(
        &self,
        permit: &ProductCalendarReadPermit,
        scope: &ExecutionScope,
        claims: &ProductCalendarClaims,
        response: ChallengeResponse,
        kind: ChallengeKind,
    ) -> Result<VerifiedChallenge, AgentFailure> {
        if response.schema_version != 1 {
            return Err(AgentFailure::CapabilityUnavailable);
        }
        let producer = pinned_producer(
            &response.producer,
            self.lease.credentials().binding(),
            &claims.source.execution_owner,
        )?;
        let canonical_bytes = proof::decode_canonical(&response.challenge_b64url, 65_536)?;
        strict_json_bytes(&canonical_bytes, 65_536)?;
        let challenge: ProductCalendarChallenge = serde_json::from_slice(&canonical_bytes)
            .map_err(|_| AgentFailure::InvalidInput)?;
        let producer_signature = proof::decode_exact(&response.producer_signature, 64)?;
        proof::verify_signature(&producer, &canonical_bytes, &producer_signature)?;
        if challenge.claims() != claims {
            return Err(AgentFailure::PolicyDenied);
        }
        match (&kind, &challenge) {
            (ChallengeKind::Admission, ProductCalendarChallenge::Admission { .. }) => {}
            (
                ChallengeKind::Release { admission_id },
                ProductCalendarChallenge::Release {
                    admission_id: actual,
                    ..
                },
            ) if actual == admission_id => {}
            _ => return Err(AgentFailure::PolicyDenied),
        }
        challenge.validate_permit(permit, Utc::now())?;
        claims.validate_permit(permit)?;
        self.lease.ensure_current()?;
        self.lease.revalidate().await?;
        permit.revalidate(scope).await?;
        Ok(VerifiedChallenge {
            response,
            challenge,
            canonical_bytes,
            producer_signature,
            producer,
        })
    }

    async fn post_json<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
        scope: &ExecutionScope,
        permit: Option<&ProductCalendarReadPermit>,
    ) -> Result<T, AgentFailure> {
        let body = serde_json::to_vec(body).map_err(|_| AgentFailure::InvalidInput)?;
        self.post_bytes_json(path, body, scope, permit).await
    }

    async fn post_bytes_json<T: DeserializeOwned>(
        &self,
        path: &str,
        body: Vec<u8>,
        scope: &ExecutionScope,
        permit: Option<&ProductCalendarReadPermit>,
    ) -> Result<T, AgentFailure> {
        let bytes = self.post_bytes(path, body, scope, permit).await?;
        strict_json_bytes(&bytes, 128 * 1024)?;
        serde_json::from_slice(&bytes).map_err(|_| AgentFailure::InvalidInput)
    }

    async fn post_bytes(
        &self,
        path: &str,
        body: Vec<u8>,
        scope: &ExecutionScope,
        permit: Option<&ProductCalendarReadPermit>,
    ) -> Result<Vec<u8>, AgentFailure> {
        self.lease.ensure_current()?;
        self.lease.revalidate().await?;
        if let Some(permit) = permit {
            permit.revalidate(scope).await?;
        }
        let mut deadline = scope.deadline();
        let clamp = tokio::time::Instant::now() + MAX_HTTP_CALL;
        deadline = deadline.min(clamp);
        if let Some(permit) = permit {
            let until_expiry = permit
                .expires_at()
                .signed_duration_since(Utc::now())
                .to_std()
                .map_err(|_| AgentFailure::DeadlineExceeded)?;
            deadline = deadline.min(tokio::time::Instant::now() + until_expiry);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(AgentFailure::DeadlineExceeded);
        }
        let http = GatewayHttpTransport::new()?;
        let response_limit = if path == "/v1/calendar/mirror/release" { MAX_PAGE_BYTES + 1024 } else { 128 * 1024 };
        let call = http.request_bounded(
            self.lease.credentials().base_url(),
            Some(self.lease.credentials().bearer_token()),
            Method::POST,
            path,
            Some(body),
            deadline,
            scope.cancellation(),
            response_limit,
        );
        let (status, bytes) = tokio::select! {
            _ = self.lease.cancellation().cancelled() => return Err(AgentFailure::Cancelled),
            result = call => result?,
        };
        if status != 200 {
            return Err(parse_http_error(status, &bytes)?);
        }
        if bytes.len() > response_limit {
            return Err(AgentFailure::ServerModelInvalidOutput);
        }
        self.lease.ensure_current()?;
        self.lease.revalidate().await?;
        if let Some(permit) = permit {
            permit.revalidate(scope).await?;
        }
        Ok(bytes)
    }
}

#[derive(Clone, Copy)]
enum ChallengeKind {
    Admission,
    Release { admission_id: Uuid },
}

struct VerifiedChallenge {
    response: ChallengeResponse,
    challenge: ProductCalendarChallenge,
    canonical_bytes: Vec<u8>,
    producer_signature: Vec<u8>,
    producer: RemoteProducerIdentity,
}

struct SignedChallenge {
    challenge: ProductCalendarChallenge,
    challenge_id: String,
    signature: AuthorizationSignature,
    result_sha256: String,
}

fn provider_for(source: &SourceConnection) -> Result<CalendarProvider, AgentFailure> {
    provider_for_connector(source.connector_id().as_str())
}

fn provider_for_connector(connector: &str) -> Result<CalendarProvider, AgentFailure> {
    match connector {
        "calendar.google" => Ok(CalendarProvider::Google),
        "calendar.microsoft" => Ok(CalendarProvider::Microsoft),
        _ => Err(AgentFailure::CapabilityUnavailable),
    }
}

fn configured_resources(
    source: &SourceConnection,
) -> Result<(Vec<ResourceHandle>, Vec<String>), AgentFailure> {
    let handles = source
        .resources()
        .iter()
        .map(|resource| resource.handle().clone())
        .collect::<Vec<_>>();
    if handles.is_empty()
        || handles.len() > floe_context_contract::MAX_SOURCE_RESOURCES
        || handles.windows(2).any(|pair| pair[0] >= pair[1])
    {
        return Err(AgentFailure::InvalidInput);
    }
    let ids = handles
        .iter()
        .map(|resource| resource.as_str().to_owned())
        .collect();
    Ok((handles, ids))
}

fn validate_preview(
    descriptor: &ProductCalendarSourcePreview,
    actor: &OwnerActor,
    source: &SourceConnection,
    provider: CalendarProvider,
    handles: &[ResourceHandle],
    resource_ids: &[String],
    binding: &floe_access::VerifiedGatewayBinding,
    producer: &RemoteProducerIdentity,
) -> Result<(), AgentFailure> {
    let nonce = proof::decode_exact(&descriptor.nonce, 32)?;
    let now = Utc::now().timestamp_millis();
    let preview_ttl = descriptor
        .expires_at_unix_ms
        .checked_sub(descriptor.issued_at_unix_ms)
        .ok_or(AgentFailure::PolicyDenied)?;
    let authority = source.source_authority();
    if descriptor.v != 1
        || descriptor.challenge_id.is_nil()
        || URL_SAFE_NO_PAD.encode(nonce) != descriptor.nonce
        || descriptor.issued_at_unix_ms <= 0
        || descriptor.issued_at_unix_ms > now.saturating_add(5_000)
        || descriptor.expires_at_unix_ms <= now
        || preview_ttl <= 0
        || preview_ttl > 30_000
        || descriptor.person_id != actor.person_id.to_string()
        || descriptor.client_id != binding.client_id
        || descriptor.device_id != actor.device_id
        || descriptor.audience != binding.producer_audience
        || descriptor.producer_instance != binding.producer_instance
        || descriptor.producer_key_fingerprint != binding.producer_key_fingerprint
        || descriptor.source.connector_id != source.connector_id().as_str()
        || descriptor.source.connection_id != source.connection_id().as_str()
        || descriptor.source.execution_owner != source.execution_owner_id().as_str()
        || descriptor.source.local_revision != source.revision()
        || descriptor.source.provider_revision == 0
        || descriptor.source.incarnation != authority.incarnation()
        || descriptor.source.epoch != authority.epoch().get()
        || descriptor.source.identity_generation == 0
        || descriptor.source.provider_identity.is_empty()
        || descriptor.source.provider_identity.len() > 256
        || descriptor.source.provider_identity.chars().any(char::is_control)
        || descriptor.resources != resource_ids
        || handles.len() != descriptor.resources.len()
        || producer.execution_owner != descriptor.source.execution_owner
        || producer.instance_id != descriptor.producer_instance
        || producer.fingerprint != descriptor.producer_key_fingerprint
        || producer.audience != descriptor.audience
        || provider != provider_for(source)?
    {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(())
}

fn pinned_producer(
    observed: &ProducerIdentityResponse,
    binding: &floe_access::VerifiedGatewayBinding,
    expected_execution_owner: &str,
) -> Result<RemoteProducerIdentity, AgentFailure> {
    let identity = access_producer_identity(observed);
    proof::validate_producer(&identity)?;
    if identity.schema_version != 1
        || identity.instance_id != binding.producer_instance
        || identity.fingerprint != binding.producer_key_fingerprint
        || identity.audience != binding.producer_audience
        || identity.execution_owner != expected_execution_owner
    {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(identity)
}

fn source_claims(
    expectation: &SourceExpectation,
    identity_generation: u64,
) -> Result<ProductCalendarSourceClaims, AgentFailure> {
    let source = &expectation.source;
    let claims = ProductCalendarSourceClaims {
        connector_id: source.connector().as_str().to_owned(),
        connection_id: source.connection_id().as_str().to_owned(),
        execution_owner: source.execution_owner().as_str().to_owned(),
        local_revision: expectation.revision.ok_or(AgentFailure::StaleContext)?,
        provider_revision: expectation.provider_revision.ok_or(AgentFailure::StaleContext)?,
        incarnation: expectation.authority.incarnation(),
        epoch: expectation.authority.epoch().get(),
        provider_identity: expectation.subject_fingerprint.clone(),
        identity_generation,
    };
    if claims.local_revision == 0
        || claims.provider_revision == 0
        || claims.epoch == 0
        || claims.identity_generation == 0
    {
        return Err(AgentFailure::StaleContext);
    }
    Ok(claims)
}

fn validate_page(
    page: &CalendarProductPage,
    claims: &ProductCalendarClaims,
    permit: &ProductCalendarReadPermit,
    calendar_id: &str,
) -> Result<(DateTime<Utc>, DateTime<Utc>), AgentFailure> {
    let request = permit.request();
    let observed_at = DateTime::from_timestamp_millis(page.observed_at_unix_ms)
        .ok_or(AgentFailure::InvalidInput)?;
    let expires_at = DateTime::from_timestamp_millis(page.expires_at_unix_ms)
        .ok_or(AgentFailure::InvalidInput)?;
    let now = Utc::now();
    if page.schema_version != 1
        || page.result_kind != CalendarProductResultKind::CalendarMirror
        || page.refresh_operation_id != request.refresh_operation_id
        || page.read_operation_id != request.read_operation_id
        || page.page_id != claims.page_id
        || page.person_id != request.actor.person_id.to_string()
        || page.device_id != request.actor.device_id
        || page.source != claims.source
        || page.calendar_id != calendar_id
        || page.range_start_unix_ms != request.range_start.timestamp_millis()
        || page.range_end_unix_ms != request.range_end.timestamp_millis()
        || observed_at < permit.observation().observed_at
        || observed_at > now + chrono::Duration::seconds(5)
        || expires_at <= now
        || expires_at <= observed_at
        || expires_at > permit.expires_at()
    {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok((observed_at, expires_at))
}

fn validate_record(record: &CalendarRecord, calendar_id: &str) -> Result<(), AgentFailure> {
    if record.can_modify
        || record.calendar_id != calendar_id
        || record.external_id.trim().is_empty()
        || record.external_id.len() > MAX_EXTERNAL_ID_BYTES
        || record.external_id.chars().any(char::is_control)
        || record.title.len() > MAX_TITLE_BYTES
        || !record.external_revision.is_valid()
    {
        return Err(AgentFailure::InvalidInput);
    }
    match &record.schedule {
        EventSchedule::Timed(schedule) => {
            if schedule.starts_at >= schedule.ends_at
                || schedule.timezone.is_empty()
                || schedule.timezone.len() > MAX_PROVIDER_TEXT_BYTES
                || schedule.timezone.chars().any(char::is_control)
            {
                return Err(AgentFailure::InvalidInput);
            }
        }
        EventSchedule::AllDay(schedule) => {
            if schedule.end_date_exclusive <= schedule.start_date {
                return Err(AgentFailure::InvalidInput);
            }
        }
    }
    Ok(())
}

fn validate_raw_page_schedules(page: &RawCalendarPage) -> Result<(), AgentFailure> {
    if page.schema_version != 1
        || page.result_kind != ProductCalendarResultKind::CalendarMirror
        || page.refresh_operation_id.is_nil()
        || page.read_operation_id.is_nil()
        || page.page_id.is_nil()
        || page.person_id.is_empty()
        || page.device_id.is_empty()
        || page.calendar_id.is_empty()
        || page.range_start_unix_ms < 0
        || page.range_end_unix_ms <= page.range_start_unix_ms
        || page.observed_at_unix_ms <= 0
        || page.expires_at_unix_ms <= page.observed_at_unix_ms
        || page.source.local_revision == 0
        || page.source.provider_revision == 0
        || page.source.epoch == 0
        || page.source.identity_generation == 0
        || page.source.incarnation.is_nil()
    {
        return Err(AgentFailure::InvalidInput);
    }
    let records = match &page.outcome {
        RawCalendarPageOutcome::Complete { records } => Some(records),
        RawCalendarPageOutcome::More { records, cursor } => {
            if cursor.is_empty()
                || cursor.len() > MAX_CURSOR_BYTES
                || cursor.chars().any(char::is_control)
            {
                return Err(AgentFailure::InvalidInput);
            }
            Some(records)
        }
        RawCalendarPageOutcome::Failed { reason } => {
            let _closed_reason = reason;
            None
        }
    };
    if let Some(records) = records {
        for record in records {
            if record.can_modify
                || record.calendar_id != page.calendar_id
                || record.external_id.trim().is_empty()
                || record.external_id.len() > MAX_EXTERNAL_ID_BYTES
                || record.external_id.chars().any(char::is_control)
                || record.title.len() > MAX_TITLE_BYTES
                || !record.external_revision.is_valid()
            {
                return Err(AgentFailure::InvalidInput);
            }
            match &record.schedule {
                RawCalendarSchedule::Timed {
                    starts_at,
                    ends_at,
                    timezone,
                } => {
                    let start = DateTime::parse_from_rfc3339(starts_at)
                        .map_err(|_| AgentFailure::InvalidInput)?;
                    let end = DateTime::parse_from_rfc3339(ends_at)
                        .map_err(|_| AgentFailure::InvalidInput)?;
                    if !starts_at.ends_with('Z')
                        || !ends_at.ends_with('Z')
                        || start >= end
                        || timezone.is_empty()
                        || timezone.len() > MAX_PROVIDER_TEXT_BYTES
                        || timezone.chars().any(char::is_control)
                    {
                        return Err(AgentFailure::InvalidInput);
                    }
                }
                RawCalendarSchedule::AllDay {
                    start_date,
                    end_date_exclusive,
                } => {
                    let start = NaiveDate::parse_from_str(start_date, "%Y-%m-%d")
                        .map_err(|_| AgentFailure::InvalidInput)?;
                    let end = NaiveDate::parse_from_str(end_date_exclusive, "%Y-%m-%d")
                        .map_err(|_| AgentFailure::InvalidInput)?;
                    if start.format("%Y-%m-%d").to_string() != start_date.as_str()
                        || end.format("%Y-%m-%d").to_string() != end_date_exclusive.as_str()
                        || end <= start
                    {
                        return Err(AgentFailure::InvalidInput);
                    }
                }
            }
        }
    }
    Ok(())
}

fn failed_page_result(
    reason: CalendarFailure,
    observed_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    received_records: usize,
    received_bytes: usize,
) -> PageResult {
    PageResult {
        outcome: CalendarProductPageOutcome::Failed { reason },
        observed_at,
        expires_at,
        received_records,
        received_bytes,
    }
}

fn parse_http_error(status: u16, bytes: &[u8]) -> Result<AgentFailure, AgentFailure> {
    strict_json_bytes(bytes, 16_384)?;
    let envelope: ErrorEnvelope =
        serde_json::from_slice(bytes).map_err(|_| AgentFailure::ServerModelInvalidOutput)?;
    if !proof::valid_text(&envelope.error.code, 128) {
        return Err(AgentFailure::ServerModelInvalidOutput);
    }
    Ok(match status {
        400 => AgentFailure::InvalidInput,
        401 => AgentFailure::CredentialExpired,
        403 => AgentFailure::PolicyDenied,
        404 => AgentFailure::NotFound,
        409 => AgentFailure::Conflict,
        429 => AgentFailure::QuotaExceeded,
        502 => AgentFailure::ServerModelUnavailable,
        503 => AgentFailure::CapabilityUnavailable,
        500 => AgentFailure::CapabilityUnavailable,
        _ => AgentFailure::ServerModelUnavailable,
    })
}

fn hex_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
