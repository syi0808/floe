//! Fixed encrypted layout. These declarations are shared by create, open and preflight.
use super::SchemaObject;

pub(super) const CORE: &[SchemaObject] = &[
    SchemaObject::marker(
        "vault_identity",
        super::ENCRYPTED_LAYOUT_VERSION,
        "CREATE TABLE vault_identity (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL, person_id TEXT NOT NULL, vault_id TEXT NOT NULL)",
    ),
    SchemaObject::table(
        "agent_sessions",
        "CREATE TABLE agent_sessions (id TEXT PRIMARY KEY, revision INTEGER NOT NULL, payload TEXT NOT NULL)",
    ),
];

pub(super) const KNOWLEDGE: &[SchemaObject] = &[
    SchemaObject::marker(
        "knowledge_store_schema",
        1,
        "CREATE TABLE knowledge_store_schema (id INTEGER PRIMARY KEY CHECK(id = 1), version INTEGER NOT NULL CHECK(version = 1))",
    ),
    SchemaObject::table(
        "learning_observations",
        "CREATE TABLE learning_observations (id TEXT PRIMARY KEY, person_id TEXT NOT NULL, content_hash TEXT NOT NULL, payload TEXT NOT NULL, UNIQUE(person_id, content_hash))",
    ),
    SchemaObject::table(
        "knowledge_candidates",
        "CREATE TABLE knowledge_candidates (id TEXT PRIMARY KEY, person_id TEXT NOT NULL, idempotency_key TEXT NOT NULL, kind TEXT NOT NULL, state TEXT NOT NULL, target_id TEXT, created_at TEXT NOT NULL, payload TEXT NOT NULL, stage_payload TEXT NOT NULL, UNIQUE(person_id, idempotency_key))",
    ),
    SchemaObject::index(
        "knowledge_candidates_review",
        "CREATE INDEX knowledge_candidates_review ON knowledge_candidates(person_id, kind, state, created_at)",
    ),
    SchemaObject::table(
        "knowledge_stage_receipts",
        "CREATE TABLE knowledge_stage_receipts (person_id TEXT NOT NULL, candidate_key TEXT NOT NULL, candidate_id TEXT NOT NULL UNIQUE, observation_id TEXT NOT NULL, payload TEXT NOT NULL, PRIMARY KEY(person_id, candidate_key))",
    ),
    SchemaObject::table(
        "knowledge_candidate_decisions",
        "CREATE TABLE knowledge_candidate_decisions (id TEXT PRIMARY KEY, candidate_id TEXT NOT NULL UNIQUE, payload TEXT NOT NULL)",
    ),
    SchemaObject::table(
        "knowledge_revisions",
        "CREATE TABLE knowledge_revisions (target_id TEXT NOT NULL, revision INTEGER NOT NULL, person_id TEXT NOT NULL, kind TEXT NOT NULL, state TEXT NOT NULL, payload TEXT NOT NULL, PRIMARY KEY(target_id, revision))",
    ),
    SchemaObject::index(
        "knowledge_revisions_active",
        "CREATE INDEX knowledge_revisions_active ON knowledge_revisions(person_id, kind, state)",
    ),
    SchemaObject::table(
        "knowledge_mutations",
        "CREATE TABLE knowledge_mutations (id TEXT PRIMARY KEY, candidate_id TEXT NOT NULL UNIQUE, target_id TEXT NOT NULL, created_at TEXT NOT NULL, payload TEXT NOT NULL)",
    ),
    SchemaObject::table(
        "knowledge_command_receipts",
        "CREATE TABLE knowledge_command_receipts (command_id TEXT PRIMARY KEY, person_id TEXT NOT NULL, device_id TEXT NOT NULL, command_kind TEXT NOT NULL, candidate_id TEXT NOT NULL, decision_kind TEXT NOT NULL, payload TEXT NOT NULL)",
    ),
    SchemaObject::table(
        "learner_review_jobs",
        "CREATE TABLE learner_review_jobs (id TEXT PRIMARY KEY, person_id TEXT NOT NULL, idempotency_key TEXT NOT NULL, state TEXT NOT NULL, attempts INTEGER NOT NULL, available_at TEXT NOT NULL, payload TEXT NOT NULL, UNIQUE(person_id, idempotency_key))",
    ),
    SchemaObject::index(
        "learner_review_jobs_ready",
        "CREATE INDEX learner_review_jobs_ready ON learner_review_jobs(person_id, state, available_at)",
    ),
    SchemaObject::table(
        "learner_execution_journal",
        "CREATE TABLE learner_execution_journal (job_id TEXT NOT NULL, person_id TEXT NOT NULL, claim_attempt INTEGER NOT NULL, sequence INTEGER NOT NULL, event_key TEXT NOT NULL, payload TEXT NOT NULL, PRIMARY KEY(job_id, person_id, claim_attempt, sequence), UNIQUE(job_id, person_id, claim_attempt, event_key))",
    ),
    SchemaObject::table(
        "learner_journal_heads",
        "CREATE TABLE learner_journal_heads (job_id TEXT NOT NULL, person_id TEXT NOT NULL, claim_attempt INTEGER NOT NULL, device_id TEXT NOT NULL, journal_revision INTEGER NOT NULL, journal_digest TEXT NOT NULL, payload TEXT NOT NULL, PRIMARY KEY(job_id, person_id, claim_attempt))",
    ),
    SchemaObject::table(
        "learner_settlement_receipts",
        "CREATE TABLE learner_settlement_receipts (job_id TEXT NOT NULL, person_id TEXT NOT NULL, claim_attempt INTEGER NOT NULL, device_id TEXT NOT NULL, settlement TEXT NOT NULL, result TEXT NOT NULL, PRIMARY KEY(job_id, person_id, claim_attempt))",
    ),
];

pub(super) const ACCESS: &[SchemaObject] = &[
    SchemaObject::marker(
        "data_access_grant_schema",
        2,
        "CREATE TABLE data_access_grant_schema (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL CHECK (version = 2))",
    ),
    SchemaObject::table(
        "data_access_grants",
        "CREATE TABLE data_access_grants (grant_id TEXT PRIMARY KEY, person_id TEXT NOT NULL, authority_owner TEXT NOT NULL, connection_id TEXT NOT NULL, connector TEXT NOT NULL, execution_owner TEXT NOT NULL, grant_incarnation TEXT NOT NULL, access_epoch INTEGER NOT NULL, state TEXT NOT NULL, payload TEXT NOT NULL)",
    ),
    SchemaObject::index(
        "data_access_grants_person_state",
        "CREATE INDEX data_access_grants_person_state ON data_access_grants (person_id, state, grant_id)",
    ),
    SchemaObject::table(
        "data_access_grant_cleanup",
        "CREATE TABLE data_access_grant_cleanup (cleanup_id TEXT PRIMARY KEY, grant_id TEXT NOT NULL, person_id TEXT NOT NULL, invalidated_incarnation TEXT NOT NULL, invalidated_epoch INTEGER NOT NULL, payload TEXT NOT NULL, UNIQUE(grant_id, invalidated_incarnation, invalidated_epoch))",
    ),
    SchemaObject::index(
        "data_access_grant_cleanup_ready",
        "CREATE INDEX data_access_grant_cleanup_ready ON data_access_grant_cleanup (person_id, grant_id, invalidated_epoch)",
    ),
];

