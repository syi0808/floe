//! Retained Gateway reads and source-bound native Calendar acquisition for
//! Context. Access and Context keep grant, selection and projection policy.
use std::{future::Future, sync::Arc, time::Duration};

use chrono::Utc;
use floe_access::{
    CalendarReadAccessRequest, CalendarReadAccessStamp, RemoteCallWindow, RemoteGrantTransport,
    RemotePairingIdentity, RemoteProducerIdentity, RemoteSourceQuery, RemoteViewSourceReference,
    SignedSourcePreview, SourcePreviewVerifier, producer_is_pinned,
};
use floe_connections::{ConnectionsRepository, ConnectorSnapshot, SourceConnection};
use floe_context::{
    AdmittedRemoteRead, CalendarObservation, CalendarObserveRequest, ExpertRemoteSource,
    ExpertRemoteTransport, ExpertSourceTransport, RemoteViewTransport,
};
use floe_context_contract::CalendarProvider;
use floe_day::{CalendarBatch, CalendarResourceOutcome};
use floe_execution::{BoxFuture, Cancellation, ExecutionScope};
use floe_kernel::{AgentFailure, OwnerActor};
use floe_native::{
    CalendarAcquisitionMode, CalendarAcquisitionRequest, CalendarAcquisitionResult, CalendarBroker,
    NATIVE_CALENDAR_WIRE_VERSION,
};
use tokio::time::Instant;
use uuid::Uuid;

use super::{AuthorizedSourceClient, ServerSourceClient};
use crate::gateway::{
    GatewaySourcePreviewVerifier, ProductGatewayLease, ProductGatewayLeaseRegistry,
};

const MAX_NATIVE_CALL: Duration = Duration::from_secs(30);
const MAX_NATIVE_RECORDS: usize = 128;
const MAX_NATIVE_BYTES: usize = 65_536;

/// The actual provider handles for one ready Context generation. No service
/// lookup, selected source, grant or package policy is captured here.
pub struct ExpertSourceAdapter {
    broker: Arc<CalendarBroker>,
    connections: Arc<dyn ConnectionsRepository>,
    gateway: Arc<ProductGatewayLeaseRegistry>,
}

impl ExpertSourceAdapter {
    pub fn new(
        broker: Arc<CalendarBroker>,
        connections: Arc<dyn ConnectionsRepository>,
        gateway: Arc<ProductGatewayLeaseRegistry>,
    ) -> Self {
        Self {
            broker,
            connections,
            gateway,
        }
    }

    async fn current_source(
        &self,
        actor: &OwnerActor,
        source: &SourceConnection,
    ) -> Result<(), AgentFailure> {
        actor.validate()?;
        source.validate().map_err(|_| AgentFailure::InvalidInput)?;
        if source.person_id() != actor.person_id
            || source.execution_owner_id().as_str()
                != floe_access::apple_execution_owner(&actor.device_id)
            || source.connector_id().as_str() != "calendar.event_kit"
        {
            return Err(AgentFailure::PolicyDenied);
        }
        if !source.is_serving()
            || self
                .connections
                .source_is_fenced(actor.person_id, source.connection_id())
                .await
                .map_err(|_| AgentFailure::StorageUnavailable)?
        {
            return Err(AgentFailure::StaleContext);
        }
        let current = self
            .connections
            .load(actor.person_id, source.connection_id())
            .await
            .map_err(|_| AgentFailure::StorageUnavailable)?
            .ok_or(AgentFailure::StaleContext)?;
        if current != *source {
            return Err(AgentFailure::StaleContext);
        }
        Ok(())
    }

