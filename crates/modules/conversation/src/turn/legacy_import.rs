//! Pure preparation for an explicitly requested one-time frozen Session import.
//!
//! This module decodes Conversation's typed owner snapshot and produces inert
//! records. It performs no storage access, reference resolution, activation,
//! dispatch, or quiescence check.

use std::collections::HashMap;

use floe_agent_contract::{AGENT_VERSION, AgentFailure, DataClass, ModelStep};
use floe_kernel::PersonId;
use serde::Serialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::{
    AgentContinuation, AgentMessage, AgentOutcome, AgentSession, AgentSessionScope, AgentUsage,
    MAX_SESSION_BYTES, TypedAgentMessageProvenance, session_message_aliases,
};

/// Separate identity namespace for inert imported rows awaiting a later
/// transaction importer. It is distinct from live owner-recorded history.
pub const FROZEN_LEGACY_SESSION_ARCHIVE_NAMESPACE: &str =
    "floe.conversation.frozen-legacy-session-archive";

/// Maximum caller-selected byte budget for serializing prepared record
/// envelopes. This is a finite preparation-bookkeeping ceiling, not a model,
/// context, transcript, history, or record-count quota.
pub const MAX_PREPARED_LEGACY_SESSION_OUTPUT_CEILING_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FrozenLegacySessionArchiveIdentity {
    namespace: &'static str,
    person_id: PersonId,
    session_id: Uuid,
    source_revision: u64,
    source_digest: [u8; 32],
}

impl FrozenLegacySessionArchiveIdentity {
    pub fn namespace(&self) -> &'static str {
        self.namespace
    }

    pub fn person_id(&self) -> PersonId {
        self.person_id
    }

    pub fn session_id(&self) -> Uuid {
        self.session_id
    }

    pub fn source_revision(&self) -> u64 {
        self.source_revision
    }

    pub fn source_digest(&self) -> [u8; 32] {
        self.source_digest
    }
}

/// Historical coverage remains an unproven input for the transaction importer.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LegacyCoverageStatus {
    Unproven,
}

/// Existence and ownership of references inside a legacy message are not
/// resolved by pure preparation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LegacyExternalReferenceStatus {
    Unverified,
}

/// The prepared data grants no new command, Run, dispatch, or recorder
/// authority. Historical Task receipts remain preserved in the message only.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LegacyExecutionAuthority {
    None,
}

/// Every prepared row carries these explicit trust limits. The original
/// `AgentMessage` is preserved unchanged and its claims do not change these
/// status values.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct LegacyUnprovenAuthority {
    evidence_provenance: TypedAgentMessageProvenance,
    coverage: LegacyCoverageStatus,
    external_references: LegacyExternalReferenceStatus,
    execution: LegacyExecutionAuthority,
}

impl LegacyUnprovenAuthority {
    fn imported_legacy_unproven() -> Self {
        Self {
            evidence_provenance: TypedAgentMessageProvenance::ImportedLegacyUnproven,
            coverage: LegacyCoverageStatus::Unproven,
            external_references: LegacyExternalReferenceStatus::Unverified,
            execution: LegacyExecutionAuthority::None,
        }
    }

    pub fn evidence_provenance(self) -> TypedAgentMessageProvenance {
        self.evidence_provenance
    }

    pub fn coverage(self) -> LegacyCoverageStatus {
        self.coverage
    }

    pub fn external_references(self) -> LegacyExternalReferenceStatus {
        self.external_references
    }

    pub fn execution(self) -> LegacyExecutionAuthority {
        self.execution
    }
}

/// Non-message fields of the exact typed legacy AgentSession snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedLegacySessionShell {
    schema_version: u32,
    scope: Option<AgentSessionScope>,
    data_classes: Vec<DataClass>,
    usage: AgentUsage,
    pending_output: Option<Vec<ModelStep>>,
    active_turn: Option<Uuid>,
    last_outcome: Option<AgentOutcome>,
    continuation: Option<AgentContinuation>,
}

impl PreparedLegacySessionShell {
    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }

    pub fn scope(&self) -> Option<AgentSessionScope> {
        self.scope
    }

    pub fn data_classes(&self) -> &[DataClass] {
        &self.data_classes
    }

    pub fn usage(&self) -> AgentUsage {
        self.usage
    }

    pub fn pending_output(&self) -> Option<&[ModelStep]> {
        self.pending_output.as_deref()
    }

    pub fn active_turn(&self) -> Option<Uuid> {
        self.active_turn
    }

    pub fn last_outcome(&self) -> Option<AgentOutcome> {
        self.last_outcome
    }

    pub fn continuation(&self) -> Option<AgentContinuation> {
        self.continuation
    }
}