pub(super) const REVIEWS: &[SchemaObject] = &[
    SchemaObject::table(
        "access_connection_reviews",
        "CREATE TABLE access_connection_reviews (review_id TEXT PRIMARY KEY, person_id TEXT NOT NULL, command_id TEXT NOT NULL, intent_digest TEXT NOT NULL, payload TEXT NOT NULL, UNIQUE(person_id, command_id))",
    ),
    SchemaObject::table(
        "access_grant_operations",
        "CREATE TABLE access_grant_operations (operation_id TEXT PRIMARY KEY, person_id TEXT NOT NULL, payload TEXT NOT NULL)",
    ),
];

pub(super) const ACTIONS: &[SchemaObject] = &[
    SchemaObject::marker(
        "actions_schema",
        1,
        "CREATE TABLE actions_schema (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL CHECK (version = 1))",
    ),
    SchemaObject::table(
        "actions_records",
        "CREATE TABLE actions_records (person_id TEXT NOT NULL, action_id TEXT NOT NULL, revision INTEGER NOT NULL CHECK (revision > 0), effect_digest TEXT NOT NULL CHECK (length(effect_digest) = 64), execution_id TEXT NOT NULL, state TEXT NOT NULL CHECK (state IN ('pending_review', 'approved', 'rejected', 'cancelled', 'expired', 'executing', 'blocked', 'failed', 'unknown', 'succeeded')), collection_state TEXT NOT NULL CHECK (collection_state IN ('none', 'pending', 'collected')), dispatch_revision INTEGER NOT NULL CHECK (dispatch_revision >= 0), grant_key TEXT NOT NULL, payload TEXT NOT NULL CHECK (length(CAST(payload AS BLOB)) > 0 AND length(CAST(payload AS BLOB)) <= 65536), origin_kind TEXT NOT NULL CHECK (origin_kind IN ('direct', 'expert')), PRIMARY KEY (person_id, action_id), CHECK ((origin_kind = 'direct' AND grant_key = '') OR (origin_kind = 'expert' AND grant_key <> '')), CHECK ((state = 'succeeded' AND collection_state IN ('pending', 'collected')) OR (state <> 'succeeded' AND collection_state = 'none')), CHECK ((state IN ('executing', 'failed', 'unknown', 'succeeded') AND dispatch_revision > 0) OR (state NOT IN ('executing', 'failed', 'unknown', 'succeeded') AND dispatch_revision = 0)))",
    ),
    SchemaObject::table(
        "actions_authorities",
        "CREATE TABLE actions_authorities (person_id TEXT PRIMARY KEY, revision INTEGER NOT NULL CHECK (revision > 0), mode TEXT NOT NULL CHECK (mode IN ('allow', 'ask', 'deny')), digest TEXT NOT NULL CHECK (length(digest) = 64), payload TEXT NOT NULL CHECK (length(CAST(payload AS BLOB)) > 0 AND length(CAST(payload AS BLOB)) <= 4096))",
    ),
    SchemaObject::table(
        "actions_command_receipts",
        "CREATE TABLE actions_command_receipts (person_id TEXT NOT NULL, command_id TEXT NOT NULL, kind TEXT NOT NULL CHECK (kind IN ('submit', 'decision', 'reconciliation', 'authority')), intent_digest TEXT NOT NULL CHECK (length(intent_digest) = 64), action_id TEXT NOT NULL, PRIMARY KEY (person_id, command_id), CHECK ((kind = 'authority' AND action_id = '') OR (kind <> 'authority' AND action_id <> '')))",
    ),
    SchemaObject::table(
        "actions_settlement_receipts",
        "CREATE TABLE actions_settlement_receipts (person_id TEXT NOT NULL, execution_id TEXT NOT NULL, expected_revision INTEGER NOT NULL CHECK (expected_revision > 0), action_id TEXT NOT NULL, effect_digest TEXT NOT NULL CHECK (length(effect_digest) = 64), outcome_digest TEXT NOT NULL CHECK (length(outcome_digest) = 64), PRIMARY KEY (person_id, execution_id, expected_revision))",
    ),
    SchemaObject::table(
        "actions_collection_receipts",
        "CREATE TABLE actions_collection_receipts (person_id TEXT NOT NULL, execution_id TEXT NOT NULL, receipt_digest TEXT NOT NULL CHECK (length(receipt_digest) = 64), ticket_revision INTEGER NOT NULL CHECK (ticket_revision > 0), action_id TEXT NOT NULL, intent_digest TEXT NOT NULL CHECK (length(intent_digest) = 64), PRIMARY KEY (person_id, execution_id, receipt_digest, ticket_revision))",
    ),
    SchemaObject::index(
        "actions_records_execution",
        "CREATE UNIQUE INDEX actions_records_execution ON actions_records (execution_id)",
    ),
    SchemaObject::index(
        "actions_records_page",
        "CREATE INDEX actions_records_page ON actions_records (person_id, action_id)",
    ),
    SchemaObject::index(
        "actions_records_recovery",
        "CREATE INDEX actions_records_recovery ON actions_records (person_id, state, collection_state, action_id)",
    ),
    SchemaObject::index(
        "actions_records_grant",
        "CREATE INDEX actions_records_grant ON actions_records (person_id, grant_key, state, action_id)",
    ),
];

