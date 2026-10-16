//! Encrypted Surface custody. Read-only replay precedes writer reservation;
//! fresh admission rechecks all identity/CAS/uniqueness inside the transaction.
use super::*;
use floe_agent_contract::BoxFuture;
use floe_conversation::{
    AppendSurfaceInput, MAX_PENDING_SURFACE_WORK, MAX_SURFACE_PAGE_ITEMS, SubmitSurfaceWork,
    SurfaceAnchor, SurfaceEntryReceipt, SurfaceInputOrigin, SurfaceInputReceipt, SurfaceRepository,
    SurfaceResultReceipt, SurfaceResultReference, SurfaceWorkReceipt, WorkOrigin,
};
use floe_kernel::{CommandFailure, OwnerActor};
use serde::{Serialize, de::DeserializeOwned};
use turso::{Connection, transaction::TransactionBehavior};

const MAX_RECORD_BYTES: usize = 32768;
fn encode(value: &impl Serialize) -> Result<String, AgentFailure> {
    let json = serde_json::to_string(value).map_err(|_| AgentFailure::InvalidInput)?;
    if json.len() > MAX_RECORD_BYTES {
        return Err(AgentFailure::BudgetExceeded);
    }
    Ok(json)
}
fn decode<T: DeserializeOwned>(value: String) -> Result<T, AgentFailure> {
    if value.len() > MAX_RECORD_BYTES {
        return Err(AgentFailure::StorageUnavailable);
    }
    serde_json::from_str(&value).map_err(|_| AgentFailure::StorageUnavailable)
}
fn input_origin(receipt: &SurfaceInputReceipt) -> String {
    match &receipt.input.origin {
        SurfaceInputOrigin::Text => format!("text:{}", receipt.input.message_id.as_uuid()),
        SurfaceInputOrigin::Voice {
            voice_session_id,
            utterance_id,
        } => format!("voice:{voice_session_id}:{utterance_id}"),
    }
}
fn work_origin(receipt: &SurfaceWorkReceipt) -> String {
    match &receipt.origin {
        WorkOrigin::Text => format!("text:{}", receipt.input_id.as_uuid()),
        WorkOrigin::Voice {
            voice_session_id,
            delegation_id,
        } => format!("voice:{voice_session_id}:{delegation_id}"),
    }
}

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub(super) async fn surface_session_on(
        &self,
        connection: &Connection,
        actor: &OwnerActor,
        session_id: Uuid,
    ) -> Result<(), AgentFailure> {
        self.check_access()?;
        crate::schema::inspect_family(connection, crate::schema::Family::Conversation)
            .await
            .map_err(crate::schema::SchemaFailure::into_agent)?;
        actor.validate()?;
        if actor.person_id != self.person_id || session_id.is_nil() {
            return Err(AgentFailure::PolicyDenied);
        }
        let session = self.session_on(connection, session_id).await?;
        if session.person_id != self.person_id
            || session.scope.is_some()
            || session.data_classes != [floe_agent_contract::DataClass::Personal]
        {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(())
    }
    /// Text availability is committed with the canonical completed Run. Audio
    /// delivery is separate evidence and is never inferred from this entry.
    pub(super) async fn publish_completed_surface_result_on(
        &self,
        connection: &Connection,
        run: &floe_conversation::RunRecord,
    ) -> Result<(), AgentFailure> {
        let Some(work) = self.surface_work_for_run_on(connection, run.run_id).await? else {
            return Ok(());
        };
        if work.person_id != run.person_id || work.anchor.session_id != run.session_id {
            return Err(AgentFailure::StorageUnavailable);
        }
        let reference = SurfaceResultReference::for_run(run)?;
        if let Some(existing) = self
            .surface_entry_command_on(connection, reference.command_id())
            .await?
        {
            return match existing {
                SurfaceEntryReceipt::PublishedResult(receipt)
                    if receipt.result == reference
                        && receipt.through.session_id == run.session_id =>
                {
                    Ok(())
                }
                _ => Err(AgentFailure::StorageUnavailable),
            };
        }
        self.surface_command_free_on(connection, reference.command_id())
            .await?;
        let head = self.surface_head_on(connection, run.session_id).await?;
        self.surface_anchor_on(connection, head).await?;
        let actor = OwnerActor {
            person_id: run.person_id,
            device_id: run.device_id.clone(),
            runtime_epoch: 1,
        };
        let receipt = SurfaceResultReceipt::admit(&actor, head, reference)?;
        let payload = encode(&SurfaceEntryReceipt::PublishedResult(receipt.clone()))?;
        connection.execute("INSERT INTO agent_surface_entries (command_id, session_id, person_id, sequence, message_id, origin_key, kind, payload) VALUES (?, ?, ?, ?, ?, ?, 'published_result', ?)", (receipt.result.command_id().as_uuid().to_string(), run.session_id.to_string(), self.person_id.to_string(), receipt.through.sequence as i64, receipt.result.message_id().as_uuid().to_string(), format!("result:{}", run.run_id.as_uuid()), payload)).await.map_err(database_failure)?;
        connection.execute("INSERT INTO agent_surface_heads (session_id, person_id, sequence, digest) VALUES (?, ?, ?, ?) ON CONFLICT(session_id) DO UPDATE SET sequence = excluded.sequence, digest = excluded.digest", (run.session_id.to_string(), self.person_id.to_string(), receipt.through.sequence as i64, receipt.through.digest.to_vec())).await.map_err(database_failure)?;
        Ok(())
    }
    pub(super) async fn surface_head_on(
        &self,
        connection: &Connection,
        session_id: Uuid,
    ) -> Result<SurfaceAnchor, AgentFailure> {
        let mut rows = connection
            .query(
                "SELECT person_id, sequence, digest FROM agent_surface_heads WHERE session_id = ?",
                [session_id.to_string()],
            )
            .await
            .map_err(database_failure)?;
        let Some(row) = rows.next().await.map_err(database_failure)? else {
            let mut inputs = connection
                .query(
                    "SELECT 1 FROM agent_surface_entries WHERE session_id = ? LIMIT 1",
                    [session_id.to_string()],
                )
                .await
                .map_err(database_failure)?;
            if inputs.next().await.map_err(database_failure)?.is_some() {
                return Err(AgentFailure::StorageUnavailable);
            }
            return SurfaceAnchor::empty(session_id);
        };
        let person: String = row.get(0).map_err(storage)?;
        let sequence: i64 = row.get(1).map_err(storage)?;
        let digest: Vec<u8> = row.get(2).map_err(storage)?;
        if person != self.person_id.to_string() || sequence < 0 {
            return Err(AgentFailure::StorageUnavailable);
        }
        let head = SurfaceAnchor {
            session_id,
            sequence: sequence as u64,
            digest: digest
                .try_into()
                .map_err(|_| AgentFailure::StorageUnavailable)?,
        };
        head.validate()?;
        Ok(head)
    }
    pub(super) async fn surface_entry_command_on(
        &self,
        connection: &Connection,
        command: floe_kernel::CommandId,
    ) -> Result<Option<SurfaceEntryReceipt>, AgentFailure> {
        let mut rows = connection.query("SELECT session_id, person_id, sequence, message_id, origin_key, payload, kind FROM agent_surface_entries WHERE command_id = ?", [command.as_uuid().to_string()]).await.map_err(database_failure)?;
        let Some(row) = rows.next().await.map_err(database_failure)? else {
            return Ok(None);
        };
        let receipt: SurfaceEntryReceipt = decode(row.get(5).map_err(storage)?)?;
        receipt.validate()?;
        let (kind, origin) = match &receipt {
            SurfaceEntryReceipt::Input(input) => ("input", input_origin(input)),
            SurfaceEntryReceipt::PendingWorkCancelled(entry) => {
                let mut indexed = connection.query("SELECT c.work_command_id, c.person_id, w.payload, w.state, w.run_id FROM agent_surface_work_cancellations c JOIN agent_surface_work w ON w.command_id = c.work_command_id WHERE c.command_id = ?", [entry.cancellation.command_id.as_uuid().to_string()]).await.map_err(database_failure)?;
                let row = indexed
                    .next()
                    .await
                    .map_err(database_failure)?
                    .ok_or(AgentFailure::StorageUnavailable)?;
                let target: SurfaceWorkReceipt = decode(row.get(2).map_err(storage)?)?;
                target.validate()?;
                if target.command_id != entry.cancellation.work_command_id
                    || target.work_id != entry.cancellation.work_id
                    || target.person_id != entry.cancellation.person_id
                    || target.anchor.session_id != entry.before.session_id
                    || target.anchor.sequence > entry.before.sequence
                    || row.get::<String>(3).map_err(storage)? != "cancelled"
                    || row.get::<Option<String>>(4).map_err(storage)?.is_some()
                    || row.get::<String>(0).map_err(storage)?
                        != entry.cancellation.work_command_id.as_uuid().to_string()
                    || row.get::<String>(1).map_err(storage)?
                        != entry.cancellation.person_id.to_string()
                {
                    return Err(AgentFailure::StorageUnavailable);
                }
                (
                    "work_cancelled",
                    format!("cancel:{}", entry.cancellation.command_id.as_uuid()),
                )
            }
            SurfaceEntryReceipt::PublishedResult(result) => {
                let run = self
                    .conversation_run_on(connection, result.result.run_id)
                    .await?
                    .ok_or(AgentFailure::StorageUnavailable)?;
                if run.person_id != receipt.person_id()
                    || run.device_id != receipt.device_id()
                    || run.session_id != receipt.through().session_id
                    || SurfaceResultReference::for_run(&run)? != result.result
                {
                    return Err(AgentFailure::StorageUnavailable);
                }
                (
                    "published_result",
                    format!("result:{}", result.result.run_id.as_uuid()),
                )
            }
        };
        if receipt.command_id() != command
            || receipt.person_id() != self.person_id
            || row.get::<String>(0).map_err(storage)? != receipt.through().session_id.to_string()
            || row.get::<String>(1).map_err(storage)? != receipt.person_id().to_string()
            || row.get::<i64>(2).map_err(storage)? != receipt.through().sequence as i64
            || row.get::<String>(3).map_err(storage)? != receipt.message_id().as_uuid().to_string()
            || row.get::<String>(4).map_err(storage)? != origin
            || row.get::<String>(6).map_err(storage)? != kind
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(Some(receipt))
    }
    async fn surface_input_command_on(
        &self,
        connection: &Connection,
        command: floe_kernel::CommandId,
    ) -> Result<Option<SurfaceInputReceipt>, AgentFailure> {
        match self.surface_entry_command_on(connection, command).await? {
            Some(SurfaceEntryReceipt::Input(input)) => Ok(Some(input)),
            Some(_) => Err(AgentFailure::Conflict),
            None => Ok(None),
        }
    }
    pub(super) async fn surface_anchor_on(
        &self,
        connection: &Connection,
        anchor: SurfaceAnchor,
    ) -> Result<(), AgentFailure> {
        anchor.validate()?;
        if anchor.sequence == 0 {
            return Ok(());
        }
        let mut rows = connection
            .query(
                "SELECT command_id FROM agent_surface_entries WHERE session_id = ? AND sequence = ?",
                (anchor.session_id.to_string(), anchor.sequence as i64),
            )
            .await
            .map_err(database_failure)?;
        let row = rows
            .next()
            .await
            .map_err(database_failure)?
            .ok_or(AgentFailure::Conflict)?;
        let command = Uuid::parse_str(&row.get::<String>(0).map_err(storage)?)
            .ok()
            .and_then(floe_kernel::CommandId::from_uuid)
            .ok_or(AgentFailure::StorageUnavailable)?;
        let receipt = self
            .surface_entry_command_on(connection, command)
            .await?
            .ok_or(AgentFailure::StorageUnavailable)?;
        if receipt.through() != anchor {
            return Err(AgentFailure::Conflict);
        }
        Ok(())
    }
    pub(super) async fn surface_work_command_on(
        &self,
        connection: &Connection,
        command: floe_kernel::CommandId,
    ) -> Result<Option<SurfaceWorkReceipt>, AgentFailure> {
        let mut rows = connection.query("SELECT work_id, session_id, person_id, origin_key, payload, state, run_id, queue_sequence FROM agent_surface_work WHERE command_id = ?", [command.as_uuid().to_string()]).await.map_err(database_failure)?;
        let Some(row) = rows.next().await.map_err(database_failure)? else {
            return Ok(None);
        };
        let receipt: SurfaceWorkReceipt = decode(row.get(4).map_err(storage)?)?;
        receipt.validate()?;
        if receipt.command_id != command
            || receipt.person_id != self.person_id
            || row.get::<String>(0).map_err(storage)? != receipt.work_id.as_uuid().to_string()
            || row.get::<String>(1).map_err(storage)? != receipt.anchor.session_id.to_string()
            || row.get::<String>(2).map_err(storage)? != receipt.person_id.to_string()
            || row.get::<String>(3).map_err(storage)? != work_origin(&receipt)
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        let state: String = row.get(5).map_err(storage)?;
        let linked: Option<String> = row.get(6).map_err(storage)?;
        if row.get::<i64>(7).map_err(storage)? <= 0 {
            return Err(AgentFailure::StorageUnavailable);
        }
        match (state.as_str(), linked) {
            ("queued", None) => {}
            ("cancelled", None) => {
                let mut proofs = connection.query("SELECT command_id FROM agent_surface_work_cancellations WHERE work_command_id = ? ORDER BY command_id LIMIT 1", [command.as_uuid().to_string()]).await.map_err(database_failure)?;
                let row = proofs
                    .next()
                    .await
                    .map_err(database_failure)?
                    .ok_or(AgentFailure::StorageUnavailable)?;
                let proof_id = Uuid::parse_str(&row.get::<String>(0).map_err(storage)?)
                    .ok()
                    .and_then(floe_kernel::CommandId::from_uuid)
                    .ok_or(AgentFailure::StorageUnavailable)?;
                let proof = self
                    .pending_work_cancellation_on(connection, proof_id)
                    .await?
                    .ok_or(AgentFailure::StorageUnavailable)?;
                if proof.work_command_id != command || proof.work_id != receipt.work_id {
                    return Err(AgentFailure::StorageUnavailable);
                }
            }
            ("linked", Some(value)) => {
                let id = Uuid::parse_str(&value)
                    .ok()
                    .and_then(floe_kernel::RunId::from_uuid)
                    .ok_or(AgentFailure::StorageUnavailable)?;
                let run = self
                    .conversation_run_on(connection, id)
                    .await?
                    .ok_or(AgentFailure::StorageUnavailable)?;
                if run.command_id != receipt.execution_command_id()
                    || run.person_id != receipt.person_id
                    || run.session_id != receipt.anchor.session_id
                    || run.device_id != receipt.device_id
                {
                    return Err(AgentFailure::StorageUnavailable);
                }
            }
            _ => return Err(AgentFailure::StorageUnavailable),
        }
        Ok(Some(receipt))
    }
    pub(super) async fn surface_command_free_on(
        &self,
        connection: &Connection,
        command: floe_kernel::CommandId,
    ) -> Result<(), AgentFailure> {
        // Surface command IDs share the product command namespace, including
        // cancelled work/input identities. A Run's own command is separately
        // derived by the queue claim, never this admission ID.
        if super::conversations::command_occupant(connection, command.as_uuid())
            .await?
            .is_some()
        {
            return Err(AgentFailure::Conflict);
        }
        Ok(())
    }
}

