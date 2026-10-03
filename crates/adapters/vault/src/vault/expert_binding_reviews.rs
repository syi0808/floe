use floe_agent_contract::{AgentFailure, CommandId, OwnerActor};
use floe_experts::{
    AgentRegistry, BindingReplacementReceipt, BindingReviewDescriptor, BindingReviewRef,
    RegistryCommitReceipt, RegistrySnapshot, binding_review_digest,
    project_binding_mutation_receipt, validate_binding_review_descriptor,
};
use floe_kernel::PersonId;
use turso::{Connection, Row};
use uuid::Uuid;

use super::*;

pub(crate) const EXPERT_COMMAND_LIMIT: i64 = 4_096;
pub(crate) const MAX_BINDING_REVIEW_PAYLOAD_BYTES: usize = 256 * 1024;
pub(crate) const MAX_EXPERT_RECEIPT_PAYLOAD_BYTES: usize = 512 * 1024;

pub(crate) const EXPERT_COMMAND_PREPARE: &str = "binding_prepare";
pub(crate) const EXPERT_COMMAND_REGISTRY: &str = "registry";
pub(crate) const EXPERT_COMMAND_REPLACEMENT: &str = "binding_replacement";

#[derive(Clone, Debug)]
pub(crate) struct ExpertCommandAdmission {
    pub command_id: CommandId,
    pub family: String,
    pub person_id: PersonId,
    pub device_id: String,
    pub request_digest: [u8; 32],
    pub review_id: Option<Uuid>,
}

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    /// Called only while explicitly creating a new Vault.
    pub(super) async fn initialize_expert_binding_reviews(&self) -> Result<(), AgentFailure> {
        self.check_access()?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = create_expert_binding_tables_on(&transaction).await;
        self.finish_registry_transaction_checked(transaction, result)
            .await
    }

    /// Reopen validates durable evidence without creating missing tables.
    pub(super) async fn validate_expert_binding_reviews(&self) -> Result<(), AgentFailure> {
        self.check_access()?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(turso::transaction::TransactionBehavior::Deferred)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = ensure_expert_binding_tables_on(&transaction).await;
        self.finish_registry_transaction_checked(transaction, result)
            .await
    }

    /// Conversation calls this with its open turn transaction so the review and
    /// consumed receipt are authenticated from the same encrypted snapshot.
    pub(crate) async fn read_binding_review_on(
        &self,
        connection: &Connection,
        device_id: &str,
        reference: &BindingReviewRef,
    ) -> Result<BindingReviewDescriptor, AgentFailure> {
        self.check_access()?;
        let result = read_binding_review_on(
            connection,
            self.person_id,
            device_id,
            reference,
            self.registry_instance_id(),
        )
        .await;
        self.check_access()?;
        result
    }

    /// A review with no consumption link is still unconsumed. Once a link
    /// exists, a missing or mismatched command receipt is Vault corruption.
    pub(crate) async fn read_binding_review_receipt_on(
        &self,
        connection: &Connection,
        device_id: &str,
        reference: &BindingReviewRef,
    ) -> Result<Option<BindingReplacementReceipt>, AgentFailure> {
        self.check_access()?;
        let result = async {
            let descriptor = read_binding_review_on(
                connection,
                self.person_id,
                device_id,
                reference,
                self.registry_instance_id(),
            )
            .await?;
            read_binding_replacement_for_review_on(
                connection,
                self.person_id,
                device_id,
                &descriptor,
                self.registry_instance_id(),
            )
            .await
        }
        .await;
        self.check_access()?;
        result
    }
}

const EXPERT_BINDING_TABLES: &[(&str, &str)] = &[
    (
        "agent_expert_command_admissions",
        "CREATE TABLE agent_expert_command_admissions (command_id TEXT PRIMARY KEY, family TEXT NOT NULL CHECK (family IN ('binding_prepare', 'registry', 'binding_replacement')), person_id TEXT NOT NULL, device_id TEXT NOT NULL, request_digest TEXT NOT NULL CHECK (length(request_digest) = 64), review_id TEXT NOT NULL CHECK ((family = 'registry' AND review_id = '') OR (family != 'registry' AND length(review_id) = 36)))",
    ),
    (
        "agent_expert_binding_reviews",
        "CREATE TABLE agent_expert_binding_reviews (review_id TEXT PRIMARY KEY, person_id TEXT NOT NULL, device_id TEXT NOT NULL, command_id TEXT NOT NULL UNIQUE, assignment_id TEXT NOT NULL, requirement_key TEXT NOT NULL, review_digest TEXT NOT NULL CHECK (length(review_digest) = 64), payload TEXT NOT NULL, UNIQUE (person_id, command_id))",
    ),
    (
        "agent_expert_registry_receipts",
        "CREATE TABLE agent_expert_registry_receipts (command_id TEXT PRIMARY KEY, person_id TEXT NOT NULL, device_id TEXT NOT NULL, request_digest TEXT NOT NULL CHECK (length(request_digest) = 64), snapshot_revision INTEGER NOT NULL, payload TEXT NOT NULL)",
    ),
    (
        "agent_expert_binding_review_consumptions",
        "CREATE TABLE agent_expert_binding_review_consumptions (review_id TEXT PRIMARY KEY, command_id TEXT NOT NULL UNIQUE, person_id TEXT NOT NULL, device_id TEXT NOT NULL, review_digest TEXT NOT NULL CHECK (length(review_digest) = 64), request_digest TEXT NOT NULL CHECK (length(request_digest) = 64), committed_at_unix_ms INTEGER NOT NULL)",
    ),
    (
        "agent_expert_binding_replacement_receipts",
        "CREATE TABLE agent_expert_binding_replacement_receipts (command_id TEXT PRIMARY KEY, consumed_review_id TEXT NOT NULL UNIQUE, person_id TEXT NOT NULL, device_id TEXT NOT NULL, review_digest TEXT NOT NULL CHECK (length(review_digest) = 64), request_digest TEXT NOT NULL CHECK (length(request_digest) = 64), committed_at_unix_ms INTEGER NOT NULL, payload TEXT NOT NULL)",
    ),
];