    async fn native_call(
        &self,
        actor: &OwnerActor,
        source: &SourceConnection,
        request: CalendarAcquisitionRequest,
        window: &RemoteCallWindow,
    ) -> Result<CalendarAcquisitionResult, AgentFailure> {
        bounded(window, async {
            self.current_source(actor, source).await?;
            let result = self
                .broker
                .submit(
                    request.clone(),
                    Utc::now().timestamp_millis(),
                    window.cancellation.clone(),
                )
                .await?;
            if self.broker.host_epoch(actor.person_id).as_deref() != Ok(request.host_epoch.as_str())
                || result.request_id != request.request_id
                || result.host_epoch != request.host_epoch
                || result.person_id != request.person_id
                || result.device_id != request.device_id
                || result.connection_id != request.connection_id
                || result.connection_revision != request.connection_revision
                || result.provider != request.provider
                || result.mode != request.mode
                || result.calendar_ids != request.calendar_ids
                || result.range_start_unix_ms != request.range_start_unix_ms
                || result.range_end_unix_ms != request.range_end_unix_ms
                || !fingerprint_valid(&result.native_subject_fingerprint_before)
                || result.native_subject_fingerprint_before
                    != result.native_subject_fingerprint_after
            {
                return Err(AgentFailure::StaleContext);
            }
            if !matches!(result.permission_class.as_str(), "full" | "authorized") {
                return Err(AgentFailure::AccessReviewRequired);
            }
            if request
                .expected_native_subject_fingerprint
                .as_deref()
                .is_some_and(|expected| expected != result.native_subject_fingerprint_before)
            {
                return Err(AgentFailure::AccessReviewRequired);
            }
            validate_inventory(&result)?;
            if request
                .calendar_ids
                .iter()
                .any(|id| !result.available_calendar_ids.contains(id))
            {
                return Err(AgentFailure::StaleContext);
            }
            self.current_source(actor, source).await?;
            Ok(result)
        })
        .await
    }

    fn native_request(
        &self,
        actor: &OwnerActor,
        source: &SourceConnection,
        read: &CalendarReadAccessRequest,
        mode: CalendarAcquisitionMode,
        range_start_unix_ms: i64,
        range_end_unix_ms: i64,
    ) -> Result<CalendarAcquisitionRequest, AgentFailure> {
        actor.validate()?;
        if read.person_id != actor.person_id
            || read.device_id != actor.device_id
            || read.provider != CalendarProvider::EventKit
            || range_start_unix_ms < 0
            || range_end_unix_ms <= range_start_unix_ms
        {
            return Err(AgentFailure::InvalidInput);
        }
        let mut configured = source
            .resources()
            .iter()
            .map(|resource| resource.handle().as_str())
            .collect::<Vec<_>>();
        configured.sort();
        if configured.is_empty()
            || configured.len() > 256
            || configured.windows(2).any(|pair| pair[0] >= pair[1])
            || read.calendar_ids.iter().map(String::as_str).ne(configured)
        {
            return Err(AgentFailure::PolicyDenied);
        }
        if read
            .expected_native_subject_fingerprint
            .as_deref()
            .is_some_and(|value| !fingerprint_valid(value))
        {
            return Err(AgentFailure::InvalidInput);
        }
        check_window(read.deadline, &read.cancellation)?;
        let remaining = read
            .deadline
            .saturating_duration_since(Instant::now())
            .min(MAX_NATIVE_CALL);
        let deadline = Utc::now()
            .checked_add_signed(
                chrono::Duration::from_std(remaining).map_err(|_| AgentFailure::InvalidInput)?,
            )
            .ok_or(AgentFailure::InvalidInput)?;
        Ok(CalendarAcquisitionRequest {
            request_id: Uuid::new_v4(),
            host_epoch: self.broker.host_epoch(actor.person_id)?,
            person_id: actor.person_id,
            device_id: actor.device_id.clone(),
            connection_id: source.connection_id().as_str().to_owned(),
            connection_revision: source.revision(),
            provider: CalendarProvider::EventKit,
            mode,
            calendar_ids: read.calendar_ids.clone(),
            range_start_unix_ms,
            range_end_unix_ms,
            deadline_unix_ms: deadline.timestamp_millis(),
            expected_native_subject_fingerprint: if mode == CalendarAcquisitionMode::ReadEvents {
                read.expected_native_subject_fingerprint.clone()
            } else {
                None
            },
        })
    }

