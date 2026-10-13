//! Conversation's bounded summarization journal. It is not a product Run or
//! an Expert Task; the generic execution parser remains the sole event parser.
use crate::{CompactionSource, MAX_COMPACTION_SUMMARY_BYTES};
use floe_agent_contract::{AgentFailure, JournalEntry, JournalEvent, ModelStep};
use floe_agent_runtime::{
    JournalExecutionBinding, JournalProjection, JournalProjectionMode, journal_digest,
    project_execution_journal,
};
use floe_kernel::{OwnerActor, PersonId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const COMPACTION_CONSUMER: &str = "conversation.compaction";
const MAX_COMPACTION_JOURNAL_ENTRIES: usize = 64;
const MAX_COMPACTION_JOURNAL_BYTES: usize = 512 * 1024;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CompactionJournalHead {
    pub execution_id: Uuid,
    pub person_id: PersonId,
    pub device_id: String,
    pub source_digest: [u8; 32],
    pub max_tokens: u64,
    pub max_cost_micros: u64,
    pub max_output_bytes: usize,
    pub journal_revision: u64,
    pub journal_digest: [u8; 32],
}

/// Recovery never derives permission to redispatch from an error code.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompactionRecovery {
    /// No durable intent exists. A generation-fenced owner may start the job.
    NotStarted,
    /// Replay this output through live source checks and the archive CAS.
    Output(String),
    /// An attempted execution without a fully recorded answer is not repeated.
    Interrupted,
}

fn source_digest(source: &CompactionSource) -> Result<[u8; 32], AgentFailure> {
    source.validate()?;
    let encoded = serde_json::to_vec(&("floe.compaction-source.v1", source))
        .map_err(|_| AgentFailure::StorageUnavailable)?;
    Ok(Sha256::digest(encoded).into())
}

impl CompactionJournalHead {
    pub fn new(
        actor: &OwnerActor,
        source: &CompactionSource,
        max_tokens: u64,
        max_cost_micros: u64,
        max_output_bytes: usize,
    ) -> Result<Self, AgentFailure> {
        actor.validate()?;
        if source.head.identity.person_id != actor.person_id {
            return Err(AgentFailure::PolicyDenied);
        }
        let source_digest = source_digest(source)?;
        let execution_id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            &serde_json::to_vec(&(
                "floe.compaction-execution.v1",
                actor.person_id,
                source_digest,
            ))
            .map_err(|_| AgentFailure::InvalidInput)?,
        );
        let head = Self {
            execution_id,
            person_id: actor.person_id,
            device_id: actor.device_id.clone(),
            source_digest,
            max_tokens,
            max_cost_micros,
            max_output_bytes,
            journal_revision: 0,
            journal_digest: journal_digest(&[])?,
        };
        head.binding(source)?;
        Ok(head)
    }

    fn binding(&self, source: &CompactionSource) -> Result<JournalExecutionBinding, AgentFailure> {
        let digest = source_digest(source)?;
        let expected_id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            &serde_json::to_vec(&("floe.compaction-execution.v1", self.person_id, digest))
                .map_err(|_| AgentFailure::StorageUnavailable)?,
        );
        if self.person_id != source.head.identity.person_id
            || self.source_digest != digest
            || self.execution_id != expected_id
            || self.device_id.is_empty()
            || self.device_id.trim() != self.device_id
            || self.device_id.len() > 256
            || self.device_id.chars().any(char::is_control)
            || self.max_tokens == 0
            || self.max_output_bytes == 0
            || self.max_output_bytes > MAX_COMPACTION_SUMMARY_BYTES
            || self.journal_revision > MAX_COMPACTION_JOURNAL_ENTRIES as u64
            || self.journal_digest == [0; 32]
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(JournalExecutionBinding {
            principal: self.person_id.to_string(),
            device_id: self.device_id.clone(),
            execution_id: self.execution_id,
            catalog_revision: 1,
            root_run_id: None,
            owning_task_id: None,
        })
    }
}

