use crate::*;
use chrono::Utc;
use floe_access::{OperationApprovalRef, OperationPolicyMode};
use floe_context_contract::{GrantOperation, GrantPurpose};
use floe_execution::ExecutionScope;
use floe_kernel::{AgentFailure, OwnerActor};
use uuid::Uuid;

impl CalendarOperationsService {
    pub(super) async fn proposal_evidence(
        &self,
        actor: &OwnerActor,
        receipt: &floe_agent_contract::TaskExecutionReceiptRef,
        artifact_id: Uuid,
        scope: &ExecutionScope,
    ) -> Result<ExpertProposalEvidence, AgentFailure> {
        receipt.validate()?;
        if artifact_id.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        let evidence = self
            .proposals
            .read(actor, receipt, artifact_id, scope)
            .await?;
        evidence.proposal.validate()?;
        let proposal = &evidence.proposal;
        let dependency = &evidence.dependency;
        if evidence.receipt != *receipt
            || evidence.artifact_id != artifact_id
            || proposal.person_id != actor.person_id
            || proposal.task_id != receipt.execution.task_id.as_uuid()
            || proposal.invocation_id != evidence.invocation_id
            || proposal.assignment_id != evidence.assignment_id
            || dependency.person_id() != actor.person_id
            || dependency.observation_id() != proposal.evidence_id
            || dependency.consumer().identifier() != proposal.package.id
            || dependency.operation() != GrantOperation::Read
            || dependency.purpose() != GrantPurpose::Assistant
            || floe_access::local_calendar_provider(dependency.source().connector().as_str())
                .is_none()
        {
            return Err(AgentFailure::PolicyDenied);
        }
        self.capture_dependency_sources(actor, &evidence.coverage, scope)
            .await?;
        Ok(evidence)
    }

    pub(super) fn proposal_matches_destination(
        evidence: &ExpertProposalEvidence,
        destination: &CalendarDestination,
    ) -> bool {
        evidence.dependency.source().connection_id() == destination.connection_id
            && evidence
                .dependency
                .source_resources()
                .iter()
                .any(|resource| resource.as_str() == destination.calendar_id)
    }

    pub(super) fn proposal_schedule(
        actor: &OwnerActor,
        evidence: &ExpertProposalEvidence,
        now: chrono::DateTime<Utc>,
    ) -> Result<(floe_day::TimedSchedule, chrono::DateTime<Utc>), AgentFailure> {
        let proposal = &evidence.proposal;
        let dependency = &evidence.dependency;
        let starts_at = chrono::DateTime::<Utc>::from_timestamp_millis(
            i64::try_from(proposal.draft.starts_at_unix_ms)
                .map_err(|_| AgentFailure::InvalidInput)?,
        )
        .ok_or(AgentFailure::InvalidInput)?;
        let ends_at = chrono::DateTime::<Utc>::from_timestamp_millis(
            i64::try_from(proposal.draft.ends_at_unix_ms)
                .map_err(|_| AgentFailure::InvalidInput)?,
        )
        .ok_or(AgentFailure::InvalidInput)?;
        let proposal_expiry = chrono::DateTime::<Utc>::from_timestamp_millis(
            i64::try_from(proposal.expires_at_unix_ms).map_err(|_| AgentFailure::InvalidInput)?,
        )
        .ok_or(AgentFailure::InvalidInput)?;
        if starts_at <= now {
            return Err(AgentFailure::PolicyDenied);
        }
        evidence
            .coverage
            .validate()
            .map_err(|_| AgentFailure::PolicyDenied)?;
        let floe_agent_contract::DependencyCoverage::Dependent { dependencies } =
            &evidence.coverage
        else {
            return Err(AgentFailure::PolicyDenied);
        };
        if !dependencies.contains(dependency)
            || dependencies
                .iter()
                .any(|entry| entry.person_id() != actor.person_id || entry.observed_at() > now)
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let coverage_expiry = dependencies
            .iter()
            .map(|entry| entry.expires_at())
            .min()
            .ok_or(AgentFailure::PolicyDenied)?;
        let expiry = (now + chrono::Duration::minutes(15))
            .min(proposal_expiry)
            .min(coverage_expiry)
            .min(starts_at);
        if expiry <= now {
            return Err(AgentFailure::PolicyDenied);
        }
        let schedule = floe_day::TimedSchedule::new(starts_at, ends_at, "UTC".to_owned())
            .map_err(|_| AgentFailure::InvalidInput)?;
        Ok((schedule, expiry))
    }