    async fn inspect_calendar(
        &self,
        actor: &OwnerActor,
        source: &SourceConnection,
        read: CalendarReadAccessRequest,
    ) -> Result<CalendarReadAccessStamp, AgentFailure> {
        let request = self.native_request(
            actor,
            source,
            &read,
            CalendarAcquisitionMode::InspectSubject,
            0,
            1,
        )?;
        let result = self
            .native_call(
                actor,
                source,
                request,
                &RemoteCallWindow {
                    deadline: read.deadline,
                    cancellation: read.cancellation,
                },
            )
            .await?;
        if !result.batches.is_empty() {
            return Err(AgentFailure::InvalidInput);
        }
        if read
            .expected_native_subject_fingerprint
            .as_deref()
            .is_some_and(|expected| expected != result.native_subject_fingerprint_after)
        {
            return Err(AgentFailure::AccessReviewRequired);
        }
        Ok(native_stamp(&result))
    }

    async fn read_calendar(
        &self,
        actor: &OwnerActor,
        source: &SourceConnection,
        read: CalendarObserveRequest,
    ) -> Result<CalendarObservation, AgentFailure> {
        if read.cursor.is_some()
            || read.starts_at >= read.ends_at
            || read.ends_at - read.starts_at > chrono::Duration::days(32)
            || read.expected_native_subject_fingerprint.is_none()
        {
            return Err(AgentFailure::InvalidInput);
        }
        let check = CalendarReadAccessRequest {
            person_id: read.person_id,
            device_id: read.device_id,
            provider: read.provider,
            calendar_ids: read.calendar_ids,
            expected_native_subject_fingerprint: read.expected_native_subject_fingerprint,
            deadline: read.deadline,
            cancellation: read.cancellation,
        };
        let before = self.inspect_calendar(actor, source, check.clone()).await?;
        let request = self.native_request(
            actor,
            source,
            &check,
            CalendarAcquisitionMode::ReadEvents,
            read.starts_at.timestamp_millis(),
            read.ends_at.timestamp_millis(),
        )?;
        if request.host_epoch != before.generation {
            return Err(AgentFailure::StaleContext);
        }
        let result = self
            .native_call(
                actor,
                source,
                request,
                &RemoteCallWindow {
                    deadline: check.deadline,
                    cancellation: check.cancellation.clone(),
                },
            )
            .await?;
        let observed_at = Utc::now();
        let after = self.inspect_calendar(actor, source, check.clone()).await?;
        if before != native_stamp(&result) || before != after {
            return Err(AgentFailure::StaleContext);
        }
        let records = result.batches.iter().try_fold(0usize, |total, batch| {
            total
                .checked_add(batch.records.len())
                .ok_or(AgentFailure::BudgetExceeded)
        })?;
        if records > MAX_NATIVE_RECORDS
            || serde_json::to_vec(&result.batches)
                .map_err(|_| AgentFailure::InvalidInput)?
                .len()
                > MAX_NATIVE_BYTES
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        let batches = super::calendar_product::normalize_native_batches(
            result.batches,
            &check.calendar_ids,
            observed_at,
        )?
        .into_iter()
        .map(|outcome| match outcome {
            CalendarResourceOutcome::Complete {
                calendar_id,
                records,
                ..
            } => CalendarBatch {
                calendar_id,
                records,
                failure: None,
            },
            CalendarResourceOutcome::Failed {
                calendar_id,
                reason,
                ..
            } => CalendarBatch {
                calendar_id,
                records: Vec::new(),
                failure: Some(reason),
            },
        })
        .collect();
        check_window(check.deadline, &check.cancellation)?;
        Ok(CalendarObservation {
            stamp: before,
            observed_at,
            batches,
        })
    }
}