/// One inert owner-typed record from the frozen source snapshot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PreparedLegacySessionRecord {
    source_ordinal: usize,
    public_cursor_alias: Uuid,
    synthetic_occurrence_ordinal: Option<usize>,
    message: AgentMessage,
    authority: LegacyUnprovenAuthority,
    #[serde(skip)]
    measured_serialized_bytes: usize,
}

impl PreparedLegacySessionRecord {
    /// Stable zero-based source position. It is not a future Core sequence.
    pub fn source_ordinal(&self) -> usize {
        self.source_ordinal
    }

    /// Existing public message/cursor alias from Session snapshot projection.
    pub fn public_cursor_alias(&self) -> Uuid {
        self.public_cursor_alias
    }

    /// The zero-based per-turn Preamble or Compaction occurrence used to form
    /// `public_cursor_alias`; other message kinds return `None`.
    pub fn synthetic_occurrence_ordinal(&self) -> Option<usize> {
        self.synthetic_occurrence_ordinal
    }

    pub fn message(&self) -> &AgentMessage {
        &self.message
    }

    pub fn authority(&self) -> LegacyUnprovenAuthority {
        self.authority
    }

    /// Exact serde JSON byte count for this prepared record, excluding the
    /// non-serialized measurement field.
    pub fn measured_serialized_bytes(&self) -> usize {
        self.measured_serialized_bytes
    }
}

/// Fully checked, inert preparation result. Exact input bytes and typed
/// messages are both retained; the namespace identifies frozen archive
/// records rather than the live owner-recorded history namespace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedLegacySessionSnapshot {
    archive_identity: FrozenLegacySessionArchiveIdentity,
    raw_source_bytes: Box<[u8]>,
    shell: PreparedLegacySessionShell,
    authority: LegacyUnprovenAuthority,
    records: Vec<PreparedLegacySessionRecord>,
    prepared_record_output_byte_budget: usize,
    measured_output_bytes: usize,
}

impl PreparedLegacySessionSnapshot {
    pub fn archive_identity(&self) -> &FrozenLegacySessionArchiveIdentity {
        &self.archive_identity
    }

    /// Exact bytes supplied to the decoder, including original JSON spelling.
    pub fn raw_source_bytes(&self) -> &[u8] {
        &self.raw_source_bytes
    }

    pub fn shell(&self) -> &PreparedLegacySessionShell {
        &self.shell
    }

    pub fn authority(&self) -> LegacyUnprovenAuthority {
        self.authority
    }

    pub fn records(&self) -> &[PreparedLegacySessionRecord] {
        &self.records
    }

    /// Caller-selected ceiling used to prepare this result. It is distinct
    /// from the source snapshot's 2 MiB raw-byte limit.
    pub fn prepared_record_output_byte_budget(&self) -> usize {
        self.prepared_record_output_byte_budget
    }

