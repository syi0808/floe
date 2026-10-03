//! Existing encrypted-profile inspection before App activates any owner.
//! This does not initialize schemas, replay journals or create/change keys.

use std::{collections::BTreeSet, fs::{self, File, OpenOptions}, io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt}, path::Path};

use super::{AgentFailure, KeyringVaultKeys, PersonId, Uuid};
use super::keyring::VaultKeyReadFailure;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VaultResetReason {
    MissingKey,
    MalformedKey,
    UnsupportedSchema,
    IdentityMismatch,
    /// The existing owned database returned a typed corrupt/not-a-database
    /// failure. This is not proof that the key itself is wrong.
    StoredDataCorrupt,
}

/// Proof remains exclusive until App has moved the old development profile.
/// The guard is deliberately neither cloneable nor serializable.
pub struct VaultResetEvidence {
    reason: VaultResetReason,
    _host_lock: File,
}
impl VaultResetEvidence {
    pub fn reason(&self) -> VaultResetReason { self.reason }
    pub fn lock_file(&self) -> &File { &self._host_lock }
}

pub enum VaultOpenInspection {
    /// Identity and required schema/version metadata passed. Historical payloads
    /// and runtime authority still undergo normal validation during real open.
    Compatible,
    Absent,
    Resettable(VaultResetEvidence),
    Unavailable(AgentFailure),
}

enum InspectionFailure {
    Resettable(VaultResetReason),
    Unavailable(AgentFailure),
}
type InspectionResult<T> = Result<T, InspectionFailure>;

/// Inspect only the existing encrypted Vault for this Person. App alone decides
/// whether its explicitly approved development-profile policy may archive it.
pub async fn inspect_existing_vault(root: &Path, person_id: PersonId) -> VaultOpenInspection {
    if person_id.0.is_nil() {
        return VaultOpenInspection::Unavailable(AgentFailure::InvalidInput);
    }
    let directory = root.join(person_id.to_string());
    for path in [root, directory.as_path()] {
        match fs::symlink_metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return VaultOpenInspection::Absent,
            Ok(metadata) if metadata.is_dir() && private_owned(&metadata) => {}
            _ => return VaultOpenInspection::Unavailable(AgentFailure::VaultUnavailable),
        }
    }
    // Never create a lock while deciding whether an existing profile is owned.
    // A partial layout without this exclusion stays unavailable.
    let host_lock = match existing_file(&directory.join("host.lock"), true) {
        Ok(lock) => lock,
        Err(_) => return VaultOpenInspection::Unavailable(AgentFailure::VaultUnavailable),
    };
    if let Err(error) = host_lock.try_lock() {
        return VaultOpenInspection::Unavailable(match error {
            std::fs::TryLockError::WouldBlock => AgentFailure::Conflict,
            std::fs::TryLockError::Error(_) => AgentFailure::VaultUnavailable,
        });
    }
    match inspect_locked(&directory, person_id).await {
        Ok(()) => VaultOpenInspection::Compatible,
        Err(InspectionFailure::Resettable(reason)) => VaultOpenInspection::Resettable(
            VaultResetEvidence { reason, _host_lock: host_lock },
        ),
        Err(InspectionFailure::Unavailable(failure)) => VaultOpenInspection::Unavailable(failure),
    }
}

fn private_owned(metadata: &fs::Metadata) -> bool {
    metadata.mode() & 0o077 == 0 && metadata.uid() == unsafe { libc::geteuid() }
}

fn existing_file(path: &Path, writable: bool) -> InspectionResult<File> {
    let before = fs::symlink_metadata(path).map_err(io_failure)?;
    if !before.is_file() || !private_owned(&before) {
        return Err(unavailable());
    }
    let file = OpenOptions::new().read(true).write(writable)
        .custom_flags(libc::O_NOFOLLOW).open(path).map_err(io_failure)?;
    let actual = file.metadata().map_err(io_failure)?;
    if !actual.is_file() || !private_owned(&actual)
        || actual.dev() != before.dev() || actual.ino() != before.ino() {
        return Err(unavailable());
    }
    Ok(file)
}