impl ExpertSourceTransport for ExpertSourceAdapter {
    fn remote<'a>(
        &'a self,
        actor: &'a OwnerActor,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<Option<ExpertRemoteSource>, AgentFailure>> {
        Box::pin(async move {
            let Some(lease) = self.gateway.acquire_optional(actor, scope).await? else {
                return Ok(None);
            };
            let trust = lease.trust();
            let producer = tokio::select! {
                biased;
                _ = lease.cancellation().cancelled() => return Err(AgentFailure::Cancelled),
                result = scope.run(trust.pinned_producer()) => result?,
            };
            lease.ensure_current()?;
            let binding = lease.credentials().binding();
            if producer.instance_id != binding.producer_instance
                || producer.fingerprint != binding.producer_key_fingerprint
                || producer.audience != binding.producer_audience
            {
                return Err(AgentFailure::PolicyDenied);
            }
            let transport = Arc::new(RetainedGatewaySource {
                client: ServerSourceClient::new(lease.credentials().clone()),
                verifier: GatewaySourcePreviewVerifier::new(trust),
                producer,
                window: RemoteCallWindow {
                    deadline: scope.deadline(),
                    cancellation: scope.cancellation().clone(),
                },
                lease,
            });
            transport.producer_identity(&transport.window).await?;
            Ok(Some(ExpertRemoteSource {
                transport: transport.clone(),
                verifier: transport,
            }))
        })
    }

    fn check_calendar<'a>(
        &'a self,
        actor: &'a OwnerActor,
        source: &'a SourceConnection,
        request: CalendarReadAccessRequest,
    ) -> BoxFuture<'a, Result<CalendarReadAccessStamp, AgentFailure>> {
        Box::pin(self.inspect_calendar(actor, source, request))
    }

    fn observe_calendar<'a>(
        &'a self,
        actor: &'a OwnerActor,
        source: &'a SourceConnection,
        request: CalendarObserveRequest,
    ) -> BoxFuture<'a, Result<CalendarObservation, AgentFailure>> {
        Box::pin(self.read_calendar(actor, source, request))
    }
}

/// Credentials, signer, preview verifier and catalog all belong to the same
/// retained ready generation. Every await is fenced by that generation and
/// by the original acquisition scope as well as the individual call window.
struct RetainedGatewaySource {
    lease: ProductGatewayLease,
    client: ServerSourceClient,
    producer: RemoteProducerIdentity,
    verifier: GatewaySourcePreviewVerifier,
    window: RemoteCallWindow,
}

impl RetainedGatewaySource {
    async fn run<T>(
        &self,
        window: &RemoteCallWindow,
        operation: impl Future<Output = Result<T, AgentFailure>>,
    ) -> Result<T, AgentFailure> {
        self.lease.ensure_current()?;
        bounded(&self.window, bounded(window, async {
            let result = tokio::select! {
                biased;
                _ = self.lease.cancellation().cancelled() => return Err(AgentFailure::Cancelled),
                result = async {
                    self.lease.revalidate().await?;
                    let output = operation.await?;
                    self.lease.revalidate().await?;
                    Ok(output)
                } => result,
            };
            self.lease.ensure_current()?;
            result
        })).await
    }

    fn pairing_matches(&self, pairing: RemotePairingIdentity<'_>) -> Result<(), AgentFailure> {
        let credentials = self.lease.credentials();
        if pairing.person_id != credentials.person_id()
            || pairing.device_id != credentials.device_id()
            || pairing.client_id != credentials.client_id()
        {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(())
    }
}

impl RemoteGrantTransport for RetainedGatewaySource {
    fn producer_identity<'a>(
        &'a self,
        window: &'a RemoteCallWindow,
    ) -> BoxFuture<'a, Result<RemoteProducerIdentity, AgentFailure>> {
        Box::pin(self.run(window, async move {
            let client = AuthorizedSourceClient::new(&self.client, self.lease.signer().as_ref());
            let producer = client.producer_identity(window).await?;
            producer_is_pinned(&self.producer, &producer)?;
            Ok(producer)
        }))
    }

    fn view_source_preview<'a>(
        &'a self,
        query: RemoteSourceQuery<'a>,
        window: &'a RemoteCallWindow,
    ) -> BoxFuture<'a, Result<SignedSourcePreview, AgentFailure>> {
        Box::pin(self.run(window, async move {
            let client = AuthorizedSourceClient::new(&self.client, self.lease.signer().as_ref());
            let preview = client.view_source_preview(query, window).await?;
            producer_is_pinned(&self.producer, &preview.producer)?;
            Ok(preview)
        }))
    }
}

