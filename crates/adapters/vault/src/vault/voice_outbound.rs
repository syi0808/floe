//! Encrypted one-shot outbound custody. No provider I/O under a transaction.
use super::voice_journal::{hex, sql};
use super::*;
use floe_execution::BoxFuture;
use floe_kernel::{CommandFailure, OwnerActor};
use floe_voice::{
    VoiceAppendAdmission, VoiceAppendIntent, VoiceAppendReceipt, VoiceAppendRecord,
    VoiceCallRecord, VoiceEventOutcome, VoiceOutboundJournal,
};
use turso::{Connection, transaction::TransactionBehavior};

fn encode(record: &VoiceAppendRecord) -> Result<String, AgentFailure> {
    record.validate()?;
    let payload = serde_json::to_string(record).map_err(storage)?;
    if payload.len() > 524288 {
        return Err(AgentFailure::ModelInputCapacityExceeded);
    }
    Ok(payload)
}
async fn schema(c: &Connection) -> Result<(), AgentFailure> {
    crate::schema::inspect_family(c, crate::schema::Family::Voice)
        .await
        .map_err(crate::schema::SchemaFailure::into_agent)
}
impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    async fn voice_append_work_on(
        &self,
        c: &Connection,
        call: &VoiceCallRecord,
        intent: &VoiceAppendIntent,
    ) -> Result<floe_kernel::CommandId, AgentFailure> {
        intent.validate()?;
        if intent.binding != call.intent.binding {
            return Err(AgentFailure::PolicyDenied);
        }
        let delegation = intent
            .payload
            .delegation_id
            .as_deref()
            .ok_or(AgentFailure::InvalidInput)?;
        let event = self
            .voice_event_on(c, call, "delegation", delegation)
            .await?
            .ok_or(AgentFailure::NotFound)?;
        let VoiceEventOutcome::Work {
            work_command_id, ..
        } = event.outcome
        else {
            return Err(AgentFailure::Conflict);
        };
        let (snapshot, run_id) = self
            .read_live_result_on(c, &intent.binding.actor, &intent.source)
            .await?;
        let work = self
            .surface_work_for_run_on(c, run_id)
            .await?
            .ok_or(AgentFailure::NotFound)?;
        if work.command_id != work_command_id
            || snapshot.presentable_payload()?.as_str() != intent.payload.text
        {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(work_command_id)
    }
    async fn voice_append_on(
        &self,
        c: &Connection,
        call: &VoiceCallRecord,
        operation_id: Uuid,
    ) -> Result<Option<VoiceAppendRecord>, AgentFailure> {
        let mut rows = c.query("SELECT intent_digest, admitted_revision, state_digest, work_command_id, settled, receipt_digest, CASE WHEN length(CAST(payload AS BLOB)) <= 524288 THEN payload ELSE NULL END FROM voice_appends WHERE voice_session_id = ? AND operation_id = ?", (call.intent.binding.voice_session_id.to_string(), operation_id.to_string())).await.map_err(sql)?;
        let Some(row) = rows.next().await.map_err(sql)? else {
            return Ok(None);
        };
        let payload: String = row.get(6).map_err(storage)?;
        if payload.is_empty() || payload.len() > 524288 {
            return Err(AgentFailure::StorageUnavailable);
        }
        let record: VoiceAppendRecord = serde_json::from_str(&payload).map_err(storage)?;
        record.validate().map_err(storage)?;
        if record.intent.binding != call.intent.binding
            || record.intent.payload.operation_id != operation_id
            || row.get::<String>(0).map_err(storage)? != hex(&record.intent.digest()?)
            || row.get::<i64>(1).map_err(storage)? != record.admitted_revision as i64
            || row.get::<String>(2).map_err(storage)? != hex(&record.state_digest)
            || row.get::<String>(3).map_err(storage)?
                != record.work_command_id.as_uuid().to_string()
            || row.get::<i64>(4).map_err(storage)? != i64::from(record.receipt.is_some())
            || row.get::<Option<String>>(5).map_err(storage)?
                != record
                    .receipt
                    .as_ref()
                    .map(|r| r.digest().map(|d| hex(&d)))
                    .transpose()?
            || rows.next().await.map_err(sql)?.is_some()
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        drop(rows);
        let initial = VoiceCallRecord::prepare(call.intent.clone()).map_err(storage)?;
        let admitted = self
            .voice_advance_state_on(c, call, initial, record.admitted_revision)
            .await?;
        record
            .intent
            .validate_for_call(&admitted)
            .map_err(storage)?;
        if admitted.digest()? != record.state_digest
            || self
                .voice_append_work_on(c, call, &record.intent)
                .await
                .map_err(storage)?
                != record.work_command_id
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(Some(record))
    }
    pub(super) async fn validate_voice_appends_on(
        &self,
        c: &Connection,
        call: &VoiceCallRecord,
    ) -> Result<(), AgentFailure> {
        let mut after = String::new();
        loop {
            let mut rows = c.query("SELECT operation_id FROM voice_appends WHERE voice_session_id = ? AND operation_id > ? ORDER BY operation_id LIMIT 128", (call.intent.binding.voice_session_id.to_string(), after.clone())).await.map_err(sql)?;
            let mut ids = Vec::new();
            while let Some(row) = rows.next().await.map_err(sql)? {
                ids.push(row.get::<String>(0).map_err(storage)?);
            }
            drop(rows);
            let count = ids.len();
            for id in ids {
                let parsed = Uuid::parse_str(&id).map_err(storage)?;
                self.voice_append_on(c, call, parsed)
                    .await?
                    .ok_or(AgentFailure::StorageUnavailable)?;
                after = id;
            }
            if count < 128 {
                return Ok(());
            }
        }
    }
}
impl<Keys: VaultKeyProvider> VoiceOutboundJournal for EncryptedAgentVault<Keys> {
    fn read_append<'a>(
        &'a self,
        actor: &'a OwnerActor,
        voice: Uuid,
        operation: Uuid,
    ) -> BoxFuture<'a, Result<Option<VoiceAppendRecord>, AgentFailure>> {
        Box::pin(async move {
            self.voice_actor(actor)?;
            if voice.is_nil() || operation.is_nil() {
                return Err(AgentFailure::InvalidInput);
            }
            let mut c = self.connection()?;
            let tx = c
                .transaction_with_behavior(TransactionBehavior::Deferred)
                .await
                .map_err(sql)?;
            schema(&tx).await?;
            let record = if let Some(call) = self.voice_current_on(&tx, voice).await? {
                if call.intent.binding.actor.device_id != actor.device_id {
                    return Err(AgentFailure::PolicyDenied);
                }
                self.voice_append_on(&tx, &call, operation).await?
            } else {
                None
            };
            tx.commit().await.map_err(sql)?;
            self.check_access()?;
            Ok(record)
        })
    }
    fn claim_append<'a>(
        &'a self,
        actor: &'a OwnerActor,
        intent: VoiceAppendIntent,
    ) -> BoxFuture<'a, Result<VoiceAppendAdmission, CommandFailure<AgentFailure>>> {
        Box::pin(async move {
            self.voice_actor(actor)
                .map_err(CommandFailure::NotAdmitted)?;
            intent.validate().map_err(CommandFailure::NotAdmitted)?;
            if &intent.binding.actor != actor {
                return Err(CommandFailure::NotAdmitted(AgentFailure::PolicyDenied));
            }
            let mut c = self.connection().map_err(CommandFailure::NotAdmitted)?;
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
                    let call = self.voice_current_on(&tx, intent.binding.voice_session_id).await?.ok_or(AgentFailure::NotFound)?;
                    if let Some(record) = self.voice_append_on(&tx, &call, intent.payload.operation_id).await? {
                        if record.intent != intent { return Err(AgentFailure::Conflict); }
                        return Ok(Some(VoiceAppendAdmission { record, replayed: true }));
                    }
                    if read_only { return Ok(None); }
                    intent.validate_for_call(&call)?;
                    let work_command_id = self.voice_append_work_on(&tx, &call, &intent).await?;
                    let record = VoiceAppendRecord { intent: intent.clone(), admitted_revision: call.revision, state_digest: call.digest()?, work_command_id, receipt: None };
                    tx.execute("INSERT INTO voice_appends (voice_session_id, operation_id, intent_digest, admitted_revision, state_digest, work_command_id, settled, receipt_digest, payload) VALUES (?, ?, ?, ?, ?, ?, 0, NULL, ?)", (intent.binding.voice_session_id.to_string(), intent.payload.operation_id.to_string(), hex(&intent.digest()?), call.revision as i64, hex(&record.state_digest), work_command_id.as_uuid().to_string(), encode(&record)?)).await.map_err(sql)?;
                    Ok(Some(VoiceAppendAdmission { record, replayed: false }))
                }.await.map_err(CommandFailure::Indeterminate);
                if read_only && matches!(&result, Ok(None)) {
                    tx.commit()
                        .await
                        .map_err(|e| CommandFailure::Indeterminate(sql(e)))?;
                    continue;
                }
                let result =
                    result?.ok_or(CommandFailure::Indeterminate(AgentFailure::Conflict))?;
                tx.commit()
                    .await
                    .map_err(|e| CommandFailure::Indeterminate(sql(e)))?;
                self.check_access().map_err(CommandFailure::Indeterminate)?;
                return Ok(result);
            }
            Err(CommandFailure::Indeterminate(AgentFailure::Conflict))
        })
    }
    fn settle_append<'a>(
        &'a self,
        actor: &'a OwnerActor,
        voice: Uuid,
        receipt: VoiceAppendReceipt,
    ) -> BoxFuture<'a, Result<VoiceAppendRecord, CommandFailure<AgentFailure>>> {
        Box::pin(async move {
            self.voice_actor(actor)
                .map_err(CommandFailure::NotAdmitted)?;
            if voice.is_nil() || receipt.operation_id.is_nil() || receipt.intent_digest == [0; 32] {
                return Err(CommandFailure::NotAdmitted(AgentFailure::InvalidInput));
            }
            let mut c = self.connection().map_err(CommandFailure::NotAdmitted)?;
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
                    let call = self.voice_current_on(&tx, voice).await?.ok_or(AgentFailure::NotFound)?;
                    if &call.intent.binding.actor != actor { return Err(AgentFailure::PolicyDenied); }
                    let mut record = self.voice_append_on(&tx, &call, receipt.operation_id).await?.ok_or(AgentFailure::NotFound)?;
                    if receipt.intent_digest != record.intent.digest()? { return Err(AgentFailure::Conflict); }
                    if let Some(existing) = &record.receipt {
                        if existing != &receipt { return Err(AgentFailure::Conflict); }
                        return Ok(Some(record));
                    }
                    if read_only { return Ok(None); }
                    record.receipt = Some(receipt.clone());
                    let changed = tx.execute("UPDATE voice_appends SET settled = 1, receipt_digest = ?, payload = ? WHERE voice_session_id = ? AND operation_id = ? AND settled = 0", (hex(&receipt.digest()?), encode(&record)?, voice.to_string(), receipt.operation_id.to_string())).await.map_err(sql)?;
                    if changed != 1 { return Err(AgentFailure::Conflict); }
                    Ok(Some(record))
                }.await.map_err(CommandFailure::Indeterminate);
                if read_only && matches!(&result, Ok(None)) {
                    tx.commit()
                        .await
                        .map_err(|e| CommandFailure::Indeterminate(sql(e)))?;
                    continue;
                }
                let result =
                    result?.ok_or(CommandFailure::Indeterminate(AgentFailure::Conflict))?;
                tx.commit()
                    .await
                    .map_err(|e| CommandFailure::Indeterminate(sql(e)))?;
                self.check_access().map_err(CommandFailure::Indeterminate)?;
                return Ok(result);
            }
            Err(CommandFailure::Indeterminate(AgentFailure::Conflict))
        })
    }
}