pub(super) const BINDINGS: &[SchemaObject] = &[
    SchemaObject::table(
        "agent_expert_command_admissions",
        "CREATE TABLE agent_expert_command_admissions (command_id TEXT PRIMARY KEY, family TEXT NOT NULL CHECK (family IN ('binding_prepare', 'registry', 'binding_replacement')), person_id TEXT NOT NULL, device_id TEXT NOT NULL, request_digest TEXT NOT NULL CHECK (length(request_digest) = 64), review_id TEXT NOT NULL CHECK ((family = 'registry' AND review_id = '') OR (family != 'registry' AND length(review_id) = 36)))",
    ),
    SchemaObject::table(
        "agent_expert_binding_reviews",
        "CREATE TABLE agent_expert_binding_reviews (review_id TEXT PRIMARY KEY, person_id TEXT NOT NULL, device_id TEXT NOT NULL, command_id TEXT NOT NULL UNIQUE, assignment_id TEXT NOT NULL, requirement_key TEXT NOT NULL, review_digest TEXT NOT NULL CHECK (length(review_digest) = 64), payload TEXT NOT NULL, UNIQUE (person_id, command_id))",
    ),
    SchemaObject::table(
        "agent_expert_registry_receipts",
        "CREATE TABLE agent_expert_registry_receipts (command_id TEXT PRIMARY KEY, person_id TEXT NOT NULL, device_id TEXT NOT NULL, request_digest TEXT NOT NULL CHECK (length(request_digest) = 64), snapshot_revision INTEGER NOT NULL, payload TEXT NOT NULL)",
    ),
    SchemaObject::table(
        "agent_expert_binding_review_consumptions",
        "CREATE TABLE agent_expert_binding_review_consumptions (review_id TEXT PRIMARY KEY, command_id TEXT NOT NULL UNIQUE, person_id TEXT NOT NULL, device_id TEXT NOT NULL, review_digest TEXT NOT NULL CHECK (length(review_digest) = 64), request_digest TEXT NOT NULL CHECK (length(request_digest) = 64), committed_at_unix_ms INTEGER NOT NULL)",
    ),
    SchemaObject::table(
        "agent_expert_binding_replacement_receipts",
        "CREATE TABLE agent_expert_binding_replacement_receipts (command_id TEXT PRIMARY KEY, consumed_review_id TEXT NOT NULL UNIQUE, person_id TEXT NOT NULL, device_id TEXT NOT NULL, review_digest TEXT NOT NULL CHECK (length(review_digest) = 64), request_digest TEXT NOT NULL CHECK (length(request_digest) = 64), committed_at_unix_ms INTEGER NOT NULL, payload TEXT NOT NULL)",
    ),
];

pub(super) const CONTEXT: &[SchemaObject] = &[
    SchemaObject::marker(
        "agent_context_dependency_schema",
        1,
        "CREATE TABLE agent_context_dependency_schema (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL CHECK (version = 1))",
    ),
    SchemaObject::table(
        "agent_context_dependency_coverage",
        "CREATE TABLE agent_context_dependency_coverage (person_id TEXT NOT NULL, session_id TEXT NOT NULL, turn_id TEXT NOT NULL, version INTEGER NOT NULL CHECK (version = 1), payload TEXT NOT NULL CHECK (length(CAST(payload AS BLOB)) <= 65536), PRIMARY KEY (person_id, session_id, turn_id))",
    ),
    SchemaObject::index(
        "agent_context_dependency_coverage_session_idx",
        "CREATE INDEX agent_context_dependency_coverage_session_idx ON agent_context_dependency_coverage (person_id, session_id, turn_id)",
    ),
];

pub(super) const CONVERSATION: &[SchemaObject] = &[
    SchemaObject::marker(
        "agent_conversation_schema",
        10,
        "CREATE TABLE agent_conversation_schema (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL CHECK (version = 10))",
    ),
    SchemaObject::table(
        "agent_conversation_executor",
        "CREATE TABLE agent_conversation_executor (id INTEGER PRIMARY KEY CHECK (id = 1), generation INTEGER NOT NULL CHECK (generation >= 0))",
    ),
    SchemaObject::table(
        "agent_conversation_runs",
        "CREATE TABLE agent_conversation_runs (run_id TEXT PRIMARY KEY, command_id TEXT NOT NULL UNIQUE, session_id TEXT NOT NULL, person_id TEXT NOT NULL, state TEXT NOT NULL CHECK (state IN ('working', 'blocked', 'completed', 'failed', 'cancelled', 'timed_out', 'interrupted')), aggregate_revision INTEGER NOT NULL CHECK (aggregate_revision > 0), journal_revision INTEGER NOT NULL CHECK (journal_revision >= 0), executor_generation INTEGER NOT NULL CHECK (executor_generation > 0), payload TEXT NOT NULL CHECK (length(CAST(payload AS BLOB)) <= 131072))",
    ),
    SchemaObject::table(
        "agent_conversation_journal",
        "CREATE TABLE agent_conversation_journal (run_id TEXT NOT NULL, revision INTEGER NOT NULL CHECK (revision > 0), kind TEXT NOT NULL, payload TEXT NOT NULL CHECK (length(CAST(payload AS BLOB)) <= 1052672), PRIMARY KEY (run_id, revision), FOREIGN KEY (run_id) REFERENCES agent_conversation_runs(run_id))",
    ),
    SchemaObject::table(
        "agent_conversation_commands",
        "CREATE TABLE agent_conversation_commands (command_id TEXT PRIMARY KEY, person_id TEXT NOT NULL, kind TEXT NOT NULL CHECK (kind IN ('cancel_run')), target_id TEXT NOT NULL, FOREIGN KEY (target_id) REFERENCES agent_conversation_runs(run_id))",
    ),
    SchemaObject::table(
        "agent_conversation_resume_slots",
        "CREATE TABLE agent_conversation_resume_slots (origin_run_id TEXT PRIMARY KEY, child_run_id TEXT NOT NULL, child_command_id TEXT NOT NULL, session_id TEXT NOT NULL, person_id TEXT NOT NULL, FOREIGN KEY (child_run_id) REFERENCES agent_conversation_runs(run_id))",
    ),
    SchemaObject::table(
        "agent_conversation_resume_requests",
        "CREATE TABLE agent_conversation_resume_requests (origin_run_id TEXT PRIMARY KEY, person_id TEXT NOT NULL, session_id TEXT NOT NULL, state TEXT NOT NULL CHECK (state IN ('pending','claimed','superseded')), child_run_id TEXT, payload TEXT NOT NULL)",
    ),
    SchemaObject::table(
        "agent_conversation_session_commands",
        "CREATE TABLE agent_conversation_session_commands (command_id TEXT PRIMARY KEY, person_id TEXT NOT NULL, session_id TEXT NOT NULL REFERENCES agent_sessions(id), initial_revision INTEGER NOT NULL CHECK(initial_revision = 0))",
    ),
    SchemaObject::table(
        "agent_conversation_terminal_receipts",
        "CREATE TABLE agent_conversation_terminal_receipts (run_id TEXT PRIMARY KEY REFERENCES agent_conversation_runs(run_id), digest TEXT NOT NULL CHECK(length(digest) = 64))",
    ),
    SchemaObject::table(
        "agent_conversation_review_audits",
        "CREATE TABLE agent_conversation_review_audits (operation_id TEXT NOT NULL, run_id TEXT NOT NULL, person_id TEXT NOT NULL, payload TEXT NOT NULL, PRIMARY KEY(run_id, operation_id))",
    ),
    SchemaObject::index(
        "agent_conversation_active_session",
        "CREATE INDEX agent_conversation_active_session ON agent_conversation_runs (session_id, state, run_id)",
    ),
];

