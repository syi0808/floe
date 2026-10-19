//! Pre-planning command custody. No model/native I/O occurs in these transactions.
use super::*;
use floe_voice::{
    VoiceStartCommand, VoiceStartCommandAdmission, VoiceStartCommandRecord, VoiceStartCommandState,
};

fn state_name(state: VoiceStartCommandState) -> &'static str {
    match state {
        VoiceStartCommandState::Preparing => "preparing",
        VoiceStartCommandState::Bound => "bound",
        VoiceStartCommandState::Cancelled => "cancelled",
    }
}
impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub(super) async fn voice_start_on(
        &self,
        c: &Connection,
        id: Uuid,
    ) -> Result<Option<VoiceStartCommandRecord>, AgentFailure> {
        let mut rows = c.query("SELECT person_id, device_id, surface_session_id, request_digest, state, CASE WHEN length(CAST(payload AS BLOB)) <= 65536 THEN payload ELSE NULL END FROM voice_start_commands WHERE command_id = ?", [id.to_string()]).await.map_err(sql)?;
        let Some(row) = rows.next().await.map_err(sql)? else {
            return Ok(None);
        };
        let value: VoiceStartCommandRecord = decode(row.get::<String>(5).map_err(storage)?)?;
        value.validate().map_err(storage)?;
        if value.command.command_id != id
            || value.actor.person_id != self.person_id
            || row.get::<String>(0).map_err(storage)? != self.person_id.to_string()
            || row.get::<String>(1).map_err(storage)? != value.actor.device_id
            || row.get::<String>(2).map_err(storage)?
                != value.command.surface_session_id.to_string()
            || row.get::<String>(3).map_err(storage)? != hex(&value.command.digest(&value.actor)?)
            || row.get::<String>(4).map_err(storage)? != state_name(value.state)
            || rows.next().await.map_err(sql)?.is_some()
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(Some(value))
    }
    pub(super) async fn check_start_link(
        &self,
        c: &Connection,
        start: &VoiceStartCommandRecord,
    ) -> Result<(), AgentFailure> {
        let call = self.voice_record_on(c, start.command.command_id).await?;
        match (start.state, call) {
            (VoiceStartCommandState::Bound, Some(call))
                if call.intent.binding.actor == start.actor
                    && call.intent.binding.surface_session_id
                        == start.command.surface_session_id =>
            {
                Ok(())
            }
            (VoiceStartCommandState::Preparing | VoiceStartCommandState::Cancelled, None) => Ok(()),
            _ => Err(AgentFailure::StorageUnavailable),
        }
    }
    pub(super) async fn start_command_operation(
        &self,
        actor: &OwnerActor,
        command: VoiceStartCommand,
        cancel: bool,
    ) -> Result<VoiceStartCommandAdmission, CommandFailure<AgentFailure>> {
        self.voice_actor(actor)
            .map_err(CommandFailure::NotAdmitted)?;
        command.validate().map_err(CommandFailure::NotAdmitted)?;
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
                if let Some(mut record) = self.voice_start_on(&tx, command.command_id).await? {
                    if record.actor.device_id != actor.device_id { return Err(AgentFailure::PolicyDenied); }
                    if record.command != command { return Err(AgentFailure::Conflict); }
                    self.check_start_link(&tx, &record).await?;
                    if !cancel || record.state == VoiceStartCommandState::Cancelled {
                        return Ok(Some(VoiceStartCommandAdmission { record, replayed: true }));
                    }
                    if record.state == VoiceStartCommandState::Bound { return Err(AgentFailure::Conflict); }
                    if read_only { return Ok(None); }
                    record.state = VoiceStartCommandState::Cancelled;
                    let changed = tx.execute("UPDATE voice_start_commands SET state = 'cancelled', payload = ? WHERE command_id = ? AND state = 'preparing'",
                        (encode(&record)?, command.command_id.to_string())).await.map_err(sql)?;
                    if changed != 1 { return Err(AgentFailure::Conflict); }
                    return Ok(Some(VoiceStartCommandAdmission { record, replayed: false }));
                }
                // Missing command cannot be reconstructed from a call or a cancel.
                if self.voice_record_on(&tx, command.command_id).await?.is_some() {
                    return Err(AgentFailure::StorageUnavailable);
                }
                if cancel { return Err(AgentFailure::NotFound); }
                if read_only { return Ok(None); }
                self.surface_session_on(&tx, actor, command.surface_session_id).await?;
                let mut active = tx.query("SELECT 1 FROM voice_sessions WHERE person_id = ? AND device_id = ? AND lease_held = 1 LIMIT 1",
                    (actor.person_id.to_string(), actor.device_id.clone())).await.map_err(sql)?;
                if active.next().await.map_err(sql)?.is_some() { return Err(AgentFailure::Conflict); }
                drop(active);
                let record = VoiceStartCommandRecord { command: command.clone(), actor: actor.clone(), state: VoiceStartCommandState::Preparing };
                tx.execute("INSERT INTO voice_start_commands(command_id,person_id,device_id,surface_session_id,request_digest,state,payload) VALUES(?,?,?,?,?,'preparing',?)",
                    (command.command_id.to_string(), actor.person_id.to_string(), actor.device_id.clone(), command.surface_session_id.to_string(), hex(&command.digest(actor)?), encode(&record)?)).await.map_err(sql)?;
                Ok(Some(VoiceStartCommandAdmission { record, replayed: false }))
            }.await.map_err(CommandFailure::Indeterminate);
            if read_only && matches!(&result, Ok(None)) {
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
            return value.ok_or(CommandFailure::Indeterminate(
                AgentFailure::StorageUnavailable,
            ));
        }
        Err(CommandFailure::Indeterminate(
            AgentFailure::StorageUnavailable,
        ))
    }
}
