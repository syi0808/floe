use floe_agent_contract::{
    AgentFailure, EngineStep, JournalEvent, PreparedModelPlan, SourceProjectionReview,
    UserInteractionKind, UserInteractionRef, UserInteractionStatus,
};
use floe_agent_runtime::{EngineBlock, EngineBlockage};
use floe_execution::ExecutionScope;
use floe_kernel::{OwnerActor, RunId};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    BlockedRunCommit, ConversationInteraction, ConversationRepository, InteractionOrigin,
    InteractionRequirement, InteractionRequirementKind, InteractionState,
    PriorExhaustion, ReviewPublication, ReviewAuditRecord,
    ReviewedTarget, RunBlockOrigin, RunBlockRecord, RunReceipt,
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
) -> Result<ReviewPublication, AgentFailure> {
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
    let access_reviews = prepared.reviews.iter().map(|access| {
        let requirement = review.blockers.blockers().iter().find(|blocker| blocker.connection_id() == Some(&access.source.source.connection_id()))
            .ok_or(AgentFailure::Conflict)?.clone();
        Ok(crate::SourceReviewLink { reference: access.reference.clone(), requirement })
    }).collect::<Result<Vec<_>, AgentFailure>>()?;
    let record = ReviewAuditRecord {
        person_id: actor.person_id,
        device_id: actor.device_id.clone(),
        session_id: receipt.session_id,
        run_id: receipt.run_id,
        executor_generation: receipt.executor_generation,
        operation_id: review.projection_operation_id,
        evidence: match &origin {
            InteractionOrigin::Projection { .. } => crate::BlockedReviewEvidence::ModelProjection { plan, review, access_reviews: access_reviews.clone() },
            InteractionOrigin::Task { execution, capability_call_id: None } => crate::BlockedReviewEvidence::TaskModelProjection { execution: execution.clone(), plan, review, access_reviews: access_reviews.clone() },
            _ => return Err(AgentFailure::PolicyDenied),
        },
    };
    record.validate()?;
    let expires_at_unix_ms = now_unix_ms
        .checked_add(crate::INTERACTION_PENDING_LIFETIME_MS)
        .ok_or(AgentFailure::InvalidInput)?;
    let mut interactions = Vec::new();
    for link in &access_reviews {
        let requirement = source_requirement(&link.requirement, true);
        let target = ReviewedTarget::SourceReview(link.reference.clone());
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
            audit: record.clone(),
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
    let publication = ReviewPublication {
        record,
        interactions,
    };
    publication.validate()?;
    Ok(publication)
}

pub(crate) async fn build_blocked_run_commit<R: ConversationRepository>(
    repository: &R,
    connections: &floe_connections::ConnectionsService,
    experts: &dyn floe_experts::ExpertsOwner,
    actor: &OwnerActor,
    run_id: RunId,
    blocked: EngineBlock,
    prior_exhaustion: Option<PriorExhaustion>,
    now_unix_ms: i64,
    scope: &ExecutionScope,
) -> Result<BlockedRunCommit, AgentFailure> {
    let receipt = repository.load_receipt(run_id).await?.ok_or(AgentFailure::NotFound)?;
    let journal = repository.load_journal(run_id).await?;
    super::recovery::project_active_journal(&receipt, &journal)?;
    let steps = settled_steps(&journal)?;
    if blocked.report.output.is_some() || blocked.report.steps.iter().any(|step| !steps.contains(step)) {
        return Err(AgentFailure::StorageUnavailable);
    }
    let publications = match blocked.blockage {
        EngineBlockage::ModelProjection { plan, review } => {
            let origin = InteractionOrigin::Projection { run_id, projection_operation_id: review.projection_operation_id, target_digest: review.target_digest };
            vec![prepare_publication(connections, actor, &receipt, plan, review, origin, now_unix_ms, scope).await?]
        }
        EngineBlockage::Delegation { receipt: delegated } => {
            let floe_agent_contract::TaskExecutionEvidence::Admitted(expected) = &delegated.execution else {
                return Err(AgentFailure::PolicyDenied);
            };
            let actual = experts.read_task_execution_receipt(actor, &expected.reference, scope).await?;
            if actual != *expected || actual.snapshot != delegated.snapshot
                || actual.snapshot.principal != receipt.principal || actual.snapshot.parent_run_id.is_none()
                || !journal.iter().any(|entry| matches!(&entry.event, JournalEvent::DelegationResult { receipt } if receipt == &delegated))
            { return Err(AgentFailure::PolicyDenied); }
            let blockage = actual.snapshot.blockage.clone().ok_or(AgentFailure::StorageUnavailable)?;
            match blockage {
                floe_agent_contract::TaskBlockage::ModelProjection { plan, review } => {
                    let origin = InteractionOrigin::Task { execution: actual.reference.clone(), capability_call_id: None };
                    vec![prepare_publication(connections, actor, &receipt, plan, review, origin, now_unix_ms, scope).await?]
                }
                floe_agent_contract::TaskBlockage::SourceRead { tool_call_id, blockers } => {
                    prepare_task_source_publications(connections, actor, &receipt, actual.reference, tool_call_id, blockers, now_unix_ms, scope).await?
                }
                floe_agent_contract::TaskBlockage::Binding { requirement_keys } => {
                    let mut publications = Vec::new();
                    for key in requirement_keys {
                        let operation_id = Uuid::new_v5(&actual.reference.execution.execution_id, &serde_json::to_vec(&("binding-review", &actual.reference, &key)).map_err(|_| AgentFailure::InvalidInput)?);
                        let command = floe_kernel::CommandId::from_uuid(operation_id).ok_or(AgentFailure::InvalidInput)?;
                        let review = experts.prepare_task_binding_review(actor, command, actual.reference.clone(), key.clone(), scope).await?;
                        let target = ReviewedTarget::ExpertBinding(review.review_ref.clone());
                        let requirement = InteractionRequirement { kind: InteractionRequirementKind::ConfigureExpertBinding, source_id: "floe.expert.binding".into(), connection_id: None, consumer: actual.snapshot.agent_id.clone(), purpose: "configuration".into(), inline: false };
                        let audit = ReviewAuditRecord { person_id: actor.person_id, device_id: actor.device_id.clone(), session_id: receipt.session_id, run_id, executor_generation: receipt.executor_generation, operation_id,
                            evidence: crate::BlockedReviewEvidence::ExpertBinding { execution: actual.reference.clone(), requirement_key: key, review: review.review_ref } };
                        let origin = InteractionOrigin::Task { execution: actual.reference.clone(), capability_call_id: None };
                        let interaction = make_interaction(actor, &receipt, origin, audit.clone(), UserInteractionKind::ExpertBinding, requirement, target, now_unix_ms)?;
                        publications.push(ReviewPublication { record: audit, interactions: vec![interaction] });
                    }
                    publications
                }
            }
        }
        // Manager exposes no source tool. A fabricated root source Tool cannot
        // become a Task review by synthesizing a Task execution identity.
        EngineBlockage::SourceRead { .. } => return Err(AgentFailure::PolicyDenied),
    };
    let references = publications.iter().flat_map(|publication| &publication.interactions)
        .map(|record| UserInteractionRef { interaction_id: record.id, kind: record.kind, status: UserInteractionStatus::Pending }).collect::<Vec<_>>();
    let links = publications.iter().flat_map(|publication| &publication.interactions).map(|interaction| crate::BlockedInteractionLink {
        interaction_id: interaction.id,
        origin: RunBlockOrigin { session_id: receipt.session_id, person_id: actor.person_id, device_id: actor.device_id.clone(), run_id, executor_generation: receipt.executor_generation, origin: interaction.origin.clone() },
        target: interaction.target.clone(),
    }).collect();
    let group_bytes = serde_json::to_vec(&("floe.conversation.blocked-group", publications.iter().map(|p| &p.record).collect::<Vec<_>>())).map_err(|_| AgentFailure::InvalidInput)?;
    let coverage = settled_coverage(&steps)?;
    let commit = BlockedRunCommit { run_id, session_id: receipt.session_id, person_id: actor.person_id,
        expected_session_revision: receipt.session_revision, expected_aggregate_revision: receipt.aggregate_revision,
        expected_journal_revision: journal.last().map_or(0, |entry| entry.revision), executor_generation: receipt.executor_generation,
        terminal: RunTerminal { state: RunState::Blocked, output: None, steps, coverage, issue: None,
            blocked: Some(RunBlockRecord { review_group_id: Uuid::new_v5(&run_id.as_uuid(), &group_bytes), interactions: links, prior_exhaustion }), interactions: references }, publications };
    commit.validate()?;
    Ok(commit)
}

fn make_interaction(actor: &OwnerActor, receipt: &RunReceipt, origin: InteractionOrigin, audit: ReviewAuditRecord,
    kind: UserInteractionKind, requirement: InteractionRequirement, target: ReviewedTarget, now: i64)
    -> Result<ConversationInteraction, AgentFailure>
{
    let requirement_digest = crate::canonical_requirement_digest(&requirement)?;
    let target_digest = crate::canonical_target_digest(&target)?;
    let interaction = ConversationInteraction { id: crate::interaction_publication_id(receipt.run_id, &origin, &requirement_digest, &target_digest)?,
        person_id: actor.person_id, session_id: receipt.session_id, origin_run_id: receipt.run_id, origin_turn_id: receipt.run_id.as_uuid(), origin,
        audit, kind, requirement, requirement_digest, target, target_digest, state: InteractionState::Pending, revision: 1, created_at_unix_ms: now,
        expires_at_unix_ms: now.checked_add(crate::INTERACTION_PENDING_LIFETIME_MS).ok_or(AgentFailure::InvalidInput)? };
    interaction.validate()?; Ok(interaction)
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

async fn prepare_task_source_publications(
    connections: &floe_connections::ConnectionsService, actor: &OwnerActor, receipt: &RunReceipt,
    execution: floe_agent_contract::TaskExecutionReceiptRef, tool_call_id: Uuid,
    blockers: floe_context_contract::SourceAccessBlockers, now: i64, scope: &ExecutionScope,
) -> Result<Vec<ReviewPublication>, AgentFailure> {
    use floe_context_contract::SourceAccessRequirementKind as Reason;
    blockers.validate().map_err(|_| AgentFailure::InvalidInput)?;
    let origin = InteractionOrigin::Task { execution: execution.clone(), capability_call_id: Some(tool_call_id) };
    let mut inline = Vec::new(); let mut publications = Vec::new();
    for blocker in blockers.blockers() {
        if blocker.inline_resolution() && blocker.connection_id().is_some() && blocker.connector_id().is_some()
            && matches!(blocker.reason(), Reason::EnableObserve | Reason::ReviewChangedSource | Reason::ReviewProcessing) {
            inline.push(blocker.clone()); continue;
        }
        if blocker.reason() == Reason::ReviewProcessing { return Err(AgentFailure::PolicyDenied); }
        let requirement = source_requirement(blocker, false);
        let target = crate::NavigationOnlyTarget { destination: match blocker.reason() { Reason::RequestSystemPermission => crate::NavigationDestination::SystemPermission, Reason::SelectResource => crate::NavigationDestination::ResourcePicker, _ => crate::NavigationDestination::ConnectionSettings },
            source_id: requirement.source_id.clone(), connection_id: requirement.connection_id.clone(), consumer: requirement.consumer.clone(), purpose: requirement.purpose.clone() };
        let operation_id = Uuid::new_v5(&execution.execution.execution_id, &serde_json::to_vec(&("navigation", tool_call_id, blocker)).map_err(|_| AgentFailure::InvalidInput)?);
        let audit = ReviewAuditRecord { person_id: actor.person_id, device_id: actor.device_id.clone(), session_id: receipt.session_id, run_id: receipt.run_id,
            executor_generation: receipt.executor_generation, operation_id, evidence: crate::BlockedReviewEvidence::Navigation { execution: execution.clone(), requirement: blocker.clone(), target: target.clone() } };
        let interaction = make_interaction(actor, receipt, origin.clone(), audit.clone(), UserInteractionKind::SourceAccess, requirement, ReviewedTarget::NavigationOnly(target), now)?;
        publications.push(ReviewPublication { record: audit, interactions: vec![interaction] });
    }
    if !inline.is_empty() {
        let blockers = floe_context_contract::SourceAccessBlockers::try_new(inline.clone()).map_err(|_| AgentFailure::InvalidInput)?;
        let digest: [u8; 32] = Sha256::digest(serde_json::to_vec(&(receipt.run_id, &origin, &blockers)).map_err(|_| AgentFailure::InvalidInput)?).into();
        let operation_id = Uuid::new_v5(&execution.execution.execution_id, &digest);
        let prepared = connections.prepare_source_reviews(actor, floe_connections::PrepareSourceReviews { run_id: receipt.run_id, operation_id, target_digest: digest, blockers: blockers.clone() }, scope).await?;
        let expected = inline.iter().filter_map(|b| b.connection_id().map(|id| id.as_str().to_owned())).collect::<std::collections::BTreeSet<_>>();
        let actual = prepared.reviews.iter().map(|r| r.source.source.connection_id().as_str().to_owned()).collect::<std::collections::BTreeSet<_>>();
        if prepared.operation_id != operation_id || prepared.target_digest != digest || expected != actual || actual.len() != prepared.reviews.len() { return Err(AgentFailure::Conflict); }
        let audit = ReviewAuditRecord { person_id: actor.person_id, device_id: actor.device_id.clone(), session_id: receipt.session_id, run_id: receipt.run_id,
            executor_generation: receipt.executor_generation, operation_id, evidence: crate::BlockedReviewEvidence::SourceRead { execution, tool_call_id, blockers, access_reviews: prepared.reviews.iter().map(|review| {
                let requirement = inline.iter().find(|blocker| blocker.connection_id() == Some(&review.source.source.connection_id())).ok_or(AgentFailure::Conflict)?.clone();
                Ok(crate::SourceReviewLink { reference: review.reference.clone(), requirement })
            }).collect::<Result<Vec<_>, AgentFailure>>()? } };
        let mut interactions = Vec::new();
        for review in prepared.reviews {
            review.validate()?;
            if review.person_id != actor.person_id || review.device_id != actor.device_id { return Err(AgentFailure::PolicyDenied); }
            let blocker = inline.iter().find(|b| b.connection_id() == Some(&review.source.source.connection_id())).ok_or(AgentFailure::Conflict)?;
            interactions.push(make_interaction(actor, receipt, origin.clone(), audit.clone(), UserInteractionKind::SourceAccess, source_requirement(blocker, true), ReviewedTarget::SourceReview(review.reference), now)?);
        }
        publications.push(ReviewPublication { record: audit, interactions });
    }
    Ok(publications)
}

fn source_requirement(blocker: &floe_context_contract::SourceAccessRequirement, inline: bool) -> InteractionRequirement {
    InteractionRequirement::from_source(blocker, inline)
}