/// Root-owned recorder custody family, versioned independently from the
/// Session/Run journal family. It reuses the stable Core marker table name and
/// bumps its stored value so older binaries fail before reading normalized rows.
pub(super) const CONVERSATION_CORE_V3: &[SchemaObject] = &[
    SchemaObject::marker(
        "agent_conversation_core_schema",
        3,
        "CREATE TABLE agent_conversation_core_schema (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL CHECK (version = 3))",
    ),
    SchemaObject::table(
        "agent_conversation_core_v3_heads",
        "CREATE TABLE agent_conversation_core_v3_heads (person_id TEXT NOT NULL, conversation_id TEXT NOT NULL, branch_id TEXT NOT NULL, identity_json TEXT NOT NULL CHECK (length(CAST(identity_json AS BLOB)) BETWEEN 1 AND 1024), head_revision INTEGER NOT NULL CHECK (head_revision >= 0), state_revision INTEGER NOT NULL CHECK (state_revision >= 0), settled_prefix INTEGER NOT NULL CHECK (settled_prefix >= 0 AND settled_prefix <= head_revision), recorder_epoch INTEGER NOT NULL CHECK (recorder_epoch >= 0), PRIMARY KEY (person_id, conversation_id, branch_id))",
    ),
    SchemaObject::table(
        "agent_conversation_core_v3_entries",
        "CREATE TABLE agent_conversation_core_v3_entries (person_id TEXT NOT NULL, conversation_id TEXT NOT NULL, branch_id TEXT NOT NULL, sequence INTEGER NOT NULL CHECK (sequence > 0), message_id TEXT NOT NULL, message_json TEXT NOT NULL CHECK (length(CAST(message_json AS BLOB)) BETWEEN 1 AND 132096), message_bytes INTEGER NOT NULL CHECK (message_bytes = length(CAST(message_json AS BLOB))), entry_kind TEXT NOT NULL CHECK (entry_kind IN ('inbound', 'generated_output')), producer_run_id TEXT, contribution_id TEXT, producing_task_json TEXT CHECK (producing_task_json IS NULL OR length(CAST(producing_task_json AS BLOB)) BETWEEN 1 AND 4096), prefix_digest TEXT NOT NULL CHECK (length(prefix_digest) = 64), PRIMARY KEY (person_id, conversation_id, branch_id, sequence), UNIQUE (person_id, conversation_id, branch_id, message_id), FOREIGN KEY (person_id, conversation_id, branch_id) REFERENCES agent_conversation_core_v3_heads(person_id, conversation_id, branch_id), CHECK ((entry_kind = 'inbound' AND producer_run_id IS NULL AND contribution_id IS NULL AND producing_task_json IS NULL) OR (entry_kind = 'generated_output' AND producer_run_id IS NOT NULL AND contribution_id IS NOT NULL)))",
    ),
    SchemaObject::table(
        "agent_conversation_core_v3_input_receipts",
        "CREATE TABLE agent_conversation_core_v3_input_receipts (person_id TEXT NOT NULL, conversation_id TEXT NOT NULL, branch_id TEXT NOT NULL, message_id TEXT NOT NULL, sequence INTEGER NOT NULL CHECK (sequence > 0), receipt_json TEXT NOT NULL CHECK (length(CAST(receipt_json AS BLOB)) BETWEEN 1 AND 2048), PRIMARY KEY (person_id, conversation_id, branch_id, message_id), UNIQUE (person_id, conversation_id, branch_id, sequence), FOREIGN KEY (person_id, conversation_id, branch_id, sequence) REFERENCES agent_conversation_core_v3_entries(person_id, conversation_id, branch_id, sequence))",
    ),
    SchemaObject::table(
        "agent_conversation_core_v3_owner_bindings",
        "CREATE TABLE agent_conversation_core_v3_owner_bindings (person_id TEXT NOT NULL, run_id TEXT NOT NULL, session_id TEXT NOT NULL, owner_user_message_id TEXT NOT NULL, conversation_id TEXT NOT NULL, branch_id TEXT NOT NULL, input_sequence INTEGER NOT NULL CHECK (input_sequence > 0), input_message_id TEXT NOT NULL, binding_json TEXT NOT NULL CHECK (length(CAST(binding_json AS BLOB)) BETWEEN 1 AND 4096), PRIMARY KEY (person_id, run_id), FOREIGN KEY (person_id, conversation_id, branch_id, input_sequence) REFERENCES agent_conversation_core_v3_entries(person_id, conversation_id, branch_id, sequence))",
    ),
    SchemaObject::index(
        "agent_conversation_core_v3_owner_bindings_input",
        "CREATE INDEX agent_conversation_core_v3_owner_bindings_input ON agent_conversation_core_v3_owner_bindings(person_id, session_id, owner_user_message_id, run_id)",
    ),
    SchemaObject::table(
        "agent_conversation_core_v3_open_receipts",
        "CREATE TABLE agent_conversation_core_v3_open_receipts (person_id TEXT NOT NULL, run_id TEXT NOT NULL, conversation_id TEXT NOT NULL, branch_id TEXT NOT NULL, receipt_json TEXT NOT NULL CHECK (length(CAST(receipt_json AS BLOB)) BETWEEN 1 AND 4096), PRIMARY KEY (person_id, run_id))",
    ),
    SchemaObject::table(
        "agent_conversation_core_v3_active_recorders",
        "CREATE TABLE agent_conversation_core_v3_active_recorders (person_id TEXT NOT NULL, conversation_id TEXT NOT NULL, branch_id TEXT NOT NULL, run_id TEXT NOT NULL, fence_json TEXT NOT NULL CHECK (length(CAST(fence_json AS BLOB)) BETWEEN 1 AND 4096), PRIMARY KEY (person_id, conversation_id, branch_id), UNIQUE (person_id, run_id), FOREIGN KEY (person_id, conversation_id, branch_id) REFERENCES agent_conversation_core_v3_heads(person_id, conversation_id, branch_id))",
    ),
    SchemaObject::table(
        "agent_conversation_core_v3_recording_receipts",
        "CREATE TABLE agent_conversation_core_v3_recording_receipts (person_id TEXT NOT NULL, contribution_id TEXT NOT NULL, conversation_id TEXT NOT NULL, branch_id TEXT NOT NULL, run_id TEXT NOT NULL, sequence INTEGER NOT NULL CHECK (sequence > 0), message_id TEXT NOT NULL, receipt_json TEXT NOT NULL CHECK (length(CAST(receipt_json AS BLOB)) BETWEEN 1 AND 8192), PRIMARY KEY (person_id, contribution_id), UNIQUE (person_id, conversation_id, branch_id, sequence), UNIQUE (person_id, conversation_id, branch_id, message_id), FOREIGN KEY (person_id, conversation_id, branch_id, sequence) REFERENCES agent_conversation_core_v3_entries(person_id, conversation_id, branch_id, sequence))",
    ),
    SchemaObject::table(
        "agent_conversation_core_v3_close_receipts",
        "CREATE TABLE agent_conversation_core_v3_close_receipts (person_id TEXT NOT NULL, run_id TEXT NOT NULL, conversation_id TEXT NOT NULL, branch_id TEXT NOT NULL, receipt_json TEXT NOT NULL CHECK (length(CAST(receipt_json AS BLOB)) BETWEEN 1 AND 8192), PRIMARY KEY (person_id, run_id))",
    ),
    SchemaObject::table(
        "agent_conversation_core_v3_retirement_receipts",
        "CREATE TABLE agent_conversation_core_v3_retirement_receipts (person_id TEXT NOT NULL, run_id TEXT NOT NULL, conversation_id TEXT NOT NULL, branch_id TEXT NOT NULL, receipt_json TEXT NOT NULL CHECK (length(CAST(receipt_json AS BLOB)) BETWEEN 1 AND 8192), PRIMARY KEY (person_id, run_id))",
    ),
    SchemaObject::table(
        "agent_conversation_core_v3_checkpoints",
        "CREATE TABLE agent_conversation_core_v3_checkpoints (person_id TEXT NOT NULL, conversation_id TEXT NOT NULL, branch_id TEXT NOT NULL, sequence INTEGER NOT NULL CHECK (sequence > 0), prefix_digest TEXT NOT NULL CHECK (length(prefix_digest) = 64), summary TEXT NOT NULL CHECK (length(CAST(summary AS BLOB)) BETWEEN 1 AND 65536), PRIMARY KEY (person_id, conversation_id, branch_id), FOREIGN KEY (person_id, conversation_id, branch_id, sequence) REFERENCES agent_conversation_core_v3_entries(person_id, conversation_id, branch_id, sequence))",
    ),
];