async fn create_expert_binding_tables_on(connection: &Connection) -> Result<(), AgentFailure> {
    match expert_binding_tables_on(connection).await? {
        0 => {}
        5 => return ensure_expert_binding_tables_on(connection).await,
        _ => return Err(AgentFailure::VaultUnavailable),
    }
    for (_, statement) in EXPERT_BINDING_TABLES {
        connection.execute(statement, ()).await.map_err(storage)?;
    }
    ensure_expert_binding_tables_on(connection).await
}

/// The schema is created only with a new Vault. Missing receipts on an existing
/// Vault cannot become permission to repeat a registry or binding command.
pub(crate) async fn ensure_expert_binding_tables_on(
    connection: &Connection,
) -> Result<(), AgentFailure> {
    if expert_binding_tables_on(connection).await? != 5 {
        return Err(AgentFailure::VaultUnavailable);
    }
    for (name, statement) in EXPERT_BINDING_TABLES {
        let mut rows = connection
            .query(
                "SELECT sql FROM sqlite_schema WHERE type = 'table' AND name = ?",
                (*name,),
            )
            .await
            .map_err(storage)?;
        let row = rows
            .next()
            .await
            .map_err(storage)?
            .ok_or(AgentFailure::VaultUnavailable)?;
        let actual = row.get::<String>(0).map_err(storage)?;
        if rows.next().await.map_err(storage)?.is_some() {
            return Err(AgentFailure::VaultUnavailable);
        }
        match crate::schema_sql::compare(&actual, statement) {
            crate::schema_sql::Comparison::Equivalent => {}
            crate::schema_sql::Comparison::Different
            | crate::schema_sql::Comparison::InvalidStored => {
                return Err(AgentFailure::VaultUnavailable);
            }
            crate::schema_sql::Comparison::InvalidExpected => {
                return Err(AgentFailure::StorageUnavailable);
            }
        }
    }
    let mut rows=connection.query("SELECT name FROM sqlite_schema WHERE type IN ('trigger', 'view') AND (name GLOB 'agent_expert_binding_*' OR name GLOB 'agent_expert_command_*' OR name = 'agent_expert_registry_receipts' OR tbl_name GLOB 'agent_expert_binding_*' OR tbl_name GLOB 'agent_expert_command_*' OR tbl_name = 'agent_expert_registry_receipts')",()).await.map_err(storage)?;
    if rows.next().await.map_err(storage)?.is_some() {
        return Err(AgentFailure::VaultUnavailable);
    }
    drop(rows);
    count_expert_command_admissions_on(connection).await?;
    Ok(())
}