impl<Keys: VaultKeyProvider + 'static> SurfaceRepository for EncryptedAgentVault<Keys> {
    fn cancel_pending_work<'a>(
        &'a self,
        actor: &'a OwnerActor,
        request: floe_conversation::CancelPendingWork,
    ) -> BoxFuture<
        'a,
        Result<floe_conversation::PendingWorkCancellationReceipt, CommandFailure<AgentFailure>>,
    > {
        Box::pin(async move { self.cancel_pending_surface_work(actor, request).await })
    }

    fn queued_surface_sessions<'a>(
        &'a self,
        actor: &'a OwnerActor,
        after: Option<Uuid>,
        limit: usize,
    ) -> BoxFuture<'a, Result<floe_conversation::RecoveryPage<Uuid, Uuid>, AgentFailure>> {
        Box::pin(async move {
            actor.validate()?;
            if actor.person_id != self.person_id || limit == 0 || limit > 128 {
                return Err(AgentFailure::InvalidInput);
            }
            let connection = self.connection()?;
            let mut rows = connection.query("SELECT DISTINCT session_id FROM agent_surface_work WHERE state = 'queued' AND person_id = ? AND session_id > ? ORDER BY session_id LIMIT ?", (self.person_id.to_string(), after.map(|id| id.to_string()).unwrap_or_default(), (limit + 1) as i64)).await.map_err(database_failure)?;
            let mut items = Vec::new();
            while let Some(row) = rows.next().await.map_err(database_failure)? {
                let value: String = row.get(0).map_err(storage)?;
                let id = Uuid::parse_str(&value).map_err(storage)?;
                if id.is_nil() {
                    return Err(AgentFailure::StorageUnavailable);
                }
                items.push(id);
            }
            let next_cursor = if items.len() > limit {
                items.pop();
                items.last().copied()
            } else {
                None
            };
            self.check_access()?;
            Ok(floe_conversation::RecoveryPage { items, next_cursor })
        })
    }
    fn surface_work_input<'a>(
        &'a self,
        actor: &'a OwnerActor,
        work: &'a SurfaceWorkReceipt,
    ) -> BoxFuture<'a, Result<SurfaceInputReceipt, AgentFailure>> {
        Box::pin(async move {
            let mut connection = self.connection()?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Deferred)
                .await
                .map_err(database_failure)?;
            self.surface_session_on(&tx, actor, work.anchor.session_id)
                .await?;
            let stored = self
                .surface_work_command_on(&tx, work.command_id)
                .await?
                .ok_or(AgentFailure::NotFound)?;
            if stored != *work {
                return Err(AgentFailure::Conflict);
            }
            self.surface_anchor_on(&tx, work.anchor).await?;
            let mut rows = tx.query("SELECT command_id FROM agent_surface_entries WHERE kind = 'input' AND session_id = ? AND message_id = ? AND sequence <= ?", (work.anchor.session_id.to_string(), work.input_id.as_uuid().to_string(), work.anchor.sequence as i64)).await.map_err(database_failure)?;
            let row = rows
                .next()
                .await
                .map_err(database_failure)?
                .ok_or(AgentFailure::StorageUnavailable)?;
            let command = Uuid::parse_str(&row.get::<String>(0).map_err(storage)?)
                .ok()
                .and_then(floe_kernel::CommandId::from_uuid)
                .ok_or(AgentFailure::StorageUnavailable)?;
            let input = self
                .surface_input_command_on(&tx, command)
                .await?
                .ok_or(AgentFailure::StorageUnavailable)?;
            tx.commit().await.map_err(database_failure)?;
            self.check_access()?;
            Ok(input)
        })
    }
    fn submit_text_work<'a>(
        &'a self,
        actor: &'a OwnerActor,
        request: floe_conversation::SubmitTextWork,
    ) -> BoxFuture<'a, Result<SurfaceWorkReceipt, CommandFailure<AgentFailure>>> {
        Box::pin(async move {
            let (input, expected) = request
                .prepare(actor)
                .map_err(CommandFailure::NotAdmitted)?;
            let mut connection = self.connection().map_err(CommandFailure::Indeterminate)?;
            let read = connection
                .transaction_with_behavior(TransactionBehavior::Deferred)
                .await
                .map_err(|e| CommandFailure::Indeterminate(database_failure(e)))?;
            self.surface_session_on(&read, actor, expected.anchor.session_id)
                .await
                .map_err(CommandFailure::NotAdmitted)?;
            if let Some(existing) = self
                .surface_work_command_on(&read, expected.command_id)
                .await
                .map_err(CommandFailure::Indeterminate)?
            {
                let retained = self
                    .surface_input_command_on(&read, input.command_id)
                    .await
                    .map_err(CommandFailure::Indeterminate)?;
                if existing != expected || retained.as_ref() != Some(&input) {
                    return Err(CommandFailure::Indeterminate(AgentFailure::Conflict));
                }
                read.commit()
                    .await
                    .map_err(|e| CommandFailure::Indeterminate(database_failure(e)))?;
                self.check_access().map_err(CommandFailure::Indeterminate)?;
                return Ok(existing);
            }
            read.commit()
                .await
                .map_err(|e| CommandFailure::Indeterminate(database_failure(e)))?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .await
                .map_err(|e| CommandFailure::Indeterminate(database_failure(e)))?;
            let result = async {
                self.append_surface_input_on(&tx, actor, input).await?;
                self.submit_surface_work_on(&tx, actor, expected).await
            }
            .await;
            self.finish_registry_command_transaction(tx, result).await
        })
    }
    fn read_surface_work<'a>(
        &'a self,
        actor: &'a OwnerActor,
        command_id: floe_kernel::CommandId,
    ) -> BoxFuture<'a, Result<Option<floe_conversation::SurfaceWorkSnapshot>, AgentFailure>> {
        Box::pin(async move {
            actor.validate()?;
            if actor.person_id != self.person_id {
                return Err(AgentFailure::PolicyDenied);
            }
            let mut connection = self.connection()?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Deferred)
                .await
                .map_err(database_failure)?;
            let Some(receipt) = self.surface_work_command_on(&tx, command_id).await? else {
                tx.commit().await.map_err(database_failure)?;
                self.check_access()?;
                return Ok(None);
            };
            self.surface_session_on(&tx, actor, receipt.anchor.session_id)
                .await?;
            let mut rows = tx
                .query(
                    "SELECT state, run_id FROM agent_surface_work WHERE command_id = ?",
                    [command_id.as_uuid().to_string()],
                )
                .await
                .map_err(database_failure)?;
            let row = rows
                .next()
                .await
                .map_err(database_failure)?
                .ok_or(AgentFailure::StorageUnavailable)?;
            let state: String = row.get(0).map_err(storage)?;
            let state = match state.as_str() {
                "queued" => floe_conversation::SurfaceWorkState::Queued,
                "cancelled" => floe_conversation::SurfaceWorkState::Cancelled,
                "linked" => {
                    let id: String = row.get(1).map_err(storage)?;
                    let id = Uuid::parse_str(&id)
                        .ok()
                        .and_then(floe_kernel::RunId::from_uuid)
                        .ok_or(AgentFailure::StorageUnavailable)?;
                    floe_conversation::SurfaceWorkState::Linked(id)
                }
                _ => return Err(AgentFailure::StorageUnavailable),
            };
            tx.commit().await.map_err(database_failure)?;
            self.check_access()?;
            Ok(Some(floe_conversation::SurfaceWorkSnapshot {
                receipt,
                state,
            }))
        })
    }
    fn pending_surface_work<'a>(
        &'a self,
        actor: &'a OwnerActor,
        session_id: Uuid,
    ) -> BoxFuture<'a, Result<Vec<SurfaceWorkReceipt>, AgentFailure>> {
        Box::pin(async move {
            let mut connection = self.connection()?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Deferred)
                .await
                .map_err(database_failure)?;
            self.surface_session_on(&tx, actor, session_id).await?;
            let mut rows = tx.query("SELECT command_id FROM agent_surface_work WHERE session_id = ? AND state = 'queued' ORDER BY queue_sequence LIMIT ?", (session_id.to_string(), (MAX_PENDING_SURFACE_WORK + 1) as i64)).await.map_err(database_failure)?;
            let mut result = Vec::new();
            while let Some(row) = rows.next().await.map_err(database_failure)? {
                let id: String = row.get(0).map_err(storage)?;
                let id = Uuid::parse_str(&id)
                    .ok()
                    .and_then(floe_kernel::CommandId::from_uuid)
                    .ok_or(AgentFailure::StorageUnavailable)?;
                result.push(
                    self.surface_work_command_on(&tx, id)
                        .await?
                        .ok_or(AgentFailure::StorageUnavailable)?,
                );
                if result.len() > MAX_PENDING_SURFACE_WORK {
                    return Err(AgentFailure::StorageUnavailable);
                }
            }
            tx.commit().await.map_err(database_failure)?;
            self.check_access()?;
            Ok(result)
        })
    }
    fn surface_head<'a>(
        &'a self,
        actor: &'a OwnerActor,
        session_id: Uuid,
    ) -> BoxFuture<'a, Result<SurfaceAnchor, AgentFailure>> {
        Box::pin(async move {
            let mut connection = self.connection()?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Deferred)
                .await
                .map_err(database_failure)?;
            self.surface_session_on(&tx, actor, session_id).await?;
            let head = self.surface_head_on(&tx, session_id).await?;
            self.surface_anchor_on(&tx, head).await?;
            tx.commit().await.map_err(database_failure)?;
            self.check_access()?;
            Ok(head)
        })
    }
    fn append_surface_input<'a>(
        &'a self,
        actor: &'a OwnerActor,
        request: AppendSurfaceInput,
    ) -> BoxFuture<'a, Result<SurfaceInputReceipt, CommandFailure<AgentFailure>>> {
        Box::pin(async move {
            let expected = request.admit(actor).map_err(CommandFailure::NotAdmitted)?;
            let mut connection = self.connection().map_err(CommandFailure::Indeterminate)?;
            let read = connection
                .transaction_with_behavior(TransactionBehavior::Deferred)
                .await
                .map_err(|e| CommandFailure::Indeterminate(database_failure(e)))?;
            self.surface_session_on(&read, actor, expected.through.session_id)
                .await
                .map_err(CommandFailure::NotAdmitted)?;
            if let Some(existing) = self
                .surface_input_command_on(&read, expected.command_id)
                .await
                .map_err(CommandFailure::Indeterminate)?
            {
                if existing != expected {
                    return Err(CommandFailure::Indeterminate(AgentFailure::Conflict));
                }
                read.commit()
                    .await
                    .map_err(|e| CommandFailure::Indeterminate(database_failure(e)))?;
                self.check_access().map_err(CommandFailure::Indeterminate)?;
                return Ok(existing);
            }
            read.commit()
                .await
                .map_err(|e| CommandFailure::Indeterminate(database_failure(e)))?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .await
                .map_err(|e| CommandFailure::Indeterminate(database_failure(e)))?;
            let result = self.append_surface_input_on(&tx, actor, expected).await;
            self.finish_registry_command_transaction(tx, result).await
        })
    }
    fn submit_surface_work<'a>(
        &'a self,
        actor: &'a OwnerActor,
        request: SubmitSurfaceWork,
    ) -> BoxFuture<'a, Result<SurfaceWorkReceipt, CommandFailure<AgentFailure>>> {
        Box::pin(async move {
            let expected = request.admit(actor).map_err(CommandFailure::NotAdmitted)?;
            let mut connection = self.connection().map_err(CommandFailure::Indeterminate)?;
            let read = connection
                .transaction_with_behavior(TransactionBehavior::Deferred)
                .await
                .map_err(|e| CommandFailure::Indeterminate(database_failure(e)))?;
            self.surface_session_on(&read, actor, expected.anchor.session_id)
                .await
                .map_err(CommandFailure::NotAdmitted)?;
            if let Some(existing) = self
                .surface_work_command_on(&read, expected.command_id)
                .await
                .map_err(CommandFailure::Indeterminate)?
            {
                if existing != expected {
                    return Err(CommandFailure::Indeterminate(AgentFailure::Conflict));
                }
                read.commit()
                    .await
                    .map_err(|e| CommandFailure::Indeterminate(database_failure(e)))?;
                self.check_access().map_err(CommandFailure::Indeterminate)?;
                return Ok(existing);
            }
            read.commit()
                .await
                .map_err(|e| CommandFailure::Indeterminate(database_failure(e)))?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .await
                .map_err(|e| CommandFailure::Indeterminate(database_failure(e)))?;
            let result = self.submit_surface_work_on(&tx, actor, expected).await;
            self.finish_registry_command_transaction(tx, result).await
        })
    }
    fn surface_entries<'a>(
        &'a self,
        actor: &'a OwnerActor,
        through: SurfaceAnchor,
        after: u64,
        limit: usize,
    ) -> BoxFuture<'a, Result<Vec<SurfaceEntryReceipt>, AgentFailure>> {
        Box::pin(async move {
            if limit == 0 || limit > MAX_SURFACE_PAGE_ITEMS || after > through.sequence {
                return Err(AgentFailure::InvalidInput);
            }
            let mut connection = self.connection()?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Deferred)
                .await
                .map_err(database_failure)?;
            self.surface_session_on(&tx, actor, through.session_id)
                .await?;
            self.surface_anchor_on(&tx, through).await?;
            let mut rows = tx.query("SELECT command_id FROM agent_surface_entries WHERE session_id = ? AND sequence > ? AND sequence <= ? ORDER BY sequence LIMIT ?", (through.session_id.to_string(), after as i64, through.sequence as i64, limit as i64)).await.map_err(database_failure)?;
            let mut result = Vec::new();
            while let Some(row) = rows.next().await.map_err(database_failure)? {
                let command = Uuid::parse_str(&row.get::<String>(0).map_err(storage)?)
                    .ok()
                    .and_then(floe_kernel::CommandId::from_uuid)
                    .ok_or(AgentFailure::StorageUnavailable)?;
                let receipt = self
                    .surface_entry_command_on(&tx, command)
                    .await?
                    .ok_or(AgentFailure::StorageUnavailable)?;
                if receipt.through().sequence != after + result.len() as u64 + 1 {
                    return Err(AgentFailure::StorageUnavailable);
                }
                if let Some(previous) = result.last() {
                    let previous: &SurfaceEntryReceipt = previous;
                    if receipt.before() != previous.through() {
                        return Err(AgentFailure::StorageUnavailable);
                    }
                } else {
                    self.surface_anchor_on(&tx, receipt.before()).await?;
                }
                result.push(receipt);
            }
            if result.len() != limit.min((through.sequence - after) as usize) {
                return Err(AgentFailure::StorageUnavailable);
            }
            tx.commit().await.map_err(database_failure)?;
            self.check_access()?;
            Ok(result)
        })
    }
}