    /// Sum of the individually serialized prepared record envelopes.
    pub fn measured_output_bytes(&self) -> usize {
        self.measured_output_bytes
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LegacySessionPreparationError {
    InvalidExpectedIdentity,
    SnapshotTooLarge {
        actual_bytes: usize,
        maximum_bytes: usize,
    },
    RawDigestMismatch,
    MalformedSnapshot,
    UnsupportedSchemaVersion {
        actual: u32,
        expected: u32,
    },
    PersonMismatch,
    SessionMismatch,
    SourceRevisionMismatch,
    OwnerSnapshotRejected {
        reason: AgentFailure,
    },
    InvalidMessage {
        source_ordinal: usize,
        reason: AgentFailure,
    },
    AmbiguousAlias {
        alias: Uuid,
        first_source_ordinal: usize,
        second_source_ordinal: usize,
    },
    PreparedOutputBudgetExceeded {
        source_ordinal: usize,
        measured_record_bytes: usize,
        cumulative_bytes: usize,
        budget_bytes: usize,
    },
    InvalidPreparedOutputBudget {
        requested_bytes: usize,
        maximum_bytes: usize,
    },
}

/// Decode and prepare one explicit legacy AgentSession snapshot.
///
/// The raw digest is SHA-256 over the exact supplied bytes. Expected identity
/// values are checked against the decoded typed owner snapshot before records
/// are returned. `prepared_record_output_byte_budget` applies only to the sum
/// of serialized prepared record envelopes and must not exceed the finite
/// 16 MiB preparation ceiling. It does not include retained source bytes,
/// shell fields, enclosing archive identity, or in-memory overhead. This
/// function never reads or writes storage, resolves external references,
/// checks migration quiescence, or activates a namespace.
pub fn prepare_legacy_session_snapshot(
    raw_source_bytes: &[u8],
    expected_person_id: PersonId,
    expected_session_id: Uuid,
    expected_source_revision: u64,
    expected_raw_digest: [u8; 32],
    prepared_record_output_byte_budget: usize,
) -> Result<PreparedLegacySessionSnapshot, LegacySessionPreparationError> {
    if raw_source_bytes.len() > MAX_SESSION_BYTES {
        return Err(LegacySessionPreparationError::SnapshotTooLarge {
            actual_bytes: raw_source_bytes.len(),
            maximum_bytes: MAX_SESSION_BYTES,
        });
    }
    if prepared_record_output_byte_budget > MAX_PREPARED_LEGACY_SESSION_OUTPUT_CEILING_BYTES {
        return Err(LegacySessionPreparationError::InvalidPreparedOutputBudget {
            requested_bytes: prepared_record_output_byte_budget,
            maximum_bytes: MAX_PREPARED_LEGACY_SESSION_OUTPUT_CEILING_BYTES,
        });
    }
    if !expected_person_id.is_valid() || expected_session_id.is_nil() {
        return Err(LegacySessionPreparationError::InvalidExpectedIdentity);
    }

    let actual_digest: [u8; 32] = Sha256::digest(raw_source_bytes).into();
    if actual_digest != expected_raw_digest {
        return Err(LegacySessionPreparationError::RawDigestMismatch);
    }

    let session: AgentSession = serde_json::from_slice(raw_source_bytes)
        .map_err(|_| LegacySessionPreparationError::MalformedSnapshot)?;
    session
        .validate_owner_snapshot(expected_person_id)
        .map_err(|reason| match reason {
            AgentFailure::NotFound => LegacySessionPreparationError::PersonMismatch,
            AgentFailure::UnsupportedVersion => {
                LegacySessionPreparationError::UnsupportedSchemaVersion {
                    actual: session.schema_version,
                    expected: AGENT_VERSION,
                }
            }
            reason => LegacySessionPreparationError::OwnerSnapshotRejected { reason },
        })?;
    if session.id != expected_session_id {
        return Err(LegacySessionPreparationError::SessionMismatch);
    }
    if session.revision != expected_source_revision {
        return Err(LegacySessionPreparationError::SourceRevisionMismatch);
    }

    let AgentSession {
        schema_version,
        id: session_id,
        person_id,
        scope,
        revision: source_revision,
        data_classes,
        messages,
        usage,
        pending_output,
        active_turn,
        last_outcome,
        continuation,
    } = session;

    let aliases = session_message_aliases(&messages);
    let mut seen_aliases = HashMap::with_capacity(aliases.len());
    for (source_ordinal, alias) in aliases.iter().enumerate() {
        if let Some(first_source_ordinal) = seen_aliases.insert(alias.message_id, source_ordinal) {
            return Err(LegacySessionPreparationError::AmbiguousAlias {
                alias: alias.message_id,
                first_source_ordinal,
                second_source_ordinal: source_ordinal,
            });
        }
    }

    let archive_identity = FrozenLegacySessionArchiveIdentity {
        namespace: FROZEN_LEGACY_SESSION_ARCHIVE_NAMESPACE,
        person_id,
        session_id,
        source_revision,
        source_digest: actual_digest,
    };
    let authority = LegacyUnprovenAuthority::imported_legacy_unproven();
    let mut records = Vec::with_capacity(messages.len());
    let mut measured_output_bytes = 0usize;
    for (source_ordinal, (message, alias)) in messages.into_iter().zip(aliases).enumerate() {
        super::typed_history::validate_agent_message(&message).map_err(|reason| {
            LegacySessionPreparationError::InvalidMessage {
                source_ordinal,
                reason,
            }
        })?;

        let mut record = PreparedLegacySessionRecord {
            source_ordinal,
            public_cursor_alias: alias.message_id,
            synthetic_occurrence_ordinal: alias.synthetic_occurrence_ordinal,
            message,
            authority,
            measured_serialized_bytes: 0,
        };
        record.measured_serialized_bytes = serde_json::to_vec(&record)
            .map_err(|_| LegacySessionPreparationError::MalformedSnapshot)?
            .len();
        measured_output_bytes = measured_output_bytes
            .checked_add(record.measured_serialized_bytes)
            .ok_or(
                LegacySessionPreparationError::PreparedOutputBudgetExceeded {
                    source_ordinal,
                    measured_record_bytes: record.measured_serialized_bytes,
                    cumulative_bytes: usize::MAX,
                    budget_bytes: prepared_record_output_byte_budget,
                },
            )?;
        if measured_output_bytes > prepared_record_output_byte_budget {
            return Err(
                LegacySessionPreparationError::PreparedOutputBudgetExceeded {
                    source_ordinal,
                    measured_record_bytes: record.measured_serialized_bytes,
                    cumulative_bytes: measured_output_bytes,
                    budget_bytes: prepared_record_output_byte_budget,
                },
            );
        }
        records.push(record);
    }

    let shell = PreparedLegacySessionShell {
        schema_version,
        scope,
        data_classes,
        usage,
        pending_output,
        active_turn,
        last_outcome,
        continuation,
    };

    Ok(PreparedLegacySessionSnapshot {
        archive_identity,
        raw_source_bytes: raw_source_bytes.into(),
        shell,
        authority,
        records,
        prepared_record_output_byte_budget,
        measured_output_bytes,
    })
}

#[cfg(test)]
mod tests {
    use floe_agent_contract::{
        CalendarProvider, DependencyCoverage, TaskExecutionKey, TaskExecutionReceiptRef,
        TaskSnapshot, TaskState, UserInteractionKind,
    };
    use floe_kernel::{PersonId, TaskId};
    use sha2::{Digest, Sha256};

    use super::*;

    const PERSON: PersonId = PersonId(Uuid::from_u128(0x100));
    const SESSION: Uuid = Uuid::from_u128(0x200);
    const TURN: Uuid = Uuid::from_u128(0x300);
    const SOURCE_REVISION: u64 = 19;

    fn session_with(messages: Vec<AgentMessage>) -> AgentSession {
        AgentSession {
            schema_version: AGENT_VERSION,
            id: SESSION,
            person_id: PERSON,
            scope: Some(AgentSessionScope::Calendar {
                setup_id: Uuid::from_u128(0x400),
                provider: CalendarProvider::Fixture,
            }),
            revision: SOURCE_REVISION,
            data_classes: vec![DataClass::Synthetic],
            messages,
            usage: AgentUsage {
                model_attempts: 2,
                iterations: 3,
                ..AgentUsage::default()
            },
            pending_output: None,
            active_turn: Some(TURN),
            last_outcome: Some(AgentOutcome::Completed),
            continuation: None,
        }
    }

    fn raw(session: &AgentSession) -> Vec<u8> {
        serde_json::to_vec(session).expect("serialize typed owner snapshot")
    }

    fn digest(bytes: &[u8]) -> [u8; 32] {
        Sha256::digest(bytes).into()
    }

    fn prepare_bytes(
        bytes: &[u8],
        person_id: PersonId,
        session_id: Uuid,
        revision: u64,
    ) -> Result<PreparedLegacySessionSnapshot, LegacySessionPreparationError> {
        prepare_bytes_with_budget(
            bytes,
            person_id,
            session_id,
            revision,
            MAX_PREPARED_LEGACY_SESSION_OUTPUT_CEILING_BYTES,
        )
    }

    fn prepare_bytes_with_budget(
        bytes: &[u8],
        person_id: PersonId,
        session_id: Uuid,
        revision: u64,
        output_byte_budget: usize,
    ) -> Result<PreparedLegacySessionSnapshot, LegacySessionPreparationError> {
        prepare_legacy_session_snapshot(
            bytes,
            person_id,
            session_id,
            revision,
            digest(bytes),
            output_byte_budget,
        )
    }

    fn prepare(session: &AgentSession) -> PreparedLegacySessionSnapshot {
        prepare_session_result(session).expect("prepare typed legacy snapshot")
    }

    fn prepare_session_result(
        session: &AgentSession,
    ) -> Result<PreparedLegacySessionSnapshot, LegacySessionPreparationError> {
        let bytes = raw(session);
        prepare_bytes(&bytes, session.person_id, session.id, session.revision)
    }

    fn dense_user_messages(count: usize) -> Vec<AgentMessage> {
        (0..count)
            .map(|index| AgentMessage::User {
                turn_id: Uuid::from_u128(0x10_0000 + index as u128),
                message_id: Uuid::from_u128(0x20_0000 + index as u128),
                text: String::new(),
            })
            .collect()
    }

    fn all_variants() -> Vec<AgentMessage> {
        let task_id = TaskId::from_uuid(Uuid::from_u128(0x500)).expect("non-nil TaskId");
        let task = TaskSnapshot {
            task_id,
            parent_run_id: Some(Uuid::from_u128(0x501)),
            principal: "expert:test".to_owned(),
            agent_id: "expert-test".to_owned(),
            definition_revision: 4,
            state: TaskState::Completed,
            result: Some("delegated result".to_owned()),
            artifacts: Vec::new(),
            coverage: DependencyCoverage::Independent,
            issue: None,
            blockage: None,
        };
        let execution_receipt = TaskExecutionReceiptRef {
            execution: TaskExecutionKey {
                task_id,
                execution_id: Uuid::from_u128(0x502),
                executor_generation: 7,
            },
            task_revision: 2,
            journal_revision: 1,
            digest: [9; 32],
        };
        vec![
            AgentMessage::Compaction {
                turn_id: TURN,
                summary: "summary".to_owned(),
                recovery: super::super::SessionRecoveryPointer {
                    archive_id: Uuid::from_u128(0x600),
                    source_revision: 17,
                    through_turn_id: Uuid::from_u128(0x601),
                    archived_message_count: 3,
                },
            },
            AgentMessage::Preamble {
                turn_id: TURN,
                text: "system preamble".to_owned(),
            },
            AgentMessage::User {
                turn_id: TURN,
                message_id: Uuid::from_u128(0x700),
                text: "hello".to_owned(),
            },
            AgentMessage::Assistant {
                turn_id: TURN,
                text: "answer".to_owned(),
            },
            AgentMessage::Capability {
                turn_id: TURN,
                call_id: Uuid::from_u128(0x800),
                capability_id: "calendar.read".to_owned(),
                input: "{}".to_owned(),
                result: Err(AgentFailure::PolicyDenied),
            },
            AgentMessage::Delegation {
                turn_id: TURN,
                task,
                execution_receipt: Some(execution_receipt),
            },
            AgentMessage::Interaction {
                turn_id: TURN,
                interaction_id: Uuid::from_u128(0x900),
                interaction_kind: UserInteractionKind::SourceAccess,
            },
        ]
    }

    #[test]
    fn preserves_all_typed_variants_and_exact_source_bytes() {
        let messages = all_variants();
        let session = session_with(messages.clone());
        let source = raw(&session);
        let prepared = prepare_bytes(&source, PERSON, SESSION, SOURCE_REVISION)
            .expect("prepare complete typed snapshot");

        assert_eq!(prepared.raw_source_bytes(), source);
        assert_eq!(
            prepared.archive_identity().namespace(),
            FROZEN_LEGACY_SESSION_ARCHIVE_NAMESPACE
        );
        assert_ne!(
            prepared.archive_identity().namespace(),
            super::super::TYPED_AGENT_MESSAGE_OWNER_NAMESPACE
        );
        assert_eq!(prepared.archive_identity().source_digest(), digest(&source));
        assert_eq!(prepared.records().len(), messages.len());
        assert_eq!(
            prepared
                .records()
                .iter()
                .map(|record| record.message().clone())
                .collect::<Vec<_>>(),
            messages
        );
        assert_eq!(prepared.shell().schema_version(), AGENT_VERSION);
        assert_eq!(prepared.shell().scope(), session.scope);
        assert_eq!(prepared.shell().data_classes(), session.data_classes);
        assert_eq!(prepared.shell().usage(), session.usage);
        assert_eq!(prepared.shell().active_turn(), session.active_turn);
        assert_eq!(prepared.shell().last_outcome(), session.last_outcome);

        assert!(matches!(
            prepared.records()[4].message(),
            AgentMessage::Capability {
                result: Err(AgentFailure::PolicyDenied),
                ..
            }
        ));
        assert!(matches!(
            prepared.records()[5].message(),
            AgentMessage::Delegation {
                execution_receipt: Some(receipt),
                task,
                ..
            } if receipt.execution.task_id == task.task_id
        ));
        assert!(matches!(
            prepared.records()[6].message(),
            AgentMessage::Interaction { .. }
        ));
    }

    #[test]
    fn preserves_repeated_synthetic_alias_ordinals() {
        let recovery = || super::super::SessionRecoveryPointer {
            archive_id: Uuid::from_u128(0xa00),
            source_revision: 12,
            through_turn_id: Uuid::from_u128(0xa01),
            archived_message_count: 2,
        };
        let messages = vec![
            AgentMessage::Preamble {
                turn_id: TURN,
                text: "same".to_owned(),
            },
            AgentMessage::Preamble {
                turn_id: TURN,
                text: "same".to_owned(),
            },
            AgentMessage::Compaction {
                turn_id: TURN,
                summary: "same summary".to_owned(),
                recovery: recovery(),
            },
            AgentMessage::Compaction {
                turn_id: TURN,
                summary: "same summary".to_owned(),
                recovery: recovery(),
            },
        ];
        let prepared = prepare(&session_with(messages));
        let records = prepared.records();

        assert_eq!(
            records
                .iter()
                .map(PreparedLegacySessionRecord::source_ordinal)
                .collect::<Vec<_>>(),
            vec![0, 1, 2, 3]
        );
        assert_eq!(records[0].synthetic_occurrence_ordinal(), Some(0));
        assert_eq!(records[1].synthetic_occurrence_ordinal(), Some(1));
        assert_eq!(records[2].synthetic_occurrence_ordinal(), Some(0));
        assert_eq!(records[3].synthetic_occurrence_ordinal(), Some(1));
        assert_eq!(
            records[0].public_cursor_alias(),
            Uuid::new_v5(&TURN, b"preamble:0:same")
        );
        assert_eq!(
            records[1].public_cursor_alias(),
            Uuid::new_v5(&TURN, b"preamble:1:same")
        );
        assert_eq!(
            records[2].public_cursor_alias(),
            Uuid::new_v5(&TURN, b"compaction:0")
        );
        assert_eq!(
            records[3].public_cursor_alias(),
            Uuid::new_v5(&TURN, b"compaction:1")
        );
    }

    #[test]
    fn rejects_duplicate_assistant_user_and_cross_kind_aliases() {
        let cases = [
            vec![
                AgentMessage::Assistant {
                    turn_id: TURN,
                    text: "first".to_owned(),
                },
                AgentMessage::Assistant {
                    turn_id: TURN,
                    text: "second".to_owned(),
                },
            ],
            vec![
                AgentMessage::User {
                    turn_id: TURN,
                    message_id: Uuid::from_u128(0xb00),
                    text: "first".to_owned(),
                },
                AgentMessage::User {
                    turn_id: Uuid::from_u128(0xb01),
                    message_id: Uuid::from_u128(0xb00),
                    text: "second".to_owned(),
                },
            ],
            vec![
                AgentMessage::Assistant {
                    turn_id: Uuid::from_u128(0xb02),
                    text: "assistant alias".to_owned(),
                },
                AgentMessage::User {
                    turn_id: TURN,
                    message_id: Uuid::from_u128(0xb02),
                    text: "user alias".to_owned(),
                },
            ],
        ];

        for messages in cases {
            let error = prepare_bytes(
                &raw(&session_with(messages)),
                PERSON,
                SESSION,
                SOURCE_REVISION,
            )
            .expect_err("duplicate aliases are ambiguous");
            assert!(matches!(
                error,
                LegacySessionPreparationError::AmbiguousAlias { .. }
            ));
        }
    }

    #[test]
    fn verifies_exact_digest_revision_person_and_session() {
        let source = raw(&session_with(Vec::new()));
        let source_digest = digest(&source);

        assert_eq!(
            prepare_legacy_session_snapshot(
                &source,
                PERSON,
                SESSION,
                SOURCE_REVISION,
                [0x77; 32],
                MAX_PREPARED_LEGACY_SESSION_OUTPUT_CEILING_BYTES,
            ),
            Err(LegacySessionPreparationError::RawDigestMismatch)
        );
        assert_eq!(
            prepare_legacy_session_snapshot(
                &source,
                PERSON,
                SESSION,
                SOURCE_REVISION + 1,
                source_digest,
                MAX_PREPARED_LEGACY_SESSION_OUTPUT_CEILING_BYTES,
            ),
            Err(LegacySessionPreparationError::SourceRevisionMismatch)
        );
        assert_eq!(
            prepare_legacy_session_snapshot(
                &source,
                PersonId(Uuid::from_u128(0x101)),
                SESSION,
                SOURCE_REVISION,
                source_digest,
                MAX_PREPARED_LEGACY_SESSION_OUTPUT_CEILING_BYTES,
            ),
            Err(LegacySessionPreparationError::PersonMismatch)
        );
        assert_eq!(
            prepare_legacy_session_snapshot(
                &source,
                PERSON,
                Uuid::from_u128(0x201),
                SOURCE_REVISION,
                source_digest,
                MAX_PREPARED_LEGACY_SESSION_OUTPUT_CEILING_BYTES,
            ),
            Err(LegacySessionPreparationError::SessionMismatch)
        );
    }

    #[test]
    fn shares_vault_owner_snapshot_admission_rules() {
        let mut session = session_with(Vec::new());
        session.data_classes.clear();
        assert_eq!(
            prepare_session_result(&session),
            Err(LegacySessionPreparationError::OwnerSnapshotRejected {
                reason: AgentFailure::PolicyDenied,
            })
        );

        let mut session = session_with(Vec::new());
        session.data_classes = vec![DataClass::Personal];
        assert_eq!(
            prepare_session_result(&session),
            Err(LegacySessionPreparationError::OwnerSnapshotRejected {
                reason: AgentFailure::PolicyDenied,
            })
        );

        let mut session = session_with(Vec::new());
        session.data_classes = vec![DataClass::Credential];
        session.scope = None;
        assert_eq!(
            prepare_session_result(&session),
            Err(LegacySessionPreparationError::OwnerSnapshotRejected {
                reason: AgentFailure::PolicyDenied,
            })
        );

        let mut session = session_with(Vec::new());
        session.data_classes = vec![DataClass::DeviceOnlyRaw];
        session.scope = None;
        assert_eq!(
            prepare_session_result(&session),
            Err(LegacySessionPreparationError::OwnerSnapshotRejected {
                reason: AgentFailure::PolicyDenied,
            })
        );

        let mut session = session_with(Vec::new());
        session.schema_version = AGENT_VERSION + 1;
        assert_eq!(
            prepare_session_result(&session),
            Err(LegacySessionPreparationError::UnsupportedSchemaVersion {
                actual: AGENT_VERSION + 1,
                expected: AGENT_VERSION,
            })
        );
    }

    #[test]
    fn rejects_malformed_and_oversized_source_bytes() {
        let malformed = b"{";
        assert_eq!(
            prepare_bytes(malformed, PERSON, SESSION, SOURCE_REVISION),
            Err(LegacySessionPreparationError::MalformedSnapshot)
        );

        let oversized = vec![0u8; MAX_SESSION_BYTES + 1];
        assert_eq!(
            prepare_legacy_session_snapshot(
                &oversized,
                PERSON,
                SESSION,
                SOURCE_REVISION,
                digest(&oversized),
                MAX_PREPARED_LEGACY_SESSION_OUTPUT_CEILING_BYTES,
            ),
            Err(LegacySessionPreparationError::SnapshotTooLarge {
                actual_bytes: MAX_SESSION_BYTES + 1,
                maximum_bytes: MAX_SESSION_BYTES,
            })
        );
    }

    #[test]
    fn source_bound_not_product_page_cap_limits_import_count() {
        let messages = (0..300)
            .map(|index| AgentMessage::User {
                turn_id: Uuid::from_u128(0xc00 + index),
                message_id: Uuid::from_u128(0xd00 + index),
                text: String::new(),
            })
            .collect();
        let session = session_with(messages);
        let source = raw(&session);
        assert!(source.len() < MAX_SESSION_BYTES);
        let prepared = prepare_bytes(&source, PERSON, SESSION, SOURCE_REVISION)
            .expect("prepare more than one product page");

        assert_eq!(prepared.records().len(), 300);
        assert!(prepared.measured_output_bytes() <= prepared.prepared_record_output_byte_budget());
    }

    #[test]
    fn near_source_limit_expands_past_two_mib_with_caller_budget() {
        let empty_length = raw(&session_with(Vec::new())).len();
        let calibration_length = raw(&session_with(dense_user_messages(1_024))).len();
        let bytes_per_message = (calibration_length - empty_length) / 1_024;
        let estimated_count = (MAX_SESSION_BYTES - empty_length) / bytes_per_message;
        let mut session = session_with(dense_user_messages(estimated_count));
        let mut source = raw(&session);
        while source.len() > MAX_SESSION_BYTES {
            session.messages.pop();
            source = raw(&session);
        }

        assert!(source.len() > MAX_SESSION_BYTES - 1_024);
        let prepared = prepare_bytes_with_budget(
            &source,
            PERSON,
            SESSION,
            SOURCE_REVISION,
            MAX_PREPARED_LEGACY_SESSION_OUTPUT_CEILING_BYTES,
        )
        .expect("bounded output ceiling admits expanded prepared records");
        assert!(prepared.measured_output_bytes() > MAX_SESSION_BYTES);
        assert!(
            prepared.measured_output_bytes() <= MAX_PREPARED_LEGACY_SESSION_OUTPUT_CEILING_BYTES
        );

        assert!(matches!(
            prepare_bytes_with_budget(&source, PERSON, SESSION, SOURCE_REVISION, MAX_SESSION_BYTES,),
            Err(
                LegacySessionPreparationError::PreparedOutputBudgetExceeded {
                    budget_bytes: MAX_SESSION_BYTES,
                    ..
                }
            )
        ));
    }

    #[test]
    fn dense_small_records_expand_past_two_mib_without_a_count_quota() {
        let session = session_with(dense_user_messages(8_192));
        let source = raw(&session);
        assert!(source.len() < MAX_SESSION_BYTES);

        let prepared = prepare_bytes_with_budget(
            &source,
            PERSON,
            SESSION,
            SOURCE_REVISION,
            MAX_PREPARED_LEGACY_SESSION_OUTPUT_CEILING_BYTES,
        )
        .expect("the explicit finite preparation budget covers dense records");
        assert_eq!(prepared.records().len(), 8_192);
        assert!(prepared.measured_output_bytes() > MAX_SESSION_BYTES);
        assert!(
            prepared.measured_output_bytes() <= MAX_PREPARED_LEGACY_SESSION_OUTPUT_CEILING_BYTES
        );

        assert!(matches!(
            prepare_bytes_with_budget(&source, PERSON, SESSION, SOURCE_REVISION, MAX_SESSION_BYTES,),
            Err(
                LegacySessionPreparationError::PreparedOutputBudgetExceeded {
                    budget_bytes: MAX_SESSION_BYTES,
                    ..
                }
            )
        ));
    }

    #[test]
    fn prepared_output_budget_checks_exact_fit_zero_and_over_ceiling() {
        let one_message = session_with(vec![AgentMessage::User {
            turn_id: TURN,
            message_id: Uuid::from_u128(0xe00),
            text: String::new(),
        }]);
        let source = raw(&one_message);
        let measured = prepare_bytes(&source, PERSON, SESSION, SOURCE_REVISION)
            .expect("prepare with maximum allowed caller budget")
            .measured_output_bytes();
        assert!(measured > 0);

        let exact_fit =
            prepare_bytes_with_budget(&source, PERSON, SESSION, SOURCE_REVISION, measured)
                .expect("exact measured output budget fits");
        assert_eq!(exact_fit.measured_output_bytes(), measured);
        assert_eq!(exact_fit.prepared_record_output_byte_budget(), measured);

        assert!(matches!(
            prepare_bytes_with_budget(
                &source,
                PERSON,
                SESSION,
                SOURCE_REVISION,
                measured - 1,
            ),
            Err(LegacySessionPreparationError::PreparedOutputBudgetExceeded {
                budget_bytes,
                ..
            }) if budget_bytes == measured - 1
        ));
        assert!(matches!(
            prepare_bytes_with_budget(&source, PERSON, SESSION, SOURCE_REVISION, 0),
            Err(
                LegacySessionPreparationError::PreparedOutputBudgetExceeded {
                    budget_bytes: 0,
                    ..
                }
            )
        ));

        let empty_source = raw(&session_with(Vec::new()));
        let zero_budget_empty =
            prepare_bytes_with_budget(&empty_source, PERSON, SESSION, SOURCE_REVISION, 0)
                .expect("zero output budget admits an empty record set");
        assert_eq!(zero_budget_empty.measured_output_bytes(), 0);
        assert_eq!(zero_budget_empty.prepared_record_output_byte_budget(), 0);

        assert_eq!(
            prepare_bytes_with_budget(
                &empty_source,
                PERSON,
                SESSION,
                SOURCE_REVISION,
                MAX_PREPARED_LEGACY_SESSION_OUTPUT_CEILING_BYTES + 1,
            ),
            Err(LegacySessionPreparationError::InvalidPreparedOutputBudget {
                requested_bytes: MAX_PREPARED_LEGACY_SESSION_OUTPUT_CEILING_BYTES + 1,
                maximum_bytes: MAX_PREPARED_LEGACY_SESSION_OUTPUT_CEILING_BYTES,
            })
        );
    }

    #[test]
    fn replay_is_deterministic_and_carries_no_execution_authority() {
        let session = session_with(all_variants());
        let source = raw(&session);
        let first = prepare_bytes(&source, PERSON, SESSION, SOURCE_REVISION)
            .expect("first deterministic preparation");
        let replay = prepare_bytes(&source, PERSON, SESSION, SOURCE_REVISION)
            .expect("replayed deterministic preparation");

        assert_eq!(first, replay);
        assert_eq!(
            first.prepared_record_output_byte_budget(),
            MAX_PREPARED_LEGACY_SESSION_OUTPUT_CEILING_BYTES
        );
        assert_eq!(first.raw_source_bytes(), source);
        assert_eq!(
            first.authority().evidence_provenance(),
            TypedAgentMessageProvenance::ImportedLegacyUnproven
        );
        for record in first.records() {
            let authority = record.authority();
            assert_eq!(
                authority.evidence_provenance(),
                TypedAgentMessageProvenance::ImportedLegacyUnproven
            );
            assert_eq!(authority.coverage(), LegacyCoverageStatus::Unproven);
            assert_eq!(
                authority.external_references(),
                LegacyExternalReferenceStatus::Unverified
            );
            assert_eq!(authority.execution(), LegacyExecutionAuthority::None);
        }
        assert!(matches!(
            first.records()[5].message(),
            AgentMessage::Delegation {
                task,
                execution_receipt: Some(_),
                ..
            } if task.coverage == DependencyCoverage::Independent
        ));
        assert_eq!(
            first.records()[5].authority().coverage(),
            LegacyCoverageStatus::Unproven
        );
    }
}
