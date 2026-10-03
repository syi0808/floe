use floe_agent_contract::{
    AgentFailure, EngineStep, JournalEvent, PreparedModelPlan, SourceProjectionReview,
    UserInteractionKind, UserInteractionRef, UserInteractionStatus,
};
use floe_agent_runtime::EngineSourceReview;
use floe_execution::ExecutionScope;
use floe_kernel::{OwnerActor, RunId};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    BlockedRunCommit, ConversationInteraction, ConversationRepository, InteractionOrigin,
    InteractionRepository, InteractionRequirement, InteractionRequirementKind, InteractionState,
    PriorExhaustion, ProjectionReviewPublication, ProjectionReviewRecord,
    PublishTaskProjectionReview, ReviewedTarget, RunBlockOrigin, RunBlockRecord, RunReceipt,
    RunState, RunTerminal,
};

async fn prepare_publication(
    connections: &floe_connections::ConnectionsService,
    actor: &OwnerActor,
    receipt: &RunReceipt,
    plan: PreparedModelPlan,
    review: SourceProjectionReview,
    origin: InteractionOrigin,
    now_unix_ms: i64,
    scope: &ExecutionScope,
) -> Result<ProjectionReviewPublication, AgentFailure> {
    actor.validate()?;
    receipt.validate()?;
    plan.validate()?;
    review.validate()?;
    origin.validate()?;
    if receipt.state != RunState::Working
        || receipt.principal != actor.person_id.to_string()
        || receipt.device_id != actor.device_id
        || plan.principal != receipt.principal
        || plan.device_id != receipt.device_id
        || now_unix_ms < 0
    {
        return Err(AgentFailure::PolicyDenied);
    }
    let actual: [u8; 32] = Sha256::digest(
        serde_json::to_vec(&(&plan, review.projection_operation_id, &review.blockers))
            .map_err(|_| AgentFailure::InvalidInput)?,
    )
    .into();
    if actual != review.target_digest {
        return Err(AgentFailure::PolicyDenied);
    }
    let prepared = connections
        .prepare_projection_reviews(
            actor,
            floe_connections::PrepareProjectionReviews {
                run_id: receipt.run_id,
                projection_operation_id: review.projection_operation_id,
                target_digest: review.target_digest,
                blockers: review.blockers.clone(),
            },
            scope,
        )
        .await?;
    if prepared.projection_operation_id != review.projection_operation_id
        || prepared.target_digest != review.target_digest
    {
        return Err(AgentFailure::Conflict);
    }
    let expected_connections = review
        .blockers
        .blockers()
        .iter()
        .map(|blocker| {
            blocker
                .connection_id()
                .map(|id| id.as_str().to_owned())
                .ok_or(AgentFailure::Conflict)
        })
        .collect::<Result<std::collections::BTreeSet<_>, _>>()?;
    let actual_connections = prepared
        .reviews
        .iter()
        .map(|review| review.source.source.connection_id().as_str().to_owned())
        .collect::<std::collections::BTreeSet<_>>();
    if expected_connections != actual_connections
        || actual_connections.len() != prepared.reviews.len()
    {
        return Err(AgentFailure::Conflict);
    }
    for access_review in &prepared.reviews {
        access_review.validate()?;
        if access_review.person_id != actor.person_id || access_review.device_id != actor.device_id
        {
            return Err(AgentFailure::PolicyDenied);
        }
    }
    let record = ProjectionReviewRecord {
        person_id: actor.person_id,
        device_id: actor.device_id.clone(),
        session_id: receipt.session_id,
        run_id: receipt.run_id,
        executor_generation: receipt.executor_generation,
        plan,
        review,
        access_reviews: prepared
            .reviews
            .iter()
            .map(|review| review.reference.clone())
            .collect(),
    };
    record.validate()?;
    let expires_at_unix_ms = now_unix_ms
        .checked_add(crate::INTERACTION_PENDING_LIFETIME_MS)
        .ok_or(AgentFailure::InvalidInput)?;
    let mut interactions = Vec::new();
    for (access_review, reference) in prepared.reviews.iter().zip(&record.access_reviews) {
        let blocker = record
            .review
            .blockers
            .blockers()
            .iter()
            .find(|blocker| {
                blocker.connection_id() == Some(&access_review.source.source.connection_id())
            })
            .ok_or(AgentFailure::Conflict)?;
        let requirement = InteractionRequirement {
            kind: InteractionRequirementKind::ReviewProcessing,
            source_id: blocker.source_id().to_owned(),
            connection_id: blocker.connection_id().map(|id| id.as_str().to_owned()),
            consumer: blocker.consumer().identifier().to_owned(),
            purpose: match blocker.purpose() {
                floe_context_contract::GrantPurpose::Assistant => "assistant",
                floe_context_contract::GrantPurpose::Scheduling => "scheduling",
                floe_context_contract::GrantPurpose::Summarization => "summarization",
            }
            .into(),
            inline: true,
        };
        let target = ReviewedTarget::SourceReview(reference.clone());
        let requirement_digest = crate::canonical_requirement_digest(&requirement)?;
        let target_digest = crate::canonical_target_digest(&target)?;
        let interaction = ConversationInteraction {
            id: crate::interaction_publication_id(
                receipt.run_id,
                &origin,
                &requirement_digest,
                &target_digest,
            )?,
            person_id: actor.person_id,
            session_id: receipt.session_id,
            origin_run_id: receipt.run_id,
            origin_turn_id: receipt.run_id.as_uuid(),
            origin: origin.clone(),
            projection: Some(record.clone()),
            kind: UserInteractionKind::SourceAccess,
            requirement,
            requirement_digest,
            target,
            target_digest,
            state: InteractionState::Pending,
            revision: 1,
            created_at_unix_ms: now_unix_ms,
            expires_at_unix_ms,
        };
        interaction.validate()?;
        interactions.push(interaction);
    }
    let publication = ProjectionReviewPublication {
        record,
        interactions,
    };
    publication.validate()?;
    Ok(publication)
}