#[cfg(test)]
mod tests;

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    /// Startup validation scans bounded keyset pages without imposing a lifetime
    /// cap. Missing/corrupt prefixes fail closed; they are never reconstructed.
    pub(super) async fn validate_surface_store(&self) -> Result<(), AgentFailure> {
        let mut connection = self.connection()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(database_failure)?;
        let mut cursor = String::new();
        loop {
            let mut rows = tx.query("SELECT session_id, person_id FROM agent_surface_heads WHERE session_id > ? ORDER BY session_id LIMIT 128", [cursor.clone()]).await.map_err(database_failure)?;
            let mut count = 0;
            while let Some(row) = rows.next().await.map_err(database_failure)? {
                cursor = row.get(0).map_err(storage)?;
                count += 1;
                let session = Uuid::parse_str(&cursor).map_err(storage)?;
                if row.get::<String>(1).map_err(storage)? != self.person_id.to_string() {
                    return Err(AgentFailure::StorageUnavailable);
                }
                let head = self.surface_head_on(&tx, session).await?;
                if head.sequence == 0 {
                    return Err(AgentFailure::StorageUnavailable);
                }
                self.surface_anchor_on(&tx, head).await?;
                let mut totals = tx
                    .query(
                        "SELECT count(*) FROM agent_surface_entries WHERE session_id = ?",
                        [cursor.clone()],
                    )
                    .await
                    .map_err(database_failure)?;
                let n: i64 = totals
                    .next()
                    .await
                    .map_err(database_failure)?
                    .ok_or(AgentFailure::StorageUnavailable)?
                    .get(0)
                    .map_err(storage)?;
                if n != head.sequence as i64 {
                    return Err(AgentFailure::StorageUnavailable);
                }
            }
            if count < 128 {
                break;
            }
        }
        for work in [false, true] {
            cursor.clear();
            loop {
                let query = if work {
                    "SELECT command_id FROM agent_surface_work WHERE command_id > ? ORDER BY command_id LIMIT 128"
                } else {
                    "SELECT command_id FROM agent_surface_entries WHERE command_id > ? ORDER BY command_id LIMIT 128"
                };
                let mut rows = tx
                    .query(query, [cursor.clone()])
                    .await
                    .map_err(database_failure)?;
                let mut count = 0;
                while let Some(row) = rows.next().await.map_err(database_failure)? {
                    cursor = row.get(0).map_err(storage)?;
                    count += 1;
                    let command = Uuid::parse_str(&cursor)
                        .ok()
                        .and_then(floe_kernel::CommandId::from_uuid)
                        .ok_or(AgentFailure::StorageUnavailable)?;
                    let (session, device) = if work {
                        let receipt = self
                            .surface_work_command_on(&tx, command)
                            .await?
                            .ok_or(AgentFailure::StorageUnavailable)?;
                        self.surface_anchor_on(&tx, receipt.anchor).await?;
                        let mut inputs = tx.query("SELECT 1 FROM agent_surface_entries WHERE kind = 'input' AND session_id = ? AND message_id = ? AND sequence <= ?", (receipt.anchor.session_id.to_string(), receipt.input_id.as_uuid().to_string(), receipt.anchor.sequence as i64)).await.map_err(database_failure)?;
                        if inputs.next().await.map_err(database_failure)?.is_none() {
                            return Err(AgentFailure::StorageUnavailable);
                        }
                        (receipt.anchor.session_id, receipt.device_id)
                    } else {
                        let receipt = self
                            .surface_entry_command_on(&tx, command)
                            .await?
                            .ok_or(AgentFailure::StorageUnavailable)?;
                        self.surface_anchor_on(&tx, receipt.before()).await?;
                        let head = self
                            .surface_head_on(&tx, receipt.through().session_id)
                            .await?;
                        if head.sequence < receipt.through().sequence {
                            return Err(AgentFailure::StorageUnavailable);
                        }
                        (receipt.through().session_id, receipt.device_id().to_owned())
                    };
                    let actor = OwnerActor {
                        person_id: self.person_id,
                        device_id: device,
                        runtime_epoch: 1,
                    };
                    self.surface_session_on(&tx, &actor, session).await?;
                    let expected = if work {
                        floe_conversation::ConversationCommandKind::SurfaceWork
                    } else {
                        floe_conversation::ConversationCommandKind::SurfaceEntry
                    };
                    if super::conversations::command_occupant(&tx, command.as_uuid()).await?
                        != Some(expected)
                    {
                        return Err(AgentFailure::StorageUnavailable);
                    }
                }
                if count < 128 {
                    break;
                }
            }
        }
        tx.commit().await.map_err(database_failure)?;
        self.check_access()
    }
}

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    /// Validate a queue claim inside the *same* transaction as Run/Core
    /// admission. A caller never marks work linked before the Run exists.
    pub(super) async fn verify_surface_claim_on(
        &self,
        connection: &Connection,
        work: &SurfaceWorkReceipt,
        request: &floe_conversation::TurnAdmissionRequest,
    ) -> Result<(), AgentFailure> {
        work.validate()?;
        let stored = self
            .surface_work_command_on(connection, work.command_id)
            .await?
            .ok_or(AgentFailure::NotFound)?;
        if stored != *work
            || request.command_id != work.execution_command_id()
            || request.session_id != work.anchor.session_id
            || request.principal != work.person_id.to_string()
            || request.device_id != work.device_id
            || request.mode != floe_conversation::TurnMode::New
            || request.retry_of.is_some()
        {
            return Err(AgentFailure::Conflict);
        }
        self.surface_anchor_on(connection, work.anchor).await?;
        let mut rows = connection.query("SELECT command_id FROM agent_surface_entries WHERE kind = 'input' AND session_id = ? AND message_id = ? AND sequence <= ?", (work.anchor.session_id.to_string(), work.input_id.as_uuid().to_string(), work.anchor.sequence as i64)).await.map_err(database_failure)?;
        let row = rows
            .next()
            .await
            .map_err(database_failure)?
            .ok_or(AgentFailure::NotFound)?;
        let command = Uuid::parse_str(&row.get::<String>(0).map_err(storage)?)
            .ok()
            .and_then(floe_kernel::CommandId::from_uuid)
            .ok_or(AgentFailure::StorageUnavailable)?;
        let input = self
            .surface_input_command_on(connection, command)
            .await?
            .ok_or(AgentFailure::NotFound)?;
        match &request.input {
            floe_conversation::TurnInput::NewMessage(message)
                if message.text == input.input.text => {}
            _ => return Err(AgentFailure::Conflict),
        }
        let mut rows = connection
            .query(
                "SELECT state, run_id, queue_sequence FROM agent_surface_work WHERE work_id = ?",
                [work.work_id.as_uuid().to_string()],
            )
            .await
            .map_err(database_failure)?;
        let row = rows
            .next()
            .await
            .map_err(database_failure)?
            .ok_or(AgentFailure::StorageUnavailable)?;
        let state: String = row.get(0).map_err(storage)?;
        let run_id: Option<String> = row.get(1).map_err(storage)?;
        let sequence: i64 = row.get(2).map_err(storage)?;
        if state == "linked" {
            if run_id.as_deref() != Some(request.run_id.as_uuid().to_string().as_str()) {
                return Err(AgentFailure::Conflict);
            }
            return Ok(());
        }
        if state != "queued" || run_id.is_some() || sequence <= 0 {
            return Err(AgentFailure::Conflict);
        }
        let mut earlier = connection.query("SELECT 1 FROM agent_surface_work WHERE session_id = ? AND state = 'queued' AND queue_sequence < ? LIMIT 1", (work.anchor.session_id.to_string(), sequence)).await.map_err(database_failure)?;
        if earlier.next().await.map_err(database_failure)?.is_some() {
            return Err(AgentFailure::Conflict);
        }
        Ok(())
    }
    pub(super) async fn link_surface_claim_on(
        &self,
        connection: &Connection,
        work: &SurfaceWorkReceipt,
        run_id: floe_kernel::RunId,
    ) -> Result<(), AgentFailure> {
        let run = self
            .conversation_run_on(connection, run_id)
            .await?
            .ok_or(AgentFailure::StorageUnavailable)?;
        if run.command_id != work.execution_command_id()
            || run.person_id != work.person_id
            || run.session_id != work.anchor.session_id
            || run.device_id != work.device_id
        {
            return Err(AgentFailure::Conflict);
        }
        let changed = connection.execute("UPDATE agent_surface_work SET state = 'linked', run_id = ? WHERE work_id = ? AND state = 'queued' AND run_id IS NULL", (run_id.as_uuid().to_string(), work.work_id.as_uuid().to_string())).await.map_err(database_failure)?;
        if changed == 0 {
            let mut rows = connection.query("SELECT 1 FROM agent_surface_work WHERE work_id = ? AND state = 'linked' AND run_id = ?", (work.work_id.as_uuid().to_string(), run_id.as_uuid().to_string())).await.map_err(database_failure)?;
            if rows.next().await.map_err(database_failure)?.is_none() {
                return Err(AgentFailure::Conflict);
            }
        }
        Ok(())
    }
}

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    async fn append_surface_input_on(
        &self,
        tx: &Connection,
        actor: &OwnerActor,
        expected: SurfaceInputReceipt,
    ) -> Result<SurfaceInputReceipt, CommandFailure<AgentFailure>> {
        self.surface_session_on(tx, actor, expected.through.session_id)
            .await
            .map_err(CommandFailure::NotAdmitted)?;
        if let Some(existing) = self
            .surface_input_command_on(tx, expected.command_id)
            .await
            .map_err(CommandFailure::Indeterminate)?
        {
            return if existing == expected {
                Ok(existing)
            } else {
                Err(CommandFailure::Indeterminate(AgentFailure::Conflict))
            };
        }
        self.surface_command_free_on(tx, expected.command_id)
            .await
            .map_err(CommandFailure::Indeterminate)?;
        let head = self
            .surface_head_on(tx, expected.through.session_id)
            .await
            .map_err(CommandFailure::Indeterminate)?;
        if head != expected.before {
            return Err(CommandFailure::NotApplied(AgentFailure::Conflict));
        }
        self.surface_anchor_on(tx, head)
            .await
            .map_err(CommandFailure::Indeterminate)?;
        let mut occupied = tx.query("SELECT 1 FROM agent_surface_entries WHERE session_id = ? AND (message_id = ? OR origin_key = ?) LIMIT 1", (head.session_id.to_string(), expected.input.message_id.as_uuid().to_string(), input_origin(&expected))).await.map_err(|e| CommandFailure::Indeterminate(database_failure(e)))?;
        if occupied
            .next()
            .await
            .map_err(|e| CommandFailure::Indeterminate(database_failure(e)))?
            .is_some()
        {
            return Err(CommandFailure::NotApplied(AgentFailure::Conflict));
        }
        let payload = encode(&SurfaceEntryReceipt::Input(expected.clone()))
            .map_err(CommandFailure::NotApplied)?;
        tx.execute("INSERT INTO agent_surface_entries (command_id, session_id, person_id, sequence, message_id, origin_key, kind, payload) VALUES (?, ?, ?, ?, ?, ?, 'input', ?)", (expected.command_id.as_uuid().to_string(), head.session_id.to_string(), self.person_id.to_string(), expected.through.sequence as i64, expected.input.message_id.as_uuid().to_string(), input_origin(&expected), payload)).await.map_err(|e| CommandFailure::Indeterminate(database_failure(e)))?;
        tx.execute("INSERT INTO agent_surface_heads (session_id, person_id, sequence, digest) VALUES (?, ?, ?, ?) ON CONFLICT(session_id) DO UPDATE SET sequence = excluded.sequence, digest = excluded.digest", (head.session_id.to_string(), self.person_id.to_string(), expected.through.sequence as i64, expected.through.digest.to_vec())).await.map_err(|e| CommandFailure::Indeterminate(database_failure(e)))?;
        Ok(expected)
    }
    async fn submit_surface_work_on(
        &self,
        tx: &Connection,
        actor: &OwnerActor,
        expected: SurfaceWorkReceipt,
    ) -> Result<SurfaceWorkReceipt, CommandFailure<AgentFailure>> {
        self.surface_session_on(tx, actor, expected.anchor.session_id)
            .await
            .map_err(CommandFailure::NotAdmitted)?;
        if let Some(existing) = self
            .surface_work_command_on(tx, expected.command_id)
            .await
            .map_err(CommandFailure::Indeterminate)?
        {
            return if existing == expected {
                Ok(existing)
            } else {
                Err(CommandFailure::Indeterminate(AgentFailure::Conflict))
            };
        }
        self.surface_command_free_on(tx, expected.command_id)
            .await
            .map_err(CommandFailure::Indeterminate)?;
        self.surface_anchor_on(tx, expected.anchor)
            .await
            .map_err(CommandFailure::NotApplied)?;
        let mut rows = tx.query("SELECT command_id FROM agent_surface_entries WHERE kind = 'input' AND session_id = ? AND message_id = ? AND sequence <= ?", (expected.anchor.session_id.to_string(), expected.input_id.as_uuid().to_string(), expected.anchor.sequence as i64)).await.map_err(|e| CommandFailure::Indeterminate(database_failure(e)))?;
        let row = rows
            .next()
            .await
            .map_err(|e| CommandFailure::Indeterminate(database_failure(e)))?
            .ok_or(CommandFailure::NotApplied(AgentFailure::NotFound))?;
        let command = Uuid::parse_str(
            &row.get::<String>(0)
                .map_err(|_| CommandFailure::Indeterminate(AgentFailure::StorageUnavailable))?,
        )
        .ok()
        .and_then(floe_kernel::CommandId::from_uuid)
        .ok_or(CommandFailure::Indeterminate(
            AgentFailure::StorageUnavailable,
        ))?;
        let input = self
            .surface_input_command_on(tx, command)
            .await
            .map_err(CommandFailure::Indeterminate)?
            .ok_or(CommandFailure::Indeterminate(
                AgentFailure::StorageUnavailable,
            ))?;
        match (&expected.origin, &input.input.origin) {
            (WorkOrigin::Text, SurfaceInputOrigin::Text) => {}
            (
                WorkOrigin::Voice {
                    voice_session_id: a,
                    ..
                },
                SurfaceInputOrigin::Voice {
                    voice_session_id: b,
                    ..
                },
            ) if a == b && input.device_id == expected.device_id => {}
            _ => return Err(CommandFailure::NotApplied(AgentFailure::PolicyDenied)),
        }
        let mut rows = tx
            .query(
                "SELECT count(*) FROM agent_surface_work WHERE session_id = ? AND state = 'queued'",
                [expected.anchor.session_id.to_string()],
            )
            .await
            .map_err(|e| CommandFailure::Indeterminate(database_failure(e)))?;
        let count: i64 = rows
            .next()
            .await
            .map_err(|e| CommandFailure::Indeterminate(database_failure(e)))?
            .ok_or(CommandFailure::Indeterminate(
                AgentFailure::StorageUnavailable,
            ))?
            .get(0)
            .map_err(|_| CommandFailure::Indeterminate(AgentFailure::StorageUnavailable))?;
        if count >= MAX_PENDING_SURFACE_WORK as i64 {
            return Err(CommandFailure::NotApplied(AgentFailure::BudgetExceeded));
        }
        let mut occupied = tx
            .query(
                "SELECT 1 FROM agent_surface_work WHERE session_id = ? AND origin_key = ? LIMIT 1",
                (
                    expected.anchor.session_id.to_string(),
                    work_origin(&expected),
                ),
            )
            .await
            .map_err(|e| CommandFailure::Indeterminate(database_failure(e)))?;
        if occupied
            .next()
            .await
            .map_err(|e| CommandFailure::Indeterminate(database_failure(e)))?
            .is_some()
        {
            return Err(CommandFailure::NotApplied(AgentFailure::Conflict));
        }
        tx.execute("INSERT INTO agent_surface_work (command_id, work_id, session_id, person_id, origin_key, queue_sequence, state, payload) SELECT ?1, ?2, ?3, ?4, ?5, COALESCE(MAX(queue_sequence), 0) + 1, 'queued', ?6 FROM agent_surface_work WHERE session_id = ?3", (expected.command_id.as_uuid().to_string(), expected.work_id.as_uuid().to_string(), expected.anchor.session_id.to_string(), self.person_id.to_string(), work_origin(&expected), encode(&expected).map_err(CommandFailure::NotApplied)?)).await.map_err(|e| CommandFailure::Indeterminate(database_failure(e)))?;
        Ok(expected)
    }
}

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub(crate) async fn surface_work_for_run(
        &self,
        run_id: floe_kernel::RunId,
    ) -> Result<Option<SurfaceWorkReceipt>, AgentFailure> {
        let mut connection = self.connection()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(database_failure)?;
        let found = self.surface_work_for_run_on(&tx, run_id).await?;
        tx.commit().await.map_err(database_failure)?;
        self.check_access()?;
        Ok(found)
    }
    pub(super) async fn surface_work_for_run_on(
        &self,
        connection: &Connection,
        run_id: floe_kernel::RunId,
    ) -> Result<Option<SurfaceWorkReceipt>, AgentFailure> {
        let mut current = run_id;
        let mut seen = std::collections::HashSet::new();
        let mut found = None;
        let mut reached_root = false;
        // Each allowed Resume may carry up to three Continue links.
        let limit = (usize::from(floe_conversation::MAX_RESUME_LINEAGE) + 1) * 4;
        for _ in 0..limit {
            if !seen.insert(current) {
                return Err(AgentFailure::StorageUnavailable);
            }
            let mut rows = connection.query("SELECT command_id FROM agent_surface_work WHERE run_id = ? AND state = 'linked'", [current.as_uuid().to_string()]).await.map_err(database_failure)?;
            if let Some(row) = rows.next().await.map_err(database_failure)? {
                let command: String = row.get(0).map_err(storage)?;
                let command = Uuid::parse_str(&command)
                    .ok()
                    .and_then(floe_kernel::CommandId::from_uuid)
                    .ok_or(AgentFailure::StorageUnavailable)?;
                found = self.surface_work_command_on(connection, command).await?;
                break;
            }
            let run = self
                .conversation_run_on(connection, current)
                .await?
                .ok_or(AgentFailure::NotFound)?;
            if let Some(parent) = run.continuation_of.or(run.resume_of) {
                let source = self
                    .conversation_run_on(connection, parent)
                    .await?
                    .ok_or(AgentFailure::StorageUnavailable)?;
                if source.person_id != run.person_id
                    || source.session_id != run.session_id
                    || source.user_message_id != run.user_message_id
                {
                    return Err(AgentFailure::StorageUnavailable);
                }
                current = parent;
            } else {
                reached_root = true;
                break;
            }
        }
        if !reached_root && found.is_none() {
            return Err(AgentFailure::BudgetExceeded);
        }
        Ok(found)
    }
}
