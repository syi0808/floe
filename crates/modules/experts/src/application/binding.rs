use std::collections::HashSet;

use floe_agent_contract::{
    AgentFailure, BoxFuture, CommandId, ExecutionScope, OwnerActor, TaskBlockage, TaskState,
    TaskExecutionReceipt, TaskExecutionReceiptRef,
};
use floe_context_contract::SourceSelectionReference;
use floe_kernel::PersonId;
use serde::Serialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    AgentRegistry, BindingCandidateSummary, BindingInspection, BindingInspectionCandidate,
    BindingMutationReceipt, BindingPrepareIdentity, BindingReplacementReceipt, BindingReview,
    BindingReviewAction, BindingReviewDescriptor, Candidate, CandidateAvailability,
    CandidateQuery, CandidateSnapshot, CandidateSourceExpectation,
    ExpertAssignmentSummary, ExpertDirectorySnapshot, ExpertInstallationSummary,
    ExpertRequirementSummary, ExpertSourceRequirement, ExpertsOwner, ExpertsService,
    RegistryCommit, RegistryCommitReceipt, RegistrySnapshot, ReviewedBindingReplacement,
    ReviewedCandidate, TaskRecord, TaskRepository, MAX_REQUIREMENT_SOURCES,
};

const MAX_REVIEW_CANDIDATES: usize = 64;
const BINDING_REVIEW_TTL_MS: i64 = 10 * 60 * 1000;

struct BindingContext {
    registry: AgentRegistry,
    resolved: crate::ResolvedExpert,
    requirement: ExpertSourceRequirement,
    selected: Vec<SourceSelectionReference>,
}

struct CandidateEvidence {
    revision: u64,
    digest: [u8; 32],
    candidates: Vec<Candidate>,
    source_expectations: Vec<CandidateSourceExpectation>,
}

impl<Tasks: TaskRepository + 'static> ExpertsService<Tasks> {
    async fn project_stored_review(&self, actor: &OwnerActor, descriptor: &BindingReviewDescriptor,
        scope: &ExecutionScope) -> Result<BindingReview, AgentFailure>
    {
        let mut review = project_binding_review(descriptor, self.dependencies.clock.now_unix_ms())?;
        if let Some(receipt) = self.dependencies.binding_reviews
            .find_review_replacement(actor, descriptor.review_ref.clone(), scope).await?
        {
            project_binding_mutation_receipt(&receipt, descriptor)?;
            review.allowed_actions = vec![BindingReviewAction::Refresh];
        }
        Ok(review)
    }

    async fn project_stored_replacement(&self, actor: &OwnerActor,
        receipt: &BindingReplacementReceipt, scope: &ExecutionScope)
        -> Result<BindingMutationReceipt, AgentFailure>
    {
        let descriptor = self.dependencies.binding_reviews.get(actor, receipt.review_ref.clone(), scope).await?;
        if descriptor.identity.person_id != actor.person_id || descriptor.identity.device_id != actor.device_id {
            return Err(AgentFailure::PolicyDenied);
        }
        project_binding_mutation_receipt(receipt, &descriptor)
    }

    async fn read_registry(
        &self,
        actor: &OwnerActor,
        scope: &ExecutionScope,
    ) -> Result<RegistrySnapshot, AgentFailure> {
        let snapshot = scope
            .run(self.dependencies.registry.read(actor, scope))
            .await?;
        AgentRegistry::restore(snapshot.clone(), snapshot.instance_id)?;
        Ok(snapshot)
    }

    async fn binding_context(
        &self,
        actor: &OwnerActor,
        assignment_id: Uuid,
        requirement_key: &str,
        expected_binding_revision: Option<u64>,
        admitted: Option<&crate::ExpertAdmissionIdentity>,
        scope: &ExecutionScope,
    ) -> Result<BindingContext, AgentFailure> {
        let snapshot = self.read_registry(actor, scope).await?;
        binding_context_from_snapshot(
            snapshot,
            actor,
            assignment_id,
            requirement_key,
            expected_binding_revision,
            admitted,
        )
    }

    async fn candidate_evidence(
        &self,
        actor: &OwnerActor,
        context: &BindingContext,
        scope: &ExecutionScope,
    ) -> Result<CandidateEvidence, AgentFailure> {
        let query = CandidateQuery {
            actor: actor.clone(),
            package_ref: context.resolved.manifest.package.clone(),
            definition_revision: context.resolved.manifest.definition.definition_revision,
            requirement_key: context.requirement.key.clone(),
            current_candidate_refs: context.selected.clone(),
        };
        let snapshot = scope
            .run(self.dependencies.candidates.inspect(query, scope))
            .await?;
        if snapshot.revision != context.resolved.manifest.definition.definition_revision {
            return Err(AgentFailure::StaleContext);
        }
        candidate_evidence(snapshot, &context.requirement, &context.selected)
    }

    async fn republish_latest(
        &self,
        actor: &OwnerActor,
        acknowledged: &RegistrySnapshot,
        scope: &ExecutionScope,
    ) -> Result<(), AgentFailure> {
        let latest = self.read_registry(actor, scope).await?;
        if latest.instance_id != acknowledged.instance_id || latest.revision < acknowledged.revision {
            return Err(AgentFailure::StorageUnavailable);
        }
        self.republish(&latest)
    }

    async fn prepare_review(
        &self,
        actor: &OwnerActor,
        identity: BindingPrepareIdentity,
        admitted: Option<&crate::ExpertAdmissionIdentity>,
        scope: &ExecutionScope,
    ) -> Result<BindingReview, AgentFailure> {
        let context = self
            .binding_context(
                actor,
                identity.assignment_id,
                &identity.requirement_key,
                Some(identity.expected_binding_revision),
                admitted,
                scope,
            )
            .await?;
        let evidence = self.candidate_evidence(actor, &context, scope).await?;
        let descriptor = prepare_descriptor(
            identity.clone(),
            &context,
            evidence,
            self.dependencies.clock.now_unix_ms(),
        )?;
        let stored = scope
            .run(self.dependencies.binding_reviews.prepare(descriptor, scope))
            .await?;
        if stored.identity != identity {
            return Err(AgentFailure::Conflict);
        }
        validate_stored_descriptor(&stored)?;
        self.project_stored_review(actor, &stored, scope).await
    }

    async fn replay_prepared_review(
        &self,
        actor: &OwnerActor,
        identity: &BindingPrepareIdentity,
        scope: &ExecutionScope,
    ) -> Result<Option<BindingReview>, AgentFailure> {
        let stored = scope
            .run(
                self.dependencies
                    .binding_reviews
                    .find_prepare(actor, identity.command_id, scope),
            )
            .await?;
        let Some(stored) = stored else {
            return Ok(None);
        };
        validate_stored_descriptor(&stored)?;
        if stored.identity != *identity {
            return Err(AgentFailure::Conflict);
        }
        self.project_stored_review(actor, &stored, scope).await.map(Some)
    }

    async fn read_task_record(
        &self,
        actor: &OwnerActor,
        reference: &TaskExecutionReceiptRef,
        scope: &ExecutionScope,
    ) -> Result<TaskRecord, AgentFailure> {
        scope
            .run(
                self.dependencies
                    .tasks
                    .read_execution_record(actor, reference, scope),
            )
            .await
    }

}

