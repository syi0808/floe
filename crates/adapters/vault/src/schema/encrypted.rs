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

pub(super) const ARCHIVE: &[SchemaObject] = &[
    SchemaObject::table(
        "agent_session_archives",
        "CREATE TABLE agent_session_archives (id TEXT PRIMARY KEY, session_id TEXT NOT NULL, source_revision INTEGER NOT NULL, through_turn_id TEXT NOT NULL, message_count INTEGER NOT NULL, payload TEXT NOT NULL)",
    ),
    SchemaObject::index(
        "agent_session_archives_session",
        "CREATE INDEX agent_session_archives_session ON agent_session_archives(session_id, source_revision)",
    ),
    SchemaObject::table(
        "agent_session_search",
        "CREATE TABLE agent_session_search (session_id TEXT NOT NULL, archive_id TEXT NOT NULL, revision INTEGER NOT NULL, body TEXT NOT NULL, PRIMARY KEY(session_id, archive_id))",
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

/// Role-neutral Conversation Core custody is versioned independently from the
/// existing Session/Run journal family. The rows are append-oriented and never
/// encode a whole conversation as one growing payload.
pub(super) const CONVERSATION_CORE: &[SchemaObject] = &[
    SchemaObject::marker(
        "agent_conversation_core_schema",
        2,
        "CREATE TABLE agent_conversation_core_schema (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL CHECK (version = 2))",
    ),
    SchemaObject::table(
        "agent_conversation_core_heads",
        "CREATE TABLE agent_conversation_core_heads (person_id TEXT NOT NULL, conversation_id TEXT NOT NULL, branch_id TEXT NOT NULL, identity_json TEXT NOT NULL CHECK (length(CAST(identity_json AS BLOB)) BETWEEN 1 AND 1024), head_revision INTEGER NOT NULL CHECK (head_revision > 0), state_revision INTEGER NOT NULL CHECK (state_revision > 0), completed_prefix INTEGER NOT NULL CHECK (completed_prefix >= 0 AND completed_prefix <= head_revision), PRIMARY KEY (person_id, conversation_id, branch_id))",
    ),
    SchemaObject::table(
        "agent_conversation_core_entries",
        "CREATE TABLE agent_conversation_core_entries (person_id TEXT NOT NULL, conversation_id TEXT NOT NULL, branch_id TEXT NOT NULL, sequence INTEGER NOT NULL CHECK (sequence > 0), message_id TEXT NOT NULL, message_json TEXT NOT NULL CHECK (length(CAST(message_json AS BLOB)) BETWEEN 1 AND 132096), message_bytes INTEGER NOT NULL CHECK (message_bytes = length(CAST(message_json AS BLOB))), prefix_digest TEXT NOT NULL CHECK (length(prefix_digest) = 64), PRIMARY KEY (person_id, conversation_id, branch_id, sequence), UNIQUE (person_id, conversation_id, branch_id, message_id), FOREIGN KEY (person_id, conversation_id, branch_id) REFERENCES agent_conversation_core_heads(person_id, conversation_id, branch_id))",
    ),
    SchemaObject::table(
        "agent_conversation_core_message_receipts",
        "CREATE TABLE agent_conversation_core_message_receipts (person_id TEXT NOT NULL, conversation_id TEXT NOT NULL, branch_id TEXT NOT NULL, message_id TEXT NOT NULL, sequence INTEGER NOT NULL CHECK (sequence > 0), receipt_json TEXT NOT NULL CHECK (length(CAST(receipt_json AS BLOB)) BETWEEN 1 AND 1024), PRIMARY KEY (person_id, conversation_id, branch_id, message_id), UNIQUE (person_id, conversation_id, branch_id, sequence), FOREIGN KEY (person_id, conversation_id, branch_id, sequence) REFERENCES agent_conversation_core_entries(person_id, conversation_id, branch_id, sequence))",
    ),
    SchemaObject::table(
        "agent_conversation_core_command_receipts",
        "CREATE TABLE agent_conversation_core_command_receipts (person_id TEXT NOT NULL, command_id TEXT NOT NULL, conversation_id TEXT NOT NULL, branch_id TEXT NOT NULL, message_id TEXT NOT NULL, PRIMARY KEY (person_id, command_id), FOREIGN KEY (person_id, conversation_id, branch_id, message_id) REFERENCES agent_conversation_core_message_receipts(person_id, conversation_id, branch_id, message_id))",
    ),
    SchemaObject::index(
        "agent_conversation_core_command_scope",
        "CREATE INDEX agent_conversation_core_command_scope ON agent_conversation_core_command_receipts (person_id, conversation_id, branch_id, command_id, message_id)",
    ),
    SchemaObject::table(
        "agent_conversation_core_pending_inputs",
        "CREATE TABLE agent_conversation_core_pending_inputs (person_id TEXT NOT NULL, conversation_id TEXT NOT NULL, branch_id TEXT NOT NULL, sequence INTEGER NOT NULL CHECK (sequence > 0), message_id TEXT NOT NULL, PRIMARY KEY (person_id, conversation_id, branch_id, sequence), UNIQUE (person_id, conversation_id, branch_id, message_id), FOREIGN KEY (person_id, conversation_id, branch_id, sequence) REFERENCES agent_conversation_core_entries(person_id, conversation_id, branch_id, sequence))",
    ),
    SchemaObject::table(
        "agent_conversation_core_active_writers",
        "CREATE TABLE agent_conversation_core_active_writers (person_id TEXT NOT NULL, conversation_id TEXT NOT NULL, branch_id TEXT NOT NULL, run_id TEXT NOT NULL, task_id TEXT, message_sequence INTEGER NOT NULL CHECK (message_sequence > 0), writer_epoch INTEGER NOT NULL CHECK (writer_epoch > 0), executor_generation INTEGER NOT NULL CHECK (executor_generation > 0), PRIMARY KEY (person_id, conversation_id, branch_id), UNIQUE (person_id, conversation_id, branch_id, run_id), FOREIGN KEY (person_id, conversation_id, branch_id, message_sequence) REFERENCES agent_conversation_core_entries(person_id, conversation_id, branch_id, sequence))",
    ),
    SchemaObject::table(
        "agent_conversation_core_writer_receipts",
        "CREATE TABLE agent_conversation_core_writer_receipts (person_id TEXT NOT NULL, conversation_id TEXT NOT NULL, branch_id TEXT NOT NULL, run_id TEXT NOT NULL, task_id TEXT, message_sequence INTEGER NOT NULL CHECK (message_sequence > 0), writer_epoch INTEGER NOT NULL CHECK (writer_epoch > 0), executor_generation INTEGER NOT NULL CHECK (executor_generation > 0), state TEXT NOT NULL CHECK (state IN ('active', 'completed')), PRIMARY KEY (person_id, conversation_id, branch_id, run_id), UNIQUE (person_id, conversation_id, branch_id, message_sequence), FOREIGN KEY (person_id, conversation_id, branch_id, message_sequence) REFERENCES agent_conversation_core_entries(person_id, conversation_id, branch_id, sequence))",
    ),
    SchemaObject::table(
        "agent_conversation_core_checkpoints",
        "CREATE TABLE agent_conversation_core_checkpoints (person_id TEXT NOT NULL, conversation_id TEXT NOT NULL, branch_id TEXT NOT NULL, sequence INTEGER NOT NULL CHECK (sequence > 0), prefix_digest TEXT NOT NULL CHECK (length(prefix_digest) = 64), summary TEXT NOT NULL CHECK (length(CAST(summary AS BLOB)) BETWEEN 1 AND 65536), PRIMARY KEY (person_id, conversation_id, branch_id), FOREIGN KEY (person_id, conversation_id, branch_id, sequence) REFERENCES agent_conversation_core_entries(person_id, conversation_id, branch_id, sequence))",
    ),
];

pub(super) const INTERACTIONS: &[SchemaObject] = &[
    SchemaObject::marker(
        "agent_conversation_interaction_schema",
        1,
        "CREATE TABLE agent_conversation_interaction_schema (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL CHECK (version = 1))",
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
