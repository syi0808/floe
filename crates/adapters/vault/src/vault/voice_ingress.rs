//! Atomic normalized event custody and Conversation-owned input/Work admission.
use super::voice_journal::{hex, sql};
use super::*;
use floe_conversation::{SurfaceInputReceipt, SurfaceWorkReceipt, WorkOrigin};
use floe_execution::BoxFuture;
use floe_kernel::{CommandFailure, CommandId, OwnerActor};
use floe_model_contract::LiveProviderEvent;
use floe_voice::{
    VoiceCallRecord, VoiceEventOutcome, VoiceEventRecord, VoiceIngressAdmission,
    VoiceIngressJournal, VoiceIngressPlan, VoiceInputCorrelation, VoiceUnroutedReason,
    event_digest, event_kind, input_lookup_digest, plan_ingress, validate_input_link,
    validate_work_link, work_command_id,
};
use turso::{Connection, transaction::TransactionBehavior};

fn query_key(kind: &str, id: &str) -> Result<(), AgentFailure> {
    if !matches!(kind, "person_final" | "assistant_final" | "delegation")
        || id.is_empty()
        || id.len() > 256
        || id.trim() != id
        || id.chars().any(char::is_control)
    {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(())
}
fn refs(record: &VoiceEventRecord) -> (Option<CommandId>, Option<CommandId>) {
    match &record.outcome {
        VoiceEventOutcome::Input { command_id } => (Some(*command_id), None),
        VoiceEventOutcome::Work {
            input_command_id,
            work_command_id,
        } => (Some(*input_command_id), Some(*work_command_id)),
        VoiceEventOutcome::Unrouted {
            input_command_id, ..
        } => (*input_command_id, None),
        _ => (None, None),
    }
}
fn lookup(record: &VoiceEventRecord) -> Result<Option<String>, AgentFailure> {
    match (&record.event, &record.outcome) {
        (LiveProviderEvent::PersonUtteranceFinal { text, .. }, VoiceEventOutcome::Input { .. }) => {
            Ok(Some(hex(&input_lookup_digest(
                record.voice_session_id,
                text,
            )?)))
        }
        _ => Ok(None),
    }
}
fn encode(record: &VoiceEventRecord) -> Result<String, AgentFailure> {
    let payload = serde_json::to_string(record).map_err(storage)?;
    if payload.len() > 524288 {
        return Err(AgentFailure::BudgetExceeded);
    }
    Ok(payload)
}
struct WorkLink {
    work: SurfaceWorkReceipt,
    first_sequence: u64,
}
impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub(super) async fn voice_event_on(
        &self,
        c: &Connection,
        call: &VoiceCallRecord,
        kind: &str,
        id: &str,
    ) -> Result<Option<VoiceEventRecord>, AgentFailure> {
        query_key(kind, id)?;
        let mut rows=c.query("SELECT sequence, admitted_revision, state_digest, event_digest, input_command_id, input_lookup_digest, work_command_id, CASE WHEN length(CAST(payload AS BLOB)) <= 524288 THEN payload ELSE NULL END FROM voice_events WHERE voice_session_id = ? AND kind = ? AND event_id = ?",(call.intent.binding.voice_session_id.to_string(),kind.to_owned(),id.to_owned())).await.map_err(sql)?;
        let Some(row) = rows.next().await.map_err(sql)? else {
            return Ok(None);
        };
        let payload: String = row.get(7).map_err(storage)?;
        if payload.is_empty() || payload.len() > 524288 {
            return Err(AgentFailure::StorageUnavailable);
        }
        let record: VoiceEventRecord = serde_json::from_str(&payload).map_err(storage)?;
        record.validate(call).map_err(storage)?;
        let key = record
            .event
            .durable_key()
            .ok_or(AgentFailure::StorageUnavailable)?;
        let (input, work) = refs(&record);
        if event_kind(&record.event)? != kind
            || key.id != id
            || row.get::<i64>(0).map_err(storage)? != record.sequence as i64
            || row.get::<i64>(1).map_err(storage)? != record.admitted_revision as i64
            || row.get::<String>(2).map_err(storage)? != hex(&record.state_digest)
            || row.get::<String>(3).map_err(storage)? != hex(&event_digest(&record.event)?)
            || row.get::<Option<String>>(4).map_err(storage)?
                != input.map(|id| id.as_uuid().to_string())
            || row.get::<Option<String>>(5).map_err(storage)? != lookup(&record)?
            || row.get::<Option<String>>(6).map_err(storage)?
                != work.map(|id| id.as_uuid().to_string())
            || record.state_digest
                != self
                    .voice_revision_digest_on(c, call, record.admitted_revision)
                    .await?
            || rows.next().await.map_err(sql)?.is_some()
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(Some(record))
    }
    async fn voice_input_event_on(
        &self,
        c: &Connection,
        call: &VoiceCallRecord,
        event: &VoiceEventRecord,
    ) -> Result<SurfaceInputReceipt, AgentFailure> {
        let VoiceEventOutcome::Input { command_id } = event.outcome else {
            return Err(AgentFailure::StorageUnavailable);
        };
        let input = self
            .surface_input_command_on(c, command_id)
            .await?
            .ok_or(AgentFailure::StorageUnavailable)?;
        validate_input_link(call, &event.event, &input).map_err(storage)?;
        Ok(input)
    }
    async fn voice_correlate_on(
        &self,
        c: &Connection,
        call: &VoiceCallRecord,
        event: &LiveProviderEvent,
        before: u64,
    ) -> Result<VoiceInputCorrelation, AgentFailure> {
        let LiveProviderEvent::DelegationRequested {
            utterance_ref,
            input_hint,
            ..
        } = event
        else {
            return Ok(VoiceInputCorrelation::Missing);
        };
        if let Some(id) = utterance_ref {
            let Some(source) = self.voice_event_on(c, call, "person_final", id).await? else {
                return Ok(VoiceInputCorrelation::Missing);
            };
            if source.sequence >= before
                || !matches!(source.outcome, VoiceEventOutcome::Input { .. })
            {
                return Ok(VoiceInputCorrelation::Missing);
            }
            return Ok(VoiceInputCorrelation::Exact(
                self.voice_input_event_on(c, call, &source).await?,
            ));
        }
        let Some(hint) = input_hint else {
            return Ok(VoiceInputCorrelation::Missing);
        };
        let Ok(normalized) = floe_conversation::normalize_turn_text(hint) else {
            return Ok(VoiceInputCorrelation::Missing);
        };
        let fingerprint = hex(&input_lookup_digest(
            call.intent.binding.voice_session_id,
            &normalized,
        )?);
        let mut rows=c.query("SELECT event_id FROM voice_events WHERE voice_session_id = ? AND kind = 'person_final' AND input_lookup_digest = ? AND sequence < ? ORDER BY sequence DESC LIMIT 2",(call.intent.binding.voice_session_id.to_string(),fingerprint,before as i64)).await.map_err(sql)?;
        let mut matches = Vec::new();
        while let Some(row) = rows.next().await.map_err(sql)? {
            let id: String = row.get(0).map_err(storage)?;
            let source = self
                .voice_event_on(c, call, "person_final", &id)
                .await?
                .ok_or(AgentFailure::StorageUnavailable)?;
            let input = self.voice_input_event_on(c, call, &source).await?;
            if input.input.text == normalized {
                matches.push(input);
            }
        }
        Ok(match matches.len() {
            0 => VoiceInputCorrelation::Missing,
            1 => VoiceInputCorrelation::Exact(matches.remove(0)),
            _ => VoiceInputCorrelation::Ambiguous,
        })
    }
    async fn voice_work_link_on(
        &self,
        c: &Connection,
        call: &VoiceCallRecord,
        input: &SurfaceInputReceipt,
    ) -> Result<Option<WorkLink>, AgentFailure> {
        let command = work_command_id(call.intent.binding.voice_session_id, input.input.message_id);
        let Some(work) = self.surface_work_command_on(c, command).await? else {
            return Ok(None);
        };
        validate_work_link(call, input, &work).map_err(storage)?;
        let WorkOrigin::Voice { delegation_id, .. } = &work.origin else {
            return Err(AgentFailure::StorageUnavailable);
        };
        let first = self
            .voice_event_on(c, call, "delegation", delegation_id)
            .await?
            .ok_or(AgentFailure::StorageUnavailable)?;
        if first.outcome
            != (VoiceEventOutcome::Work {
                input_command_id: input.command_id,
                work_command_id: work.command_id,
            })
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(Some(WorkLink {
            work,
            first_sequence: first.sequence,
        }))
    }
    async fn voice_project_event_on(
        &self,
        c: &Connection,
        call: &VoiceCallRecord,
        record: VoiceEventRecord,
        replayed: bool,
    ) -> Result<VoiceIngressAdmission, AgentFailure> {
        let (input_ref, work_ref) = refs(&record);
        let input = if let Some(command) = input_ref {
            let input = self
                .surface_input_command_on(c, command)
                .await?
                .ok_or(AgentFailure::StorageUnavailable)?;
            validate_input_link(call, &record.event, &input).map_err(storage)?;
            if let floe_conversation::SurfaceInputOrigin::Voice { utterance_id, .. } =
                &input.input.origin
            {
                let source = self
                    .voice_event_on(c, call, "person_final", utterance_id)
                    .await?
                    .ok_or(AgentFailure::StorageUnavailable)?;
                if source.outcome
                    != (VoiceEventOutcome::Input {
                        command_id: command,
                    })
                    || source.sequence > record.sequence
                    || (event_kind(&record.event)? == "delegation"
                        && source.sequence == record.sequence)
                {
                    return Err(AgentFailure::StorageUnavailable);
                }
            }
            Some(input)
        } else {
            None
        };
        let work = if let Some(command) = work_ref {
            let link = self
                .voice_work_link_on(
                    c,
                    call,
                    input.as_ref().ok_or(AgentFailure::StorageUnavailable)?,
                )
                .await?
                .ok_or(AgentFailure::StorageUnavailable)?;
            if link.work.command_id != command || link.first_sequence > record.sequence {
                return Err(AgentFailure::StorageUnavailable);
            }
            Some(link.work)
        } else {
            None
        };
        Ok(VoiceIngressAdmission {
            record,
            input,
            work,
            replayed,
        })
    }
    pub(super) async fn validate_voice_events_on(
        &self,
        c: &Connection,
        call: &VoiceCallRecord,
    ) -> Result<(), AgentFailure> {
        let mut after = 0;
        let mut state = VoiceCallRecord::prepare(call.intent.clone()).map_err(storage)?;
        loop {
            let mut rows=c.query("SELECT kind, event_id FROM voice_events WHERE voice_session_id = ? AND sequence > ? ORDER BY sequence LIMIT 128",(call.intent.binding.voice_session_id.to_string(),after as i64)).await.map_err(sql)?;
            let mut keys = Vec::new();
            while let Some(row) = rows.next().await.map_err(sql)? {
                keys.push((
                    row.get::<String>(0).map_err(storage)?,
                    row.get::<String>(1).map_err(storage)?,
                ));
            }
            let count = keys.len();
            for (kind, id) in keys {
                let record = self
                    .voice_event_on(c, call, &kind, &id)
                    .await?
                    .ok_or(AgentFailure::StorageUnavailable)?;
                if record.sequence != after + 1 || record.admitted_revision < state.revision {
                    return Err(AgentFailure::StorageUnavailable);
                }
                state = self
                    .voice_advance_state_on(c, call, state, record.admitted_revision)
                    .await?;
                let projected = self
                    .voice_project_event_on(c, call, record.clone(), true)
                    .await?;
                let correlation = self
                    .voice_correlate_on(c, call, &record.event, record.sequence)
                    .await?;
                let existing = if let VoiceInputCorrelation::Exact(input) = &correlation {
                    self.voice_work_link_on(c, call, input)
                        .await?
                        .filter(|link| link.first_sequence < record.sequence)
                        .map(|link| link.work)
                } else {
                    None
                };
                let head = if matches!(record.outcome, VoiceEventOutcome::Input { .. }) {
                    projected
                        .input
                        .as_ref()
                        .ok_or(AgentFailure::StorageUnavailable)?
                        .before
                } else {
                    call.intent.source.through()
                };
                let plan = plan_ingress(
                    &state,
                    &call.intent.binding.actor,
                    &record.event,
                    head,
                    correlation,
                    existing,
                )
                .map_err(storage)?;
                let valid = match plan {
                    VoiceIngressPlan::AppendInput(request) => {
                        Some(request.admit(&call.intent.binding.actor).map_err(storage)?)
                            == projected.input
                    }
                    VoiceIngressPlan::Retain(outcome) => outcome == record.outcome,
                    VoiceIngressPlan::ExistingWork { input, work } => {
                        Some(input) == projected.input && Some(work) == projected.work
                    }
                    VoiceIngressPlan::SubmitWork { input, request } => match &record.outcome {
                        VoiceEventOutcome::Work { .. } => {
                            Some(input) == projected.input
                                && Some(request.admit(&call.intent.binding.actor).map_err(storage)?)
                                    == projected.work
                        }
                        VoiceEventOutcome::Unrouted {
                            reason: VoiceUnroutedReason::QueueFull,
                            input_command_id: Some(id),
                        } => *id == input.command_id && projected.work.is_none(),
                        _ => false,
                    },
                };
                if !valid {
                    return Err(AgentFailure::StorageUnavailable);
                }
                after = record.sequence;
            }
            if count < 128 {
                break;
            }
        }
        Ok(())
    }
    async fn voice_ingest_on(
        &self,
        c: &Connection,
        actor: &OwnerActor,
        voice: Uuid,
        event: LiveProviderEvent,
    ) -> Result<VoiceIngressAdmission, AgentFailure> {
        let call = self
            .voice_current_on(c, voice)
            .await?
            .ok_or(AgentFailure::NotFound)?;
        if &call.intent.binding.actor != actor {
            return Err(AgentFailure::PolicyDenied);
        }
        let kind = event_kind(&event)?;
        let id = event
            .durable_key()
            .ok_or(AgentFailure::InvalidInput)?
            .id
            .to_owned();
        if let Some(record) = self.voice_event_on(c, &call, kind, &id).await? {
            if record.event != event {
                return Err(AgentFailure::Conflict);
            }
            return self.voice_project_event_on(c, &call, record, true).await;
        }
        let mut rows = c
            .query(
                "SELECT COALESCE(MAX(sequence),0) FROM voice_events WHERE voice_session_id = ?",
                [voice.to_string()],
            )
            .await
            .map_err(sql)?;
        let last: i64 = rows
            .next()
            .await
            .map_err(sql)?
            .ok_or(AgentFailure::StorageUnavailable)?
            .get(0)
            .map_err(storage)?;
        let sequence = last
            .checked_add(1)
            .filter(|n| *n > 0)
            .ok_or(AgentFailure::BudgetExceeded)? as u64;
        let correlation = self.voice_correlate_on(c, &call, &event, sequence).await?;
        let existing = if let VoiceInputCorrelation::Exact(input) = &correlation {
            self.voice_work_link_on(c, &call, input)
                .await?
                .map(|link| link.work)
        } else {
            None
        };
        let head = self
            .surface_head_on(c, call.intent.binding.surface_session_id)
            .await?;
        let plan = plan_ingress(&call, actor, &event, head, correlation, existing)?;
        let outcome = match plan {
            VoiceIngressPlan::Retain(outcome) => outcome,
            VoiceIngressPlan::AppendInput(request) => {
                let receipt = self
                    .append_surface_input_on(c, actor, request.admit(actor)?)
                    .await
                    .map_err(CommandFailure::into_failure)?;
                VoiceEventOutcome::Input {
                    command_id: receipt.command_id,
                }
            }
            VoiceIngressPlan::ExistingWork { input, work } => VoiceEventOutcome::Work {
                input_command_id: input.command_id,
                work_command_id: work.command_id,
            },
            VoiceIngressPlan::SubmitWork { input, request } => match self
                .submit_surface_work_on(c, actor, request.admit(actor)?)
                .await
            {
                Ok(work) => VoiceEventOutcome::Work {
                    input_command_id: input.command_id,
                    work_command_id: work.command_id,
                },
                Err(CommandFailure::NotApplied(AgentFailure::BudgetExceeded)) => {
                    VoiceEventOutcome::Unrouted {
                        reason: VoiceUnroutedReason::QueueFull,
                        input_command_id: Some(input.command_id),
                    }
                }
                Err(failure) => return Err(failure.into_failure()),
            },
        };
        #[cfg(test)]
        if self
            .voice_fault_after_state_write
            .swap(false, Ordering::AcqRel)
        {
            return Err(AgentFailure::Conflict);
        }
        let record = VoiceEventRecord {
            voice_session_id: voice,
            sequence,
            admitted_revision: call.revision,
            state_digest: call.digest()?,
            event,
            outcome,
        };
        record.validate(&call)?;
        let (input, work) = refs(&record);
        let values = (
            voice.to_string(),
            kind.to_owned(),
            id,
            sequence as i64,
            record.admitted_revision as i64,
            hex(&record.state_digest),
            hex(&event_digest(&record.event)?),
            input.map(|id| id.as_uuid().to_string()),
            lookup(&record)?,
            work.map(|id| id.as_uuid().to_string()),
            encode(&record)?,
        );
        c.execute("INSERT INTO voice_events (voice_session_id, kind, event_id, sequence, admitted_revision, state_digest, event_digest, input_command_id, input_lookup_digest, work_command_id, payload) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",values).await.map_err(sql)?;
        self.voice_project_event_on(c, &call, record, false).await
    }
}
async fn schema(c: &Connection) -> Result<(), AgentFailure> {
    crate::schema::inspect_family(c, crate::schema::Family::Voice)
        .await
        .map_err(crate::schema::SchemaFailure::into_agent)
}
impl<Keys: VaultKeyProvider> VoiceIngressJournal for EncryptedAgentVault<Keys> {
    fn read_event<'a>(
        &'a self,
        actor: &'a OwnerActor,
        voice: Uuid,
        kind: &'a str,
        id: &'a str,
    ) -> BoxFuture<'a, Result<Option<VoiceIngressAdmission>, AgentFailure>> {
        Box::pin(async move {
            self.voice_actor(actor)?;
            query_key(kind, id)?;
            if voice.is_nil() {
                return Err(AgentFailure::InvalidInput);
            }
            let mut c = self.connection()?;
            let tx = c
                .transaction_with_behavior(TransactionBehavior::Deferred)
                .await
                .map_err(sql)?;
            schema(&tx).await?;
            let value = if let Some(call) = self.voice_current_on(&tx, voice).await? {
                if call.intent.binding.actor.device_id != actor.device_id {
                    return Err(AgentFailure::PolicyDenied);
                }
                match self.voice_event_on(&tx, &call, kind, id).await? {
                    Some(record) => Some(
                        self.voice_project_event_on(&tx, &call, record, true)
                            .await?,
                    ),
                    None => None,
                }
            } else {
                None
            };
            tx.commit().await.map_err(sql)?;
            self.check_access()?;
            Ok(value)
        })
    }
    fn ingest<'a>(
        &'a self,
        actor: &'a OwnerActor,
        voice: Uuid,
        event: LiveProviderEvent,
    ) -> BoxFuture<'a, Result<VoiceIngressAdmission, CommandFailure<AgentFailure>>> {
        Box::pin(async move {
            self.voice_actor(actor)
                .map_err(CommandFailure::NotAdmitted)?;
            let kind = event_kind(&event).map_err(CommandFailure::Indeterminate)?;
            let id = event
                .durable_key()
                .ok_or(CommandFailure::Indeterminate(AgentFailure::InvalidInput))?
                .id
                .to_owned();
            let mut c = self.connection().map_err(CommandFailure::Indeterminate)?;
            let read = c
                .transaction_with_behavior(TransactionBehavior::Deferred)
                .await
                .map_err(|e| CommandFailure::Indeterminate(sql(e)))?;
            schema(&read).await.map_err(CommandFailure::Indeterminate)?;
            let call = self
                .voice_current_on(&read, voice)
                .await
                .map_err(CommandFailure::Indeterminate)?
                .ok_or(CommandFailure::Indeterminate(AgentFailure::NotFound))?;
            if &call.intent.binding.actor != actor {
                return Err(CommandFailure::Indeterminate(AgentFailure::PolicyDenied));
            }
            if let Some(record) = self
                .voice_event_on(&read, &call, kind, &id)
                .await
                .map_err(CommandFailure::Indeterminate)?
            {
                if record.event != event {
                    return Err(CommandFailure::Indeterminate(AgentFailure::Conflict));
                }
                let value = self
                    .voice_project_event_on(&read, &call, record, true)
                    .await
                    .map_err(CommandFailure::Indeterminate)?;
                read.commit()
                    .await
                    .map_err(|e| CommandFailure::Indeterminate(sql(e)))?;
                self.check_access().map_err(CommandFailure::Admitted)?;
                return Ok(value);
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
                self.voice_ingest_on(&tx, actor, voice, event).await
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