async fn inspect_locked(directory: &Path, person_id: PersonId) -> InspectionResult<()> {
    let mut marker = existing_file(&directory.join("vault.id"), false)?;
    if marker.metadata().map_err(io_failure)?.len() != 36 {
        return Err(reset(VaultResetReason::IdentityMismatch));
    }
    let mut identity = String::new();
    (&mut marker).take(37).read_to_string(&mut identity).map_err(io_failure)?;
    let vault_id = Uuid::parse_str(&identity)
        .ok().filter(|id| !id.is_nil() && id.to_string() == identity)
        .ok_or_else(|| reset(VaultResetReason::IdentityMismatch))?;
    let path = directory.join("sessions.db");
    let database_file = existing_file(&path, false)?;
    if database_file.metadata().map_err(io_failure)?.len() == 0 {
        return Err(reset(VaultResetReason::UnsupportedSchema));
    }
    for name in ["sessions.db-wal", "sessions.db-shm"] {
        let sidecar = directory.join(name);
        match fs::symlink_metadata(&sidecar) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            // Turso owns sidecar creation modes. The private parent directory
            // protects them; do not invent a stricter format than normal open.
            Ok(metadata) if metadata.is_file() && metadata.uid() == unsafe { libc::geteuid() } => {}
            _ => return Err(unavailable()),
        }
    }
    let key = KeyringVaultKeys.inspect_existing(person_id, vault_id).map_err(|failure| match failure {
        VaultKeyReadFailure::Missing => reset(VaultResetReason::MissingKey),
        VaultKeyReadFailure::Malformed => reset(VaultResetReason::MalformedKey),
        VaultKeyReadFailure::Unavailable(failure) => InspectionFailure::Unavailable(failure),
    })?;
    let database = turso::Builder::new_local(path.to_str().ok_or_else(unavailable)?)
        .read_only(true)
        .experimental_encryption(true)
        .with_encryption(turso::EncryptionOpts { cipher: "aes256gcm".into(), hexkey: key.hex() })
        .build().await.map_err(database_failure)?;
    let connection = database.connect().map_err(database_failure)?;
    let mut tables = BTreeSet::new();
    let mut rows = connection.query(
        "SELECT CASE WHEN length(CAST(name AS BLOB)) <= 128 THEN name ELSE '' END FROM sqlite_schema WHERE type = 'table' LIMIT 129", (),
    ).await.map_err(database_failure)?;
    while let Some(row) = rows.next().await.map_err(database_failure)? {
        let name = row.get::<String>(0).map_err(database_failure)?;
        if name.is_empty() || !tables.insert(name) || tables.len() > 128 {
            return Err(unavailable());
        }
    }
    drop(rows);
    if REQUIRED_TABLES.iter().any(|table| !tables.contains(*table))
        || OBSOLETE_TABLES.iter().any(|table| tables.contains(*table)) {
        return Err(reset(VaultResetReason::UnsupportedSchema));
    }
    let mut rows = connection.query(
        "SELECT id, version, person_id, vault_id FROM vault_identity LIMIT 2", (),
    ).await.map_err(database_failure)?;
    let row = rows.next().await.map_err(database_failure)?
        .ok_or_else(|| reset(VaultResetReason::IdentityMismatch))?;
    let version = row.get::<i64>(1).map_err(database_failure)?;
    if row.get::<i64>(0).map_err(database_failure)? != 1
        || row.get::<String>(2).map_err(database_failure)? != person_id.to_string()
        || row.get::<String>(3).map_err(database_failure)? != vault_id.to_string()
        || rows.next().await.map_err(database_failure)?.is_some() {
        return Err(reset(VaultResetReason::IdentityMismatch));
    }
    drop(rows);
    if ![1, 2].contains(&version) || (version == 2) != tables.contains("agent_expert_registry") {
        return Err(reset(VaultResetReason::UnsupportedSchema));
    }
    for (table, expected_version) in SCHEMA_MARKERS {
        let mut rows = connection.query(&format!("SELECT id, version FROM {table} LIMIT 2"), ())
            .await.map_err(database_failure)?;
        let row = rows.next().await.map_err(database_failure)?
            .ok_or_else(|| reset(VaultResetReason::UnsupportedSchema))?;
        if row.get::<i64>(0).map_err(database_failure)? != 1
            || row.get::<i64>(1).map_err(database_failure)? != *expected_version
            || rows.next().await.map_err(database_failure)?.is_some() {
            return Err(reset(VaultResetReason::UnsupportedSchema));
        }
    }
    Ok(())
}