impl RemoteViewTransport for RetainedGatewaySource {
    fn read_admitted_view<'a>(
        &'a self,
        read: AdmittedRemoteRead<'a>,
        window: &'a RemoteCallWindow,
    ) -> BoxFuture<'a, Result<serde_json::Value, AgentFailure>> {
        Box::pin(self.run(window, async move {
            self.pairing_matches(read.pairing)?;
            let client = AuthorizedSourceClient::new(&self.client, self.lease.signer().as_ref());
            client.read_admitted_view(read, window).await
        }))
    }
}

impl ExpertRemoteTransport for RetainedGatewaySource {
    fn client_id(&self) -> &str {
        self.lease.credentials().client_id()
    }
    fn producer(&self) -> &RemoteProducerIdentity {
        &self.producer
    }
    fn gateway_binding(&self) -> &floe_access::VerifiedGatewayBinding {
        self.lease.credentials().binding()
    }

    fn catalog<'a>(
        &'a self,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<Vec<ConnectorSnapshot>, AgentFailure>> {
        Box::pin(async move {
            let window = RemoteCallWindow {
                deadline: scope.deadline(),
                cancellation: scope.cancellation().clone(),
            };
            self.run(
                &window,
                self.client
                    .observe_source_connections(window.deadline, &window.cancellation),
            )
            .await
        })
    }
}

impl SourcePreviewVerifier for RetainedGatewaySource {
    fn verify<'a>(
        &'a self,
        preview: &'a SignedSourcePreview,
        pairing: RemotePairingIdentity<'a>,
        query: RemoteSourceQuery<'a>,
    ) -> BoxFuture<'a, Result<RemoteViewSourceReference, AgentFailure>> {
        Box::pin(self.run(&self.window, async move {
            self.pairing_matches(pairing)?;
            producer_is_pinned(&self.producer, &preview.producer)?;
            self.verifier.verify(preview, pairing, query).await
        }))
    }
}

fn native_stamp(result: &CalendarAcquisitionResult) -> CalendarReadAccessStamp {
    CalendarReadAccessStamp {
        schema_version: NATIVE_CALENDAR_WIRE_VERSION,
        person_id: result.person_id,
        device_id: result.device_id.clone(),
        provider: result.provider,
        calendar_ids: result.calendar_ids.clone(),
        native_subject_fingerprint: result.native_subject_fingerprint_after.clone(),
        generation: result.host_epoch.clone(),
    }
}

fn validate_inventory(result: &CalendarAcquisitionResult) -> Result<(), AgentFailure> {
    let ids = &result.available_calendar_ids;
    if ids.len() > 256
        || ids.len() != result.available_calendars.len()
        || ids.windows(2).any(|pair| pair[0] >= pair[1])
        || result
            .available_calendars
            .iter()
            .zip(ids)
            .any(|(resource, id)| {
                resource.handle != *id
                    || id.is_empty()
                    || id.len() > 512
                    || id.chars().any(char::is_control)
                    || resource.label.is_empty()
                    || resource.label.len() > 256
                    || resource.label.chars().any(char::is_control)
            })
    {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(())
}

fn fingerprint_valid(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        && value.bytes().any(|byte| byte != b'0')
}

fn check_window(deadline: Instant, cancellation: &Cancellation) -> Result<(), AgentFailure> {
    if cancellation.is_cancelled() {
        return Err(AgentFailure::Cancelled);
    }
    if Instant::now() >= deadline {
        return Err(AgentFailure::DeadlineExceeded);
    }
    Ok(())
}

async fn bounded<T>(
    window: &RemoteCallWindow,
    future: impl Future<Output = Result<T, AgentFailure>>,
) -> Result<T, AgentFailure> {
    check_window(window.deadline, &window.cancellation)?;
    let result = tokio::select! {
        biased;
        _ = window.cancellation.cancelled() => return Err(AgentFailure::Cancelled),
        _ = tokio::time::sleep_until(window.deadline) => return Err(AgentFailure::DeadlineExceeded),
        result = future => result,
    };
    check_window(window.deadline, &window.cancellation)?;
    result
}