pub(super) const CONVERSATION_CORE_OUTPUTS_V2: &[SchemaObject] = &[
    SchemaObject::marker(
        "agent_conversation_core_outputs_schema",
        2,
        "CREATE TABLE agent_conversation_core_outputs_schema (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL CHECK (version = 2))",
    ),
    SchemaObject::table(
        "agent_conversation_core_output_receipts_v2",
        "CREATE TABLE agent_conversation_core_output_receipts_v2 (person_id TEXT NOT NULL, agent_instance_id TEXT NOT NULL, message_id TEXT NOT NULL, contribution_id TEXT NOT NULL, conversation_id TEXT NOT NULL, branch_id TEXT NOT NULL, producer_run_id TEXT NOT NULL, sequence INTEGER NOT NULL CHECK (sequence > 0), content_digest TEXT NOT NULL CHECK (length(content_digest) = 64), producing_task_json TEXT CHECK (producing_task_json IS NULL OR length(CAST(producing_task_json AS BLOB)) BETWEEN 1 AND 4096), receipt_json TEXT NOT NULL CHECK (length(CAST(receipt_json AS BLOB)) BETWEEN 1 AND 8192), PRIMARY KEY (person_id, contribution_id), UNIQUE (person_id, agent_instance_id, message_id), FOREIGN KEY (person_id, conversation_id, branch_id, sequence) REFERENCES agent_conversation_core_v3_entries(person_id, conversation_id, branch_id, sequence))",
    ),
];

/// Optional owner-defined typed Session evidence. This family is created only
/// by an explicit typed-history write transaction, never by Vault creation or
/// open/read inspection.
pub(super) const TYPED_HISTORY_V2: &[SchemaObject] = &[
    SchemaObject::marker(
        "agent_conversation_typed_history_schema",
        2,
        "CREATE TABLE agent_conversation_typed_history_schema (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL CHECK (version = 2))",
    ),
    SchemaObject::table(
        "agent_conversation_typed_history_entries",
        "CREATE TABLE agent_conversation_typed_history_entries (person_id TEXT NOT NULL, owner_namespace TEXT NOT NULL CHECK (owner_namespace = 'floe.conversation.session'), session_id TEXT NOT NULL, entry_id TEXT NOT NULL, turn_id TEXT NOT NULL, provenance TEXT NOT NULL CHECK (provenance = 'owner_recorded'), schema_id TEXT NOT NULL CHECK (length(schema_id) BETWEEN 1 AND 128), schema_version INTEGER NOT NULL CHECK (schema_version = 2), payload_byte_length INTEGER NOT NULL, digest TEXT NOT NULL CHECK (length(digest) = 64), payload TEXT NOT NULL CHECK (length(CAST(payload AS BLOB)) BETWEEN 1 AND 2097152), PRIMARY KEY (person_id, owner_namespace, session_id, entry_id), FOREIGN KEY (session_id) REFERENCES agent_sessions(id))",
    ),
    SchemaObject::index(
        "agent_conversation_typed_history_session_turn",
        "CREATE INDEX agent_conversation_typed_history_session_turn ON agent_conversation_typed_history_entries(person_id, session_id, turn_id, entry_id)",
    ),
    SchemaObject::index(
        "agent_conversation_typed_history_session_digest",
        "CREATE UNIQUE INDEX agent_conversation_typed_history_session_digest ON agent_conversation_typed_history_entries(person_id, owner_namespace, session_id, digest)",
    ),
];