/// A compacted checkpoint must actually reduce source text. Metadata overhead
/// never counts as useful compression; neither whitespace nor truncation repairs
/// an invalid generated answer.
pub fn validate_compaction_summary(
    source: &CompactionSource,
    text: &str,
    max_output_bytes: usize,
) -> Result<(), AgentFailure> {
    source.validate()?;
    let source_bytes = source
        .messages
        .iter()
        .try_fold(0usize, |total, message| {
            total.checked_add(message.text.len())
        })
        .ok_or(AgentFailure::BudgetExceeded)?;
    if text.is_empty()
        || text.trim() != text
        || text.len() >= source_bytes
        || max_output_bytes == 0
        || max_output_bytes > MAX_COMPACTION_SUMMARY_BYTES
        || text.len() > max_output_bytes
        || text.chars().any(|c| c.is_control() && c != '\n')
    {
        return Err(AgentFailure::InvalidModelOutput);
    }
    Ok(())
}

fn project(
    head: &CompactionJournalHead,
    source: &CompactionSource,
    entries: &[JournalEntry],
) -> Result<JournalProjection, AgentFailure> {
    let binding = head.binding(source)?;
    if entries.len() > MAX_COMPACTION_JOURNAL_ENTRIES
        || serde_json::to_vec(entries)
            .map_err(|_| AgentFailure::StorageUnavailable)?
            .len()
            > MAX_COMPACTION_JOURNAL_BYTES
    {
        return Err(AgentFailure::BudgetExceeded);
    }
    for (index, entry) in entries.iter().enumerate() {
        match &entry.event {
            JournalEvent::ModelIntent {
                parent_task_id,
                plan,
                reservation_ceiling,
                ..
            } => {
                if parent_task_id.is_some()
                    || plan.consumer != COMPACTION_CONSUMER
                    || plan.purpose != crate::CONVERSATION_PURPOSE
                {
                    return Err(AgentFailure::PolicyDenied);
                }
                let prior = project_execution_journal(
                    &binding,
                    &entries[..index],
                    JournalProjectionMode::DurablePrefix,
                )?;
                if prior
                    .usage
                    .tokens
                    .checked_add(reservation_ceiling.tokens)
                    .is_none_or(|n| n > head.max_tokens)
                    || prior
                        .usage
                        .cost_micros
                        .checked_add(reservation_ceiling.cost_micros)
                        .is_none_or(|n| n > head.max_cost_micros)
                {
                    return Err(AgentFailure::BudgetExceeded);
                }
            }
            JournalEvent::ValidatedBatch { batch } => {
                if batch.projection_coverage == floe_agent_contract::DependencyCoverage::Unknown {
                    return Err(AgentFailure::PolicyDenied);
                }
                if !batch.tool_revisions.is_empty()
                    || !batch.agent_revisions.is_empty()
                    || batch.delegation_context.is_some()
                    || batch.steps.len() != 1
                {
                    return Err(AgentFailure::CapabilityDenied);
                }
                match &batch.steps[0] {
                    ModelStep::Answer { text, artifacts } if artifacts.is_empty() => {
                        validate_compaction_summary(source, text, head.max_output_bytes)?
                    }
                    _ => return Err(AgentFailure::CapabilityDenied),
                }
            }
            JournalEvent::Output { text, artifacts } => {
                if !artifacts.is_empty() {
                    return Err(AgentFailure::CapabilityDenied);
                }
                validate_compaction_summary(source, text, head.max_output_bytes)?;
            }
            JournalEvent::ModelResult { .. }
            | JournalEvent::BatchProgress { .. }
            | JournalEvent::Checkpoint { .. } => {}
            _ => return Err(AgentFailure::CapabilityDenied),
        }
    }
    let projection =
        project_execution_journal(&binding, entries, JournalProjectionMode::DurablePrefix)?;
    // An acknowledged intent always leaves room for its accounting result.
    if entries.len() + projection.unresolved_attempts.len() > MAX_COMPACTION_JOURNAL_ENTRIES {
        return Err(AgentFailure::BudgetExceeded);
    }
    Ok(projection)
}

pub fn validate_compaction_journal(
    head: &CompactionJournalHead,
    source: &CompactionSource,
    entries: &[JournalEntry],
) -> Result<JournalProjection, AgentFailure> {
    let projection = project(head, source, entries)?;
    if projection.journal_revision != head.journal_revision
        || projection.journal_digest != head.journal_digest
    {
        return Err(AgentFailure::StorageUnavailable);
    }
    Ok(projection)
}