    pub(super) fn dependency_matches_fence(
        dependency: &floe_context_contract::ContextDependency,
        source: &ActionSourceFence,
    ) -> bool {
        let mut resources: Vec<_> = dependency
            .source_resources()
            .iter()
            .map(|r| r.as_str().to_owned())
            .collect();
        resources.sort();
        dependency.source_authority() == source.authority
            && dependency.source().execution_owner().as_str() == source.execution_owner
            && resources == source.resources
    }

    pub async fn submit(
        &self,
        actor: &OwnerActor,
        command_id: Uuid,
        intent: ActionIntent,
        scope: &ExecutionScope,
    ) -> Result<ActionSnapshot, floe_kernel::CommandFailure<AgentFailure>> {
        let admission = self
            .prepare_admission(actor, command_id, intent, scope)
            .await?;
        let record = self
            .repository
            .admit(admission)
            .await
            .map_err(|failure| failure.map_failure(AgentFailure::from))?
            .record;
        self.complete_admission(actor, record, scope).await
    }

    /// Normalize and fence a selected Expert proposal without persisting it.
    /// Conversation uses this value to atomically compose operation admission
    /// with its own approval interaction in the Vault.
    pub async fn prepare_expert_proposal(
        &self,
        actor: &OwnerActor,
        command_id: Uuid,
        receipt: floe_agent_contract::TaskExecutionReceiptRef,
        artifact_id: Uuid,
        destination_ref: Uuid,
        session_id: Uuid,
        origin_run_id: Uuid,
        scope: &ExecutionScope,
    ) -> Result<OperationAdmission, floe_kernel::CommandFailure<AgentFailure>> {
        self.prepare_admission(
            actor,
            command_id,
            ActionIntent::ExpertProposal {
                receipt,
                artifact_id,
                destination_ref,
                session_id,
                origin_run_id,
            },
            scope,
        )
        .await
    }

    /// Finish dispatch after an owner transaction has durably admitted this
    /// exact immutable record. Pending review and policy denial remain inert.
    pub async fn complete_admission(
        &self,
        actor: &OwnerActor,
        mut record: ActionRecord,
        scope: &ExecutionScope,
    ) -> Result<ActionSnapshot, floe_kernel::CommandFailure<AgentFailure>> {
        self.admit_actor(actor, scope)
            .map_err(floe_kernel::CommandFailure::NotAdmitted)?;
        self.validate_record_actor(actor, &record)
            .map_err(floe_kernel::CommandFailure::Admitted)?;
        if record.state == ActionState::Approved {
            record = self
                .spawn_or_block(record, false, scope)
                .await
                .map_err(floe_kernel::CommandFailure::Admitted)?;
        }
        self.project(&record)
            .map_err(floe_kernel::CommandFailure::Admitted)
    }