pub(crate) async fn expert_binding_tables_on(connection: &Connection) -> Result<i64, AgentFailure> {
    let mut rows = connection
        .query(
            "SELECT count(*) FROM sqlite_schema WHERE type = 'table' AND name IN ('agent_expert_command_admissions', 'agent_expert_binding_reviews', 'agent_expert_registry_receipts', 'agent_expert_binding_review_consumptions', 'agent_expert_binding_replacement_receipts')",
            (),
        )
        .await
        .map_err(storage)?;
    let count = rows
        .next()
        .await
        .map_err(storage)?
        .ok_or(AgentFailure::VaultUnavailable)?
        .get::<i64>(0)
        .map_err(storage)?;
    if !(0..=5).contains(&count) {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(count)
}

pub(crate) async fn count_expert_command_admissions_on(
    connection: &Connection,
) -> Result<i64, AgentFailure> {
    if expert_binding_tables_on(connection).await? != 5 {
        return Err(AgentFailure::VaultUnavailable);
    }
    let mut rows = connection
        .query("SELECT count(*) FROM agent_expert_command_admissions", ())
        .await
        .map_err(storage)?;
    let count = rows
        .next()
        .await
        .map_err(storage)?
        .ok_or(AgentFailure::VaultUnavailable)?
        .get::<i64>(0)
        .map_err(storage)?;
    if count < 0 || count > EXPERT_COMMAND_LIMIT {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(count)
}

pub(crate) async fn read_expert_command_admission_on(
    connection: &Connection,
    command_id: CommandId,
) -> Result<Option<ExpertCommandAdmission>, AgentFailure> {
    let table_count = expert_binding_tables_on(connection).await?;
    if table_count != 5 {
        return Err(AgentFailure::VaultUnavailable);
    }
    let mut rows = connection
        .query(
            "SELECT command_id, family, person_id, device_id, request_digest, review_id FROM agent_expert_command_admissions WHERE command_id = ?",
            (command_id.as_uuid().to_string(),),
        )
        .await
        .map_err(storage)?;
    let Some(row) = rows.next().await.map_err(storage)? else {
        drop(rows);
        let mut orphans=connection.query("SELECT command_id FROM agent_expert_binding_reviews WHERE command_id = ? UNION ALL SELECT command_id FROM agent_expert_registry_receipts WHERE command_id = ? UNION ALL SELECT command_id FROM agent_expert_binding_replacement_receipts WHERE command_id = ? UNION ALL SELECT command_id FROM agent_expert_binding_review_consumptions WHERE command_id = ? LIMIT 1",(command_id.as_uuid().to_string(),command_id.as_uuid().to_string(),command_id.as_uuid().to_string(),command_id.as_uuid().to_string())).await.map_err(storage)?;
        if orphans.next().await.map_err(storage)?.is_some() {
            return Err(AgentFailure::VaultUnavailable);
        }
        return Ok(None);
    };
    let admission = decode_admission(&row)?;
    drop(rows);
    if admission.command_id != command_id {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(Some(admission))
}

pub(crate) async fn insert_expert_command_admission_on(
    connection: &Connection,
    command_id: CommandId,
    family: &str,
    person_id: PersonId,
    device_id: &str,
    request_digest: [u8; 32],
    review_id: Option<Uuid>,
) -> Result<(), AgentFailure> {
    if !command_id.is_valid()
        || request_digest == [0; 32]
        || !matches!(
            family,
            EXPERT_COMMAND_PREPARE | EXPERT_COMMAND_REGISTRY | EXPERT_COMMAND_REPLACEMENT
        )
        || (family == EXPERT_COMMAND_REGISTRY && review_id.is_some())
        || (family != EXPERT_COMMAND_REGISTRY && review_id.is_none())
        || review_id.is_some_and(|id| id.is_nil())
    {
        return Err(AgentFailure::InvalidInput);
    }
    connection
        .execute(
            "INSERT INTO agent_expert_command_admissions (command_id, family, person_id, device_id, request_digest, review_id) VALUES (?, ?, ?, ?, ?, ?)",
            (
                command_id.as_uuid().to_string(),
                family,
                person_id.to_string(),
                device_id,
                digest_text(&request_digest),
                review_id.map_or_else(String::new, |id| id.to_string()),
            ),
        )
        .await
        .map_err(map_unique_conflict)?;
    Ok(())
}

pub(crate) async fn read_binding_review_on(
    connection: &Connection,
    person_id: PersonId,
    device_id: &str,
    reference: &BindingReviewRef,
    registry_instance_id: Uuid,
) -> Result<BindingReviewDescriptor, AgentFailure> {
    reference.validate()?;
    let table_count = expert_binding_tables_on(connection).await?;
    if table_count != 5 {
        return Err(AgentFailure::VaultUnavailable);
    }
    let mut rows = connection
        .query(
            "SELECT person_id, device_id, command_id, assignment_id, requirement_key, review_digest, CASE WHEN length(CAST(payload AS BLOB)) <= 262144 THEN payload ELSE NULL END FROM agent_expert_binding_reviews WHERE review_id = ?",
            (reference.id.to_string(),),
        )
        .await
        .map_err(storage)?;
    let Some(row) = rows.next().await.map_err(storage)? else {
        drop(rows);
        let mut linked=connection.query("SELECT command_id FROM agent_expert_command_admissions WHERE family = 'binding_prepare' AND review_id = ? LIMIT 1",(reference.id.to_string(),)).await.map_err(storage)?;
        if linked.next().await.map_err(storage)?.is_some() {
            return Err(AgentFailure::VaultUnavailable);
        }
        return Err(AgentFailure::NotFound);
    };
    let stored_person = parse_person_id(&row.get::<String>(0).map_err(storage)?)?;
    let stored_device = row.get::<String>(1).map_err(storage)?;
    let command_id = parse_command_id(&row.get::<String>(2).map_err(storage)?)?;
    let assignment_id = parse_uuid(&row.get::<String>(3).map_err(storage)?)?;
    let requirement_key = row.get::<String>(4).map_err(storage)?;
    let stored_digest = parse_digest(&row.get::<String>(5).map_err(storage)?)?;
    let payload = row.get::<String>(6).map_err(storage)?;
    drop(rows);

    if payload.len() > MAX_BINDING_REVIEW_PAYLOAD_BYTES {
        return Err(AgentFailure::VaultUnavailable);
    }
    let descriptor: BindingReviewDescriptor = decode_canonical(&payload)?;
    validate_binding_review_descriptor(&descriptor).map_err(|_| AgentFailure::VaultUnavailable)?;
    if descriptor.review_ref.id != reference.id
        || descriptor.review_ref.digest != stored_digest
        || stored_digest
            != binding_review_digest(&descriptor).map_err(|_| AgentFailure::VaultUnavailable)?
        || descriptor.identity.person_id != stored_person
        || descriptor.identity.device_id != stored_device
        || descriptor.identity.command_id != command_id
        || descriptor.identity.assignment_id != assignment_id
        || descriptor.identity.requirement_key != requirement_key
        || descriptor.registry_instance_id != registry_instance_id
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    if stored_person != person_id || stored_device != device_id {
        return Err(AgentFailure::NotFound);
    }
    if descriptor.review_ref.digest != reference.digest {
        return Err(AgentFailure::NotFound);
    }

    let admission = read_expert_command_admission_on(connection, command_id)
        .await?
        .ok_or(AgentFailure::VaultUnavailable)?;
    if admission.family != EXPERT_COMMAND_PREPARE
        || admission.person_id != stored_person
        || admission.device_id != stored_device
        || admission.request_digest != stored_digest
        || admission.review_id != Some(descriptor.review_ref.id)
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(descriptor)
}

pub(crate) async fn read_registry_receipt_on(
    connection: &Connection,
    person_id: PersonId,
    device_id: &str,
    command_id: CommandId,
    registry_instance_id: Uuid,
) -> Result<Option<RegistryCommitReceipt>, AgentFailure> {
    let table_count = expert_binding_tables_on(connection).await?;
    if table_count != 5 {
        return Err(AgentFailure::VaultUnavailable);
    }
    let Some(admission) = read_expert_command_admission_on(connection, command_id).await? else {
        return Ok(None);
    };
    if admission.family != EXPERT_COMMAND_REGISTRY {
        return Err(AgentFailure::Conflict);
    }
    if admission.review_id.is_some() {
        return Err(AgentFailure::VaultUnavailable);
    }
    if admission.person_id != person_id || admission.device_id != device_id {
        return Err(AgentFailure::CapabilityDenied);
    }

    let mut rows = connection
        .query(
            "SELECT person_id, device_id, request_digest, snapshot_revision, CASE WHEN length(CAST(payload AS BLOB)) <= 524288 THEN payload ELSE NULL END FROM agent_expert_registry_receipts WHERE command_id = ?",
            (command_id.as_uuid().to_string(),),
        )
        .await
        .map_err(storage)?;
    let Some(row) = rows.next().await.map_err(storage)? else {
        return Err(AgentFailure::VaultUnavailable);
    };
    let stored_person = parse_person_id(&row.get::<String>(0).map_err(storage)?)?;
    let stored_device = row.get::<String>(1).map_err(storage)?;
    let stored_digest = parse_digest(&row.get::<String>(2).map_err(storage)?)?;
    let stored_revision = u64::try_from(row.get::<i64>(3).map_err(storage)?)
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    let payload = row.get::<String>(4).map_err(storage)?;
    drop(rows);
    if payload.len() > MAX_EXPERT_RECEIPT_PAYLOAD_BYTES {
        return Err(AgentFailure::VaultUnavailable);
    }
    let receipt: RegistryCommitReceipt = decode_canonical(&payload)?;
    validate_registry_receipt(
        &receipt,
        command_id,
        person_id,
        device_id,
        stored_digest,
        registry_instance_id,
    )?;
    if stored_person != person_id
        || stored_device != device_id
        || stored_digest != admission.request_digest
        || receipt.snapshot.revision != stored_revision
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(Some(receipt))
}

pub(crate) async fn read_binding_replacement_by_command_on(
    connection: &Connection,
    person_id: PersonId,
    device_id: &str,
    command_id: CommandId,
    registry_instance_id: Uuid,
) -> Result<Option<BindingReplacementReceipt>, AgentFailure> {
    let table_count = expert_binding_tables_on(connection).await?;
    if table_count != 5 {
        return Err(AgentFailure::VaultUnavailable);
    }
    let Some(admission) = read_expert_command_admission_on(connection, command_id).await? else {
        return Ok(None);
    };
    if admission.family != EXPERT_COMMAND_REPLACEMENT {
        return Err(AgentFailure::Conflict);
    }
    if admission.person_id != person_id || admission.device_id != device_id {
        return Err(AgentFailure::CapabilityDenied);
    }
    read_binding_replacement_for_command_on(
        connection,
        person_id,
        device_id,
        &admission,
        registry_instance_id,
    )
    .await
    .map(Some)
}

pub(crate) async fn read_binding_replacement_for_review_on(
    connection: &Connection,
    person_id: PersonId,
    device_id: &str,
    descriptor: &BindingReviewDescriptor,
    registry_instance_id: Uuid,
) -> Result<Option<BindingReplacementReceipt>, AgentFailure> {
    let table_count = expert_binding_tables_on(connection).await?;
    if table_count != 5 {
        return Err(AgentFailure::VaultUnavailable);
    }
    if descriptor.identity.person_id != person_id
        || descriptor.identity.device_id != device_id
        || descriptor.registry_instance_id != registry_instance_id
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    let review_id = descriptor.review_ref.id.to_string();
    let mut consumption_rows = connection
        .query(
            "SELECT command_id, person_id, device_id, review_digest, request_digest, committed_at_unix_ms FROM agent_expert_binding_review_consumptions WHERE review_id = ?",
            (review_id.clone(),),
        )
        .await
        .map_err(storage)?;
    let Some(consumption_row) = consumption_rows.next().await.map_err(storage)? else {
        drop(consumption_rows);
        let mut admission_rows = connection
            .query(
                "SELECT command_id FROM agent_expert_command_admissions WHERE family = 'binding_replacement' AND review_id = ?",
                (review_id.clone(),),
            )
            .await
            .map_err(storage)?;
        if admission_rows.next().await.map_err(storage)?.is_some() {
            return Err(AgentFailure::VaultUnavailable);
        }
        drop(admission_rows);
        let mut orphan_rows = connection
            .query(
                "SELECT command_id FROM agent_expert_binding_replacement_receipts WHERE consumed_review_id = ?",
                (review_id,),
            )
            .await
            .map_err(storage)?;
        if orphan_rows.next().await.map_err(storage)?.is_some() {
            return Err(AgentFailure::VaultUnavailable);
        }
        return Ok(None);
    };
    let command_id = parse_command_id(&consumption_row.get::<String>(0).map_err(storage)?)?;
    let stored_person = parse_person_id(&consumption_row.get::<String>(1).map_err(storage)?)?;
    let stored_device = consumption_row.get::<String>(2).map_err(storage)?;
    let stored_review_digest = parse_digest(&consumption_row.get::<String>(3).map_err(storage)?)?;
    let stored_request_digest = parse_digest(&consumption_row.get::<String>(4).map_err(storage)?)?;
    let stored_committed_at = consumption_row.get::<i64>(5).map_err(storage)?;
    drop(consumption_rows);

    if stored_person != person_id
        || stored_device != device_id
        || stored_review_digest != descriptor.review_ref.digest
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    let admission = read_expert_command_admission_on(connection, command_id)
        .await?
        .ok_or(AgentFailure::VaultUnavailable)?;
    if admission.family != EXPERT_COMMAND_REPLACEMENT
        || admission.person_id != stored_person
        || admission.device_id != stored_device
        || admission.request_digest != stored_request_digest
        || admission.review_id != Some(descriptor.review_ref.id)
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    let receipt = read_binding_replacement_for_command_on(
        connection,
        person_id,
        device_id,
        &admission,
        registry_instance_id,
    )
    .await?;
    if receipt.review_ref != descriptor.review_ref
        || receipt.committed_at_unix_ms != stored_committed_at
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(Some(receipt))
}

async fn read_binding_replacement_for_command_on(
    connection: &Connection,
    person_id: PersonId,
    device_id: &str,
    admission: &ExpertCommandAdmission,
    registry_instance_id: Uuid,
) -> Result<BindingReplacementReceipt, AgentFailure> {
    if admission.family != EXPERT_COMMAND_REPLACEMENT {
        return Err(AgentFailure::Conflict);
    }
    let command_id = admission.command_id.as_uuid().to_string();
    let mut rows = connection
        .query(
            "SELECT consumed_review_id, person_id, device_id, review_digest, request_digest, committed_at_unix_ms, CASE WHEN length(CAST(payload AS BLOB)) <= 524288 THEN payload ELSE NULL END FROM agent_expert_binding_replacement_receipts WHERE command_id = ?",
            (command_id.clone(),),
        )
        .await
        .map_err(storage)?;
    let Some(row) = rows.next().await.map_err(storage)? else {
        return Err(AgentFailure::VaultUnavailable);
    };
    let consumed_review_id = parse_uuid(&row.get::<String>(0).map_err(storage)?)?;
    let stored_person = parse_person_id(&row.get::<String>(1).map_err(storage)?)?;
    let stored_device = row.get::<String>(2).map_err(storage)?;
    let stored_review_digest = parse_digest(&row.get::<String>(3).map_err(storage)?)?;
    let stored_request_digest = parse_digest(&row.get::<String>(4).map_err(storage)?)?;
    let stored_committed_at = row.get::<i64>(5).map_err(storage)?;
    let payload = row.get::<String>(6).map_err(storage)?;
    drop(rows);
    if payload.len() > MAX_EXPERT_RECEIPT_PAYLOAD_BYTES {
        return Err(AgentFailure::VaultUnavailable);
    }
    let receipt: BindingReplacementReceipt = decode_canonical(&payload)?;
    if stored_person != person_id
        || stored_device != device_id
        || stored_review_digest != receipt.review_ref.digest
        || stored_request_digest != admission.request_digest
        || stored_request_digest != receipt.registry.request_digest
        || stored_committed_at != receipt.committed_at_unix_ms
        || consumed_review_id != receipt.review_ref.id
        || receipt.registry.command_id != admission.command_id
        || receipt.registry.person_id != stored_person
        || receipt.registry.device_id != stored_device
        || admission.person_id != stored_person
        || admission.device_id != stored_device
        || admission.review_id != Some(consumed_review_id)
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    let mut link_rows = connection
        .query(
            "SELECT command_id, person_id, device_id, review_digest, request_digest, committed_at_unix_ms FROM agent_expert_binding_review_consumptions WHERE review_id = ?",
            (consumed_review_id.to_string(),),
        )
        .await
        .map_err(storage)?;
    let Some(link) = link_rows.next().await.map_err(storage)? else {
        return Err(AgentFailure::VaultUnavailable);
    };
    if parse_command_id(&link.get::<String>(0).map_err(storage)?)? != admission.command_id
        || parse_person_id(&link.get::<String>(1).map_err(storage)?)? != stored_person
        || link.get::<String>(2).map_err(storage)? != stored_device
        || parse_digest(&link.get::<String>(3).map_err(storage)?)? != stored_review_digest
        || parse_digest(&link.get::<String>(4).map_err(storage)?)? != stored_request_digest
        || link.get::<i64>(5).map_err(storage)? != stored_committed_at
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    drop(link_rows);

    let reference = receipt.review_ref.clone();
    let descriptor = read_binding_review_on(
        connection,
        person_id,
        device_id,
        &reference,
        registry_instance_id,
    )
    .await?;
    project_binding_mutation_receipt(&receipt, &descriptor)
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    Ok(receipt)
}

fn decode_admission(row: &Row) -> Result<ExpertCommandAdmission, AgentFailure> {
    let command_id = parse_command_id(&row.get::<String>(0).map_err(storage)?)?;
    let family = row.get::<String>(1).map_err(storage)?;
    if !matches!(
        family.as_str(),
        EXPERT_COMMAND_PREPARE | EXPERT_COMMAND_REGISTRY | EXPERT_COMMAND_REPLACEMENT
    ) {
        return Err(AgentFailure::VaultUnavailable);
    }
    let review_id = match row.get::<String>(5).map_err(storage)?.as_str() {
        "" => None,
        value => Some(parse_uuid(value)?),
    };
    if (family == EXPERT_COMMAND_REGISTRY && review_id.is_some())
        || (family != EXPERT_COMMAND_REGISTRY && review_id.is_none())
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    let request_digest = parse_digest(&row.get::<String>(4).map_err(storage)?)?;
    if request_digest == [0; 32] {
        return Err(AgentFailure::VaultUnavailable);
    }
    let device_id = row.get::<String>(3).map_err(storage)?;
    if device_id.is_empty()
        || device_id.trim() != device_id
        || device_id.len() > 128
        || device_id.chars().any(char::is_control)
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(ExpertCommandAdmission {
        command_id,
        family,
        person_id: parse_person_id(&row.get::<String>(2).map_err(storage)?)?,
        device_id,
        request_digest,
        review_id,
    })
}

pub(crate) fn validate_registry_receipt(
    receipt: &RegistryCommitReceipt,
    command_id: CommandId,
    person_id: PersonId,
    device_id: &str,
    request_digest: [u8; 32],
    registry_instance_id: Uuid,
) -> Result<(), AgentFailure> {
    if receipt.command_id != command_id
        || receipt.person_id != person_id
        || receipt.device_id != device_id
        || receipt.request_digest != request_digest
        || receipt.request_digest == [0; 32]
        || receipt.snapshot.revision == 0
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    validate_registry_snapshot(&receipt.snapshot, registry_instance_id, person_id)
        .map_err(|_| AgentFailure::VaultUnavailable)
}

pub(crate) fn validate_registry_snapshot(
    snapshot: &RegistrySnapshot,
    registry_instance_id: Uuid,
    person_id: PersonId,
) -> Result<(), AgentFailure> {
    if snapshot.instance_id != registry_instance_id
        || snapshot
            .assignments
            .iter()
            .any(|entry| entry.person_id != person_id)
        || snapshot
            .install_receipts
            .iter()
            .any(|entry| entry.person_id != person_id)
    {
        return Err(AgentFailure::NotFound);
    }
    AgentRegistry::restore(snapshot.clone(), registry_instance_id)?;
    let payload = serde_json::to_vec(snapshot).map_err(|_| AgentFailure::InvalidInput)?;
    if payload.len() > 262_144 {
        return Err(AgentFailure::BudgetExceeded);
    }
    i64::try_from(snapshot.revision).map_err(|_| AgentFailure::Conflict)?;
    Ok(())
}

/// Preserve the append-only installation and assignment state guarantees used
/// by the existing Vault registry CAS while allowing the owner-produced
/// configuration/install successor.
pub(crate) fn validate_registry_successor(
    previous: &RegistrySnapshot,
    next: &RegistrySnapshot,
    expected_revision: u64,
    person_id: PersonId,
) -> Result<(), AgentFailure> {
    if previous.revision != expected_revision
        || expected_revision.checked_add(1) != Some(next.revision)
        || previous.instance_id != next.instance_id
        || next
            .assignments
            .iter()
            .any(|entry| entry.person_id != person_id)
        || next
            .install_receipts
            .iter()
            .any(|entry| entry.person_id != person_id)
        || previous
            .install_receipts
            .iter()
            .any(|before| !next.install_receipts.iter().any(|after| before == after))
        || previous.assignments.iter().any(|before| {
            !next.assignments.iter().any(|after| {
                before.id == after.id
                    && before.person_id == after.person_id
                    && before.installation_id == after.installation_id
                    && before.private_state == after.private_state
                    && before.binding == after.binding
            })
        })
        || previous
            .manifests
            .iter()
            .any(|before| !next.manifests.contains(before))
        || previous.installations.iter().any(|before| {
            !next
                .installations
                .iter()
                .any(|after| before.id == after.id && before.package == after.package)
        })
    {
        return Err(AgentFailure::Conflict);
    }
    for receipt in &next.install_receipts {
        if previous
            .install_receipts
            .iter()
            .any(|before| before == receipt)
        {
            continue;
        }
        if receipt.expected_revision != expected_revision
            || previous
                .install_receipts
                .iter()
                .any(|before| before.operation_id == receipt.operation_id)
            || receipt.installed.iter().any(|created| {
                previous
                    .installations
                    .iter()
                    .any(|before| before.id == created.installation_id)
                    || previous
                        .assignments
                        .iter()
                        .any(|before| before.id == created.assignment_id)
            })
        {
            return Err(AgentFailure::Conflict);
        }
    }
    for assignment in &next.assignments {
        if previous
            .assignments
            .iter()
            .any(|before| before.id == assignment.id)
        {
            continue;
        }
        if assignment.private_state != floe_experts::ExpertPrivateState::default() {
            return Err(AgentFailure::Conflict);
        }
    }
    validate_registry_snapshot(next, previous.instance_id, person_id)
}

/// A binding replacement may change exactly one reviewed requirement entry.
pub(crate) fn validate_binding_registry_successor(
    previous: &RegistrySnapshot,
    next: &RegistrySnapshot,
    descriptor: &BindingReviewDescriptor,
    candidate_refs: &[Uuid],
    command_id: CommandId,
) -> Result<(), AgentFailure> {
    if previous.instance_id != descriptor.registry_instance_id
        || next.instance_id != descriptor.registry_instance_id
        || previous.revision.checked_add(1) != Some(next.revision)
        || next.manifests != previous.manifests
        || next.installations != previous.installations
        || next.install_receipts != previous.install_receipts
        || next.assignments.len() != previous.assignments.len()
        || !previous.assignments.iter().any(|assignment| {
            assignment.id == descriptor.identity.assignment_id
                && assignment.person_id == descriptor.identity.person_id
                && assignment.installation_id == descriptor.installation_id
        })
    {
        return Err(AgentFailure::Conflict);
    }
    let expected_binding_revision = descriptor.identity.expected_binding_revision;
    let next_binding_revision = expected_binding_revision
        .checked_add(1)
        .ok_or(AgentFailure::BudgetExceeded)?;
    for before in &previous.assignments {
        let after = next
            .assignments
            .iter()
            .find(|candidate| candidate.id == before.id)
            .ok_or(AgentFailure::Conflict)?;
        if after.person_id != before.person_id
            || after.installation_id != before.installation_id
            || after.enabled != before.enabled
            || after.private_state != before.private_state
        {
            return Err(AgentFailure::Conflict);
        }
        if before.id != descriptor.identity.assignment_id {
            if after.binding != before.binding {
                return Err(AgentFailure::Conflict);
            }
            continue;
        }
        if before.person_id != descriptor.identity.person_id
            || before.installation_id != descriptor.installation_id
            || before.binding.revision != expected_binding_revision
            || after.binding.schema_version != before.binding.schema_version
            || after.binding.revision != next_binding_revision
            || after.binding.entries.len() != before.binding.entries.len()
        {
            return Err(AgentFailure::Conflict);
        }
        for old_entry in &before.binding.entries {
            let new_entry = after
                .binding
                .entries
                .iter()
                .find(|entry| entry.requirement_key == old_entry.requirement_key)
                .ok_or(AgentFailure::Conflict)?;
            if old_entry.requirement_key == descriptor.identity.requirement_key {
                if old_entry.capability != new_entry.capability
                    || old_entry.contract_version != new_entry.contract_version
                {
                    return Err(AgentFailure::Conflict);
                }
            } else if new_entry != old_entry {
                return Err(AgentFailure::Conflict);
            }
        }
        let operation = after
            .binding
            .last_operation
            .as_ref()
            .ok_or(AgentFailure::StorageUnavailable)?;
        if operation.operation_id != command_id.as_uuid()
            || operation.resulting_revision != next_binding_revision
        {
            return Err(AgentFailure::StorageUnavailable);
        }
    }
    validate_registry_snapshot(
        next,
        descriptor.registry_instance_id,
        descriptor.identity.person_id,
    )?;
    verify_binding_candidate_selection(descriptor, candidate_refs, next)
}

pub(crate) fn verify_prepare_identity(
    descriptor: &BindingReviewDescriptor,
    person_id: PersonId,
    actor: &OwnerActor,
    registry_instance_id: Uuid,
) -> Result<(), AgentFailure> {
    if descriptor.identity.person_id != person_id
        || descriptor.identity.person_id != actor.person_id
        || descriptor.identity.device_id != actor.device_id
        || descriptor.registry_instance_id != registry_instance_id
    {
        return Err(AgentFailure::CapabilityDenied);
    }
    Ok(())
}

pub(crate) fn verify_binding_candidate_selection(
    descriptor: &BindingReviewDescriptor,
    candidate_refs: &[Uuid],
    snapshot: &RegistrySnapshot,
) -> Result<(), AgentFailure> {
    if candidate_refs.windows(2).any(|pair| pair[0] >= pair[1])
        || candidate_refs.len() > usize::from(descriptor.requirement.maximum_sources)
    {
        return Err(AgentFailure::InvalidInput);
    }
    validate_binding_review_descriptor(descriptor)?;
    let mut expected = Vec::with_capacity(candidate_refs.len());
    for candidate_ref in candidate_refs {
        let reviewed = descriptor
            .candidates
            .iter()
            .find(|candidate| candidate.candidate_ref == *candidate_ref)
            .ok_or(AgentFailure::InvalidInput)?;
        expected.push(reviewed.candidate.reference.clone());
    }
    expected.sort();
    if expected.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(AgentFailure::InvalidInput);
    }
    let assignment = snapshot
        .assignments
        .iter()
        .find(|assignment| {
            assignment.id == descriptor.identity.assignment_id
                && assignment.person_id == descriptor.identity.person_id
        })
        .ok_or(AgentFailure::StorageUnavailable)?;
    let selected = assignment
        .binding
        .entries
        .iter()
        .find(|entry| entry.requirement_key == descriptor.identity.requirement_key)
        .ok_or(AgentFailure::StorageUnavailable)?;
    if selected.selected != expected {
        return Err(AgentFailure::StorageUnavailable);
    }
    Ok(())
}

fn parse_uuid(value: &str) -> Result<Uuid, AgentFailure> {
    let parsed = Uuid::parse_str(value).map_err(|_| AgentFailure::VaultUnavailable)?;
    if parsed.is_nil() || parsed.to_string() != value {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(parsed)
}

fn parse_person_id(value: &str) -> Result<PersonId, AgentFailure> {
    PersonId::from_uuid(parse_uuid(value)?).ok_or(AgentFailure::VaultUnavailable)
}

fn parse_command_id(value: &str) -> Result<CommandId, AgentFailure> {
    CommandId::from_uuid(parse_uuid(value)?).ok_or(AgentFailure::VaultUnavailable)
}

fn digest_text(digest: &[u8; 32]) -> String {
    let mut text = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut text, "{byte:02x}");
    }
    text
}

fn parse_digest(value: &str) -> Result<[u8; 32], AgentFailure> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(AgentFailure::VaultUnavailable);
    }
    let mut digest = [0; 32];
    for (index, byte) in digest.iter_mut().enumerate() {
        let offset = index * 2;
        *byte = (hex_nibble(value.as_bytes()[offset])? << 4)
            | hex_nibble(value.as_bytes()[offset + 1])?;
    }
    if digest_text(&digest) != value {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(digest)
}

fn hex_nibble(byte: u8) -> Result<u8, AgentFailure> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => Err(AgentFailure::VaultUnavailable),
    }
}

pub(crate) fn map_unique_conflict(error: turso::Error) -> AgentFailure {
    match error {
        turso::Error::Constraint(_) => AgentFailure::Conflict,
        other => storage(other),
    }
}

fn decode_canonical<T: serde::de::DeserializeOwned + serde::Serialize>(
    payload: &str,
) -> Result<T, AgentFailure> {
    let value: T = serde_json::from_str(payload).map_err(|_| AgentFailure::VaultUnavailable)?;
    if serde_json::to_string(&value).map_err(|_| AgentFailure::VaultUnavailable)? != payload {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(value)
}
