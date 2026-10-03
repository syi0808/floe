//! Direct product display authority. These values never grant assistant access.
use chrono::{DateTime, Utc};
use floe_context_contract::{CalendarProvider, GrantSourceBinding};
use floe_execution::Cancellation;
use floe_kernel::{AgentFailure, OwnerActor};
use serde::{Deserialize, Serialize};
use tokio::time::Instant;
use uuid::Uuid;
use crate::SourceExpectation;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductReadPurpose { DayCalendarRefresh }

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarReadLimits { pub max_records: u32, pub max_bytes: u32, pub max_page_records: u32, pub max_page_bytes: u32 }
impl CalendarReadLimits {
    pub fn validate(self) -> Result<(), AgentFailure> {
        if self.max_records == 0 || self.max_records > 10_000 || self.max_bytes == 0 || self.max_bytes > 4 * 1024 * 1024 || self.max_page_records == 0 || self.max_page_records > 128 || self.max_page_records > self.max_records || self.max_page_bytes == 0 || self.max_page_bytes > 1024 * 1024 || self.max_page_bytes > self.max_bytes { return Err(AgentFailure::BudgetExceeded); } Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct ProductCalendarReadRequest {
    pub actor: OwnerActor,
    pub refresh_operation_id: Uuid,
    pub read_operation_id: Uuid,
    pub source: GrantSourceBinding,
    pub range_start: DateTime<Utc>,
    pub range_end: DateTime<Utc>,
    pub limits: CalendarReadLimits,
}
impl ProductCalendarReadRequest {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.actor.validate()?; self.source.validate().map_err(|_| AgentFailure::InvalidInput)?; self.limits.validate()?;
        if self.refresh_operation_id.is_nil() || self.read_operation_id.is_nil() || self.source.person_id() != self.actor.person_id || self.range_start >= self.range_end || self.range_end.signed_duration_since(self.range_start) > chrono::Duration::hours(48) || !matches!(self.source.connector().as_str(), "calendar.event_kit" | "calendar.google" | "calendar.microsoft") { return Err(AgentFailure::InvalidInput); } Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProductCalendarPermission { NativeRead, ProviderRead { identity_generation: u64, gateway_runtime_generation: u64 } }

/// Produced by the injected real source-fact port, never decoded from AppWire.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductSourceObservation { pub expectation: SourceExpectation, pub provider: CalendarProvider, pub permission: ProductCalendarPermission, pub observed_at: DateTime<Utc> }
impl ProductSourceObservation {
    pub fn validate(&self, request: &ProductCalendarReadRequest, now: DateTime<Utc>) -> Result<(), AgentFailure> {
        self.validate_metadata(&request.actor, &request.source, self.provider, now)
    }
    /// Metadata-only status checks reuse the exact source/permission policy
    /// without inventing a refresh operation or minting a read permit.
    pub fn validate_metadata(&self, actor: &OwnerActor, source: &GrantSourceBinding, expected_provider: CalendarProvider, now: DateTime<Utc>) -> Result<(), AgentFailure> {
        actor.validate()?; source.validate().map_err(|_| AgentFailure::InvalidInput)?;
        self.expectation.validate()?; self.expectation.validate_device(&actor.device_id)?;
        if source.person_id() != actor.person_id || self.expectation.source != *source || self.provider != expected_provider || self.expectation.revision.is_none() || self.observed_at > now || now.signed_duration_since(self.observed_at) > chrono::Duration::seconds(30) || self.expectation.physical_resources.len() > 256 { return Err(AgentFailure::StaleContext); }
        match (source.connector().as_str(), self.provider, self.permission, self.expectation.gateway.is_some()) {
            ("calendar.event_kit", CalendarProvider::EventKit, ProductCalendarPermission::NativeRead, false) => Ok(()),
            ("calendar.google", CalendarProvider::Google, ProductCalendarPermission::ProviderRead { identity_generation, gateway_runtime_generation }, true) | ("calendar.microsoft", CalendarProvider::Microsoft, ProductCalendarPermission::ProviderRead { identity_generation, gateway_runtime_generation }, true) if identity_generation > 0 && gateway_runtime_generation > 0 => Ok(()),
            _ => Err(AgentFailure::PolicyDenied),
        }
    }

}

pub struct ProductCalendarReadPermit {
    pub(crate) request: ProductCalendarReadRequest,
    pub(crate) observation: ProductSourceObservation,
    pub(crate) expires_at: DateTime<Utc>,
    pub(crate) deadline: Instant,
    pub(crate) cancellation: Cancellation,
    pub(crate) sources: std::sync::Arc<dyn crate::ProductSourceAuthority>,
    pub(crate) clock: std::sync::Arc<dyn crate::AccessClock>,
}
impl ProductCalendarReadPermit {
    pub fn request(&self) -> &ProductCalendarReadRequest { &self.request }
    pub fn observation(&self) -> &ProductSourceObservation { &self.observation }
    pub fn expires_at(&self) -> DateTime<Utc> { self.expires_at }
    pub fn purpose(&self) -> ProductReadPurpose { ProductReadPurpose::DayCalendarRefresh }
    pub fn gateway_runtime_generation(&self) -> Option<u64> { match self.observation.permission { ProductCalendarPermission::NativeRead => None, ProductCalendarPermission::ProviderRead { gateway_runtime_generation, .. } => Some(gateway_runtime_generation) } }
    pub fn check_lifetime(&self, now: DateTime<Utc>) -> Result<(), AgentFailure> {
        if self.cancellation.is_cancelled() { return Err(AgentFailure::Cancelled); }
        if Instant::now() >= self.deadline || now >= self.expires_at { return Err(AgentFailure::DeadlineExceeded); } Ok(())
    }
    /// The provider and encrypted signer call this before each page admission
    /// and release. The retained owner capability performs policy revalidation.
    pub async fn revalidate(&self, scope: &floe_execution::ExecutionScope) -> Result<(), AgentFailure> {
        self.check_lifetime(self.clock.now())?;
        if scope.cancellation().is_cancelled() { return Err(AgentFailure::Cancelled); }
        if Instant::now() >= scope.deadline() { return Err(AgentFailure::DeadlineExceeded); }
        let current = self.sources.observe_current(&self.request.actor, &self.request.source, scope).await?;
        current.validate(&self.request, self.clock.now())?;
        if current.expectation != self.observation.expectation || current.provider != self.observation.provider || current.permission != self.observation.permission { return Err(AgentFailure::StaleContext); }
        if scope.cancellation().is_cancelled() { return Err(AgentFailure::Cancelled); }
        if Instant::now() >= scope.deadline() { return Err(AgentFailure::DeadlineExceeded); }
        self.check_lifetime(self.clock.now())
    }
}

pub struct ProductCalendarDispatchFence<'a> { pub(crate) permit: &'a ProductCalendarReadPermit }
impl ProductCalendarDispatchFence<'_> { pub fn permit(&self) -> &ProductCalendarReadPermit { self.permit } }

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductCalendarResultBinding {
    pub read_operation_id: Uuid,
    pub source: SourceExpectation,
    pub payload_digest: [u8; 32],
    pub record_count: u32,
    pub byte_count: u32,
    pub observed_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

pub struct ProductCalendarReadReceipt { pub(crate) request: ProductCalendarReadRequest, pub(crate) binding: ProductCalendarResultBinding }
impl ProductCalendarReadReceipt {
    pub fn request(&self) -> &ProductCalendarReadRequest { &self.request }
    pub fn binding(&self) -> &ProductCalendarResultBinding { &self.binding }
}