/// Optional immutable proofs joining owner Session evidence and exact Core
/// transcript entries. The family has one stored meaning and is initialized
/// only by an explicit composed owner/Core write.
pub(super) const CONVERSATION_OWNER_CUSTODY_V2: &[SchemaObject] = &[
    SchemaObject::marker(
        "agent_conversation_owner_custody_schema_v2",
        3,
        "CREATE TABLE agent_conversation_owner_custody_schema_v2 (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL CHECK (version = 3))",
    ),
    SchemaObject::table(
        "agent_conversation_manager_session_bindings_v2",
        "CREATE TABLE agent_conversation_manager_session_bindings_v2 (person_id TEXT NOT NULL, session_id TEXT NOT NULL, identity_json TEXT NOT NULL CHECK (length(CAST(identity_json AS BLOB)) BETWEEN 1 AND 4096), conversation_id TEXT NOT NULL, branch_id TEXT NOT NULL, binding_json TEXT NOT NULL CHECK (length(CAST(binding_json AS BLOB)) BETWEEN 1 AND 8192), PRIMARY KEY (person_id, session_id), UNIQUE (person_id, conversation_id, branch_id), FOREIGN KEY (session_id) REFERENCES agent_sessions(id))",
    ),
    SchemaObject::table(
        "agent_conversation_manager_session_aliases_v2",
        "CREATE TABLE agent_conversation_manager_session_aliases_v2 (person_id TEXT NOT NULL, session_id TEXT NOT NULL, alias_id TEXT NOT NULL, conversation_id TEXT NOT NULL, branch_id TEXT NOT NULL, sequence INTEGER NOT NULL CHECK (sequence > 0), message_id TEXT NOT NULL, PRIMARY KEY (person_id, session_id, alias_id), UNIQUE (person_id, conversation_id, branch_id, sequence, message_id), FOREIGN KEY (person_id, conversation_id, branch_id, sequence) REFERENCES agent_conversation_core_v3_entries(person_id, conversation_id, branch_id, sequence), FOREIGN KEY (person_id, conversation_id, branch_id, message_id) REFERENCES agent_conversation_core_v3_entries(person_id, conversation_id, branch_id, message_id), FOREIGN KEY (session_id) REFERENCES agent_sessions(id))",
    ),
    SchemaObject::table(
        "agent_conversation_manager_archives_v3",
        "CREATE TABLE agent_conversation_manager_archives_v3 (person_id TEXT NOT NULL, session_id TEXT NOT NULL, archive_id TEXT NOT NULL, source_revision INTEGER NOT NULL CHECK (source_revision > 0), through_turn_id TEXT NOT NULL, summary_alias_id TEXT NOT NULL, message_count INTEGER NOT NULL CHECK (message_count BETWEEN 1 AND 256), conversation_id TEXT NOT NULL, branch_id TEXT NOT NULL, start_sequence INTEGER NOT NULL CHECK (start_sequence >= 0), through_sequence INTEGER NOT NULL CHECK (through_sequence > start_sequence), through_message_id TEXT NOT NULL, prefix_digest TEXT NOT NULL CHECK (length(prefix_digest) = 64), previous_archive_id TEXT, summary TEXT NOT NULL CHECK (length(CAST(summary AS BLOB)) BETWEEN 1 AND 16384), PRIMARY KEY (person_id, session_id, archive_id), UNIQUE (person_id, session_id, source_revision), UNIQUE (person_id, session_id, summary_alias_id), UNIQUE (person_id, conversation_id, branch_id, through_sequence), FOREIGN KEY (session_id) REFERENCES agent_sessions(id), FOREIGN KEY (person_id, conversation_id, branch_id, through_sequence) REFERENCES agent_conversation_core_v3_entries(person_id, conversation_id, branch_id, sequence), FOREIGN KEY (person_id, conversation_id, branch_id, through_message_id) REFERENCES agent_conversation_core_v3_entries(person_id, conversation_id, branch_id, message_id))",
    ),
    SchemaObject::index(
        "agent_conversation_manager_archives_checkpoint_v3",
        "CREATE INDEX agent_conversation_manager_archives_checkpoint_v3 ON agent_conversation_manager_archives_v3 (person_id, conversation_id, branch_id, through_sequence)",
    ),
    SchemaObject::table(
        "agent_conversation_owner_transcript_inputs_v1",
        "CREATE TABLE agent_conversation_owner_transcript_inputs_v1 (person_id TEXT NOT NULL, conversation_id TEXT NOT NULL, branch_id TEXT NOT NULL, input_sequence INTEGER NOT NULL CHECK (input_sequence > 0), input_message_id TEXT NOT NULL, session_id TEXT NOT NULL, owner_user_message_id TEXT NOT NULL, original_owner_run_id TEXT NOT NULL, mapping_json TEXT NOT NULL CHECK (length(CAST(mapping_json AS BLOB)) BETWEEN 1 AND 8192), UNIQUE (person_id, original_owner_run_id), FOREIGN KEY (person_id, conversation_id, branch_id, input_sequence) REFERENCES agent_conversation_core_v3_entries(person_id, conversation_id, branch_id, sequence), FOREIGN KEY (person_id, conversation_id, branch_id, input_message_id) REFERENCES agent_conversation_core_v3_entries(person_id, conversation_id, branch_id, message_id), FOREIGN KEY (session_id) REFERENCES agent_sessions(id))",
    ),
    SchemaObject::index(
        "agent_conversation_owner_transcript_inputs_owner_key_v1",
        "CREATE UNIQUE INDEX agent_conversation_owner_transcript_inputs_owner_key_v1 ON agent_conversation_owner_transcript_inputs_v1(person_id, session_id, owner_user_message_id)",
    ),
    SchemaObject::index(
        "agent_conversation_owner_transcript_inputs_core_key_v1",
        "CREATE UNIQUE INDEX agent_conversation_owner_transcript_inputs_core_key_v1 ON agent_conversation_owner_transcript_inputs_v1(person_id, conversation_id, branch_id, input_sequence, input_message_id)",
    ),
    SchemaObject::table(
        "agent_conversation_owner_transcript_run_inputs_v1",
        "CREATE TABLE agent_conversation_owner_transcript_run_inputs_v1 (person_id TEXT NOT NULL, run_id TEXT NOT NULL, conversation_id TEXT NOT NULL, branch_id TEXT NOT NULL, input_sequence INTEGER NOT NULL CHECK (input_sequence > 0), input_message_id TEXT NOT NULL, PRIMARY KEY (person_id, run_id), FOREIGN KEY (person_id, conversation_id, branch_id, input_sequence, input_message_id) REFERENCES agent_conversation_owner_transcript_inputs_v1(person_id, conversation_id, branch_id, input_sequence, input_message_id))",
    ),
    SchemaObject::table(
        "agent_conversation_owner_transcript_evidence_v1",
        "CREATE TABLE agent_conversation_owner_transcript_evidence_v1 (person_id TEXT NOT NULL, contribution_id TEXT NOT NULL, conversation_id TEXT NOT NULL, branch_id TEXT NOT NULL, sequence INTEGER NOT NULL CHECK (sequence > 0), message_id TEXT NOT NULL, owner_input_conversation_id TEXT NOT NULL, owner_input_branch_id TEXT NOT NULL, owner_input_sequence INTEGER NOT NULL CHECK (owner_input_sequence > 0), owner_input_message_id TEXT NOT NULL, session_id TEXT NOT NULL, typed_digest TEXT NOT NULL CHECK (length(typed_digest) = 64), typed_reference_json TEXT NOT NULL CHECK (length(CAST(typed_reference_json AS BLOB)) BETWEEN 1 AND 4096), transcript_entry_json TEXT NOT NULL CHECK (length(CAST(transcript_entry_json AS BLOB)) BETWEEN 1 AND 196608), first_recording_run_id TEXT NOT NULL, original_task_receipt_json TEXT CHECK (original_task_receipt_json IS NULL OR length(CAST(original_task_receipt_json AS BLOB)) BETWEEN 1 AND 4096), PRIMARY KEY (person_id, contribution_id), UNIQUE (person_id, session_id, typed_digest), UNIQUE (person_id, conversation_id, branch_id, sequence, message_id), FOREIGN KEY (person_id, conversation_id, branch_id, sequence) REFERENCES agent_conversation_core_v3_entries(person_id, conversation_id, branch_id, sequence), FOREIGN KEY (person_id, conversation_id, branch_id, message_id) REFERENCES agent_conversation_core_v3_entries(person_id, conversation_id, branch_id, message_id), FOREIGN KEY (person_id, owner_input_conversation_id, owner_input_branch_id, owner_input_sequence, owner_input_message_id) REFERENCES agent_conversation_owner_transcript_inputs_v1(person_id, conversation_id, branch_id, input_sequence, input_message_id), FOREIGN KEY (session_id) REFERENCES agent_sessions(id))",
    ),
];

