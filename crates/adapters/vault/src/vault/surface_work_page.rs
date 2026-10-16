//! Bounded Work recovery under a stable admission watermark.
use super::*;
use floe_conversation::{SurfaceWorkCursor, SurfaceWorkPage, SurfaceWorkPageRequest};
use floe_kernel::{CommandId, OwnerActor};
use turso::{Connection, transaction::TransactionBehavior};

fn command_id(value: String) -> Result<CommandId, AgentFailure> {
    Uuid::parse_str(&value)
        .ok()
        .and_then(CommandId::from_uuid)
        .ok_or(AgentFailure::StorageUnavailable)
}
impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    async fn work_page_sequence_on(
        &self,
        connection: &Connection,
        session_id: Uuid,
        command: CommandId,
    ) -> Result<i64, AgentFailure> {
        let work = self
            .surface_work_command_on(connection, command)
            .await?
            .ok_or(AgentFailure::NotFound)?;
        if work.anchor.session_id != session_id || work.person_id != self.person_id {
            return Err(AgentFailure::Conflict);
        }
        let mut rows = connection.query("SELECT queue_sequence FROM agent_surface_work WHERE command_id = ? AND session_id = ?", (command.as_uuid().to_string(), session_id.to_string())).await.map_err(database_failure)?;
        let sequence: i64 = rows
            .next()
            .await
            .map_err(database_failure)?
            .ok_or(AgentFailure::StorageUnavailable)?
            .get(0)
            .map_err(storage)?;
        if sequence <= 0 {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(sequence)
    }
    pub(super) async fn surface_work_page(
        &self,
        actor: &OwnerActor,
        request: SurfaceWorkPageRequest,
    ) -> Result<SurfaceWorkPage, AgentFailure> {
        request.validate()?;
        let mut connection = self.connection()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(database_failure)?;
        self.surface_session_on(&tx, actor, request.session_id)
            .await?;
        let (through, upper) = if let Some(cursor) = &request.cursor {
            let through = self
                .work_page_sequence_on(&tx, request.session_id, cursor.through)
                .await?;
            let before = self
                .work_page_sequence_on(&tx, request.session_id, cursor.before)
                .await?;
            if before > through {
                return Err(AgentFailure::Conflict);
            }
            (Some(cursor.through), before - 1)
        } else {
            let mut rows = tx.query("SELECT command_id, queue_sequence FROM agent_surface_work WHERE session_id = ? ORDER BY queue_sequence DESC LIMIT 1", [request.session_id.to_string()]).await.map_err(database_failure)?;
            match rows.next().await.map_err(database_failure)? {
                None => (None, 0),
                Some(row) => {
                    let command = command_id(row.get(0).map_err(storage)?)?;
                    let sequence = self
                        .work_page_sequence_on(&tx, request.session_id, command)
                        .await?;
                    if row.get::<i64>(1).map_err(storage)? != sequence {
                        return Err(AgentFailure::StorageUnavailable);
                    }
                    (Some(command), sequence)
                }
            }
        };
        let mut rows = tx.query("SELECT command_id FROM agent_surface_work WHERE session_id = ? AND queue_sequence <= ? ORDER BY queue_sequence DESC LIMIT ?", (request.session_id.to_string(), upper, (request.limit + 1) as i64)).await.map_err(database_failure)?;
        let mut commands = Vec::new();
        while let Some(row) = rows.next().await.map_err(database_failure)? {
            commands.push(command_id(row.get(0).map_err(storage)?)?);
        }
        drop(rows);
        let more = commands.len() > request.limit;
        if more {
            commands.pop();
        }
        let next = if more {
            Some(SurfaceWorkCursor {
                through: through.ok_or(AgentFailure::StorageUnavailable)?,
                before: *commands.last().ok_or(AgentFailure::StorageUnavailable)?,
            })
        } else {
            None
        };
        commands.reverse();
        let mut items = Vec::with_capacity(commands.len());
        for command in commands {
            let item = self
                .surface_work_snapshot_on(&tx, actor, command)
                .await?
                .ok_or(AgentFailure::StorageUnavailable)?;
            if item.receipt.anchor.session_id != request.session_id {
                return Err(AgentFailure::StorageUnavailable);
            }
            items.push(item);
        }
        tx.commit().await.map_err(database_failure)?;
        self.check_access()?;
        Ok(SurfaceWorkPage { items, next })
    }
}
