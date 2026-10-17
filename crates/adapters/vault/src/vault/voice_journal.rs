//! Encrypted Voice lifecycle custody. No native/provider I/O occurs under a
//! transaction. Exact replay is read-only; new state and receipt commit together.
use super::*;
use floe_execution::BoxFuture;
use floe_kernel::{CommandFailure, OwnerActor};
use floe_voice::{
    VoiceCallRecord, VoiceJournal, VoiceMutation, VoiceMutationAdmission, VoiceMutationReceipt,
    VoiceStartAdmission, VoiceStartIntent,
};
use serde::{Deserialize, Serialize};
use turso::{Connection, transaction::TransactionBehavior};

pub(super) fn hex(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn encode(value: &impl Serialize) -> Result<String, AgentFailure> {
    let text = serde_json::to_string(value).map_err(storage)?;
    if text.len() > 65536 {
        return Err(AgentFailure::BudgetExceeded);
    }
    Ok(text)
}
fn decode<T: serde::de::DeserializeOwned>(value: String) -> Result<T, AgentFailure> {
    if value.is_empty() || value.len() > 65536 {
        return Err(AgentFailure::StorageUnavailable);
    }
    serde_json::from_str(&value).map_err(storage)
}
pub(super) fn sql(error: turso::Error) -> AgentFailure {
    match error {
        turso::Error::Constraint(_) => AgentFailure::Conflict,
        other => database_failure(other),
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    request: VoiceMutation,
    receipt: VoiceMutationReceipt,
}
impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub(super) fn voice_actor(&self, actor: &OwnerActor) -> Result<(), AgentFailure> {
        self.check_access()?;
        actor.validate()?;
        if actor.person_id != self.person_id {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(())
    }
    async fn voice_record_on(
        &self,
        c: &Connection,
        id: Uuid,
    ) -> Result<Option<VoiceCallRecord>, AgentFailure> {
        let mut rows=c.query("SELECT person_id, device_id, surface_session_id, media_lease_id, intent_digest, revision, lease_held, CASE WHEN length(CAST(payload AS BLOB)) <= 65536 THEN payload ELSE NULL END FROM voice_sessions WHERE voice_session_id = ?",[id.to_string()]).await.map_err(sql)?;
        let Some(row) = rows.next().await.map_err(sql)? else {
            let mut orphan = c
                .query(
                    "SELECT 1 FROM voice_session_commands WHERE voice_session_id = ?1 UNION ALL SELECT 1 FROM voice_events WHERE voice_session_id = ?1 LIMIT 1",
                    [id.to_string()],
                )
                .await
                .map_err(sql)?;
            if orphan.next().await.map_err(sql)?.is_some() {
                return Err(AgentFailure::StorageUnavailable);
            }
            return Ok(None);
        };
        let record: VoiceCallRecord = decode(row.get::<String>(7).map_err(storage)?)?;
        record.validate().map_err(storage)?;
        let binding = &record.intent.binding;
        if binding.voice_session_id != id
            || binding.actor.person_id != self.person_id
            || row.get::<String>(0).map_err(storage)? != self.person_id.to_string()
            || row.get::<String>(1).map_err(storage)? != binding.actor.device_id
            || row.get::<String>(2).map_err(storage)? != binding.surface_session_id.to_string()
            || row.get::<String>(3).map_err(storage)? != binding.media_lease_id.to_string()
            || row.get::<String>(4).map_err(storage)? != hex(&record.intent.digest()?)
            || row.get::<i64>(5).map_err(storage)? != record.revision as i64
            || row.get::<i64>(6).map_err(storage)?
                != i64::from(!record.session.local_lease_released())
            || rows.next().await.map_err(sql)?.is_some()
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(Some(record))
    }
    async fn voice_entry_on(
        &self,
        c: &Connection,
        record: &VoiceCallRecord,
        command: Uuid,
    ) -> Result<Option<Entry>, AgentFailure> {
        let mut rows=c.query("SELECT request_digest, revision, CASE WHEN length(CAST(payload AS BLOB)) <= 65536 THEN payload ELSE NULL END FROM voice_session_commands WHERE voice_session_id = ? AND command_id = ?",(record.intent.binding.voice_session_id.to_string(),command.to_string())).await.map_err(sql)?;
        let Some(row) = rows.next().await.map_err(sql)? else {
            return Ok(None);
        };
        let entry: Entry = decode(row.get::<String>(2).map_err(storage)?)?;
        self.validate_voice_entry(record, &entry)?;
        if entry.request.command_id != command
            || row.get::<String>(0).map_err(storage)? != hex(&entry.receipt.request_digest)
            || row.get::<i64>(1).map_err(storage)? != entry.receipt.revision as i64
            || rows.next().await.map_err(sql)?.is_some()
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(Some(entry))
    }
    fn validate_voice_entry(
        &self,
        record: &VoiceCallRecord,
        entry: &Entry,
    ) -> Result<(), AgentFailure> {
        record.validate_receipt(&entry.receipt).map_err(storage)?;
        let mut actor = record.intent.binding.actor.clone();
        actor.runtime_epoch = entry.receipt.caller_epoch;
        record
            .authorize_mutation(&actor, &entry.request.kind)
            .map_err(storage)?;
        if entry.request.voice_session_id != record.intent.binding.voice_session_id
            || entry.request.command_id != entry.receipt.command_id
            || entry.request.expected_revision.checked_add(1) != Some(entry.receipt.revision)
            || entry.request.digest(&actor)? != entry.receipt.request_digest
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(())
    }
    pub(super) async fn voice_current_on(
        &self,
        c: &Connection,
        id: Uuid,
    ) -> Result<Option<VoiceCallRecord>, AgentFailure> {
        let Some(record) = self.voice_record_on(c, id).await? else {
            return Ok(None);
        };
        let mut rows=c.query("SELECT command_id FROM voice_session_commands WHERE voice_session_id = ? ORDER BY revision DESC LIMIT 1",[id.to_string()]).await.map_err(sql)?;
        let last = rows.next().await.map_err(sql)?;
        if record.revision == 1 {
            if last.is_some() {
                return Err(AgentFailure::StorageUnavailable);
            }
        } else {
            let last = last.ok_or(AgentFailure::StorageUnavailable)?;
            let command =
                Uuid::parse_str(&last.get::<String>(0).map_err(storage)?).map_err(storage)?;
            let entry = self
                .voice_entry_on(c, &record, command)
                .await?
                .ok_or(AgentFailure::StorageUnavailable)?;
            if entry.receipt.revision != record.revision {
                return Err(AgentFailure::StorageUnavailable);
            }
        }
        Ok(Some(record))
    }
    pub(super) async fn voice_revision_digest_on(
        &self,
        c: &Connection,
        record: &VoiceCallRecord,
        revision: u64,
    ) -> Result<[u8; 32], AgentFailure> {
        if revision == 0 || revision > record.revision {
            return Err(AgentFailure::StorageUnavailable);
        }
        if revision == 1 {
            return VoiceCallRecord::prepare(record.intent.clone())
                .map_err(storage)?
                .digest();
        }
        let mut rows=c.query("SELECT command_id FROM voice_session_commands WHERE voice_session_id = ? AND revision = ?",(record.intent.binding.voice_session_id.to_string(),revision as i64)).await.map_err(sql)?;
        let row = rows
            .next()
            .await
            .map_err(sql)?
            .ok_or(AgentFailure::StorageUnavailable)?;
        let command = Uuid::parse_str(&row.get::<String>(0).map_err(storage)?).map_err(storage)?;
        let entry = self
            .voice_entry_on(c, record, command)
            .await?
            .ok_or(AgentFailure::StorageUnavailable)?;
        Ok(entry.receipt.state_digest)
    }
    pub(super) async fn voice_advance_state_on(
        &self,
        c: &Connection,
        record: &VoiceCallRecord,
        mut replay: VoiceCallRecord,
        through: u64,
    ) -> Result<VoiceCallRecord, AgentFailure> {
        if through < replay.revision || through > record.revision {
            return Err(AgentFailure::StorageUnavailable);
        }
        while replay.revision < through {
            let mut rows=c.query("SELECT command_id FROM voice_session_commands WHERE voice_session_id = ? AND revision > ? AND revision <= ? ORDER BY revision LIMIT 128",(record.intent.binding.voice_session_id.to_string(),replay.revision as i64,through as i64)).await.map_err(sql)?;
            let mut commands = Vec::new();
            while let Some(row) = rows.next().await.map_err(sql)? {
                commands.push(
                    Uuid::parse_str(&row.get::<String>(0).map_err(storage)?).map_err(storage)?,
                );
            }
            if commands.is_empty() {
                return Err(AgentFailure::StorageUnavailable);
            }
            for command in commands {
                let entry = self
                    .voice_entry_on(c, record, command)
                    .await?
                    .ok_or(AgentFailure::StorageUnavailable)?;
                let mut actor = record.intent.binding.actor.clone();
                actor.runtime_epoch = entry.receipt.caller_epoch;
                let (next, expected) = replay.apply(&actor, &entry.request).map_err(storage)?;
                if expected != entry.receipt {
                    return Err(AgentFailure::StorageUnavailable);
                }
                replay = next;
            }
        }
        Ok(replay)
    }
    /// Read-only startup validation replays bounded pages through the same
    /// lifecycle owner; no missing/invalid row is repaired or re-dispatched.
    pub(super) async fn validate_voice_journal(&self) -> Result<(), AgentFailure> {
        let mut c = self.connection()?;
        let tx = c
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(sql)?;
        schema(&tx).await?;
        let mut orphan = tx.query("SELECT 1 FROM voice_session_commands c LEFT JOIN voice_sessions s ON s.voice_session_id = c.voice_session_id WHERE s.voice_session_id IS NULL UNION ALL SELECT 1 FROM voice_events e LEFT JOIN voice_sessions s ON s.voice_session_id = e.voice_session_id WHERE s.voice_session_id IS NULL LIMIT 1", ()).await.map_err(sql)?;
        if orphan.next().await.map_err(sql)?.is_some() {
            return Err(AgentFailure::StorageUnavailable);
        }
        drop(orphan);
        let mut after = String::new();
        loop {
            let mut rows=tx.query("SELECT voice_session_id FROM voice_sessions WHERE voice_session_id > ? ORDER BY voice_session_id LIMIT 128",[after.clone()]).await.map_err(sql)?;
            let mut ids = Vec::new();
            while let Some(row) = rows.next().await.map_err(sql)? {
                ids.push(row.get::<String>(0).map_err(storage)?);
            }
            let count = ids.len();
            for raw in ids {
                let id = Uuid::parse_str(&raw).map_err(storage)?;
                let record = self
                    .voice_current_on(&tx, id)
                    .await?
                    .ok_or(AgentFailure::StorageUnavailable)?;
                self.surface_session_on(
                    &tx,
                    &record.intent.binding.actor,
                    record.intent.binding.surface_session_id,
                )
                .await?;
                self.surface_anchor_on(&tx, record.intent.source.through())
                    .await?;
                let initial = VoiceCallRecord::prepare(record.intent.clone()).map_err(storage)?;
                let replay = self
                    .voice_advance_state_on(&tx, &record, initial, record.revision)
                    .await?;
                self.validate_voice_events_on(&tx, &record).await?;
                if replay.digest()? != record.digest()? {
                    return Err(AgentFailure::StorageUnavailable);
                }
                after = raw;
            }
            if count < 128 {
                break;
            }
        }
        tx.commit().await.map_err(sql)?;
        self.check_access()
    }
    async fn voice_mutate_on(
        &self,
        c: &Connection,
        actor: &OwnerActor,
        request: VoiceMutation,
    ) -> Result<VoiceMutationAdmission, AgentFailure> {
        let record = self
            .voice_current_on(c, request.voice_session_id)
            .await?
            .ok_or(AgentFailure::NotFound)?;
        record.authorize_mutation(actor, &request.kind)?;
        let request_digest = request.digest(actor)?;
        if let Some(entry) = self.voice_entry_on(c, &record, request.command_id).await? {
            if entry.receipt.request_digest != request_digest {
                return Err(AgentFailure::Conflict);
            }
            return Ok(VoiceMutationAdmission {
                record,
                receipt: entry.receipt,
                replayed: true,
            });
        }
        let (next, receipt) = record.apply(actor, &request)?;
        let changed=c.execute("UPDATE voice_sessions SET revision = ?, lease_held = ?, payload = ? WHERE voice_session_id = ? AND revision = ? AND intent_digest = ?",(next.revision as i64,i64::from(!next.session.local_lease_released()),encode(&next)?,request.voice_session_id.to_string(),record.revision as i64,hex(&record.intent.digest()?))).await.map_err(sql)?;
        if changed != 1 {
            return Err(AgentFailure::Conflict);
        }
        #[cfg(test)]
        if self
            .voice_fault_after_state_write
            .swap(false, Ordering::AcqRel)
        {
            return Err(AgentFailure::Conflict);
        }
        let entry = Entry {
            request,
            receipt: receipt.clone(),
        };
        c.execute("INSERT INTO voice_session_commands (voice_session_id, command_id, request_digest, revision, payload) VALUES (?, ?, ?, ?, ?)",(receipt.voice_session_id.to_string(),receipt.command_id.to_string(),hex(&receipt.request_digest),receipt.revision as i64,encode(&entry)?)).await.map_err(sql)?;
        Ok(VoiceMutationAdmission {
            record: next,
            receipt,
            replayed: false,
        })
    }
}
async fn schema(c: &Connection) -> Result<(), AgentFailure> {
    crate::schema::inspect_family(c, crate::schema::Family::Voice)
        .await
        .map_err(crate::schema::SchemaFailure::into_agent)
}
impl<Keys: VaultKeyProvider> VoiceJournal for EncryptedAgentVault<Keys> {
    fn read_call<'a>(
        &'a self,
        actor: &'a OwnerActor,
        id: Uuid,
    ) -> BoxFuture<'a, Result<Option<VoiceCallRecord>, AgentFailure>> {
        Box::pin(async move {
            self.voice_actor(actor)?;
            if id.is_nil() {
                return Err(AgentFailure::InvalidInput);
            }
            let mut c = self.connection()?;
            let tx = c
                .transaction_with_behavior(TransactionBehavior::Deferred)
                .await
                .map_err(sql)?;
            schema(&tx).await?;
            let record = self.voice_current_on(&tx, id).await?;
            if record
                .as_ref()
                .is_some_and(|r| r.intent.binding.actor.device_id != actor.device_id)
            {
                return Err(AgentFailure::PolicyDenied);
            }
            tx.commit().await.map_err(sql)?;
            self.check_access()?;
            Ok(record)
        })
    }
    fn admit_start<'a>(
        &'a self,
        actor: &'a OwnerActor,
        intent: VoiceStartIntent,
    ) -> BoxFuture<'a, Result<VoiceStartAdmission, CommandFailure<AgentFailure>>> {
        Box::pin(async move {
            self.voice_actor(actor)
                .map_err(CommandFailure::NotAdmitted)?;
            intent.validate().map_err(CommandFailure::Indeterminate)?;
            if &intent.binding.actor != actor {
                return Err(CommandFailure::Indeterminate(AgentFailure::PolicyDenied));
            }
            let mut c = self.connection().map_err(CommandFailure::Indeterminate)?;
            for read_only in [true, false] {
                let tx = c
                    .transaction_with_behavior(if read_only {
                        TransactionBehavior::Deferred
                    } else {
                        TransactionBehavior::Immediate
                    })
                    .await
                    .map_err(|e| CommandFailure::Indeterminate(sql(e)))?;
                let result = async {
                    schema(&tx).await?;
                    if let Some(record) = self.voice_current_on(&tx, intent.binding.voice_session_id).await? {
                        if record.intent != intent { return Err(AgentFailure::Conflict); }
                        return Ok(Some(VoiceStartAdmission { record, replayed: true }));
                    }
                    if read_only { return Ok(None); }
                    self.surface_session_on(&tx, actor, intent.binding.surface_session_id).await?;
                    self.surface_anchor_on(&tx, intent.source.through()).await?;
                    let record = VoiceCallRecord::prepare(intent.clone())?;
                    let values = (
                        intent.binding.voice_session_id.to_string(), self.person_id.to_string(),
                        actor.device_id.clone(), intent.binding.surface_session_id.to_string(),
                        intent.binding.media_lease_id.to_string(), hex(&intent.digest()?), encode(&record)?,
                    );
                    tx.execute(
                        "INSERT INTO voice_sessions (voice_session_id, person_id, device_id, surface_session_id, media_lease_id, intent_digest, revision, lease_held, payload) VALUES (?, ?, ?, ?, ?, ?, 1, 1, ?)",
                        values,
                    ).await.map_err(sql)?;
                    Ok(Some(VoiceStartAdmission { record, replayed: false }))
                }.await.map_err(CommandFailure::Indeterminate);
                if read_only && matches!(&result, Ok(None)) {
                    // Committing an absence observation does not admit a start.
                    tx.commit()
                        .await
                        .map_err(|e| CommandFailure::Indeterminate(sql(e)))?;
                    self.check_access().map_err(CommandFailure::Indeterminate)?;
                    continue;
                }
                let value = self.finish_registry_command_transaction(tx, result).await?;
                #[cfg(test)]
                if value.as_ref().is_some_and(|v| !v.replayed)
                    && self
                        .registry_transaction_ack_loss
                        .swap(false, Ordering::AcqRel)
                {
                    self.unavailable.store(true, Ordering::Release);
                    return Err(CommandFailure::Indeterminate(
                        AgentFailure::StorageUnavailable,
                    ));
                }
                if let Some(value) = value {
                    return Ok(value);
                }
            }
            Err(CommandFailure::Indeterminate(
                AgentFailure::StorageUnavailable,
            ))
        })
    }
    fn mutate<'a>(
        &'a self,
        actor: &'a OwnerActor,
        request: VoiceMutation,
    ) -> BoxFuture<'a, Result<VoiceMutationAdmission, CommandFailure<AgentFailure>>> {
        Box::pin(async move {
            self.voice_actor(actor)
                .map_err(CommandFailure::NotAdmitted)?;
            request.validate().map_err(CommandFailure::Indeterminate)?;
            let mut c = self.connection().map_err(CommandFailure::Indeterminate)?;
            let read = c
                .transaction_with_behavior(TransactionBehavior::Deferred)
                .await
                .map_err(|e| CommandFailure::Indeterminate(sql(e)))?;
            schema(&read).await.map_err(CommandFailure::Indeterminate)?;
            let record = self
                .voice_current_on(&read, request.voice_session_id)
                .await
                .map_err(CommandFailure::Indeterminate)?
                .ok_or(CommandFailure::Indeterminate(AgentFailure::NotFound))?;
            record
                .authorize_mutation(actor, &request.kind)
                .map_err(CommandFailure::Indeterminate)?;
            if let Some(entry) = self
                .voice_entry_on(&read, &record, request.command_id)
                .await
                .map_err(CommandFailure::Indeterminate)?
            {
                if entry.receipt.request_digest
                    != request
                        .digest(actor)
                        .map_err(CommandFailure::Indeterminate)?
                {
                    return Err(CommandFailure::Indeterminate(AgentFailure::Conflict));
                }
                read.commit()
                    .await
                    .map_err(|e| CommandFailure::Indeterminate(sql(e)))?;
                self.check_access().map_err(CommandFailure::Admitted)?;
                return Ok(VoiceMutationAdmission {
                    record,
                    receipt: entry.receipt,
                    replayed: true,
                });
            }
            read.commit()
                .await
                .map_err(|e| CommandFailure::Indeterminate(sql(e)))?;
            let tx = c
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .await
                .map_err(|e| CommandFailure::Indeterminate(sql(e)))?;
            let result = async {
                schema(&tx).await?;
                self.voice_mutate_on(&tx, actor, request).await
            }
            .await
            .map_err(CommandFailure::Indeterminate);
            let value = self.finish_registry_command_transaction(tx, result).await?;
            #[cfg(test)]
            if !value.replayed
                && self
                    .registry_transaction_ack_loss
                    .swap(false, Ordering::AcqRel)
            {
                self.unavailable.store(true, Ordering::Release);
                return Err(CommandFailure::Indeterminate(
                    AgentFailure::StorageUnavailable,
                ));
            }
            Ok(value)
        })
    }
}

#[cfg(test)]
#[path = "voice_journal_tests.rs"]
mod tests;