pub(super) const INTERACTIONS: &[SchemaObject] = &[
    SchemaObject::marker(
        "agent_conversation_interaction_schema",
        2,
        "CREATE TABLE agent_conversation_interaction_schema (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL CHECK (version = 2))",
    ),
    SchemaObject::table(
        "agent_conversation_interactions",
        "CREATE TABLE agent_conversation_interactions (interaction_id TEXT PRIMARY KEY, session_id TEXT NOT NULL, person_id TEXT NOT NULL, origin_run_id TEXT NOT NULL, requirement_digest TEXT NOT NULL, target_digest TEXT NOT NULL, state TEXT NOT NULL CHECK (state IN ('pending', 'resolving', 'resolved', 'denied', 'cancelled', 'superseded', 'expired')), revision INTEGER NOT NULL CHECK (revision > 0), created_at INTEGER NOT NULL, expires_at INTEGER NOT NULL, payload TEXT NOT NULL CHECK (length(CAST(payload AS BLOB)) <= 32768))",
    ),
    SchemaObject::table(
        "agent_conversation_interaction_decisions",
        "CREATE TABLE agent_conversation_interaction_decisions (command_id TEXT PRIMARY KEY, interaction_id TEXT NOT NULL REFERENCES agent_conversation_interactions(interaction_id), kind TEXT NOT NULL CHECK (kind IN ('approve', 'deny', 'dismiss')), target_digest TEXT NOT NULL, principal TEXT NOT NULL, interaction_revision INTEGER NOT NULL CHECK (interaction_revision > 0), decided_at INTEGER NOT NULL)",
    ),
    SchemaObject::table(
        "agent_conversation_interaction_refreshes",
        "CREATE TABLE agent_conversation_interaction_refreshes (command_id TEXT PRIMARY KEY, person_id TEXT NOT NULL, session_id TEXT NOT NULL, interaction_id TEXT NOT NULL REFERENCES agent_conversation_interactions(interaction_id), expected_revision INTEGER NOT NULL CHECK(expected_revision > 0))",
    ),
    SchemaObject::index(
        "agent_conversation_interactions_run",
        "CREATE INDEX agent_conversation_interactions_run ON agent_conversation_interactions (origin_run_id, state, interaction_id)",
    ),
    SchemaObject::index(
        "agent_conversation_interactions_session",
        "CREATE INDEX agent_conversation_interactions_session ON agent_conversation_interactions (person_id, session_id, created_at, interaction_id)",
    ),
];

pub(super) const TASKS: &[SchemaObject] = &[
    SchemaObject::marker(
        "agent_task_schema",
        5,
        "CREATE TABLE agent_task_schema (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL CHECK (version = 5))",
    ),
    SchemaObject::table(
        "agent_task_executor",
        "CREATE TABLE agent_task_executor (id INTEGER PRIMARY KEY CHECK (id = 1), generation INTEGER NOT NULL CHECK (generation >= 0))",
    ),
    SchemaObject::table(
        "agent_tasks",
        "CREATE TABLE agent_tasks (task_id TEXT PRIMARY KEY, invocation_key TEXT NOT NULL UNIQUE, person_id TEXT NOT NULL, state TEXT NOT NULL CHECK (state IN ('submitted', 'working', 'completed', 'blocked', 'failed', 'rejected', 'cancelled', 'timed_out', 'interrupted')), aggregate_revision INTEGER NOT NULL CHECK (aggregate_revision > 0), executor_generation INTEGER NOT NULL CHECK (executor_generation > 0), payload TEXT NOT NULL CHECK (length(CAST(payload AS BLOB)) <= 524288))",
    ),
    SchemaObject::index(
        "agent_tasks_recovery",
        "CREATE INDEX agent_tasks_recovery ON agent_tasks (state, executor_generation, task_id)",
    ),
    SchemaObject::table(
        "agent_task_journal",
        "CREATE TABLE agent_task_journal (task_id TEXT NOT NULL, execution_id TEXT NOT NULL, executor_generation INTEGER NOT NULL CHECK (executor_generation > 0), revision INTEGER NOT NULL CHECK (revision BETWEEN 1 AND 512), kind TEXT NOT NULL CHECK (kind IN ('intent', 'result', 'output', 'checkpoint')), payload TEXT NOT NULL CHECK (length(CAST(payload AS BLOB)) <= 131072), PRIMARY KEY (task_id, execution_id, executor_generation, revision))",
    ),
    SchemaObject::index(
        "agent_task_journal_task_revision",
        "CREATE INDEX agent_task_journal_task_revision ON agent_task_journal (task_id, revision)",
    ),
];