impl<Tasks: TaskRepository + 'static> ExpertsOwner for ExpertsService<Tasks> {
    fn recover_delegation<'a>(&'a self, actor: &'a OwnerActor,
        request: &'a floe_agent_contract::DelegationRequest, scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<floe_agent_contract::TaskReceipt, AgentFailure>>
    {
        Box::pin(async move { self.authorize(actor)?;
            self.dependencies.tasks.recover_delegation(actor, request, scope).await })
    }
    fn close_admission(&self) {
        self.closing.store(true, std::sync::atomic::Ordering::Release);
        self.dependencies.tasks.close_admission();
    }

    fn shutdown<'a>(&'a self) -> BoxFuture<'a, Result<(), AgentFailure>> {
        Box::pin(async move { self.close_admission(); self.dependencies.tasks.shutdown().await })
    }

    fn read_task_execution_receipt<'a>(
        &'a self,
        actor: &'a OwnerActor,
        reference: &'a TaskExecutionReceiptRef,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<TaskExecutionReceipt, AgentFailure>> {
        Box::pin(async move {
            self.authorize(actor)?;
            reference.validate()?;
            scope
                .run(
                    self.dependencies
                        .tasks
                        .read_execution_receipt(actor, reference, scope),
                )
                .await
        })
    }

    fn directory<'a>(
        &'a self,
        actor: &'a OwnerActor,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<ExpertDirectorySnapshot, AgentFailure>> {
        Box::pin(async move {
            self.authorize(actor)?;
            let snapshot = self.read_registry(actor, scope).await?;
            project_expert_directory(&snapshot, actor.person_id)
        })
    }

    fn set_installation_enabled<'a>(
        &'a self,
        actor: &'a OwnerActor,
        command_id: CommandId,
        installation_id: Uuid,
        expected_revision: u64,
        enabled: bool,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<ExpertDirectorySnapshot, AgentFailure>> {
        Box::pin(async move {
            self.authorize(actor)?;
            if !command_id.is_valid() || installation_id.is_nil() {
                return Err(AgentFailure::InvalidInput);
            }
            let request_digest = installation_request_digest(
                actor,
                command_id,
                installation_id,
                expected_revision,
                enabled,
            )?;
            if let Some(receipt) = scope
                .run(
                    self.dependencies
                        .registry
                        .find_command(actor, command_id, scope),
                )
                .await?
            {
                validate_registry_commit_receipt(
                    &receipt,
                    actor,
                    command_id,
                    request_digest,
                    expected_revision,
                )?;
                self.republish_latest(actor, &receipt.snapshot, scope).await?;
                return project_expert_directory(&receipt.snapshot, actor.person_id);
            }

            let current = self.read_registry(actor, scope).await?;
            let mut registry =
                AgentRegistry::restore(current.clone(), current.instance_id)?;
            registry.set_installation_enabled(expected_revision, installation_id, enabled)?;
            let next = registry.snapshot();
            let expected_next_revision = expected_revision
                .checked_add(1)
                .ok_or(AgentFailure::BudgetExceeded)?;
            if next.revision != expected_next_revision {
                return Err(AgentFailure::Conflict);
            }
            let receipt = scope
                .run(
                    self.dependencies.registry.commit(
                        RegistryCommit {
                            actor: actor.clone(),
                            command_id,
                            request_digest,
                            expected_revision,
                            next: next.clone(),
                        },
                        scope,
                    ),
                )
                .await?;
            validate_registry_commit_receipt(
                &receipt,
                actor,
                command_id,
                request_digest,
                expected_revision,
            )?;
            if receipt.snapshot != next { return Err(AgentFailure::StorageUnavailable); }
            self.republish_latest(actor, &receipt.snapshot, scope).await?;
            project_expert_directory(&receipt.snapshot, actor.person_id)
        })
    }

    fn inspect_binding<'a>(
        &'a self,
        actor: &'a OwnerActor,
        assignment_id: Uuid,
        requirement_key: String,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<BindingInspection, AgentFailure>> {
        Box::pin(async move {
            self.authorize(actor)?;
            validate_requirement_key(&requirement_key)?;
            if assignment_id.is_nil() {
                return Err(AgentFailure::InvalidInput);
            }
            let context = self
                .binding_context(
                    actor,
                    assignment_id,
                    &requirement_key,
                    None,
                    None,
                    scope,
                )
                .await?;
            let evidence = self.candidate_evidence(actor, &context, scope).await?;
            let candidates = evidence
                .candidates
                .iter()
                .map(|candidate| BindingInspectionCandidate {
                    label: candidate.label.clone(),
                    availability: candidate.availability,
                    selected: context.selected.contains(&candidate.reference),
                })
                .collect();
            Ok(BindingInspection {
                assignment_ref: assignment_id,
                requirement_ref: requirement_key,
                binding_revision: context.resolved.assignment.binding.revision,
                candidates,
            })
        })
    }

    fn prepare_binding_review<'a>(
        &'a self,
        actor: &'a OwnerActor,
        command_id: CommandId,
        assignment_id: Uuid,
        requirement_key: String,
        expected_binding_revision: u64,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<BindingReview, AgentFailure>> {
        Box::pin(async move {
            self.authorize(actor)?;
            if !command_id.is_valid() || assignment_id.is_nil() {
                return Err(AgentFailure::InvalidInput);
            }
            validate_requirement_key(&requirement_key)?;
            let identity = BindingPrepareIdentity {
                command_id,
                person_id: actor.person_id,
                device_id: actor.device_id.clone(),
                assignment_id,
                requirement_key,
                expected_binding_revision,
                task_origin: None,
            };
            if let Some(review) = self
                .replay_prepared_review(actor, &identity, scope)
                .await?
            {
                return Ok(review);
            }
            self.prepare_review(actor, identity, None, scope).await
        })
    }

    fn prepare_task_binding_review<'a>(
        &'a self,
        actor: &'a OwnerActor,
        command_id: CommandId,
        task_receipt: TaskExecutionReceiptRef,
        requirement_key: String,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<BindingReview, AgentFailure>> {
        Box::pin(async move {
            self.authorize(actor)?;
            if !command_id.is_valid() {
                return Err(AgentFailure::InvalidInput);
            }
            task_receipt.validate()?;
            validate_requirement_key(&requirement_key)?;

            // Replay lookup precedes Task-record and all source/catalog reads.
            let saved = scope
                .run(
                    self.dependencies
                        .binding_reviews
                        .find_prepare(actor, command_id, scope),
                )
                .await?;
            if saved.as_ref().is_some_and(|descriptor| {
                descriptor.identity.task_origin.as_ref() != Some(&task_receipt)
            }) {
                return Err(AgentFailure::Conflict);
            }

            let record = self
                .read_task_record(actor, &task_receipt, scope)
                .await?;
            validate_task_binding_record(&record, actor, &task_receipt, &requirement_key)?;
            let identity = task_prepare_identity(
                actor,
                command_id,
                &task_receipt,
                &record,
                requirement_key,
            );
            if let Some(saved) = saved {
                validate_stored_descriptor(&saved)?;
                if saved.identity != identity {
                    return Err(AgentFailure::Conflict);
                }
                return self.project_stored_review(actor, &saved, scope).await;
            }
            self.prepare_review(actor, identity, Some(&record.admission), scope)
                .await
        })
    }

    fn inspect_binding_review<'a>(
        &'a self,
        actor: &'a OwnerActor,
        review_ref: crate::BindingReviewRef,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<BindingReview, AgentFailure>> {
        Box::pin(async move {
            self.authorize(actor)?;
            review_ref.validate()?;
            let descriptor = scope
                .run(
                    self.dependencies
                        .binding_reviews
                        .get(actor, review_ref.clone(), scope),
                )
                .await?;
            validate_stored_descriptor(&descriptor)?;
            if descriptor.review_ref != review_ref
                || descriptor.identity.person_id != actor.person_id
                || descriptor.identity.device_id != actor.device_id
            {
                return Err(AgentFailure::CapabilityDenied);
            }
            self.project_stored_review(actor, &descriptor, scope).await
        })
    }

    fn replace_binding<'a>(
        &'a self,
        actor: &'a OwnerActor,
        command_id: CommandId,
        review_ref: crate::BindingReviewRef,
        expected_binding_revision: u64,
        candidate_ids: Vec<Uuid>,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<ExpertDirectorySnapshot, AgentFailure>> {
        Box::pin(async move {
            self.authorize(actor)?;
            if !command_id.is_valid() {
                return Err(AgentFailure::InvalidInput);
            }
            review_ref.validate()?;
            let mut candidate_refs = candidate_ids;
            candidate_refs.sort_unstable();
            let request_digest = replacement_request_digest(
                actor,
                command_id,
                &review_ref,
                expected_binding_revision,
                &candidate_refs,
            )?;

            let replay = scope
                .run(
                    self.dependencies
                        .binding_reviews
                        .find_replacement(actor, command_id, scope),
                )
                .await?;
            if let Some(receipt) = replay {
                validate_replacement_receipt(
                    &receipt,
                    actor,
                    command_id,
                    &review_ref,
                    request_digest,
                )?;
                self.project_stored_replacement(actor, &receipt, scope).await?;
                self.republish_latest(actor, &receipt.registry.snapshot, scope)
                    .await?;
                return project_expert_directory(
                    &receipt.registry.snapshot,
                    actor.person_id,
                );
            }

            if expected_binding_revision == 0
                || candidate_refs.len() > usize::from(MAX_REQUIREMENT_SOURCES)
                || candidate_refs.iter().any(Uuid::is_nil)
                || candidate_refs.windows(2).any(|pair| pair[0] == pair[1])
            {
                return Err(AgentFailure::InvalidInput);
            }
            let descriptor = scope
                .run(
                    self.dependencies
                        .binding_reviews
                        .get(actor, review_ref.clone(), scope),
                )
                .await?;
            validate_stored_descriptor(&descriptor)?;
            if descriptor.review_ref != review_ref
                || descriptor.identity.person_id != actor.person_id
                || descriptor.identity.device_id != actor.device_id
            {
                return Err(AgentFailure::CapabilityDenied);
            }
            if descriptor.identity.expected_binding_revision != expected_binding_revision {
                return Err(AgentFailure::Conflict);
            }
            if candidate_refs.len() > usize::from(descriptor.requirement.maximum_sources) {
                return Err(AgentFailure::InvalidInput);
            }
            for candidate_ref in &candidate_refs {
                let reviewed = descriptor
                    .candidates
                    .iter()
                    .find(|candidate| candidate.candidate_ref == *candidate_ref)
                    .ok_or(AgentFailure::InvalidInput)?;
                if reviewed.candidate.availability == CandidateAvailability::Unavailable
                    && !reviewed.selected
                {
                    return Err(AgentFailure::InvalidInput);
                }
            }
            if self.dependencies.clock.now_unix_ms() >= descriptor.expires_at_unix_ms {
                return Err(AgentFailure::DeadlineExceeded);
            }

            let admission = crate::ExpertAdmissionIdentity {
                registry_instance_id: descriptor.registry_instance_id,
                assignment_id: descriptor.identity.assignment_id,
                installation_id: descriptor.installation_id,
                package: descriptor.package.clone(),
                definition_revision: descriptor.definition_revision,
            };
            let context = self
                .binding_context(
                    actor,
                    descriptor.identity.assignment_id,
                    &descriptor.identity.requirement_key,
                    Some(expected_binding_revision),
                    Some(&admission),
                    scope,
                )
                .await?;
            if context.resolved.registry_revision != context.registry.revision()
                || context.resolved.assignment.installation_id != descriptor.installation_id
                || context.resolved.manifest.package != descriptor.package
                || context.resolved.manifest.definition.definition_revision
                    != descriptor.definition_revision
                || context.requirement != descriptor.requirement
            {
                return Err(AgentFailure::Conflict);
            }

            let evidence = self.candidate_evidence(actor, &context, scope).await?;
            if evidence.revision != descriptor.catalog_revision
                || evidence.digest != descriptor.catalog_digest
                || evidence.source_expectations != descriptor.source_expectations
                || reviewed_candidates(
                    descriptor.review_ref.id,
                    &evidence,
                    &context.selected,
                )? != descriptor.candidates
            {
                return Err(AgentFailure::StaleContext);
            }

            let mut selected = Vec::with_capacity(candidate_refs.len());
            for candidate_ref in &candidate_refs {
                let reviewed = descriptor
                    .candidates
                    .iter()
                    .find(|candidate| candidate.candidate_ref == *candidate_ref)
                    .ok_or(AgentFailure::InvalidInput)?;
                if reviewed.candidate.availability == CandidateAvailability::Unavailable
                    && !reviewed.selected
                {
                    return Err(AgentFailure::InvalidInput);
                }
                selected.push(reviewed.candidate.reference.clone());
            }
            selected.sort();
            if selected.len() > usize::from(descriptor.requirement.maximum_sources)
                || selected.windows(2).any(|pair| pair[0] == pair[1])
            {
                return Err(AgentFailure::InvalidInput);
            }

            let mut registry = context.registry;
            registry.replace_binding(
                actor.person_id,
                command_id.as_uuid(),
                crate::ExpertBindingCommand {
                    assignment_id: descriptor.identity.assignment_id,
                    package: descriptor.package.clone(),
                    definition_revision: descriptor.definition_revision,
                    requirement_key: descriptor.identity.requirement_key.clone(),
                    expected_binding_revision,
                    selected,
                },
            )?;
            let next = registry.snapshot();
            let committed_at_unix_ms = self.dependencies.clock.now_unix_ms();
            if committed_at_unix_ms >= descriptor.expires_at_unix_ms {
                return Err(AgentFailure::DeadlineExceeded);
            }
            let receipt = scope
                .run(
                    self.dependencies.binding_reviews.commit_replacement(
                        ReviewedBindingReplacement {
                            review_ref: review_ref.clone(),
                            expected_binding_revision,
                            candidate_refs,
                            committed_at_unix_ms,
                            registry: RegistryCommit {
                                actor: actor.clone(),
                                command_id,
                                request_digest,
                                expected_revision: context.resolved.registry_revision,
                                next: next.clone(),
                            },
                        },
                        scope,
                    ),
                )
                .await?;
            validate_replacement_receipt(
                &receipt,
                actor,
                command_id,
                &review_ref,
                request_digest,
            )?;
            self.project_stored_replacement(actor, &receipt, scope).await?;
            if receipt.registry.snapshot != next || receipt.committed_at_unix_ms != committed_at_unix_ms {
                return Err(AgentFailure::StorageUnavailable);
            }
            self.republish_latest(actor, &receipt.registry.snapshot, scope)
                .await?;
            project_expert_directory(&receipt.registry.snapshot, actor.person_id)
        })
    }

    fn binding_operation_receipt<'a>(
        &'a self,
        actor: &'a OwnerActor,
        command_id: CommandId,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<Option<BindingMutationReceipt>, AgentFailure>> {
        Box::pin(async move {
            self.authorize(actor)?;
            if !command_id.is_valid() {
                return Err(AgentFailure::InvalidInput);
            }
            let Some(receipt) = scope
                .run(
                    self.dependencies
                        .binding_reviews
                        .find_replacement(actor, command_id, scope),
                )
                .await?
            else {
                return Ok(None);
            };
            self.project_stored_replacement(actor, &receipt, scope).await.map(Some)
        })
    }

    fn binding_review_receipt<'a>(
        &'a self,
        actor: &'a OwnerActor,
        review_ref: crate::BindingReviewRef,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<Option<BindingMutationReceipt>, AgentFailure>> {
        Box::pin(async move {
            self.authorize(actor)?;
            review_ref.validate()?;
            let Some(receipt) = scope
                .run(
                    self.dependencies
                        .binding_reviews
                        .find_review_replacement(actor, review_ref.clone(), scope),
                )
                .await?
            else {
                return Ok(None);
            };
            if receipt.review_ref != review_ref { return Err(AgentFailure::StorageUnavailable); }
            self.project_stored_replacement(actor, &receipt, scope).await.map(Some)
        })
    }
}

