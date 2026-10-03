//! Domain-separated exact signed Calendar product-read claims.
use chrono::{DateTime, Utc};
use floe_kernel::AgentFailure;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use crate::{CalendarReadLimits, ProductCalendarPermission, ProductCalendarReadPermit};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum CalendarProductWirePurpose { #[serde(rename = "day_refresh")] DayRefresh }
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum CalendarProductResultKind { #[serde(rename = "calendar.mirror")] CalendarMirror }
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum CalendarProductPreviewOperation { #[serde(rename = "day_calendar_source_preview")] SourcePreview }

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProductCalendarSourcePreview {
    pub v: u32,
    pub operation: CalendarProductPreviewOperation,
    pub challenge_id: Uuid,
    pub nonce: String,
    pub person_id: String,
    pub client_id: String,
    pub device_id: String,
    pub audience: String,
    pub producer_instance: String,
    pub producer_key_fingerprint: String,
    pub source: ProductCalendarSourceClaims,
    pub resources: Vec<String>,
    pub issued_at_unix_ms: i64,
    pub expires_at_unix_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProductCalendarPageQuery {
    pub calendar_id: String,
    pub range_start_unix_ms: i64,
    pub range_end_unix_ms: i64,
    pub cursor: String,
    pub limit: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProductCalendarSourceClaims {
    pub connector_id: String,
    pub connection_id: String,
    pub execution_owner: String,
    pub local_revision: u64,
    pub provider_revision: u64,
    pub incarnation: Uuid,
    pub epoch: u64,
    pub provider_identity: String,
    pub identity_generation: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProductCalendarClaims {
    pub person_id: String,
    pub client_id: String,
    pub device_id: String,
    pub audience: String,
    pub producer_instance: String,
    pub producer_key_fingerprint: String,
    pub enrollment_id: String,
    pub credential_generation: u64,
    pub purpose: CalendarProductWirePurpose,
    pub result_kind: CalendarProductResultKind,
    pub refresh_operation_id: Uuid,
    pub read_operation_id: Uuid,
    pub page_id: Uuid,
    pub source: ProductCalendarSourceClaims,
    pub resources: Vec<String>,
    pub query: ProductCalendarPageQuery,
    pub query_sha256: String,
    pub limits: CalendarReadLimits,
}
impl ProductCalendarClaims {
    pub fn validate_permit(&self, permit: &ProductCalendarReadPermit) -> Result<(), AgentFailure> {
        let request = permit.request(); let observed = permit.observation(); let source = &observed.expectation;
        let gateway = source.gateway.as_ref().ok_or(AgentFailure::PolicyDenied)?;
        let ProductCalendarPermission::ProviderRead { identity_generation, .. } = observed.permission else { return Err(AgentFailure::PolicyDenied); };
        let expected_resources = source.physical_resources.iter().map(|resource| resource.as_str().to_owned()).collect::<Vec<_>>();
        let query_digest = hex_sha256(&serde_json::to_vec(&self.query).map_err(|_| AgentFailure::InvalidInput)?);
        if self.person_id != request.actor.person_id.to_string() || self.client_id != gateway.client_id || self.device_id != request.actor.device_id || self.audience != gateway.producer_audience || self.producer_instance != gateway.producer_instance || self.producer_key_fingerprint != gateway.producer_key_fingerprint || self.enrollment_id != gateway.enrollment_id || self.credential_generation != gateway.credential_generation || self.refresh_operation_id != request.refresh_operation_id || self.read_operation_id != request.read_operation_id || self.page_id.is_nil() || self.source.connector_id != request.source.connector().as_str() || self.source.connection_id != request.source.connection_id().as_str() || self.source.execution_owner != request.source.execution_owner().as_str() || Some(self.source.local_revision) != source.revision || Some(self.source.provider_revision) != source.provider_revision || self.source.incarnation != source.authority.incarnation() || self.source.epoch != source.authority.epoch().get() || self.source.provider_identity != source.subject_fingerprint || self.source.identity_generation != identity_generation || self.resources != expected_resources || !self.resources.contains(&self.query.calendar_id) || self.query.range_start_unix_ms != request.range_start.timestamp_millis() || self.query.range_end_unix_ms != request.range_end.timestamp_millis() || self.query.limit == 0 || self.query.limit > request.limits.max_page_records || self.query.cursor.len() > 4096 || self.query.cursor.chars().any(char::is_control) || self.query_sha256 != query_digest || self.limits != request.limits { return Err(AgentFailure::PolicyDenied); }
        Ok(())
    }
}

/// This exact closed union is the private signed wire. The strict provider
/// decoder also rejects duplicate and case-variant keys before deserializing.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "operation", deny_unknown_fields)]
pub enum ProductCalendarChallenge {
    #[serde(rename = "day_calendar_admission")]
    Admission { v: u32, challenge_id: Uuid, nonce: String, key_id: String, claims: ProductCalendarClaims, issued_at_unix_ms: i64, expires_at_unix_ms: i64 },
    #[serde(rename = "day_calendar_release")]
    Release { v: u32, challenge_id: Uuid, nonce: String, key_id: String, claims: ProductCalendarClaims, admission_id: Uuid, result_sha256: String, issued_at_unix_ms: i64, expires_at_unix_ms: i64 },
}
impl ProductCalendarChallenge {
    pub fn claims(&self) -> &ProductCalendarClaims { match self { Self::Admission { claims, .. } | Self::Release { claims, .. } => claims } }
    pub fn challenge_id(&self) -> Uuid { match self { Self::Admission { challenge_id, .. } | Self::Release { challenge_id, .. } => *challenge_id } }
    pub fn key_id(&self) -> &str { match self { Self::Admission { key_id, .. } | Self::Release { key_id, .. } => key_id } }
    pub fn expires_at_unix_ms(&self) -> i64 { match self { Self::Admission { expires_at_unix_ms, .. } | Self::Release { expires_at_unix_ms, .. } => *expires_at_unix_ms } }
    pub fn validate_permit(&self, permit: &ProductCalendarReadPermit, now: DateTime<Utc>) -> Result<(), AgentFailure> {
        use base64::Engine;
        let (v, nonce, issued) = match self { Self::Admission { v, nonce, issued_at_unix_ms, .. } | Self::Release { v, nonce, issued_at_unix_ms, .. } => (*v, nonce, *issued_at_unix_ms) };
        let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(nonce).map_err(|_| AgentFailure::PolicyDenied)?;
        if v != 1 || self.challenge_id().is_nil() || Uuid::parse_str(self.key_id()).is_err() || decoded.len() != 32 || base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(decoded) != *nonce || issued <= 0 || issued > now.timestamp_millis().saturating_add(5_000) || self.expires_at_unix_ms() <= now.timestamp_millis() || self.expires_at_unix_ms() <= issued || self.expires_at_unix_ms() > permit.expires_at().timestamp_millis() { return Err(AgentFailure::PolicyDenied); }
        if let Self::Release { admission_id, result_sha256, .. } = self { if admission_id.is_nil() || !is_hex_digest(result_sha256) { return Err(AgentFailure::PolicyDenied); } }
        permit.check_lifetime(now)?; self.claims().validate_permit(permit)
    }
}

pub struct ProductCalendarSigningCommand<'a> {
    pub permit: &'a ProductCalendarReadPermit,
    pub scope: &'a floe_execution::ExecutionScope,
    pub producer: crate::RemoteProducerIdentity,
    pub expected: ProductCalendarChallenge,
    pub canonical_bytes: Vec<u8>,
    pub producer_signature: Vec<u8>,
}
impl ProductCalendarSigningCommand<'_> {
    pub async fn validate_for_signing(&self, claims: &ProductCalendarChallenge, person_id: floe_kernel::PersonId, owner_key_id: &str) -> Result<(), AgentFailure> {
        self.permit.revalidate(self.scope).await?;
        self.validate_claims(claims, person_id, owner_key_id, self.permit.clock.now())
    }
    pub fn validate_claims(&self, claims: &ProductCalendarChallenge, person_id: floe_kernel::PersonId, owner_key_id: &str, now: DateTime<Utc>) -> Result<(), AgentFailure> {
        if self.canonical_bytes.is_empty() || self.canonical_bytes.len() > 64 * 1024 || self.producer_signature.len() != 64 || claims != &self.expected || claims.claims().person_id != person_id.to_string() || claims.key_id() != owner_key_id || self.producer.schema_version != 1 || self.producer.instance_id != claims.claims().producer_instance || self.producer.fingerprint != claims.claims().producer_key_fingerprint || self.producer.audience != claims.claims().audience || self.producer.execution_owner != claims.claims().source.execution_owner { return Err(AgentFailure::PolicyDenied); }
        claims.validate_permit(self.permit, now)
    }
}
pub fn hex_sha256(bytes: &[u8]) -> String { Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect() }
pub fn is_hex_digest(value: &str) -> bool { value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)) }