/// Optional Expert Task to neutral-Core custody. This family is initialized
/// only by Task admission after proving that the profile has no legacy Task
/// rows whose missing Run mapping would otherwise be ambiguous.
pub(super) const EXPERT_TASK_CONVERSATIONS_V1: &[SchemaObject] = &[
    SchemaObject::marker(
        "agent_expert_task_conversation_schema_v1",
        1,
        "CREATE TABLE agent_expert_task_conversation_schema_v1 (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL CHECK (version = 1))",
    ),
    SchemaObject::table(
        "agent_expert_task_conversation_bindings_v1",
        "CREATE TABLE agent_expert_task_conversation_bindings_v1 (person_id TEXT NOT NULL, binding_key TEXT NOT NULL CHECK (length(binding_key) = 64), key_json TEXT NOT NULL CHECK (length(CAST(key_json AS BLOB)) BETWEEN 1 AND 4096), identity_json TEXT NOT NULL CHECK (length(CAST(identity_json AS BLOB)) BETWEEN 1 AND 4096), conversation_id TEXT NOT NULL, branch_id TEXT NOT NULL, PRIMARY KEY (person_id, binding_key), UNIQUE (person_id, conversation_id, branch_id))",
    ),
    SchemaObject::table(
        "agent_expert_task_conversation_runs_v1",
        "CREATE TABLE agent_expert_task_conversation_runs_v1 (person_id TEXT NOT NULL, task_id TEXT NOT NULL, execution_id TEXT NOT NULL, executor_generation INTEGER NOT NULL CHECK (executor_generation > 0), binding_key TEXT NOT NULL CHECK (length(binding_key) = 64), conversation_id TEXT NOT NULL, branch_id TEXT NOT NULL, run_id TEXT NOT NULL, input_json TEXT NOT NULL CHECK (length(CAST(input_json AS BLOB)) BETWEEN 1 AND 262144), input_commitment TEXT NOT NULL CHECK (length(input_commitment) = 64), input_sequence INTEGER CHECK (input_sequence IS NULL OR input_sequence > 0), input_message_id TEXT, input_reference_json TEXT CHECK (input_reference_json IS NULL OR length(CAST(input_reference_json AS BLOB)) BETWEEN 1 AND 1024), open_receipt_json TEXT CHECK (open_receipt_json IS NULL OR length(CAST(open_receipt_json AS BLOB)) BETWEEN 1 AND 8192), close_receipt_json TEXT CHECK (close_receipt_json IS NULL OR length(CAST(close_receipt_json AS BLOB)) BETWEEN 1 AND 8192), retirement_receipt_json TEXT CHECK (retirement_receipt_json IS NULL OR length(CAST(retirement_receipt_json AS BLOB)) BETWEEN 1 AND 8192), PRIMARY KEY (person_id, task_id, execution_id, executor_generation), UNIQUE (person_id, run_id), UNIQUE (person_id, conversation_id, branch_id, input_sequence, input_message_id), FOREIGN KEY (person_id, binding_key) REFERENCES agent_expert_task_conversation_bindings_v1(person_id, binding_key))",
    ),
    SchemaObject::table(
        "agent_expert_task_conversation_admissions_v1",
        "CREATE TABLE agent_expert_task_conversation_admissions_v1 (person_id TEXT NOT NULL, task_id TEXT NOT NULL, execution_id TEXT NOT NULL, executor_generation INTEGER NOT NULL CHECK (executor_generation > 0), request_digest TEXT NOT NULL CHECK (length(request_digest) = 64), input_commitment TEXT NOT NULL CHECK (length(input_commitment) = 64), reference_json TEXT NOT NULL CHECK (length(CAST(reference_json AS BLOB)) BETWEEN 1 AND 8192), PRIMARY KEY (person_id, task_id, execution_id, executor_generation), FOREIGN KEY (person_id, task_id, execution_id, executor_generation) REFERENCES agent_expert_task_conversation_runs_v1(person_id, task_id, execution_id, executor_generation))",
    ),
    SchemaObject::table(
        "agent_expert_task_conversation_reservations_v1",
        "CREATE TABLE agent_expert_task_conversation_reservations_v1 (person_id TEXT NOT NULL, binding_key TEXT NOT NULL CHECK (length(binding_key) = 64), task_id TEXT NOT NULL, execution_id TEXT NOT NULL, executor_generation INTEGER NOT NULL CHECK (executor_generation > 0), PRIMARY KEY (person_id, binding_key), UNIQUE (person_id, task_id, execution_id, executor_generation), FOREIGN KEY (person_id, binding_key) REFERENCES agent_expert_task_conversation_bindings_v1(person_id, binding_key), FOREIGN KEY (person_id, task_id, execution_id, executor_generation) REFERENCES agent_expert_task_conversation_runs_v1(person_id, task_id, execution_id, executor_generation))",
    ),
    SchemaObject::table(
        "agent_expert_task_conversation_entries_v1",
        "CREATE TABLE agent_expert_task_conversation_entries_v1 (person_id TEXT NOT NULL, conversation_id TEXT NOT NULL, branch_id TEXT NOT NULL, sequence INTEGER NOT NULL CHECK (sequence > 0), task_id TEXT NOT NULL, execution_id TEXT NOT NULL, executor_generation INTEGER NOT NULL CHECK (executor_generation > 0), journal_revision INTEGER NOT NULL CHECK (journal_revision >= 0), event_digest TEXT NOT NULL CHECK (length(event_digest) = 64), coverage_json TEXT NOT NULL CHECK (length(CAST(coverage_json AS BLOB)) BETWEEN 1 AND 16384), evidence_json TEXT NOT NULL CHECK (length(CAST(evidence_json AS BLOB)) BETWEEN 1 AND 131072), evidence_bytes INTEGER NOT NULL CHECK (evidence_bytes BETWEEN 1 AND 131072), PRIMARY KEY (person_id, conversation_id, branch_id, sequence), FOREIGN KEY (person_id, conversation_id, branch_id, sequence) REFERENCES agent_conversation_core_v3_entries(person_id, conversation_id, branch_id, sequence), FOREIGN KEY (person_id, task_id, execution_id, executor_generation) REFERENCES agent_expert_task_conversation_runs_v1(person_id, task_id, execution_id, executor_generation))",
    ),
];

pub(super) const CLEANUP: &[SchemaObject] = &[
    SchemaObject::marker(
        "agent_context_cleanup_schema",
        2,
        "CREATE TABLE agent_context_cleanup_schema (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL CHECK (version = 2))",
    ),
    SchemaObject::table(
        "agent_context_cleanup_applied",
        "CREATE TABLE agent_context_cleanup_applied (cleanup_id TEXT PRIMARY KEY, person_id TEXT NOT NULL, grant_id TEXT NOT NULL, invalidated_incarnation TEXT NOT NULL, invalidated_epoch INTEGER NOT NULL, payload TEXT NOT NULL CHECK (length(CAST(payload AS BLOB)) <= 65536), coverage_cursor INTEGER NOT NULL DEFAULT 0, coverage_complete INTEGER NOT NULL CHECK (coverage_complete IN (0, 1)))",
    ),
    SchemaObject::table(
        "agent_context_cleanup_suppression",
        "CREATE TABLE agent_context_cleanup_suppression (cleanup_id TEXT NOT NULL, person_id TEXT NOT NULL, session_id TEXT NOT NULL, turn_id TEXT NOT NULL, grant_id TEXT NOT NULL, invalidated_incarnation TEXT NOT NULL, invalidated_epoch INTEGER NOT NULL, payload TEXT NOT NULL CHECK (length(CAST(payload AS BLOB)) <= 65536), PRIMARY KEY (cleanup_id, session_id, turn_id))",
    ),
    SchemaObject::index(
        "agent_context_cleanup_suppression_turn_idx",
        "CREATE INDEX agent_context_cleanup_suppression_turn_idx ON agent_context_cleanup_suppression (person_id, session_id, turn_id)",
    ),
];

pub(super) const REGISTRY: &[SchemaObject] = &[SchemaObject::table(
    "agent_expert_registry",
    "CREATE TABLE agent_expert_registry (id INTEGER PRIMARY KEY CHECK (id = 1), revision INTEGER NOT NULL, payload TEXT NOT NULL)",
)];