fn reset(reason: VaultResetReason) -> InspectionFailure { InspectionFailure::Resettable(reason) }
fn unavailable() -> InspectionFailure { InspectionFailure::Unavailable(AgentFailure::VaultUnavailable) }
fn io_failure(_: std::io::Error) -> InspectionFailure { unavailable() }
fn database_failure(error: turso::Error) -> InspectionFailure {
    match error {
        turso::Error::Busy(_) | turso::Error::BusySnapshot(_) => InspectionFailure::Unavailable(AgentFailure::Conflict),
        turso::Error::Corrupt(_) | turso::Error::NotAdb(_) => reset(VaultResetReason::StoredDataCorrupt),
        // Pinned Turso erases decryption/checksum and some I/O failures into
        // Error(String). Its text cannot distinguish safe reset evidence.
        _ => InspectionFailure::Unavailable(AgentFailure::StorageUnavailable),
    }
}

const SCHEMA_MARKERS: &[(&str, i64)] = &[
    ("knowledge_store_schema", 1), ("data_access_grant_schema", 2),
    ("actions_schema", 1), ("agent_context_dependency_schema", 1),
    ("agent_conversation_schema", 9), ("agent_task_schema", 5),
    ("agent_context_cleanup_schema", 2), ("remote_authority_schema", 2),
];
const REQUIRED_TABLES: &[&str] = &[
    "vault_identity", "agent_sessions", "agent_session_archives", "agent_session_search",
    "knowledge_store_schema", "learning_observations", "knowledge_candidates",
    "knowledge_stage_receipts", "knowledge_candidate_decisions", "knowledge_revisions",
    "knowledge_mutations", "knowledge_command_receipts", "learner_review_jobs",
    "learner_execution_journal", "learner_journal_heads", "learner_settlement_receipts",
    "data_access_grant_schema", "data_access_grants", "data_access_grant_cleanup",
    "access_connection_reviews", "access_grant_operations", "actions_schema", "actions_records",
    "actions_authorities", "actions_command_receipts", "actions_settlement_receipts", "actions_collection_receipts",
    "agent_expert_command_admissions", "agent_expert_binding_reviews", "agent_expert_registry_receipts",
    "agent_expert_binding_review_consumptions", "agent_expert_binding_replacement_receipts",
    "agent_context_dependency_schema", "agent_context_dependency_coverage",
    "agent_conversation_schema", "agent_conversation_executor", "agent_conversation_runs",
    "agent_conversation_journal", "agent_conversation_commands", "agent_conversation_resume_slots",
    "agent_conversation_resume_requests", "agent_conversation_review_audits", "agent_conversation_terminal_receipts",
    "agent_conversation_session_commands", "agent_conversation_recovery_commands",
    "agent_task_schema", "agent_task_executor", "agent_tasks", "agent_task_journal",
    "agent_context_cleanup_schema", "agent_context_cleanup_applied", "agent_context_cleanup_suppression",
    "remote_authority_schema", "remote_authority_owner", "remote_authority_producer", "remote_authority_clock",
    "gateway_pin_receipts", "gateway_enrollment_receipts", "gateway_authorization_receipts",
    "gateway_product_authorization_receipts", "gateway_pairing_operations", "gateway_setup_receipts",
    "connections_product_records", "gateway_credential_expectation",
];
// These are precisely the old layouts rejected by the normal Vault/Actions open validators.
const OBSOLETE_TABLES: &[&str] = &[
    "calendar_grant_policy_schema", "calendar_grant_policies", "calendar_grant_mappings",
    "remote_view_grant_schema", "remote_view_grant_mappings", "personal_feasibility_review_schema",
    "personal_feasibility_reviews", "personal_grant_schema", "personal_grant_policies", "personal_feasibility_queries",
    "agent_action_schema", "agent_action_envelopes", "agent_action_policy",
];