pub fn validate_binding_review_descriptor(
    descriptor: &BindingReviewDescriptor,
) -> Result<(), AgentFailure> {
    descriptor.review_ref.validate()?;
    let identity = &descriptor.identity;
    if !identity.command_id.is_valid()
        || !identity.person_id.is_valid()
        || !valid_device_id(&identity.device_id)
        || identity.assignment_id.is_nil()
        || !valid_identifier(&identity.requirement_key)
        || identity.expected_binding_revision == 0
        || identity
            .task_origin
            .as_ref()
            .is_some_and(|reference| reference.validate().is_err())
        || descriptor.registry_instance_id.is_nil()
        || descriptor.installation_id.is_nil()
        || descriptor.package.kind != floe_agent_contract::PackageKind::Expert
        || !valid_identifier(&descriptor.package.id)
        || !valid_identifier(&descriptor.package.version)
        || descriptor.definition_revision == 0
        || descriptor.requirement.key != identity.requirement_key
        || !valid_identifier(&descriptor.requirement.key)
        || !valid_identifier(&descriptor.requirement.capability)
        || descriptor.requirement.contract_version == 0
        || descriptor.requirement.minimum_sources > descriptor.requirement.maximum_sources
        || descriptor.requirement.maximum_sources > MAX_REQUIREMENT_SOURCES
        || descriptor.candidates.len() > MAX_REVIEW_CANDIDATES
        || descriptor.catalog_digest == [0; 32]
        || descriptor.catalog_revision != descriptor.definition_revision
        || descriptor.created_at_unix_ms < 0
        || descriptor.source_expectations.len() > MAX_REVIEW_CANDIDATES
        || serde_json::to_vec(descriptor).map_err(|_| AgentFailure::InvalidInput)?.len() > 256 * 1024
        || descriptor.expires_at_unix_ms
            != descriptor
                .created_at_unix_ms
                .checked_add(BINDING_REVIEW_TTL_MS)
                .ok_or(AgentFailure::InvalidInput)?
        || deterministic_review_id(identity)? != descriptor.review_ref.id
    {
        return Err(AgentFailure::InvalidInput);
    }

    let mut candidate_refs = HashSet::new();
    let mut candidate_ids = HashSet::new();
    let mut source_refs = Vec::with_capacity(descriptor.candidates.len());
    let mut selected_count = 0usize;
    for (index, reviewed) in descriptor.candidates.iter().enumerate() {
        let candidate = &reviewed.candidate;
        candidate
            .reference
            .validate()
            .map_err(|_| AgentFailure::InvalidInput)?;
        if candidate.label.trim().is_empty() || candidate.label.len() > 256 || candidate.detail.len() > 512
            || candidate.candidate_id != source_candidate_id(&candidate.reference)?
            || candidate.candidate_id.is_empty()
            || !candidate_ids.insert(candidate.candidate_id.as_str())
            || candidate.reference.capability_id != descriptor.requirement.capability
            || candidate.reference.contract_version != descriptor.requirement.contract_version
            || !candidate_refs.insert(reviewed.candidate_ref)
            || reviewed.candidate_ref.is_nil()
            || reviewed.candidate_ref
                != candidate_review_ref(descriptor.review_ref.id, &candidate.reference)?
            || (candidate.availability == CandidateAvailability::Unavailable && !reviewed.selected)
            || (index > 0
                && descriptor.candidates[index - 1].candidate_ref >= reviewed.candidate_ref)
        {
            return Err(AgentFailure::InvalidInput);
        }
        if reviewed.selected {
            selected_count += 1;
            source_refs.push(candidate.reference.clone());
        }
    }
    if selected_count > usize::from(descriptor.requirement.maximum_sources)
        || source_refs.windows(2).any(|pair| pair[0] == pair[1])
    {
        return Err(AgentFailure::InvalidInput);
    }

    let mut expectation_refs = Vec::new();
    for (index, expectation) in descriptor.source_expectations.iter().enumerate() {
        expectation
            .reference
            .validate()
            .map_err(|_| AgentFailure::InvalidInput)?;
        if expectation.source_revision == 0
            || !expectation.source_authority.is_valid()
            || expectation.reference.capability_id != descriptor.requirement.capability
            || expectation.reference.contract_version != descriptor.requirement.contract_version
            || expectation_refs.contains(&expectation.reference)
            || (index > 0
                && descriptor.source_expectations[index - 1].reference
                    >= expectation.reference)
            || !descriptor.candidates.iter().any(|reviewed| {
                reviewed.candidate.reference == expectation.reference
                    && reviewed.candidate.availability == CandidateAvailability::Available
            })
        {
            return Err(AgentFailure::InvalidInput);
        }
        expectation_refs.push(expectation.reference.clone());
    }

    if binding_review_digest(descriptor)? != descriptor.review_ref.digest {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(())
}

pub fn project_binding_review(
    descriptor: &BindingReviewDescriptor,
    now_unix_ms: i64,
) -> Result<BindingReview, AgentFailure> {
    validate_binding_review_descriptor(descriptor)?;
    let allowed_actions = if now_unix_ms >= descriptor.expires_at_unix_ms {
        vec![BindingReviewAction::Refresh]
    } else {
        vec![BindingReviewAction::Replace, BindingReviewAction::Refresh]
    };
    Ok(BindingReview {
        review_ref: descriptor.review_ref.clone(),
        assignment_ref: descriptor.identity.assignment_id,
        requirement_ref: descriptor.identity.requirement_key.clone(),
        binding_revision: descriptor.identity.expected_binding_revision,
        candidate_refs_and_labels: descriptor
            .candidates
            .iter()
            .map(|reviewed| BindingCandidateSummary {
                candidate_ref: reviewed.candidate_ref,
                label: reviewed.candidate.label.clone(),
                availability: reviewed.candidate.availability,
                selected: reviewed.selected,
            })
            .collect(),
        expires_at_unix_ms: descriptor.expires_at_unix_ms,
        allowed_actions,
    })
}

pub fn project_expert_directory(
    snapshot: &RegistrySnapshot,
    person_id: PersonId,
) -> Result<ExpertDirectorySnapshot, AgentFailure> {
    let registry = AgentRegistry::restore(snapshot.clone(), snapshot.instance_id)?;
    let overview = registry.overview(person_id);
    let mut installations = Vec::with_capacity(overview.installations.len());
    for installation in &overview.installations {
        let definition = overview
            .definitions
            .iter()
            .find(|definition| definition.package == installation.package)
            .ok_or(AgentFailure::StorageUnavailable)?;
        installations.push(ExpertInstallationSummary {
            installation_ref: installation.id,
            display_name: definition.name.clone(),
            version: installation.package.version.clone(),
            enabled: installation.enabled,
        });
    }
    installations.sort_by_key(|installation| installation.installation_ref);

    let mut assignments = Vec::with_capacity(overview.assignments.len());
    for assignment in &overview.assignments {
        let installation = overview
            .installations
            .iter()
            .find(|installation| installation.id == assignment.installation_id)
            .ok_or(AgentFailure::StorageUnavailable)?;
        let definition = overview
            .definitions
            .iter()
            .find(|definition| definition.package == installation.package)
            .ok_or(AgentFailure::StorageUnavailable)?;
        let mut requirements = assignment
            .requirements
            .iter()
            .map(|requirement| ExpertRequirementSummary {
                requirement_ref: requirement.key.clone(),
                label: requirement.key.clone(),
                selected_count: requirement.selected_count,
                minimum_sources: requirement.minimum_sources,
            })
            .collect::<Vec<_>>();
        requirements.sort_by(|left, right| left.requirement_ref.cmp(&right.requirement_ref));
        assignments.push(ExpertAssignmentSummary {
            assignment_ref: assignment.id,
            installation_ref: assignment.installation_id,
            display_name: definition.name.clone(),
            enabled: assignment.enabled,
            binding_revision: assignment.binding_revision,
            requirements,
        });
    }
    assignments.sort_by_key(|assignment| assignment.assignment_ref);

    Ok(ExpertDirectorySnapshot {
        revision: overview.revision,
        installations,
        assignments,
    })
}

pub fn binding_review_digest(
    descriptor: &BindingReviewDescriptor,
) -> Result<[u8; 32], AgentFailure> {
    let bytes = serde_json::to_vec(&(
        "floe.expert-binding-review.sha256.v1",
        descriptor.review_ref.id,
        &descriptor.identity,
        descriptor.registry_instance_id,
        descriptor.installation_id,
        &descriptor.package,
        descriptor.definition_revision,
        &descriptor.requirement,
        &descriptor.candidates,
        descriptor.catalog_revision,
        descriptor.catalog_digest,
        &descriptor.source_expectations,
        descriptor.created_at_unix_ms,
        descriptor.expires_at_unix_ms,
    ))
    .map_err(|_| AgentFailure::InvalidInput)?;
    Ok(Sha256::digest(bytes).into())
}

fn binding_context_from_snapshot(
    snapshot: RegistrySnapshot,
    actor: &OwnerActor,
    assignment_id: Uuid,
    requirement_key: &str,
    expected_binding_revision: Option<u64>,
    admitted: Option<&crate::ExpertAdmissionIdentity>,
) -> Result<BindingContext, AgentFailure> {
    let registry = AgentRegistry::restore(snapshot.clone(), snapshot.instance_id)?;
    let resolved = if let Some(admission) = admitted {
        if admission.assignment_id != assignment_id
            || admission.registry_instance_id != snapshot.instance_id
        {
            return Err(AgentFailure::Conflict);
        }
        registry.resolve_admitted(actor.person_id, admission)?
    } else {
        let assignment = snapshot
            .assignments
            .iter()
            .find(|assignment| {
                assignment.id == assignment_id && assignment.person_id == actor.person_id
            })
            .ok_or(AgentFailure::NotFound)?;
        let installation = snapshot
            .installations
            .iter()
            .find(|installation| installation.id == assignment.installation_id)
            .ok_or(AgentFailure::StorageUnavailable)?;
        let manifest = snapshot
            .manifests
            .iter()
            .find(|manifest| manifest.package == installation.package)
            .ok_or(AgentFailure::StorageUnavailable)?;
        registry.resolve_assignment(
            snapshot.instance_id,
            actor.person_id,
            assignment_id,
            &installation.package,
            manifest.definition.definition_revision,
        )?
    };
    if expected_binding_revision.is_some_and(|revision| {
        revision != resolved.assignment.binding.revision
    }) {
        return Err(AgentFailure::Conflict);
    }
    let requirement = resolved
        .manifest
        .source_requirements
        .iter()
        .find(|requirement| requirement.key == requirement_key)
        .cloned()
        .ok_or(AgentFailure::NotFound)?;
    let binding = resolved
        .assignment
        .binding
        .entries
        .iter()
        .find(|binding| binding.requirement_key == requirement_key)
        .ok_or(AgentFailure::StorageUnavailable)?;
    if binding.capability != requirement.capability
        || binding.contract_version != requirement.contract_version
        || binding.selected.len() > usize::from(requirement.maximum_sources)
        || binding
            .selected
            .iter()
            .any(|reference| {
                reference.validate().is_err()
                    || reference.capability_id != requirement.capability
                    || reference.contract_version != requirement.contract_version
            })
        || binding.selected.windows(2).any(|pair| pair[0] >= pair[1])
    {
        return Err(AgentFailure::StorageUnavailable);
    }
    let selected = binding.selected.clone();
    Ok(BindingContext {
        registry,
        resolved,
        requirement,
        selected,
    })
}

fn candidate_evidence(
    snapshot: CandidateSnapshot,
    requirement: &ExpertSourceRequirement,
    selected: &[SourceSelectionReference],
) -> Result<CandidateEvidence, AgentFailure> {
    if snapshot.digest == [0; 32]
        || snapshot.candidates.len() > MAX_REVIEW_CANDIDATES
        || snapshot.source_expectations.len() > MAX_REVIEW_CANDIDATES
        || selected.len() > usize::from(requirement.maximum_sources)
        || selected.windows(2).any(|pair| pair[0] >= pair[1])
        || selected.iter().any(|reference| {
            reference.validate().is_err()
                || reference.capability_id != requirement.capability
                || reference.contract_version != requirement.contract_version
        })
    {
        return Err(AgentFailure::InvalidInput);
    }

    let mut candidates = snapshot.candidates;
    let mut candidate_ids = HashSet::new();
    let mut source_refs = Vec::with_capacity(candidates.len());
    for candidate in &candidates {
        candidate
            .reference
            .validate()
            .map_err(|_| AgentFailure::InvalidInput)?;
        if candidate.label.trim().is_empty() || candidate.label.len() > 256 || candidate.detail.len() > 512
            || candidate.candidate_id != source_candidate_id(&candidate.reference)?
            || !candidate_ids.insert(candidate.candidate_id.as_str())
            || candidate.reference.capability_id != requirement.capability
            || candidate.reference.contract_version != requirement.contract_version
            || source_refs.contains(&candidate.reference)
            || (candidate.availability == CandidateAvailability::Unavailable
                && !selected.contains(&candidate.reference))
        {
            return Err(AgentFailure::InvalidInput);
        }
        source_refs.push(candidate.reference.clone());
    }

    let mut expectations = snapshot.source_expectations;
    let mut expectation_refs = Vec::new();
    for expectation in &expectations {
        expectation
            .reference
            .validate()
            .map_err(|_| AgentFailure::InvalidInput)?;
        if expectation.source_revision == 0
            || !expectation.source_authority.is_valid()
            || expectation.reference.capability_id != requirement.capability
            || expectation.reference.contract_version != requirement.contract_version
            || expectation_refs.contains(&expectation.reference)
            || !candidates.iter().any(|candidate| {
                candidate.reference == expectation.reference
                    && candidate.availability == CandidateAvailability::Available
            })
        {
            return Err(AgentFailure::InvalidInput);
        }
        expectation_refs.push(expectation.reference.clone());
    }

    for reference in selected {
        if source_refs.contains(reference) {
            continue;
        }
        candidates.push(Candidate {
            candidate_id: source_candidate_id(reference)?,
            label: "Saved source".into(),
            detail: "Unavailable; choose another source or remove it".into(),
            availability: CandidateAvailability::Unavailable,
            reference: reference.clone(),
        });
        source_refs.push(reference.clone());
    }
    if candidates.len() > MAX_REVIEW_CANDIDATES {
        return Err(AgentFailure::BudgetExceeded);
    }
    candidates.sort_by(|left, right| left.reference.cmp(&right.reference));
    expectations.sort_by(|left, right| left.reference.cmp(&right.reference));

    Ok(CandidateEvidence {
        revision: snapshot.revision,
        digest: snapshot.digest,
        candidates,
        source_expectations: expectations,
    })
}

fn reviewed_candidates(
    review_id: Uuid,
    evidence: &CandidateEvidence,
    selected: &[SourceSelectionReference],
) -> Result<Vec<ReviewedCandidate>, AgentFailure> {
    let mut reviewed = evidence
        .candidates
        .iter()
        .map(|candidate| {
            Ok(ReviewedCandidate {
                candidate_ref: candidate_review_ref(review_id, &candidate.reference)?,
                candidate: candidate.clone(),
                selected: selected.contains(&candidate.reference),
            })
        })
        .collect::<Result<Vec<_>, AgentFailure>>()?;
    reviewed.sort_by_key(|candidate| candidate.candidate_ref);
    Ok(reviewed)
}

fn prepare_descriptor(
    identity: BindingPrepareIdentity,
    context: &BindingContext,
    evidence: CandidateEvidence,
    created_at_unix_ms: i64,
) -> Result<BindingReviewDescriptor, AgentFailure> {
    let review_id = deterministic_review_id(&identity)?;
    let expires_at_unix_ms = created_at_unix_ms
        .checked_add(BINDING_REVIEW_TTL_MS)
        .ok_or(AgentFailure::BudgetExceeded)?;
    let candidates = reviewed_candidates(review_id, &evidence, &context.selected)?;
    let mut descriptor = BindingReviewDescriptor {
        review_ref: crate::BindingReviewRef {
            id: review_id,
            digest: [0; 32],
        },
        identity,
        registry_instance_id: context.registry.instance_id(),
        installation_id: context.resolved.assignment.installation_id,
        package: context.resolved.manifest.package.clone(),
        definition_revision: context.resolved.manifest.definition.definition_revision,
        requirement: context.requirement.clone(),
        candidates,
        catalog_revision: evidence.revision,
        catalog_digest: evidence.digest,
        source_expectations: evidence.source_expectations,
        created_at_unix_ms,
        expires_at_unix_ms,
    };
    descriptor.review_ref.digest = binding_review_digest(&descriptor)?;
    validate_binding_review_descriptor(&descriptor)?;
    Ok(descriptor)
}

fn task_prepare_identity(
    actor: &OwnerActor,
    command_id: CommandId,
    task_receipt: &TaskExecutionReceiptRef,
    record: &TaskRecord,
    requirement_key: String,
) -> BindingPrepareIdentity {
    BindingPrepareIdentity {
        command_id,
        person_id: actor.person_id,
        device_id: actor.device_id.clone(),
        assignment_id: record.admission.assignment_id,
        requirement_key,
        expected_binding_revision: record.selection.binding_revision,
        task_origin: Some(task_receipt.clone()),
    }
}

fn validate_task_binding_record(
    record: &TaskRecord,
    actor: &OwnerActor,
    task_receipt: &TaskExecutionReceiptRef,
    requirement_key: &str,
) -> Result<(), AgentFailure> {
    if record.snapshot.state != TaskState::Blocked
        || record.snapshot.principal != actor.person_id.to_string()
        || record.device_id != actor.device_id
        || record.receipt.as_ref().map(|receipt| &receipt.reference) != Some(task_receipt)
    {
        return Err(AgentFailure::Conflict);
    }
    let Some(TaskBlockage::Binding { requirement_keys }) = record.snapshot.blockage.as_ref() else {
        return Err(AgentFailure::Conflict);
    };
    if requirement_keys != &crate::task_record::missing_requirement_keys(&record.selection) {
        return Err(AgentFailure::StorageUnavailable);
    }
    let requirement = record
        .selection
        .requirements
        .iter()
        .find(|requirement| requirement.key == requirement_key)
        .ok_or(AgentFailure::NotFound)?;
    if !requirement_keys.iter().any(|key| key == requirement_key)
        || requirement.selected.len() >= usize::from(requirement.minimum_sources)
    {
        return Err(AgentFailure::Conflict);
    }
    Ok(())
}

fn validate_stored_descriptor(
    descriptor: &BindingReviewDescriptor,
) -> Result<(), AgentFailure> {
    validate_binding_review_descriptor(descriptor)
        .map_err(|_| AgentFailure::StorageUnavailable)
}

fn validate_registry_commit_receipt(
    receipt: &RegistryCommitReceipt,
    actor: &OwnerActor,
    command_id: CommandId,
    request_digest: [u8; 32],
    expected_revision: u64,
) -> Result<(), AgentFailure> {
    let expected_next_revision = expected_revision
        .checked_add(1)
        .ok_or(AgentFailure::BudgetExceeded)?;
    if receipt.command_id != command_id
        || receipt.person_id != actor.person_id
        || receipt.device_id != actor.device_id
        || receipt.request_digest != request_digest
        || receipt.snapshot.revision != expected_next_revision
    {
        return Err(AgentFailure::Conflict);
    }
    AgentRegistry::restore(receipt.snapshot.clone(), receipt.snapshot.instance_id)
        .map_err(|_| AgentFailure::StorageUnavailable)?;
    Ok(())
}

fn validate_replacement_receipt(
    receipt: &BindingReplacementReceipt,
    actor: &OwnerActor,
    command_id: CommandId,
    review_ref: &crate::BindingReviewRef,
    request_digest: [u8; 32],
) -> Result<(), AgentFailure> {
    if receipt.review_ref != *review_ref
        || receipt.registry.command_id != command_id
        || receipt.registry.person_id != actor.person_id
        || receipt.registry.device_id != actor.device_id
        || receipt.registry.request_digest != request_digest
    {
        return Err(AgentFailure::Conflict);
    }
    AgentRegistry::restore(receipt.registry.snapshot.clone(), receipt.registry.snapshot.instance_id)
        .map(|_| ()).map_err(|_| AgentFailure::StorageUnavailable)
}

/// Historical owner evidence is bound to the exact immutable reviewed assignment.
pub fn project_binding_mutation_receipt(
    receipt: &BindingReplacementReceipt,
    descriptor: &BindingReviewDescriptor,
) -> Result<BindingMutationReceipt, AgentFailure> {
    validate_stored_descriptor(descriptor)?;
    let identity = &descriptor.identity;
    if receipt.review_ref != descriptor.review_ref
        || !receipt.registry.command_id.is_valid()
        || receipt.registry.person_id != identity.person_id
        || receipt.registry.device_id != identity.device_id
        || receipt.registry.snapshot.instance_id != descriptor.registry_instance_id
        || receipt.committed_at_unix_ms < descriptor.created_at_unix_ms
        || receipt.committed_at_unix_ms >= descriptor.expires_at_unix_ms
    { return Err(AgentFailure::StorageUnavailable); }
    let registry = AgentRegistry::restore(receipt.registry.snapshot.clone(), descriptor.registry_instance_id)
        .map_err(|_| AgentFailure::StorageUnavailable)?;
    let resolved = registry.resolve_assignment(descriptor.registry_instance_id, identity.person_id,
        identity.assignment_id, &descriptor.package, descriptor.definition_revision)
        .map_err(|_| AgentFailure::StorageUnavailable)?;
    let assignment = &resolved.assignment;
    let operation = assignment.binding.last_operation.as_ref().ok_or(AgentFailure::StorageUnavailable)?;
    if assignment.installation_id != descriptor.installation_id
        || assignment.binding.revision != identity.expected_binding_revision.checked_add(1).ok_or(AgentFailure::StorageUnavailable)?
        || operation.operation_id != receipt.registry.command_id.as_uuid()
        || operation.resulting_revision != assignment.binding.revision
    { return Err(AgentFailure::StorageUnavailable); }
    let selected = assignment.binding.entries.iter().find(|entry| entry.requirement_key == identity.requirement_key)
        .ok_or(AgentFailure::StorageUnavailable)?;
    let mut candidate_refs = selected.selected.iter().map(|reference| {
        descriptor.candidates.iter().find(|candidate| candidate.candidate.reference == *reference)
            .map(|candidate| candidate.candidate_ref).ok_or(AgentFailure::StorageUnavailable)
    }).collect::<Result<Vec<_>, AgentFailure>>()?;
    candidate_refs.sort_unstable();
    let digest = digest_json(&("floe.expert-binding-replacement.sha256.v1", "replace_binding",
        receipt.registry.command_id, identity.person_id, &identity.device_id,
        &descriptor.review_ref, identity.expected_binding_revision, &candidate_refs))?;
    if digest != receipt.registry.request_digest { return Err(AgentFailure::StorageUnavailable); }
    Ok(BindingMutationReceipt {
        command_id: receipt.registry.command_id, review_ref: receipt.review_ref.clone(),
        assignment_ref: identity.assignment_id, binding_revision: assignment.binding.revision,
        registry_revision: receipt.registry.snapshot.revision,
        committed_at_unix_ms: receipt.committed_at_unix_ms,
    })
}

fn installation_request_digest(
    actor: &OwnerActor,
    command_id: CommandId,
    installation_id: Uuid,
    expected_revision: u64,
    enabled: bool,
) -> Result<[u8; 32], AgentFailure> {
    digest_json(&(
        "floe.experts.registry-command.sha256.v1",
        "set_installation_enabled",
        command_id,
        actor.person_id,
        &actor.device_id,
        installation_id,
        expected_revision,
        enabled,
    ))
}

fn replacement_request_digest(
    actor: &OwnerActor,
    command_id: CommandId,
    review_ref: &crate::BindingReviewRef,
    expected_binding_revision: u64,
    candidate_refs: &[Uuid],
) -> Result<[u8; 32], AgentFailure> {
    digest_json(&(
        "floe.expert-binding-replacement.sha256.v1",
        "replace_binding",
        command_id,
        actor.person_id,
        &actor.device_id,
        review_ref,
        expected_binding_revision,
        candidate_refs,
    ))
}

fn deterministic_review_id(identity: &BindingPrepareIdentity) -> Result<Uuid, AgentFailure> {
    let bytes = serde_json::to_vec(&(
        "floe.expert-binding-review-id.v1",
        identity,
    ))
    .map_err(|_| AgentFailure::InvalidInput)?;
    Ok(Uuid::new_v5(&Uuid::NAMESPACE_OID, &bytes))
}

fn candidate_review_ref(
    review_id: Uuid,
    reference: &SourceSelectionReference,
) -> Result<Uuid, AgentFailure> {
    reference
        .validate()
        .map_err(|_| AgentFailure::InvalidInput)?;
    let bytes = serde_json::to_vec(reference).map_err(|_| AgentFailure::InvalidInput)?;
    Ok(Uuid::new_v5(&review_id, &bytes))
}

fn source_candidate_id(
    reference: &SourceSelectionReference,
) -> Result<String, AgentFailure> {
    reference
        .validate()
        .map_err(|_| AgentFailure::InvalidInput)?;
    let bytes = serde_json::to_vec(&(
        "floe.source-selection-candidate.sha256.v1",
        reference,
    ))
    .map_err(|_| AgentFailure::InvalidInput)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn digest_json(value: &impl Serialize) -> Result<[u8; 32], AgentFailure> {
    let bytes = serde_json::to_vec(value).map_err(|_| AgentFailure::InvalidInput)?;
    Ok(Sha256::digest(bytes).into())
}

fn validate_requirement_key(value: &str) -> Result<(), AgentFailure> {
    valid_identifier(value)
        .then_some(())
        .ok_or(AgentFailure::InvalidInput)
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn valid_device_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.trim() == value
        && !value.chars().any(char::is_control)
}