pub async fn publish_task_projection_review(
    runs: &dyn ConversationRepository,
    interactions: &dyn InteractionRepository,
    connections: &floe_connections::ConnectionsService,
    request: PublishTaskProjectionReview,
    scope: &ExecutionScope,
) -> Result<Vec<UserInteractionRef>, AgentFailure> {
    let receipt = runs
        .load_receipt(request.origin_run_id)
        .await?
        .ok_or(AgentFailure::NotFound)?;
    if receipt.session_id != request.session_id {
        return Err(AgentFailure::Conflict);
    }
    let origin = InteractionOrigin::Task {
        task_id: request.task_id,
        capability_call_id: request.capability_call_id,
    };
    if !super::interactions::origin_admitted(
        &runs.load_journal(request.origin_run_id).await?,
        &origin,
    ) {
        return Err(AgentFailure::PolicyDenied);
    }
    let publication = prepare_publication(
        connections,
        &request.actor,
        &receipt,
        request.plan,
        request.review,
        origin,
        request.now_unix_ms,
        scope,
    )
    .await?;
    let mut references = Vec::new();
    for interaction in publication.interactions {
        let recorded = match interactions.publish_interaction(interaction).await? {
            crate::PublishAdmission::Created(recorded)
            | crate::PublishAdmission::Existing(recorded) => recorded,
        };
        references.push(UserInteractionRef {
            interaction_id: recorded.id,
            kind: recorded.kind,
            status: super::task_interactions::interaction_status(&recorded.state),
        });
    }
    Ok(references)
}

pub(crate) async fn build_blocked_run_commit<R: ConversationRepository>(
    repository: &R,
    connections: &floe_connections::ConnectionsService,
    actor: &OwnerActor,
    run_id: RunId,
    blocked: EngineSourceReview,
    prior_exhaustion: Option<PriorExhaustion>,
    now_unix_ms: i64,
    scope: &ExecutionScope,
) -> Result<BlockedRunCommit, AgentFailure> {
    let receipt = repository
        .load_receipt(run_id)
        .await?
        .ok_or(AgentFailure::NotFound)?;
    let journal = repository.load_journal(run_id).await?;
    super::recovery::project_active_journal(&receipt, &journal)?;
    let steps = settled_steps(&journal)?;
    if blocked.report.output.is_some()
        || blocked
            .report
            .steps
            .iter()
            .any(|step| !steps.contains(step))
    {
        return Err(AgentFailure::StorageUnavailable);
    }
    let origin = InteractionOrigin::Projection {
        run_id,
        projection_operation_id: blocked.review.projection_operation_id,
        target_digest: blocked.review.target_digest,
    };
    let publication = prepare_publication(
        connections,
        actor,
        &receipt,
        blocked.plan,
        blocked.review,
        origin.clone(),
        now_unix_ms,
        scope,
    )
    .await?;
    let references = publication
        .interactions
        .iter()
        .map(|record| UserInteractionRef {
            interaction_id: record.id,
            kind: record.kind,
            status: UserInteractionStatus::Pending,
        })
        .collect::<Vec<_>>();
    let coverage = settled_coverage(&steps)?;
    let blocked = RunBlockRecord {
        review_group_id: Uuid::new_v5(
            &run_id.as_uuid(),
            publication.record.review.target_digest.as_slice(),
        ),
        origins: publication
            .record
            .access_reviews
            .iter()
            .map(|_| RunBlockOrigin {
                session_id: receipt.session_id,
                person_id: actor.person_id,
                device_id: actor.device_id.clone(),
                run_id,
                executor_generation: receipt.executor_generation,
                origin: origin.clone(),
            })
            .collect(),
        review_refs: publication.record.access_reviews.clone(),
        interaction_refs: references
            .iter()
            .map(|reference| reference.interaction_id)
            .collect(),
        prior_exhaustion,
    };
    let commit = BlockedRunCommit {
        run_id,
        session_id: receipt.session_id,
        person_id: actor.person_id,
        expected_session_revision: receipt.session_revision,
        expected_aggregate_revision: receipt.aggregate_revision,
        expected_journal_revision: journal.last().map_or(0, |entry| entry.revision),
        executor_generation: receipt.executor_generation,
        terminal: RunTerminal {
            state: RunState::Blocked,
            output: None,
            steps,
            coverage,
            issue: None,
            blocked: Some(blocked),
            interactions: references,
        },
        publications: vec![publication],
    };
    commit.validate()?;
    Ok(commit)
}