/// Storage must compare-and-swap the prior head, then persist this head and its
/// single new event together before returning an acknowledgement.
pub fn advance_compaction_journal(
    head: &CompactionJournalHead,
    source: &CompactionSource,
    entries: &[JournalEntry],
) -> Result<CompactionJournalHead, AgentFailure> {
    let (new, previous) = entries.split_last().ok_or(AgentFailure::InvalidInput)?;
    let prior = validate_compaction_journal(head, source, previous)?;
    if let JournalEvent::ModelIntent { plan, .. } = &new.event {
        floe_agent_contract::validate_model_intent_selection(&prior.model_selection, plan)?;
    }
    let projected = project(head, source, entries)?;
    let mut next = head.clone();
    next.journal_revision = projected.journal_revision;
    next.journal_digest = projected.journal_digest;
    Ok(next)
}

pub fn recover_compaction_journal(
    head: &CompactionJournalHead,
    source: &CompactionSource,
    entries: &[JournalEntry],
) -> Result<CompactionRecovery, AgentFailure> {
    let projection = validate_compaction_journal(head, source, entries)?;
    if entries.is_empty() {
        return Ok(CompactionRecovery::NotStarted);
    }
    if !projection.unresolved_attempts.is_empty()
        || entries.iter().any(|entry| {
            matches!(
        &entry.event, JournalEvent::ModelResult { accounting, .. }
            if accounting.unknown_tokens || accounting.unknown_cost)
        })
    {
        return Ok(CompactionRecovery::Interrupted);
    }
    Ok(match projection.output {
        Some((text, _)) => CompactionRecovery::Output(text),
        None => CompactionRecovery::Interrupted,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use floe_agent_contract::{
        AgentMessage, BatchCursor, DependencyCoverage, MessageRole, ModelBindingDigest,
        ModelBudgetProfile, ModelCapabilities, ModelSelectionCommitment, ModelUsage,
        PreparedModelPlan, ProcessingBoundary, ProjectionRef, ValidatedModelBatch,
    };
    use floe_conversation_contract::{
        ConversationBranchId, ConversationId, ConversationReference, MessageId, TranscriptReference,
    };
    use floe_execution::budget::{ModelAccounting, ModelReservationCeiling};

    fn fixture() -> (CompactionSource, CompactionJournalHead) {
        let actor = OwnerActor {
            person_id: PersonId::new(),
            device_id: "fixture-device".into(),
            runtime_epoch: 1,
        };
        let head = ConversationReference {
            conversation_id: ConversationId::new(),
            branch_id: ConversationBranchId::new(),
            identity: crate::prompts::default_manager_identity(actor.person_id).unwrap(),
            head_revision: 4,
        };
        let source = CompactionSource {
            authority: crate::CompactionAuthority::Idle,
            session_id: Uuid::new_v4(),
            session_revision: 5,
            data_classes: vec![floe_agent_contract::DataClass::Synthetic],
            through: TranscriptReference {
                conversation_id: head.conversation_id,
                branch_id: head.branch_id,
                message_id: MessageId::new(),
                sequence: 2,
            },
            head,
            previous_checkpoint: None,
            prefix_digest: [7; 32],
            through_turn_id: Uuid::new_v4(),
            messages: vec![MessageRole::User, MessageRole::Assistant]
                .into_iter()
                .map(|role| AgentMessage {
                    message_id: Uuid::new_v4(),
                    role,
                    text: "A complete original message that is kept in the archive.".into(),
                    call_id: None,
                    coverage: DependencyCoverage::Independent,
                })
                .collect(),
        };
        let journal = CompactionJournalHead::new(&actor, &source, 1000, 1000, 1024).unwrap();
        (source, journal)
    }

    fn intent(head: &CompactionJournalHead) -> JournalEvent {
        JournalEvent::ModelIntent {
            attempt_id: Uuid::new_v4(),
            parent_task_id: None,
            reservation_ceiling: ModelReservationCeiling {
                tokens: 100,
                cost_micros: 100,
            },
            projection_ref: ProjectionRef::new(),
            plan: PreparedModelPlan {
                operation_id: Uuid::new_v4(),
                principal: head.person_id.to_string(),
                device_id: head.device_id.clone(),
                purpose: crate::CONVERSATION_PURPOSE.into(),
                consumer: COMPACTION_CONSUMER.into(),
                capabilities: ModelCapabilities::chat(),
                boundary: ProcessingBoundary::Device,
                binding_digest: ModelBindingDigest([2; 32]),
                selection_commitment: Some(ModelSelectionCommitment([1; 32])),
                budget_profile: Some(ModelBudgetProfile::unknown()),
            },
        }
    }

    fn append(
        head: &mut CompactionJournalHead,
        source: &CompactionSource,
        entries: &mut Vec<JournalEntry>,
        event: JournalEvent,
    ) {
        entries.push(JournalEntry {
            revision: entries.len() as u64 + 1,
            event,
        });
        *head = advance_compaction_journal(head, source, entries).unwrap();
    }

    #[test]
    fn compaction_head_pins_source_identity_text_and_owner() {
        let (source, head) = fixture();
        assert_eq!(
            recover_compaction_journal(&head, &source, &[]).unwrap(),
            CompactionRecovery::NotStarted
        );
        let mut changed = source.clone();
        changed.data_classes = vec![floe_agent_contract::DataClass::Personal];
        assert!(validate_compaction_journal(&head, &changed, &[]).is_err());
        changed = source.clone();
        changed.messages[0].message_id = Uuid::nil();
        assert!(changed.validate().is_err());
        changed = source.clone();
        changed.messages[0].text.push('!');
        assert!(validate_compaction_journal(&head, &changed, &[]).is_err());
        changed = source.clone();
        changed.session_revision += 1;
        assert!(validate_compaction_journal(&head, &changed, &[]).is_err());
        let foreign = OwnerActor {
            person_id: PersonId::new(),
            device_id: head.device_id.clone(),
            runtime_epoch: 1,
        };
        assert_eq!(
            CompactionJournalHead::new(&foreign, &source, 100, 100, 1024),
            Err(AgentFailure::PolicyDenied)
        );
    }

    #[test]
    fn compaction_summary_must_shrink_without_silent_repair() {
        let (source, _) = fixture();
        assert!(validate_compaction_summary(&source, "Short summary.", 1024).is_ok());
        for text in [
            "",
            " padded",
            "trailing ",
            "bad\tcontrol",
            &"x".repeat(
                source
                    .messages
                    .iter()
                    .map(|message| message.text.len())
                    .sum(),
            ),
        ] {
            assert_eq!(
                validate_compaction_summary(&source, text, 1024),
                Err(AgentFailure::InvalidModelOutput)
            );
        }
        assert_eq!(
            validate_compaction_summary(&source, "too long", 3),
            Err(AgentFailure::InvalidModelOutput)
        );
        assert!(validate_compaction_summary(&source, "Line one\nLine two", 1024).is_ok());
    }

    #[test]
    fn compaction_intent_or_unknown_usage_never_becomes_not_started() {
        let (source, mut head) = fixture();
        let event = intent(&head);
        let attempt_id = match &event {
            JournalEvent::ModelIntent { attempt_id, .. } => *attempt_id,
            _ => unreachable!(),
        };
        let mut entries = vec![];
        append(&mut head, &source, &mut entries, event);
        assert_eq!(
            recover_compaction_journal(&head, &source, &entries).unwrap(),
            CompactionRecovery::Interrupted
        );
        append(
            &mut head,
            &source,
            &mut entries,
            JournalEvent::ModelResult {
                attempt_id,
                usage: ModelUsage {
                    tokens: 100,
                    cost_micros: 100,
                },
                accounting: ModelAccounting {
                    observed_tokens: None,
                    observed_cost_micros: None,
                    unknown_tokens: true,
                    unknown_cost: true,
                },
            },
        );
        assert_eq!(
            recover_compaction_journal(&head, &source, &entries).unwrap(),
            CompactionRecovery::Interrupted
        );
    }

    #[test]
    fn compaction_completed_answer_replays_exactly_and_detects_head_tampering() {
        let (source, mut head) = fixture();
        let event = intent(&head);
        let (attempt_id, projection_ref) = match &event {
            JournalEvent::ModelIntent {
                attempt_id,
                projection_ref,
                ..
            } => (*attempt_id, *projection_ref),
            _ => unreachable!(),
        };
        let mut entries = vec![];
        append(&mut head, &source, &mut entries, event);
        append(
            &mut head,
            &source,
            &mut entries,
            JournalEvent::ModelResult {
                attempt_id,
                usage: ModelUsage {
                    tokens: 10,
                    cost_micros: 10,
                },
                accounting: ModelAccounting {
                    observed_tokens: Some(10),
                    observed_cost_micros: Some(10),
                    unknown_tokens: false,
                    unknown_cost: false,
                },
            },
        );
        let batch_id = Uuid::new_v4();
        let execution_id = head.execution_id;
        let validated = JournalEvent::ValidatedBatch {
            batch: ValidatedModelBatch {
                execution_id,
                attempt_id,
                projection_ref,
                batch_id,
                steps: vec![ModelStep::Answer {
                    text: "Exact summary.".into(),
                    artifacts: vec![],
                }],
                catalog_revision: 1,
                tool_revisions: vec![],
                agent_revisions: vec![],
                projection_coverage: DependencyCoverage::Independent,
                delegation_context: None,
            },
        };
        let mut unknown = validated.clone();
        if let JournalEvent::ValidatedBatch { batch } = &mut unknown {
            batch.projection_coverage = DependencyCoverage::Unknown;
        }
        let mut invalid_entries = entries.clone();
        invalid_entries.push(JournalEntry {
            revision: entries.len() as u64 + 1,
            event: unknown,
        });
        assert_eq!(
            advance_compaction_journal(&head, &source, &invalid_entries),
            Err(AgentFailure::PolicyDenied)
        );
        append(&mut head, &source, &mut entries, validated);
        append(
            &mut head,
            &source,
            &mut entries,
            JournalEvent::BatchProgress {
                cursor: BatchCursor {
                    batch_id,
                    next_step_index: 0,
                },
            },
        );
        assert_eq!(
            recover_compaction_journal(&head, &source, &entries).unwrap(),
            CompactionRecovery::Interrupted
        );
        append(
            &mut head,
            &source,
            &mut entries,
            JournalEvent::Output {
                text: "Exact summary.".into(),
                artifacts: vec![],
            },
        );
        assert_eq!(
            recover_compaction_journal(&head, &source, &entries).unwrap(),
            CompactionRecovery::Output("Exact summary.".into())
        );
        head.journal_digest[0] ^= 1;
        assert!(recover_compaction_journal(&head, &source, &entries).is_err());
    }

    #[test]
    fn compaction_rejects_unbound_output_and_foreground_finalization() {
        let (source, head) = fixture();
        for event in [
            JournalEvent::Output {
                text: "invented answer".into(),
                artifacts: vec![],
            },
            JournalEvent::FinalizationStarted {
                prior_execution_id: Uuid::new_v4(),
                abandoned_cursor: None,
                prior_exhaustion: AgentFailure::BudgetExceeded,
            },
        ] {
            assert!(
                advance_compaction_journal(&head, &source, &[JournalEntry { revision: 1, event }])
                    .is_err()
            );
        }
    }

    #[test]
    fn compaction_checks_admission_budget_selection_and_consumer() {
        let (source, head) = fixture();
        let mut event = intent(&head);
        if let JournalEvent::ModelIntent {
            reservation_ceiling,
            ..
        } = &mut event
        {
            reservation_ceiling.tokens = 1001;
        }
        assert_eq!(
            advance_compaction_journal(&head, &source, &[JournalEntry { revision: 1, event }]),
            Err(AgentFailure::BudgetExceeded)
        );
        let mut event = intent(&head);
        if let JournalEvent::ModelIntent { plan, .. } = &mut event {
            plan.selection_commitment = None;
        }
        assert!(
            advance_compaction_journal(&head, &source, &[JournalEntry { revision: 1, event }])
                .is_err()
        );
        let mut event = intent(&head);
        if let JournalEvent::ModelIntent { plan, .. } = &mut event {
            plan.consumer = crate::CONVERSATION_CONSUMER.into();
        }
        assert_eq!(
            advance_compaction_journal(&head, &source, &[JournalEntry { revision: 1, event }]),
            Err(AgentFailure::PolicyDenied)
        );
    }
}