    async fn prepare_admission(
        &self,
        actor: &OwnerActor,
        command_id: Uuid,
        intent: ActionIntent,
        scope: &ExecutionScope,
    ) -> Result<OperationAdmission, floe_kernel::CommandFailure<AgentFailure>> {
        self.admit_actor(actor, scope)
            .map_err(floe_kernel::CommandFailure::NotAdmitted)?;
        if command_id.is_nil() {
            return Err(floe_kernel::CommandFailure::NotApplied(
                AgentFailure::InvalidInput,
            ));
        }
        let _command = self
            .lock_command(command_id, scope)
            .await
            .map_err(floe_kernel::CommandFailure::Indeterminate)?;
        let request_digest = action_digest(
            b"floe.actions.submit.v1\0",
            &(actor.person_id, &actor.device_id, command_id, &intent),
        )
        // Until the owner checks the shared command namespace, a digest
        // failure cannot prove this reusable ID was never admitted elsewhere.
        .map_err(floe_kernel::CommandFailure::Indeterminate)?;
        let publication = match &intent {
            ActionIntent::ExpertProposal {
                session_id,
                origin_run_id,
                ..
            } => Some(OperationApprovalPublicationContext {
                session_id: *session_id,
                origin_run_id: *origin_run_id,
            }),
            _ => None,
        };
        if let Some(record) = self
            .repository
            .find_admission(actor.person_id, command_id, request_digest)
            .await
            .map_err(|error| {
                floe_kernel::CommandFailure::Indeterminate(AgentFailure::from(error))
            })?
        {
            self.validate_record_actor(actor, &record)
                .map_err(floe_kernel::CommandFailure::Admitted)?;
            return Ok(OperationAdmission {
                command_id,
                request_digest,
                publication,
                record,
            });
        }
        let admission = async {
            intent.validate()?;
            let now = self.clock.now();
            // Manual Day instructions are fenced by their command, actor,
            // current source and OS authority. They do not read or bind the
            // independent agent-policy revision.
            let authority = if matches!(&intent, ActionIntent::ExpertProposal { .. }) {
                Some(self.repository.read_authority(actor.person_id).await?)
            } else {
                None
            };
            let (effect, origin, dependency, expires_at) = match intent {
                ActionIntent::DirectCreate {
                    destination_ref,
                    title,
                    schedule,
                } => {
                    let destination = self
                        .resolve_destination(actor, destination_ref, scope)
                        .await?;
                    let effect = CalendarEffect::Create {
                        destination,
                        title,
                        schedule,
                    };
                    effect.validate(actor.person_id)?;
                    (
                        effect,
                        ActionOrigin::Direct {
                            command_id,
                            actor_device_id: actor.device_id.clone(),
                        },
                        None,
                        now + chrono::Duration::minutes(15),
                    )
                }
                ActionIntent::DirectUpdate {
                    event_ref,
                    expected_revision,
                    title,
                    schedule,
                } => {
                    let original = self
                        .day
                        .calendar_event(actor, event_ref, scope)
                        .await
                        .map_err(super::owner::day_failure)?
                        .ok_or(AgentFailure::NotFound)?;
                    if original.revision != expected_revision {
                        return Err(AgentFailure::Conflict);
                    }
                    let target = CalendarTarget { original };
                    let source = target.source()?;
                    let destination = self
                        .target_destination(
                            actor,
                            &source.connection_id,
                            &source.calendar_id,
                            scope,
                        )
                        .await?;
                    let effect = CalendarEffect::Update {
                        destination,
                        target,
                        title,
                        schedule,
                    };
                    effect.validate(actor.person_id)?;
                    (
                        effect,
                        ActionOrigin::Direct {
                            command_id,
                            actor_device_id: actor.device_id.clone(),
                        },
                        None,
                        now + chrono::Duration::minutes(15),
                    )
                }
                ActionIntent::DirectDelete {
                    event_ref,
                    expected_revision,
                } => {
                    let original = self
                        .day
                        .calendar_event(actor, event_ref, scope)
                        .await
                        .map_err(super::owner::day_failure)?
                        .ok_or(AgentFailure::NotFound)?;
                    if original.revision != expected_revision {
                        return Err(AgentFailure::Conflict);
                    }
                    let target = CalendarTarget { original };
                    let source = target.source()?;
                    let destination = self
                        .target_destination(
                            actor,
                            &source.connection_id,
                            &source.calendar_id,
                            scope,
                        )
                        .await?;
                    let effect = CalendarEffect::Delete {
                        destination,
                        target,
                    };
                    effect.validate(actor.person_id)?;
                    (
                        effect,
                        ActionOrigin::Direct {
                            command_id,
                            actor_device_id: actor.device_id.clone(),
                        },
                        None,
                        now + chrono::Duration::minutes(15),
                    )
                }
                ActionIntent::ExpertProposal {
                    receipt,
                    artifact_id,
                    destination_ref,
                    ..
                } => {
                    let evidence = self
                        .proposal_evidence(actor, &receipt, artifact_id, scope)
                        .await?;
                    let destination = self
                        .resolve_destination(actor, destination_ref, scope)
                        .await?;
                    if !Self::proposal_matches_destination(&evidence, &destination) {
                        return Err(AgentFailure::PolicyDenied);
                    }
                    let proposal = &evidence.proposal;
                    let (schedule, expiry) = Self::proposal_schedule(actor, &evidence, now)?;
                    let effect = CalendarEffect::Create {
                        destination,
                        title: super::EXPERT_PROPOSAL_TITLE.to_owned(),
                        // The verified proposal carries absolute instants, not a local
                        // wall-time recurrence. UTC preserves those instants without
                        // asking Flutter to invent a timezone for hidden evidence.
                        schedule,
                    };
                    let origin = ActionOrigin::Expert {
                        task_id: proposal.task_id,
                        invocation_id: evidence.invocation_id,
                        package: proposal.package.clone(),
                        installation_id: evidence.installation_id,
                        assignment_id: evidence.assignment_id,
                        definition_revision: evidence.definition_revision,
                        evidence_ref: receipt,
                        artifact_id,
                    };
                    (effect, origin, Some(evidence.dependency), expiry)
                }
            };
            if expires_at <= now {
                return Err(AgentFailure::PolicyDenied);
            }
            effect.validate(actor.person_id)?;
            let source = self.observe_source(actor, &effect).await?;
            self.current_events(actor, &effect, scope).await?;
            if let Some(dependency) = &dependency {
                if !Self::dependency_matches_fence(dependency, &source) {
                    return Err(AgentFailure::PolicyDenied);
                }
            }
            // One admitted Expert artifact can name only one external execution.
            // A new product command cannot turn a retained proposal into a retry.
            let identity_seed = origin.identity_seed(actor.person_id)?;
            let id = action_uuid(b"floe.actions.action.v1\0", actor.person_id, identity_seed);
            let execution_id = action_uuid(
                b"floe.actions.execution.v1\0",
                actor.person_id,
                identity_seed,
            );
            let effect_digest = effect.digest()?;
            let review = OperationApprovalRef {
                id: action_uuid(b"floe.actions.review.v1\0", actor.person_id, identity_seed),
                operation_id: id,
                effect_digest,
                source_digest: source.digest()?,
                person_id: actor.person_id,
                device_id: actor.device_id.clone(),
                policy_revision: authority.as_ref().map(|value| value.revision),
                created_at: now,
                expires_at,
            };
            let (state, authorization) = match &origin {
                ActionOrigin::Direct { .. } => (
                    ActionState::Approved,
                    Some(floe_access::direct_operation_receipt(
                        command_id,
                        &review.subject(),
                    )?),
                ),
                ActionOrigin::Expert { .. } => match authority
                    .as_ref()
                    .ok_or(AgentFailure::PolicyDenied)?
                    .calendar_create
                {
                    OperationPolicyMode::Allow => (
                        ActionState::Approved,
                        Some(floe_access::standing_policy_receipt(
                            &review.subject(),
                            authority.as_ref().ok_or(AgentFailure::PolicyDenied)?,
                        )?),
                    ),
                    OperationPolicyMode::Ask => (ActionState::PendingReview, None),
                    OperationPolicyMode::Deny => (
                        ActionState::Blocked {
                            reason: ActionBlockedReason::PolicyDenied,
                        },
                        None,
                    ),
                },
            };
            let record = ActionRecord {
                id,
                person_id: actor.person_id,
                device_id: actor.device_id.clone(),
                revision: 1,
                origin,
                effect,
                effect_digest,
                execution_id,
                source,
                dependency,
                review,
                authorization,
                created_at: now,
                expires_at,
                state,
                execution: None,
                collection: None,
            };
            Ok::<_, AgentFailure>(OperationAdmission {
                command_id,
                request_digest,
                publication,
                record,
            })
        }
        .await
        .map_err(floe_kernel::CommandFailure::NotApplied)?;
        Ok(admission)
    }
}