pub(crate) fn settled_steps(
    journal: &[crate::JournalEntry],
) -> Result<Vec<EngineStep>, AgentFailure> {
    let mut identities = std::collections::HashSet::new();
    let mut steps = Vec::new();
    for entry in journal {
        let (id, step) = match &entry.event {
            JournalEvent::ToolResult { result } => {
                (result.call_id, EngineStep::Tool(result.clone()))
            }
            JournalEvent::DelegationResult { receipt } => (
                receipt.task_id.as_uuid(),
                EngineStep::Delegation(receipt.clone()),
            ),
            _ => continue,
        };
        if !identities.insert(id) {
            return Err(AgentFailure::StorageUnavailable);
        }
        steps.push(step);
    }
    Ok(steps)
}

pub(crate) fn settled_coverage(
    steps: &[EngineStep],
) -> Result<floe_agent_contract::DependencyCoverage, AgentFailure> {
    let mut coverage = floe_agent_contract::DependencyCoverage::Independent;
    for step in steps {
        let (current, artifacts) = match step {
            EngineStep::Tool(result) => (&result.coverage, &result.artifacts),
            EngineStep::Delegation(receipt) => {
                (&receipt.snapshot.coverage, &receipt.snapshot.artifacts)
            }
            EngineStep::Answer { .. } => return Err(AgentFailure::InvalidInput),
        };
        coverage = coverage
            .merge(current)
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        for artifact in artifacts {
            coverage = coverage
                .merge(&artifact.coverage)
                .map_err(|_| AgentFailure::StorageUnavailable)?;
        }
    }
    Ok(coverage)
}

#[derive(Clone, Debug)]
pub struct PublishTaskSourceReview {
    pub actor: OwnerActor,
    pub session_id: Uuid,
    pub origin_run_id: RunId,
    pub task_id: Uuid,
    pub capability_call_id: Option<Uuid>,
    pub blockers: floe_context_contract::SourceAccessBlockers,
    pub now_unix_ms: i64,
}

