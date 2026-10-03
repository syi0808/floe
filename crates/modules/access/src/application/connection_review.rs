use crate::{
    ConnectionReview, DataAccessGrant, ExpectedGrant, GrantAbort, GrantAbortOutcome, GrantCommit,
    GrantCommitKind, GrantCommitReceipt, GrantMutation, GrantOperationReceipt, GrantReceiptQuery,
    GrantRepository, GrantState, ReviewRef, ReviewedView, SourceExpectation,
    SourceReservationEvidence, TrustedConsumerCatalog,
};
use chrono::{DateTime, Duration, Utc};
use floe_context_contract::{
    GrantConsumer, GrantDataCategory, GrantId, GrantOperation, GrantPurpose, GrantScope,
    ProcessingRestriction,
};
use floe_execution::{BoxFuture, ExecutionScope};
use floe_kernel::{AgentFailure, OwnerActor};
use std::sync::Arc;
use uuid::Uuid;

pub trait AccessClock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}
pub struct SystemAccessClock;
impl AccessClock for SystemAccessClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct ViewReviewRequest {
    pub view_id: String,
    pub expected: ExpectedGrant,
    pub consumers: Vec<GrantConsumer>,
    pub purpose: GrantPurpose,
    pub categories: Vec<GrantDataCategory>,
    pub processing: ProcessingRestriction,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct PrepareConnectionReview {
    pub command_id: Uuid,
    pub source: SourceExpectation,
    pub views: Vec<ViewReviewRequest>,
}

pub struct AccessService {
    repository: Arc<dyn GrantRepository>,
    consumers: Arc<dyn TrustedConsumerCatalog>,
    clock: Arc<dyn AccessClock>,
}
impl AccessService {
    pub fn new(
        repository: Arc<dyn GrantRepository>,
        consumers: Arc<dyn TrustedConsumerCatalog>,
        clock: Arc<dyn AccessClock>,
    ) -> Self {
        Self {
            repository,
            consumers,
            clock,
        }
    }
    pub fn prepare_review<'a>(
        &'a self,
        actor: &'a OwnerActor,
        request: PrepareConnectionReview,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<ConnectionReview, AgentFailure>> {
        Box::pin(async move {
            let intent_digest = crate::domain::connection_review::digest(&(
                actor.person_id,
                &actor.device_id,
                &request,
            ))?;
            self.prepare_review_with_intent(actor, request, intent_digest, scope)
                .await
        })
    }
    fn prepare_review_with_intent<'a>(
        &'a self,
        actor: &'a OwnerActor,
        request: PrepareConnectionReview,
        intent_digest: [u8; 32],
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<ConnectionReview, AgentFailure>> {
        Box::pin(async move {
            check(actor, scope)?;
            request.source.validate()?;
            if request.command_id.is_nil()
                || request.source.source.person_id() != actor.person_id
                || request.views.is_empty()
                || request.views.len() > 64
            {
                return Err(AgentFailure::InvalidInput);
            }
            if let Some(review) = self
                .repository
                .find_review(actor.person_id, request.command_id, intent_digest)
                .await?
            {
                return Ok(review);
            }
            request.source.validate_device(&actor.device_id)?;
            let snapshot = self
                .repository
                .snapshot(request.source.source.clone())
                .await?;
            let policy_digest = consumer_policy_digest(self.consumers.as_ref())?;
            let mut views = Vec::with_capacity(request.views.len());
            for requested in request.views {
                if !crate::source_view_ids(request.source.source.connector().as_str())
                    .contains(&requested.view_id.as_str())
                    || floe_context_contract::connection_view_resource(
                        &requested.view_id,
                        &request.source.source.connection_id(),
                    )
                    .map_err(|_| AgentFailure::InvalidInput)?
                        != *requested.expected.resource()
                {
                    return Err(AgentFailure::PolicyDenied);
                }
                if requested.consumers.is_empty()
                    || requested.consumers.len() > floe_context_contract::MAX_CONSUMERS
                {
                    return Err(AgentFailure::PolicyDenied);
                }
                for consumer in &requested.consumers {
                    self.consumers
                        .registrations()
                        .iter()
                        .find(|registration| {
                            &registration.consumer_identity == consumer
                                && registration.manifest_revision > 0
                                && registration.declared_view_capabilities.iter().any(
                                    |capability| {
                                        capability.view_id == requested.view_id
                                            && requested.categories.iter().all(|category| {
                                                capability.categories.contains(category)
                                            })
                                            && capability.purposes.contains(&requested.purpose)
                                    },
                                )
                        })
                        .ok_or(AgentFailure::PolicyDenied)?;
                }
                let scope = GrantScope::try_new(
                    vec![requested.expected.resource().clone()],
                    requested.categories.clone(),
                    vec![GrantOperation::Read],
                    vec![requested.purpose],
                    requested.consumers.clone(),
                    requested.processing.clone(),
                )
                .map_err(|_| AgentFailure::InvalidInput)?;
                let matches = snapshot
                    .grants
                    .iter()
                    .filter(|grant| {
                        grant.state() != GrantState::Revoked
                            && grant
                                .scope()
                                .resources()
                                .contains(requested.expected.resource())
                    })
                    .collect::<Vec<_>>();
                let mut successor = match (&requested.expected, matches.as_slice()) {
                    (ExpectedGrant::Absent { .. }, []) => DataAccessGrant::new(
                        GrantId::new(),
                        snapshot.authority_owner,
                        snapshot.source.clone(),
                        scope.clone(),
                    )
                    .map_err(|_| AgentFailure::InvalidInput)?,
                    (
                        ExpectedGrant::Present {
                            grant_id,
                            authority,
                            ..
                        },
                        [grant],
                    ) if grant.id() == *grant_id && grant.authority() == *authority => {
                        (**grant).clone()
                    }
                    _ => return Err(AgentFailure::Conflict),
                };
                let current_processing =
                    matches!(&requested.expected, ExpectedGrant::Present { .. })
                        .then(|| successor.scope().processing().clone());
                let expected = requested.expected;
                if successor.state() == GrantState::Active {
                    successor
                        .review_active(successor.authority(), scope)
                        .map_err(|_| AgentFailure::Conflict)?;
                } else {
                    successor
                        .activate_review(successor.authority(), scope)
                        .map_err(|_| AgentFailure::Conflict)?;
                }
                views.push(ReviewedView {
                    view_id: requested.view_id,
                    expected,
                    consumers: successor.scope().consumers().to_vec(),
                    purpose: requested.purpose,
                    categories: successor.scope().categories().to_vec(),
                    current_processing,
                    requested_processing: requested.processing,
                    successor,
                });
            }
            let mut review = ConnectionReview {
                reference: ReviewRef {
                    id: request.command_id,
                    revision: 1,
                    digest: [0; 32],
                },
                command_id: request.command_id,
                intent_digest,
                person_id: actor.person_id,
                device_id: actor.device_id.clone(),
                source: request.source,
                expires_at: self.clock.now() + Duration::minutes(15),
                views,
                policy_digest,
            };
            review.reference.digest = review.digest()?;
            review.validate()?;
            let reference = self.repository.store_review(review.clone()).await?;
            self.repository.read_review(reference).await
        })
    }
    pub fn find_projection_source_review<'a>(
        &'a self,
        actor: &'a OwnerActor,
        origin: crate::ProjectionReviewOrigin,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<Option<ConnectionReview>, AgentFailure>> {
        Box::pin(async move {
            check(actor, scope)?;
            origin.validate()?;
            let review = self
                .repository
                .find_review(
                    actor.person_id,
                    origin.command_id()?,
                    origin.intent_digest(actor)?,
                )
                .await?;
            if let Some(review) = &review {
                if review.person_id != actor.person_id
                    || review.device_id != actor.device_id
                    || review.source.source.connection_id() != origin.connection_id
                {
                    return Err(AgentFailure::PolicyDenied);
                }
            }
            Ok(review)
        })
    }
    pub fn prepare_projection_source_review<'a>(
        &'a self,
        actor: &'a OwnerActor,
        origin: crate::ProjectionReviewOrigin,
        source: SourceExpectation,
        requirements: Vec<floe_context_contract::SourceAccessRequirement>,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<ConnectionReview, AgentFailure>> {
        Box::pin(async move {
            check(actor, scope)?;
            let expected_origin = crate::ProjectionReviewOrigin::for_requirements(
                origin.run_id,
                origin.projection_operation_id,
                origin.target_digest,
                origin.connection_id.clone(),
                &requirements,
            )?;
            if expected_origin != origin || source.source.connection_id() != origin.connection_id {
                return Err(AgentFailure::Conflict);
            }
            if let Some(review) = self
                .find_projection_source_review(actor, origin.clone(), scope)
                .await?
            {
                return Ok(review);
            }
            let command_id = origin.command_id()?;
            let intent_digest = origin.intent_digest(actor)?;
            if requirements.is_empty() || requirements.len() > 64 {
                return Err(AgentFailure::InvalidInput);
            }
            let snapshot = self.repository.snapshot(source.source.clone()).await?;
            let mut views: Vec<ViewReviewRequest> = Vec::new();
            for requirement in requirements {
                requirement
                    .validate()
                    .map_err(|_| AgentFailure::InvalidInput)?;
                if requirement.connector_id() != Some(source.source.connector())
                    || requirement.connection_id() != Some(&source.source.connection_id())
                    || requirement
                        .source_authority()
                        .is_some_and(|authority| authority != source.authority)
                    || (!requirement.source_resources().is_empty()
                        && requirement.source_resources() != source.physical_resources)
                    || requirement.resources().len() != 1
                {
                    return Err(AgentFailure::Conflict);
                }
                let resource = requirement.resources()[0].clone();
                let connection_id = source.source.connection_id();
                let view_id = floe_context_contract::split_connection_view_resource(
                    &resource,
                    &connection_id,
                )
                .map_err(|_| AgentFailure::InvalidInput)?
                .to_owned();
                if !crate::source_view_ids(source.source.connector().as_str())
                    .contains(&view_id.as_str())
                {
                    return Err(AgentFailure::PolicyDenied);
                }
                let matching = snapshot
                    .grants
                    .iter()
                    .filter(|grant| {
                        grant.state() != GrantState::Revoked
                            && grant.scope().resources().contains(&resource)
                    })
                    .collect::<Vec<_>>();
                let (expected, consumers, categories, mut processing) =
                    match (requirement.observed_grant(), matching.as_slice()) {
                        (Some(observed), [grant])
                            if observed.grant_id() == grant.id()
                                && observed.authority() == grant.authority() =>
                        {
                            if !grant.scope().consumers().contains(requirement.consumer())
                                || !grant.scope().purposes().contains(&requirement.purpose())
                            {
                                return Err(AgentFailure::PolicyDenied);
                            }
                            (
                                ExpectedGrant::Present {
                                    grant_id: grant.id(),
                                    authority: grant.authority(),
                                    resource,
                                },
                                grant.scope().consumers().to_vec(),
                                grant.scope().categories().to_vec(),
                                grant.scope().processing().clone(),
                            )
                        }
                        (None, []) => {
                            let capability = crate::trusted_view_capability(&view_id)?;
                            let mut consumers = self
                                .consumers
                                .registrations()
                                .iter()
                                .filter(|registration| {
                                    registration
                                        .declared_view_capabilities
                                        .iter()
                                        .any(|declared| declared.view_id == view_id)
                                })
                                .map(|registration| registration.consumer_identity.clone())
                                .collect::<Vec<_>>();
                            consumers.sort();
                            consumers.dedup();
                            if !consumers.contains(requirement.consumer()) {
                                return Err(AgentFailure::PolicyDenied);
                            }
                            (
                                ExpectedGrant::Absent { resource },
                                consumers,
                                capability.categories,
                                ProcessingRestriction::DeviceOnly,
                            )
                        }
                        _ => return Err(AgentFailure::Conflict),
                    };
                if let Some(requested) = requirement.requested_processing() {
                    processing = match (&processing, requested) {
                        (
                            ProcessingRestriction::GatewayAllowed {
                                categories: current,
                            },
                            ProcessingRestriction::GatewayAllowed {
                                categories: requested,
                            },
                        ) => {
                            let mut combined = current.clone();
                            combined.extend_from_slice(requested);
                            combined.sort();
                            combined.dedup();
                            ProcessingRestriction::gateway_allowed(combined)
                                .map_err(|_| AgentFailure::InvalidInput)?
                        }
                        (_, requested) => requested.clone(),
                    };
                }
                let view = ViewReviewRequest {
                    view_id,
                    expected,
                    consumers,
                    purpose: requirement.purpose(),
                    categories,
                    processing,
                };
                if let Some(other) = views
                    .iter_mut()
                    .find(|other| other.expected.resource() == view.expected.resource())
                {
                    if other.expected != view.expected
                        || other.consumers != view.consumers
                        || other.purpose != view.purpose
                        || other.categories != view.categories
                    {
                        return Err(AgentFailure::Conflict);
                    }
                    other.processing = match (&other.processing, &view.processing) {
                        (
                            ProcessingRestriction::GatewayAllowed { categories: left },
                            ProcessingRestriction::GatewayAllowed { categories: right },
                        ) => {
                            let mut categories = left.clone();
                            categories.extend_from_slice(right);
                            categories.sort();
                            categories.dedup();
                            ProcessingRestriction::gateway_allowed(categories)
                                .map_err(|_| AgentFailure::InvalidInput)?
                        }
                        (ProcessingRestriction::GatewayAllowed { .. }, _) => {
                            other.processing.clone()
                        }
                        (_, requested) => requested.clone(),
                    };
                } else {
                    views.push(view);
                }
            }
            self.prepare_review_with_intent(
                actor,
                PrepareConnectionReview {
                    command_id,
                    source,
                    views,
                },
                intent_digest,
                scope,
            )
            .await
        })
    }
    pub fn find_source_processing_review<'a>(
        &'a self,
        actor: &'a OwnerActor,
        command_id: Uuid,
        source_ref: Uuid,
        expected_revision: u64,
        processing: crate::SourceProcessingChoice,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<Option<ConnectionReview>, AgentFailure>> {
        Box::pin(async move {
            check(actor, scope)?;
            let intent = source_processing_intent(
                actor,
                command_id,
                source_ref,
                expected_revision,
                processing,
            )?;
            let review = self
                .repository
                .find_review(actor.person_id, command_id, intent)
                .await?;
            if let Some(review) = &review {
                review.validate()?;
                if review.person_id != actor.person_id
                    || review.device_id != actor.device_id
                    || review.source.revision != Some(expected_revision)
                {
                    return Err(AgentFailure::Conflict);
                }
            }
            Ok(review)
        })
    }
    pub fn prepare_source_processing_review<'a>(
        &'a self,
        actor: &'a OwnerActor,
        command_id: Uuid,
        source_ref: Uuid,
        source: SourceExpectation,
        processing: crate::SourceProcessingChoice,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<ConnectionReview, AgentFailure>> {
        Box::pin(async move {
            check(actor, scope)?;
            let revision = source.revision.ok_or(AgentFailure::InvalidInput)?;
            if let Some(review) = self
                .find_source_processing_review(
                    actor, command_id, source_ref, revision, processing, scope,
                )
                .await?
            {
                return Ok(review);
            }
            source.validate()?;
            if source.source.person_id() != actor.person_id {
                return Err(AgentFailure::PolicyDenied);
            }
            let snapshot = self.repository.snapshot(source.source.clone()).await?;
            let mut views = Vec::new();
            for view_id in crate::source_view_ids(source.source.connector().as_str()) {
                let resource = floe_context_contract::connection_view_resource(
                    view_id,
                    &source.source.connection_id(),
                )
                .map_err(|_| AgentFailure::InvalidInput)?;
                let matching = snapshot
                    .grants
                    .iter()
                    .filter(|grant| {
                        grant.state() != GrantState::Revoked
                            && grant.scope().resources().contains(&resource)
                    })
                    .collect::<Vec<_>>();
                let expected = match matching.as_slice() {
                    [] => ExpectedGrant::Absent { resource },
                    [grant] => ExpectedGrant::Present {
                        grant_id: grant.id(),
                        authority: grant.authority(),
                        resource,
                    },
                    _ => return Err(AgentFailure::Conflict),
                };
                let capability = crate::trusted_view_capability(view_id)?;
                let mut consumers = self
                    .consumers
                    .registrations()
                    .iter()
                    .filter(|registration| {
                        registration
                            .declared_view_capabilities
                            .iter()
                            .any(|declared| declared.view_id == *view_id)
                    })
                    .map(|registration| registration.consumer_identity.clone())
                    .collect::<Vec<_>>();
                consumers.sort();
                consumers.dedup();
                let requested = match processing {
                    crate::SourceProcessingChoice::DeviceOnly => ProcessingRestriction::DeviceOnly,
                    crate::SourceProcessingChoice::GatewayAllowed => {
                        ProcessingRestriction::gateway_allowed(capability.categories.clone())
                            .map_err(|_| AgentFailure::InvalidInput)?
                    }
                };
                views.push(ViewReviewRequest {
                    view_id: (*view_id).to_owned(),
                    expected,
                    consumers,
                    purpose: GrantPurpose::Assistant,
                    categories: capability.categories,
                    processing: requested,
                });
            }
            let intent =
                source_processing_intent(actor, command_id, source_ref, revision, processing)?;
            self.prepare_review_with_intent(
                actor,
                PrepareConnectionReview {
                    command_id,
                    source,
                    views,
                },
                intent,
                scope,
            )
            .await
        })
    }
    pub fn source_observe_state<'a>(
        &'a self,
        actor: &'a OwnerActor,
        source: &'a floe_context_contract::GrantSourceBinding,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<crate::SourceObserveState, AgentFailure>> {
        Box::pin(async move {
            check(actor, scope)?;
            if source.person_id() != actor.person_id {
                return Err(AgentFailure::PolicyDenied);
            }
            let snapshot = self.repository.snapshot(source.clone()).await?;
            let mut views = Vec::new();
            for view_id in crate::source_view_ids(source.connector().as_str()) {
                let resource = floe_context_contract::connection_view_resource(
                    view_id,
                    &source.connection_id(),
                )
                .map_err(|_| AgentFailure::InvalidInput)?;
                let matching = snapshot
                    .grants
                    .iter()
                    .filter(|grant| {
                        grant.state() != GrantState::Revoked
                            && grant.scope().resources().contains(&resource)
                    })
                    .collect::<Vec<_>>();
                let (state, processing) = match matching.as_slice() {
                    [] => (crate::SourceObserveViewState::Absent, None),
                    [grant] => (
                        if grant.review_required() {
                            crate::SourceObserveViewState::ReviewRequired
                        } else if grant.state() == GrantState::Active {
                            crate::SourceObserveViewState::Active
                        } else {
                            crate::SourceObserveViewState::Paused
                        },
                        Some(grant.scope().processing().clone()),
                    ),
                    _ => return Err(AgentFailure::PolicyDenied),
                };
                views.push(crate::SourceObserveView {
                    view_id: (*view_id).to_owned(),
                    state,
                    processing,
                });
            }
            use crate::{SourceObserveStatus as Status, SourceObserveViewState as State};
            let status = if views.is_empty() || views.iter().all(|view| view.state == State::Absent)
            {
                Status::Disabled
            } else if views.iter().all(|view| view.state == State::Active) {
                Status::Enabled
            } else if views.iter().all(|view| view.state == State::Paused) {
                Status::Paused
            } else {
                Status::ReviewRequired
            };
            Ok(crate::SourceObserveState { status, views })
        })
    }
    pub fn inspect_review<'a>(
        &'a self,
        actor: &'a OwnerActor,
        reference: ReviewRef,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<ConnectionReview, AgentFailure>> {
        Box::pin(async move {
            check(actor, scope)?;
            let review = self.repository.read_review(reference).await?;
            review.validate()?;
            if review.person_id != actor.person_id || review.device_id != actor.device_id {
                return Err(AgentFailure::PolicyDenied);
            }
            Ok(review)
        })
    }
    fn validate_review_actor(
        &self,
        actor: &OwnerActor,
        review: &ConnectionReview,
    ) -> Result<(), AgentFailure> {
        review.validate()?;
        if review.person_id != actor.person_id
            || review.device_id != actor.device_id
            || review.expires_at <= self.clock.now()
            || review.policy_digest != consumer_policy_digest(self.consumers.as_ref())?
        {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(())
    }
    pub fn replay_review_source<'a>(
        &'a self,
        actor: &'a OwnerActor,
        reference: ReviewRef,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<SourceExpectation, AgentFailure>> {
        Box::pin(async move {
            check(actor, scope)?;
            let review = self.repository.read_review(reference).await?;
            review.validate()?;
            if review.person_id != actor.person_id || review.device_id != actor.device_id {
                return Err(AgentFailure::PolicyDenied);
            }
            Ok(review.source)
        })
    }
    pub fn apply_review<'a>(
        &'a self,
        actor: &'a OwnerActor,
        reference: ReviewRef,
        reservation: SourceReservationEvidence,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<GrantCommitReceipt, AgentFailure>> {
        Box::pin(async move {
            check(actor, scope)?;
            reservation.validate()?;
            if reservation.device_id != actor.device_id
                || reservation.source.source.person_id() != actor.person_id
            {
                return Err(AgentFailure::PolicyDenied);
            }
            if let Some(receipt) = self
                .repository
                .receipt(GrantReceiptQuery {
                    identity: reservation.identity(),
                })
                .await?
            {
                return match receipt {
                    GrantOperationReceipt::Committed(receipt)
                        if receipt.kind == (GrantCommitKind::Reviewed { review: reference }) =>
                    {
                        Ok(receipt)
                    }
                    _ => Err(AgentFailure::Conflict),
                };
            }
            let review = self.inspect_review(actor, reference.clone(), scope).await?;
            self.validate_review_actor(actor, &review)?;
            if review.source != reservation.source {
                return Err(AgentFailure::Conflict);
            }
            let mutations = review
                .views
                .iter()
                .map(|view| GrantMutation {
                    expected: view.expected.clone(),
                    successor: view.successor.clone(),
                })
                .collect();
            self.repository
                .commit(GrantCommit {
                    reservation,
                    kind: GrantCommitKind::Reviewed { review: reference },
                    mutations,
                })
                .await
        })
    }
    pub fn disconnect<'a>(
        &'a self,
        actor: &'a OwnerActor,
        reservation: SourceReservationEvidence,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<GrantCommitReceipt, AgentFailure>> {
        Box::pin(async move {
            check(actor, scope)?;
            reservation.validate()?;
            if reservation.device_id != actor.device_id
                || reservation.source.source.person_id() != actor.person_id
            {
                return Err(AgentFailure::PolicyDenied);
            }
            if let Some(receipt) = self
                .repository
                .receipt(GrantReceiptQuery {
                    identity: reservation.identity(),
                })
                .await?
            {
                return match receipt {
                    GrantOperationReceipt::Committed(receipt)
                        if receipt.kind == GrantCommitKind::Disconnect =>
                    {
                        Ok(receipt)
                    }
                    _ => Err(AgentFailure::Conflict),
                };
            }
            let snapshot = self
                .repository
                .snapshot(reservation.source.source.clone())
                .await?;
            let mut mutations = Vec::new();
            for grant in snapshot
                .grants
                .into_iter()
                .filter(|grant| grant.state() != GrantState::Revoked)
            {
                let [resource] = grant.scope().resources() else {
                    return Err(AgentFailure::InvalidInput);
                };
                let expected = ExpectedGrant::Present {
                    grant_id: grant.id(),
                    authority: grant.authority(),
                    resource: resource.clone(),
                };
                let mut successor = grant;
                successor
                    .revoke(successor.authority())
                    .map_err(|_| AgentFailure::Conflict)?;
                mutations.push(GrantMutation {
                    expected,
                    successor,
                });
            }
            self.repository
                .commit(GrantCommit {
                    reservation,
                    kind: GrantCommitKind::Disconnect,
                    mutations,
                })
                .await
        })
    }
    pub fn pause_observe<'a>(
        &'a self,
        actor: &'a OwnerActor,
        reservation: SourceReservationEvidence,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<GrantCommitReceipt, AgentFailure>> {
        Box::pin(async move {
            check(actor, scope)?;
            reservation.validate()?;
            if reservation.device_id != actor.device_id
                || reservation.source.source.person_id() != actor.person_id
            {
                return Err(AgentFailure::PolicyDenied);
            }
            if let Some(receipt) = self
                .repository
                .receipt(GrantReceiptQuery {
                    identity: reservation.identity(),
                })
                .await?
            {
                return match receipt {
                    GrantOperationReceipt::Committed(receipt)
                        if receipt.kind == GrantCommitKind::PauseObserve =>
                    {
                        Ok(receipt)
                    }
                    _ => Err(AgentFailure::Conflict),
                };
            }
            let snapshot = self
                .repository
                .snapshot(reservation.source.source.clone())
                .await?;
            let mut mutations = Vec::new();
            for grant in snapshot
                .grants
                .into_iter()
                .filter(|grant| grant.state() == GrantState::Active)
            {
                let [resource] = grant.scope().resources() else {
                    return Err(AgentFailure::InvalidInput);
                };
                let expected = ExpectedGrant::Present {
                    grant_id: grant.id(),
                    authority: grant.authority(),
                    resource: resource.clone(),
                };
                let mut successor = grant;
                successor
                    .pause(successor.authority())
                    .map_err(|_| AgentFailure::Conflict)?;
                mutations.push(GrantMutation {
                    expected,
                    successor,
                });
            }
            self.repository
                .commit(GrantCommit {
                    reservation,
                    kind: GrantCommitKind::PauseObserve,
                    mutations,
                })
                .await
        })
    }
    pub fn invalidate_source<'a>(
        &'a self,
        actor: &'a OwnerActor,
        reservation: SourceReservationEvidence,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<GrantCommitReceipt, AgentFailure>> {
        Box::pin(async move {
            check(actor, scope)?;
            reservation.validate()?;
            if reservation.device_id != actor.device_id
                || reservation.source.source.person_id() != actor.person_id
            {
                return Err(AgentFailure::PolicyDenied);
            }
            if let Some(receipt) = self
                .repository
                .receipt(GrantReceiptQuery {
                    identity: reservation.identity(),
                })
                .await?
            {
                return match receipt {
                    GrantOperationReceipt::Committed(receipt)
                        if receipt.kind == GrantCommitKind::InvalidateSource =>
                    {
                        Ok(receipt)
                    }
                    _ => Err(AgentFailure::Conflict),
                };
            }
            let snapshot = self
                .repository
                .snapshot(reservation.source.source.clone())
                .await?;
            let mut mutations = Vec::new();
            for grant in snapshot
                .grants
                .into_iter()
                .filter(|grant| grant.state() != GrantState::Revoked)
            {
                let [resource] = grant.scope().resources() else {
                    return Err(AgentFailure::InvalidInput);
                };
                let expected = ExpectedGrant::Present {
                    grant_id: grant.id(),
                    authority: grant.authority(),
                    resource: resource.clone(),
                };
                let mut successor = grant;
                successor
                    .invalidate_source(successor.authority())
                    .map_err(|_| AgentFailure::Conflict)?;
                mutations.push(GrantMutation {
                    expected,
                    successor,
                });
            }
            self.repository
                .commit(GrantCommit {
                    reservation,
                    kind: GrantCommitKind::InvalidateSource,
                    mutations,
                })
                .await
        })
    }
    pub fn receipt<'a>(
        &'a self,
        actor: &'a OwnerActor,
        query: GrantReceiptQuery,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<Option<GrantOperationReceipt>, AgentFailure>> {
        Box::pin(async move {
            check(actor, scope)?;
            if query.identity.device_id != actor.device_id
                || query.identity.source.person_id() != actor.person_id
            {
                return Err(AgentFailure::PolicyDenied);
            }
            self.repository.receipt(query).await
        })
    }
    pub fn abort<'a>(
        &'a self,
        actor: &'a OwnerActor,
        command: GrantAbort,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<GrantAbortOutcome, AgentFailure>> {
        Box::pin(async move {
            check(actor, scope)?;
            if command.identity.device_id != actor.device_id
                || command.identity.source.person_id() != actor.person_id
            {
                return Err(AgentFailure::PolicyDenied);
            }
            self.repository.abort(command).await
        })
    }
}
fn check(actor: &OwnerActor, scope: &ExecutionScope) -> Result<(), AgentFailure> {
    actor.validate()?;
    if scope.cancellation().is_cancelled() {
        return Err(AgentFailure::Cancelled);
    }
    if tokio::time::Instant::now() >= scope.deadline() {
        return Err(AgentFailure::DeadlineExceeded);
    }
    Ok(())
}

fn source_processing_intent(
    actor: &OwnerActor,
    command_id: Uuid,
    source_ref: Uuid,
    revision: u64,
    processing: crate::SourceProcessingChoice,
) -> Result<[u8; 32], AgentFailure> {
    if command_id.is_nil() || source_ref.is_nil() || revision == 0 || revision > i64::MAX as u64 {
        return Err(AgentFailure::InvalidInput);
    }
    crate::digest(&(
        "floe.access.source-processing-review.v1",
        actor.person_id,
        &actor.device_id,
        command_id,
        source_ref,
        revision,
        processing,
    ))
}

fn consumer_policy_digest(catalog: &dyn TrustedConsumerCatalog) -> Result<[u8; 32], AgentFailure> {
    let mut registrations = catalog.registrations().to_vec();
    if registrations.is_empty() || registrations.len() > 128 {
        return Err(AgentFailure::PolicyDenied);
    }
    registrations.sort_by(|left, right| left.consumer_identity.cmp(&right.consumer_identity));
    for index in 0..registrations.len() {
        if index > 0
            && registrations[index - 1].consumer_identity == registrations[index].consumer_identity
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let registration = &mut registrations[index];
        if registration.package_identity.is_empty()
            || registration.package_identity.len() > 256
            || registration.manifest_revision == 0
            || registration.declared_view_capabilities.is_empty()
            || registration.declared_view_capabilities.len() > 64
        {
            return Err(AgentFailure::PolicyDenied);
        }
        registration
            .declared_view_capabilities
            .sort_by(|left, right| left.view_id.cmp(&right.view_id));
        for index in 0..registration.declared_view_capabilities.len() {
            if index > 0
                && registration.declared_view_capabilities[index - 1].view_id
                    == registration.declared_view_capabilities[index].view_id
            {
                return Err(AgentFailure::PolicyDenied);
            }
            let capability = &mut registration.declared_view_capabilities[index];
            capability.categories.sort();
            capability.purposes.sort();
            if capability.view_id.is_empty()
                || capability.view_id.len() > 128
                || capability.categories.is_empty()
                || capability.purposes.is_empty()
                || capability
                    .categories
                    .windows(2)
                    .any(|pair| pair[0] == pair[1])
                || capability
                    .purposes
                    .windows(2)
                    .any(|pair| pair[0] == pair[1])
            {
                return Err(AgentFailure::PolicyDenied);
            }
        }
    }
    crate::domain::connection_review::digest(&registrations)
}
