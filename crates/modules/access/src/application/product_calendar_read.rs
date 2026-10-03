use crate::{
    AccessClock, ProductCalendarDispatchFence, ProductCalendarReadPermit,
    ProductCalendarReadReceipt, ProductCalendarReadRequest, ProductCalendarResultBinding,
    ProductSourceAuthority,
};
use floe_execution::ExecutionScope;
use floe_kernel::AgentFailure;
use std::sync::Arc;

/// Access policy over current configured source facts. Grant storage is not a
/// prerequisite for native product display and no Observe grant is minted.
pub struct ProductCalendarReadAuthority {
    sources: Arc<dyn ProductSourceAuthority>,
    clock: Arc<dyn AccessClock>,
}
impl ProductCalendarReadAuthority {
    pub fn new(sources: Arc<dyn ProductSourceAuthority>, clock: Arc<dyn AccessClock>) -> Self {
        Self { sources, clock }
    }
    pub async fn admit(
        &self,
        request: ProductCalendarReadRequest,
        scope: &ExecutionScope,
    ) -> Result<ProductCalendarReadPermit, AgentFailure> {
        request.validate()?;
        check(scope)?;
        let observation = self
            .sources
            .observe_current(&request.actor, &request.source, scope)
            .await?;
        let now = self.clock.now();
        observation.validate(&request, now)?;
        check(scope)?;
        let remaining = scope
            .deadline()
            .saturating_duration_since(tokio::time::Instant::now())
            .min(std::time::Duration::from_secs(60));
        let expires_at = now
            .checked_add_signed(
                chrono::Duration::from_std(remaining).map_err(|_| AgentFailure::InvalidInput)?,
            )
            .ok_or(AgentFailure::InvalidInput)?;
        Ok(ProductCalendarReadPermit {
            request,
            observation,
            expires_at,
            deadline: scope.deadline(),
            cancellation: scope.cancellation().clone(),
            sources: self.sources.clone(),
            clock: self.clock.clone(),
        })
    }
    async fn revalidate(
        &self,
        permit: &ProductCalendarReadPermit,
        scope: &ExecutionScope,
    ) -> Result<(), AgentFailure> {
        permit.revalidate(scope).await
    }
    pub async fn revalidate_for_dispatch<'a>(
        &self,
        permit: &'a ProductCalendarReadPermit,
        scope: &ExecutionScope,
    ) -> Result<ProductCalendarDispatchFence<'a>, AgentFailure> {
        self.revalidate(permit, scope).await?;
        Ok(ProductCalendarDispatchFence { permit })
    }
    pub async fn release(
        &self,
        permit: ProductCalendarReadPermit,
        binding: ProductCalendarResultBinding,
        scope: &ExecutionScope,
    ) -> Result<ProductCalendarReadReceipt, AgentFailure> {
        if binding.read_operation_id != permit.request.read_operation_id
            || binding.source != permit.observation.expectation
            || binding.payload_digest == [0; 32]
            || binding.record_count > permit.request.limits.max_records
            || binding.byte_count == 0
            || binding.byte_count > permit.request.limits.max_bytes
            || binding.observed_at > self.clock.now()
            || binding.observed_at < permit.observation.observed_at
            || binding.expires_at <= self.clock.now()
            || binding.expires_at > permit.expires_at
        {
            return Err(AgentFailure::PolicyDenied);
        }
        self.revalidate(&permit, scope).await?;
        Ok(ProductCalendarReadReceipt {
            request: permit.request,
            binding,
        })
    }
}
fn check(scope: &ExecutionScope) -> Result<(), AgentFailure> {
    if scope.cancellation().is_cancelled() {
        Err(AgentFailure::Cancelled)
    } else if tokio::time::Instant::now() >= scope.deadline() {
        Err(AgentFailure::DeadlineExceeded)
    } else {
        Ok(())
    }
}
