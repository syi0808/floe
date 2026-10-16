//! Surface prefix selection and checkpoint custody for the existing compaction
//! execution. No provider call, new runtime or Core identity is introduced.
use super::*;
use floe_agent_contract::{
    AgentMessage, ArchiveMessageGroup, ArchivePointer, ArchiveReadRequest, ArchiveSnapshot,
    ArchivedMessage, DependencyCoverage, MessageRole,
};
use floe_conversation::{
    COMPACTION_MIN_RETAIN_ENTRIES, COMPACTION_PREFERRED_RETAIN_ENTRIES,
    COMPACTION_TRIGGER_MESSAGES, CompactionAuthority, CompactionDisposition, CompactionExecution,
    CompactionPrefix, CompactionReceipt, CompactionSource, SurfaceAnchor, SurfaceCheckpointRef,
    SurfaceCompactionPrefix, SurfaceEntryReceipt, SurfaceWorkReceipt,
};
use turso::{
    Connection,
    transaction::{Transaction, TransactionBehavior},
};

pub(super) async fn verify_checkpoint_on(
    connection: &Connection,
    job: &CompactionExecution,
) -> Result<(), AgentFailure> {
    let CompactionPrefix::Surface(prefix) = &job.source.prefix else {
        return Err(AgentFailure::Conflict);
    };
    let CompactionDisposition::Committed { receipt } = &job.disposition else {
        return Err(AgentFailure::Conflict);
    };
    job.validate()?;
    let mut rows = connection.query("SELECT person_id, session_id, through_sequence, through_digest FROM agent_surface_checkpoints WHERE execution_id = ?", [job.claim.execution_id.to_string()]).await.map_err(database_failure)?;
    let row = rows
        .next()
        .await
        .map_err(database_failure)?
        .ok_or(AgentFailure::StorageUnavailable)?;
    if row.get::<String>(0).map_err(storage)? != job.head.person_id.to_string()
        || row.get::<String>(1).map_err(storage)? != job.source.session_id.to_string()
        || row.get::<i64>(2).map_err(storage)? != prefix.through.sequence as i64
        || row.get::<Vec<u8>>(3).map_err(storage)? != prefix.through.digest
        || receipt.pointer.archive_id() != job.claim.execution_id
    {
        return Err(AgentFailure::StorageUnavailable);
    }
    for anchor in [prefix.head, prefix.protected_input, prefix.through] {
        let mut rows = connection.query("SELECT person_id, payload FROM agent_surface_entries WHERE session_id = ? AND sequence = ?", (anchor.session_id.to_string(), anchor.sequence as i64)).await.map_err(database_failure)?;
        let row = rows
            .next()
            .await
            .map_err(database_failure)?
            .ok_or(AgentFailure::StorageUnavailable)?;
        let payload: String = row.get(1).map_err(storage)?;
        if payload.len() > 32768
            || row.get::<String>(0).map_err(storage)? != job.head.person_id.to_string()
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        let entry: SurfaceEntryReceipt = serde_json::from_str(&payload).map_err(storage)?;
        entry.validate()?;
        if entry.through() != anchor || entry.person_id() != job.head.person_id {
            return Err(AgentFailure::StorageUnavailable);
        }
    }
    Ok(())
}

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub(super) async fn validate_surface_checkpoint_store(&self) -> Result<(), AgentFailure> {
        let mut connection = self.connection()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(database_failure)?;
        let mut cursor = String::new();
        loop {
            let mut rows = tx.query("SELECT execution_id FROM agent_surface_checkpoints WHERE execution_id > ? ORDER BY execution_id LIMIT 128", [cursor.clone()]).await.map_err(database_failure)?;
            let mut count = 0;
            while let Some(row) = rows.next().await.map_err(database_failure)? {
                cursor = row.get(0).map_err(storage)?;
                count += 1;
                let id = Uuid::parse_str(&cursor).map_err(storage)?;
                let job = super::session_compaction_execution::read_on(&tx, self.person_id, id)
                    .await?
                    .ok_or(AgentFailure::StorageUnavailable)?;
                verify_checkpoint_on(&tx, &job).await?;
            }
            if count < 128 {
                break;
            }
        }
        tx.commit().await.map_err(database_failure)?;
        self.check_access()
    }
    async fn surface_entry_at_on(
        &self,
        connection: &Connection,
        session: Uuid,
        sequence: u64,
    ) -> Result<SurfaceEntryReceipt, AgentFailure> {
        let mut rows = connection.query("SELECT command_id FROM agent_surface_entries WHERE session_id = ? AND sequence = ?", (session.to_string(), sequence as i64)).await.map_err(database_failure)?;
        let row = rows
            .next()
            .await
            .map_err(database_failure)?
            .ok_or(AgentFailure::StorageUnavailable)?;
        let id = Uuid::parse_str(&row.get::<String>(0).map_err(storage)?)
            .ok()
            .and_then(floe_kernel::CommandId::from_uuid)
            .ok_or(AgentFailure::StorageUnavailable)?;
        self.surface_entry_command_on(connection, id)
            .await?
            .ok_or(AgentFailure::StorageUnavailable)
    }
    async fn surface_message_on(
        &self,
        connection: &Connection,
        entry: &SurfaceEntryReceipt,
    ) -> Result<AgentMessage, AgentFailure> {
        entry.validate()?;
        let message = match entry {
            SurfaceEntryReceipt::Input(input) => AgentMessage {
                message_id: input.input.message_id.as_uuid(),
                role: MessageRole::User,
                text: input.input.text.clone(),
                call_id: None,
                coverage: DependencyCoverage::Independent,
            },
            SurfaceEntryReceipt::PublishedResult(result) => {
                let run = self
                    .conversation_run_on(connection, result.result.run_id)
                    .await?
                    .ok_or(AgentFailure::StorageUnavailable)?;
                if floe_conversation::SurfaceResultReference::for_run(&run)? != result.result
                    || run.person_id != entry.person_id()
                    || run.session_id != entry.through().session_id
                {
                    return Err(AgentFailure::StorageUnavailable);
                }
                AgentMessage {
                    message_id: run.run_id.as_uuid(),
                    role: MessageRole::Assistant,
                    text: run.output.ok_or(AgentFailure::StorageUnavailable)?,
                    call_id: None,
                    coverage: run.coverage,
                }
            }
            SurfaceEntryReceipt::PendingWorkCancelled(item) => {
                let target = self
                    .surface_work_command_on(connection, item.cancellation.work_command_id)
                    .await?
                    .ok_or(AgentFailure::StorageUnavailable)?;
                if target.work_id != item.cancellation.work_id
                    || target.anchor.session_id != entry.through().session_id
                {
                    return Err(AgentFailure::StorageUnavailable);
                }
                AgentMessage { message_id: item.message_id().as_uuid(), role: MessageRole::User, text: serde_json::json!({"person_intent":"cancel_pending_work", "input_message_id":target.input_id.as_uuid(), "work_id":target.work_id.as_uuid()}).to_string(), call_id: None, coverage: DependencyCoverage::Independent }
            }
        };
        message.validate()?;
        Ok(message)
    }
    async fn surface_work_input_on(
        &self,
        connection: &Connection,
        work: &SurfaceWorkReceipt,
    ) -> Result<floe_conversation::SurfaceInputReceipt, AgentFailure> {
        let mut rows = connection.query("SELECT sequence FROM agent_surface_entries WHERE kind = 'input' AND session_id = ? AND message_id = ? AND sequence <= ?", (work.anchor.session_id.to_string(), work.input_id.as_uuid().to_string(), work.anchor.sequence as i64)).await.map_err(database_failure)?;
        let sequence: i64 = rows
            .next()
            .await
            .map_err(database_failure)?
            .ok_or(AgentFailure::StorageUnavailable)?
            .get(0)
            .map_err(storage)?;
        let entry = self
            .surface_entry_at_on(connection, work.anchor.session_id, sequence as u64)
            .await?;
        match entry {
            SurfaceEntryReceipt::Input(input) => Ok(input),
            _ => Err(AgentFailure::StorageUnavailable),
        }
    }
    pub(super) async fn surface_compaction_target_on(
        &self,
        connection: &Connection,
        session: Uuid,
        authority: CompactionAuthority,
    ) -> Result<Option<(SurfaceAnchor, SurfaceAnchor)>, AgentFailure> {
        match authority {
            CompactionAuthority::BeforeFirstModel { run_id } => {
                let Some(work) = self.surface_work_for_run_on(connection, run_id).await? else {
                    return Ok(None);
                };
                if work.person_id != self.person_id || work.anchor.session_id != session {
                    return Err(AgentFailure::PolicyDenied);
                }
                self.surface_anchor_on(connection, work.anchor).await?;
                let input = self.surface_work_input_on(connection, &work).await?;
                Ok(Some((work.anchor, input.through)))
            }
            CompactionAuthority::Idle => {
                let head = self.surface_head_on(connection, session).await?;
                if head.sequence == 0 {
                    return Ok(None);
                }
                self.surface_anchor_on(connection, head).await?;
                let mut rows = connection.query("SELECT sequence FROM agent_surface_entries WHERE kind = 'input' AND session_id = ? ORDER BY sequence DESC LIMIT 1", [session.to_string()]).await.map_err(database_failure)?;
                let seq: i64 = rows
                    .next()
                    .await
                    .map_err(database_failure)?
                    .ok_or(AgentFailure::StorageUnavailable)?
                    .get(0)
                    .map_err(storage)?;
                let entry = self
                    .surface_entry_at_on(connection, session, seq as u64)
                    .await?;
                if !matches!(entry, SurfaceEntryReceipt::Input(_)) {
                    return Err(AgentFailure::StorageUnavailable);
                }
                Ok(Some((head, entry.through())))
            }
        }
    }
    async fn checkpoint_job_on(
        &self,
        connection: &Connection,
        reference: &SurfaceCheckpointRef,
    ) -> Result<CompactionExecution, AgentFailure> {
        reference.validate()?;
        let job = super::session_compaction_execution::read_on(
            connection,
            self.person_id,
            reference.execution_id,
        )
        .await?
        .ok_or(AgentFailure::StorageUnavailable)?;
        verify_checkpoint_on(connection, &job).await?;
        let CompactionPrefix::Surface(prefix) = &job.source.prefix else {
            return Err(AgentFailure::StorageUnavailable);
        };
        if prefix.through != reference.through {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(job)
    }
    pub(super) async fn verify_surface_source_payload_on(
        &self,
        connection: &Connection,
        source: &CompactionSource,
    ) -> Result<(), AgentFailure> {
        let CompactionPrefix::Surface(prefix) = &source.prefix else {
            return Err(AgentFailure::Conflict);
        };
        source.validate()?;
        if source.person_id() != self.person_id {
            return Err(AgentFailure::PolicyDenied);
        }
        self.surface_anchor_on(connection, prefix.head).await?;
        self.surface_anchor_on(connection, prefix.through).await?;
        match source.authority {
            CompactionAuthority::BeforeFirstModel { run_id } => {
                let work = self
                    .surface_work_for_run_on(connection, run_id)
                    .await?
                    .ok_or(AgentFailure::StorageUnavailable)?;
                let selected = self.surface_work_input_on(connection, &work).await?;
                let run = self
                    .conversation_run_on(connection, run_id)
                    .await?
                    .ok_or(AgentFailure::StorageUnavailable)?;
                if work.anchor != prefix.head
                    || selected.through != prefix.protected_input
                    || run.continuation_of.is_some()
                    || run.resume_of.is_some()
                    || run.retry_of.is_some()
                {
                    return Err(AgentFailure::StorageUnavailable);
                }
            }
            CompactionAuthority::Idle => {
                let mut rows = connection.query("SELECT sequence FROM agent_surface_entries WHERE kind = 'input' AND session_id = ? AND sequence <= ? ORDER BY sequence DESC LIMIT 1", (source.session_id.to_string(), prefix.head.sequence as i64)).await.map_err(database_failure)?;
                let sequence: i64 = rows
                    .next()
                    .await
                    .map_err(database_failure)?
                    .ok_or(AgentFailure::StorageUnavailable)?
                    .get(0)
                    .map_err(storage)?;
                let selected = self
                    .surface_entry_at_on(connection, source.session_id, sequence as u64)
                    .await?;
                if selected.through() != prefix.protected_input
                    || !matches!(selected, SurfaceEntryReceipt::Input(_))
                {
                    return Err(AgentFailure::StorageUnavailable);
                }
            }
        }
        let mut messages = Vec::new();
        let mut groups = Vec::new();
        let start = if let Some(previous) = &prefix.previous_checkpoint {
            // One bounded prior record, not recursive rehydration of the chain.
            let job = self.checkpoint_job_on(connection, previous).await?;
            let CompactionDisposition::Committed { receipt } = job.disposition else {
                return Err(AgentFailure::StorageUnavailable);
            };
            messages.push(receipt.summary);
            groups.push(ArchiveMessageGroup::SurfaceSummary(previous.execution_id));
            previous.through.sequence
        } else {
            0
        };
        for sequence in (start + 1)..=prefix.through.sequence {
            let entry = self
                .surface_entry_at_on(connection, source.session_id, sequence)
                .await?;
            groups.push(ArchiveMessageGroup::SurfaceEntry(
                entry.message_id().as_uuid(),
            ));
            messages.push(self.surface_message_on(connection, &entry).await?);
        }
        if messages != source.messages || groups != prefix.groups {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(())
    }
    async fn latest_surface_checkpoint_on(
        &self,
        connection: &Connection,
        head: SurfaceAnchor,
        protected_input: SurfaceAnchor,
    ) -> Result<Option<(SurfaceCheckpointRef, AgentMessage)>, AgentFailure> {
        let mut rows = connection.query("SELECT execution_id, through_sequence, through_digest FROM agent_surface_checkpoints WHERE person_id = ? AND session_id = ? AND through_sequence < ? AND through_sequence <= ? ORDER BY through_sequence DESC LIMIT 1", (self.person_id.to_string(), head.session_id.to_string(), protected_input.sequence as i64, head.sequence as i64)).await.map_err(database_failure)?;
        let Some(row) = rows.next().await.map_err(database_failure)? else {
            return Ok(None);
        };
        let sequence: i64 = row.get(1).map_err(storage)?;
        if sequence <= 0 {
            return Err(AgentFailure::StorageUnavailable);
        }
        let reference = SurfaceCheckpointRef {
            execution_id: Uuid::parse_str(&row.get::<String>(0).map_err(storage)?)
                .map_err(storage)?,
            through: SurfaceAnchor {
                session_id: head.session_id,
                sequence: sequence as u64,
                digest: row
                    .get::<Vec<u8>>(2)
                    .map_err(storage)?
                    .try_into()
                    .map_err(|_| AgentFailure::StorageUnavailable)?,
            },
        };
        let job = self.checkpoint_job_on(connection, &reference).await?;
        self.verify_surface_source_payload_on(connection, &job.source)
            .await?;
        let CompactionDisposition::Committed { receipt } = job.disposition else {
            return Err(AgentFailure::StorageUnavailable);
        };
        Ok(Some((reference, receipt.summary)))
    }
    pub(super) async fn prepare_surface_compaction_source_on(
        &self,
        connection: &Connection,
        session: &floe_conversation::AgentSession,
        expected_revision: u64,
        byte_limit: usize,
        authority: CompactionAuthority,
        head: SurfaceAnchor,
        protected_input: SurfaceAnchor,
    ) -> Result<Option<CompactionSource>, AgentFailure> {
        let previous = self
            .latest_surface_checkpoint_on(connection, head, protected_input)
            .await?;
        let start = previous
            .as_ref()
            .map_or(0, |(reference, _)| reference.through.sequence);
        let mut messages = Vec::new();
        let mut groups = Vec::new();
        if let Some((reference, summary)) = &previous {
            messages.push(summary.clone());
            groups.push(ArchiveMessageGroup::SurfaceSummary(reference.execution_id));
        }
        let available = head.sequence.saturating_sub(start);
        if available <= COMPACTION_MIN_RETAIN_ENTRIES || protected_input.sequence <= start + 1 {
            return Ok(None);
        }
        let mut pressure = authority != CompactionAuthority::Idle
            || available + u64::from(previous.is_some()) >= COMPACTION_TRIGGER_MESSAGES as u64;
        let max = head
            .sequence
            .saturating_sub(COMPACTION_MIN_RETAIN_ENTRIES)
            .min(protected_input.sequence.saturating_sub(1))
            .min(start + (COMPACTION_TRIGGER_MESSAGES - messages.len()) as u64);
        let mut preferred = None;
        let mut tail = None;
        let mut byte_pressure = false;
        for sequence in (start + 1)..=max {
            let entry = self
                .surface_entry_at_on(connection, session.id, sequence)
                .await?;
            let message = self.surface_message_on(connection, &entry).await?;
            messages.push(message);
            groups.push(ArchiveMessageGroup::SurfaceEntry(
                entry.message_id().as_uuid(),
            ));
            if serde_json::to_vec(&messages).map_err(storage)?.len() > byte_limit {
                byte_pressure = true;
                pressure = true;
                break;
            }
            let source = CompactionSource {
                authority,
                session_id: session.id,
                session_revision: expected_revision,
                data_classes: session.data_classes.clone(),
                prefix: CompactionPrefix::Surface(SurfaceCompactionPrefix {
                    person_id: self.person_id,
                    head,
                    protected_input,
                    previous_checkpoint: previous.as_ref().map(|(reference, _)| reference.clone()),
                    through: entry.through(),
                    through_entry_id: entry.message_id().as_uuid(),
                    groups: groups.clone(),
                }),
                messages: messages.clone(),
            };
            source.validate()?;
            if head.sequence - sequence >= COMPACTION_PREFERRED_RETAIN_ENTRIES {
                preferred = Some(source);
            } else {
                tail = Some(source);
            }
        }
        if pressure {
            let chosen = if byte_pressure {
                tail.or(preferred)
            } else {
                preferred.or(tail)
            };
            chosen.map(Some).ok_or(AgentFailure::BudgetExceeded)
        } else {
            Ok(None)
        }
    }
    pub(super) async fn install_surface_compaction_checkpoint_on(
        &self,
        connection: &Transaction<'_>,
        job: &CompactionExecution,
        source: &CompactionSource,
        summary: String,
        coverage: DependencyCoverage,
    ) -> Result<CompactionReceipt, AgentFailure> {
        let CompactionPrefix::Surface(prefix) = &source.prefix else {
            return Err(AgentFailure::Conflict);
        };
        if job.source != *source || job.disposition != CompactionDisposition::Pending {
            return Err(AgentFailure::Conflict);
        }
        self.verify_surface_source_payload_on(connection, source)
            .await?;
        let message_id = floe_conversation::surface_summary_id(job.claim.execution_id);
        let recorded = super::context_dependencies::merge_context_dependency_coverage(
            connection,
            self.person_id,
            source.session_id,
            message_id,
            coverage.clone(),
        )
        .await?;
        if recorded != coverage {
            return Err(AgentFailure::StorageUnavailable);
        }
        let receipt = CompactionReceipt {
            session_id: source.session_id,
            session_revision: source.session_revision,
            pointer: ArchivePointer::Surface {
                archive_id: job.claim.execution_id,
                source_revision: source.session_revision,
                through_entry_id: prefix.through_entry_id,
                through_sequence: prefix.through.sequence,
                prefix_digest: prefix.through.digest,
                archived_message_count: source.messages.len(),
            },
            summary: AgentMessage {
                message_id,
                role: MessageRole::Assistant,
                text: summary,
                call_id: None,
                coverage,
            },
        };
        receipt.validate()?;
        connection.execute("INSERT INTO agent_surface_checkpoints (execution_id, person_id, session_id, through_sequence, through_digest) VALUES (?, ?, ?, ?, ?)", (job.claim.execution_id.to_string(), self.person_id.to_string(), source.session_id.to_string(), prefix.through.sequence as i64, prefix.through.digest.to_vec())).await.map_err(database_failure)?;
        Ok(receipt)
    }
    pub(crate) async fn read_surface_compaction_archive(
        &self,
        request: &ArchiveReadRequest,
    ) -> Result<ArchiveSnapshot, AgentFailure> {
        request.validate()?;
        if request.person_id != self.person_id
            || !matches!(&request.pointer, ArchivePointer::Surface { .. })
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let mut connection = self.connection()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(database_failure)?;
        let job = super::session_compaction_execution::read_on(
            &tx,
            self.person_id,
            request.pointer.archive_id(),
        )
        .await?
        .ok_or(AgentFailure::NotFound)?;
        verify_checkpoint_on(&tx, &job).await?;
        self.verify_surface_source_payload_on(&tx, &job.source)
            .await?;
        let CompactionDisposition::Committed { receipt } = &job.disposition else {
            return Err(AgentFailure::StorageUnavailable);
        };
        if receipt.pointer != request.pointer || job.source.session_id != request.session_id {
            return Err(AgentFailure::Conflict);
        }
        if job.source.messages.len() > request.max_messages
            || serde_json::to_vec(&job.source.messages)
                .map_err(storage)?
                .len()
                > request.max_bytes
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        let CompactionPrefix::Surface(prefix) = &job.source.prefix else {
            return Err(AgentFailure::StorageUnavailable);
        };
        let snapshot = ArchiveSnapshot {
            person_id: self.person_id,
            session_id: request.session_id,
            pointer: request.pointer.clone(),
            messages: prefix
                .groups
                .iter()
                .copied()
                .zip(job.source.messages)
                .map(|(group, message)| ArchivedMessage { group, message })
                .collect(),
        };
        snapshot.validate(request)?;
        tx.commit().await.map_err(database_failure)?;
        self.check_access()?;
        Ok(snapshot)
    }
    pub(crate) async fn surface_model_history_for_work(
        &self,
        work: &SurfaceWorkReceipt,
        execution_user_id: Uuid,
    ) -> Result<(Vec<AgentMessage>, bool), AgentFailure> {
        work.validate()?;
        if work.person_id != self.person_id || execution_user_id.is_nil() {
            return Err(AgentFailure::PolicyDenied);
        }
        let mut connection = self.connection()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(database_failure)?;
        if self
            .surface_work_command_on(&tx, work.command_id)
            .await?
            .as_ref()
            != Some(work)
        {
            return Err(AgentFailure::Conflict);
        }
        self.surface_anchor_on(&tx, work.anchor).await?;
        let input = self.surface_work_input_on(&tx, work).await?;
        let checkpoint = self
            .latest_surface_checkpoint_on(&tx, work.anchor, input.through)
            .await?;
        let start = checkpoint
            .as_ref()
            .map_or(0, |(reference, _)| reference.through.sequence);
        let limit =
            floe_conversation::MAX_SURFACE_PAGE_ITEMS - usize::from(checkpoint.is_some()) - 1;
        let after = start.max(work.anchor.sequence.saturating_sub(limit as u64));
        let incomplete = after > start;
        let mut messages = Vec::new();
        if let Some((_, summary)) = checkpoint {
            messages.push(summary);
        }
        for sequence in (after + 1)..=work.anchor.sequence {
            let entry = self
                .surface_entry_at_on(&tx, work.anchor.session_id, sequence)
                .await?;
            if entry
                .input()
                .is_some_and(|input| input.input.message_id == work.input_id)
            {
                continue;
            }
            messages.push(self.surface_message_on(&tx, &entry).await?);
        }
        messages.push(AgentMessage {
            message_id: execution_user_id,
            role: MessageRole::User,
            text: input.input.text,
            call_id: None,
            coverage: DependencyCoverage::Independent,
        });
        tx.commit().await.map_err(database_failure)?;
        self.check_access()?;
        Ok((messages, incomplete))
    }
}