/// Publish a source-read blocker under its actual admitted Task, without
/// inventing a model plan or model attempt before a source has been read.
pub async fn publish_task_source_review(
    runs: &dyn ConversationRepository,
    interactions: &dyn InteractionRepository,
    connections: &floe_connections::ConnectionsService,
    request: PublishTaskSourceReview,
    scope: &ExecutionScope,
) -> Result<Vec<UserInteractionRef>, AgentFailure> {
    use floe_context_contract::SourceAccessRequirementKind as Reason;
    request.actor.validate()?;
    request
        .blockers
        .validate()
        .map_err(|_| AgentFailure::InvalidInput)?;
    let receipt = scope
        .run(runs.load_receipt(request.origin_run_id))
        .await?
        .ok_or(AgentFailure::NotFound)?;
    receipt.validate()?;
    let origin = InteractionOrigin::Task {
        task_id: request.task_id,
        capability_call_id: request.capability_call_id,
    };
    if receipt.state != RunState::Working
        || receipt.session_id != request.session_id
        || receipt.principal != request.actor.person_id.to_string()
        || receipt.device_id != request.actor.device_id
        || !super::interactions::origin_admitted(
            &scope.run(runs.load_journal(request.origin_run_id)).await?,
            &origin,
        )
    {
        return Err(AgentFailure::PolicyDenied);
    }
    let mut inline = Vec::new();
    let mut publications = Vec::new();
    for blocker in request.blockers.blockers() {
        let can_review = blocker.inline_resolution()
            && blocker.connection_id().is_some()
            && blocker.connector_id().is_some()
            && matches!(
                blocker.reason(),
                Reason::EnableObserve | Reason::ReviewChangedSource | Reason::ReviewProcessing
            );
        if can_review {
            inline.push(blocker.clone());
            continue;
        }
        if blocker.reason() == Reason::ReviewProcessing {
            return Err(AgentFailure::PolicyDenied);
        }
        let requirement = source_requirement(blocker, false);
        let destination = match blocker.reason() {
            Reason::RequestSystemPermission => crate::NavigationDestination::SystemPermission,
            Reason::SelectResource => crate::NavigationDestination::ResourcePicker,
            _ => crate::NavigationDestination::ConnectionSettings,
        };
        let target = ReviewedTarget::NavigationOnly(crate::NavigationOnlyTarget {
            destination,
            source_id: requirement.source_id.clone(),
            connection_id: requirement.connection_id.clone(),
            consumer: requirement.consumer.clone(),
            purpose: requirement.purpose.clone(),
        });
        publications.push((requirement, target));
    }
    if !inline.is_empty() {
        let blockers = floe_context_contract::SourceAccessBlockers::try_new(inline.clone())
            .map_err(|_| AgentFailure::InvalidInput)?;
        let target_digest: [u8; 32] = Sha256::digest(
            serde_json::to_vec(&(request.origin_run_id, &origin, &blockers))
                .map_err(|_| AgentFailure::InvalidInput)?,
        )
        .into();
        let operation_id = Uuid::new_v5(&request.task_id, &target_digest);
        let prepared = connections
            .prepare_source_reviews(
                &request.actor,
                floe_connections::PrepareSourceReviews {
                    run_id: request.origin_run_id,
                    operation_id,
                    target_digest,
                    blockers,
                },
                scope,
            )
            .await?;
        let expected = inline
            .iter()
            .filter_map(|blocker| blocker.connection_id().map(|id| id.as_str().to_owned()))
            .collect::<std::collections::BTreeSet<_>>();
        let actual = prepared
            .reviews
            .iter()
            .map(|review| review.source.source.connection_id().as_str().to_owned())
            .collect::<std::collections::BTreeSet<_>>();
        if prepared.operation_id != operation_id
            || prepared.target_digest != target_digest
            || expected != actual
            || actual.len() != prepared.reviews.len()
        {
            return Err(AgentFailure::Conflict);
        }
        for review in prepared.reviews {
            review.validate()?;
            if review.person_id != request.actor.person_id
                || review.device_id != request.actor.device_id
            {
                return Err(AgentFailure::PolicyDenied);
            }
            let blocker = inline
                .iter()
                .find(|blocker| {
                    blocker.connection_id() == Some(&review.source.source.connection_id())
                })
                .ok_or(AgentFailure::Conflict)?;
            publications.push((
                source_requirement(blocker, true),
                ReviewedTarget::SourceReview(review.reference),
            ));
        }
    }
    let mut refs = Vec::new();
    for (requirement, target) in publications {
        let recorded = match super::interactions::publish_interaction(
            runs,
            interactions,
            crate::PublishInteractionRequest {
                principal: request.actor.person_id.to_string(),
                session_id: request.session_id,
                origin_run_id: request.origin_run_id,
                origin: origin.clone(),
                kind: UserInteractionKind::SourceAccess,
                requirement,
                target,
            },
            request.now_unix_ms,
        )
        .await?
        {
            crate::PublishAdmission::Created(record)
            | crate::PublishAdmission::Existing(record) => record,
        };
        refs.push(UserInteractionRef {
            interaction_id: recorded.id,
            kind: recorded.kind,
            status: super::task_interactions::interaction_status(&recorded.state),
        });
    }
    Ok(refs)
}

fn source_requirement(
    blocker: &floe_context_contract::SourceAccessRequirement,
    inline: bool,
) -> InteractionRequirement {
    use floe_context_contract::SourceAccessRequirementKind as Reason;
    InteractionRequirement {
        kind: match blocker.reason() {
            Reason::EnableObserve => InteractionRequirementKind::EnableObserve,
            Reason::ReviewChangedSource => InteractionRequirementKind::ReviewChangedSource,
            Reason::RequestSystemPermission => InteractionRequirementKind::RequestSystemPermission,
            Reason::Reconnect => InteractionRequirementKind::Reconnect,
            Reason::ReviewProcessing => InteractionRequirementKind::ReviewProcessing,
            Reason::SelectResource => InteractionRequirementKind::SelectResource,
        },
        source_id: blocker.source_id().to_owned(),
        connection_id: blocker.connection_id().map(|id| id.as_str().to_owned()),
        consumer: blocker.consumer().identifier().to_owned(),
        purpose: match blocker.purpose() {
            floe_context_contract::GrantPurpose::Assistant => "assistant",
            floe_context_contract::GrantPurpose::Scheduling => "scheduling",
            floe_context_contract::GrantPurpose::Summarization => "summarization",
        }
        .into(),
        inline,
    }
}
