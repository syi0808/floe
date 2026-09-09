use std::collections::HashSet;

use floe_agent::{AgentFailure, AgentMessage, AgentSession, SessionRecoveryPointer};
use serde::{Deserialize, Serialize};
use turso::transaction::TransactionBehavior;
use uuid::Uuid;

use super::*;

const MAX_COMPACTION_SUMMARY_BYTES: usize = 16 * 1024;
const MAX_SEARCH_QUERY_BYTES: usize = 512;
const MAX_SEARCH_RESULTS: usize = 50;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SessionSearchHit {
    pub session_id: Uuid,
    pub session_revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery: Option<SessionRecoveryPointer>,
    pub excerpt: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SessionCompactionResult {
    pub session: AgentSession,
    pub recovery: SessionRecoveryPointer,
}

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub(super) async fn initialize_session_archive(&self) -> Result<(), AgentFailure> {
        let connection = self.connection()?;
        connection.execute(
            "CREATE TABLE IF NOT EXISTS agent_session_archives (id TEXT PRIMARY KEY, session_id TEXT NOT NULL, source_revision INTEGER NOT NULL, through_turn_id TEXT NOT NULL, message_count INTEGER NOT NULL, payload TEXT NOT NULL)",
            (),
        ).await.map_err(storage)?;
        connection.execute(
            "CREATE INDEX IF NOT EXISTS agent_session_archives_session ON agent_session_archives(session_id, source_revision)",
            (),
        ).await.map_err(storage)?;
        connection.execute(
            "CREATE TABLE IF NOT EXISTS agent_session_search (session_id TEXT NOT NULL, archive_id TEXT NOT NULL, revision INTEGER NOT NULL, body TEXT NOT NULL, PRIMARY KEY(session_id, archive_id))",
            (),
        ).await.map_err(storage)?;
        self.rebuild_session_search(&connection).await
    }

    pub async fn search_sessions(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SessionSearchHit>, AgentFailure> {
        let terms = search_terms(query)?;
        if limit == 0 || limit > MAX_SEARCH_RESULTS {
            return Err(AgentFailure::InvalidInput);
        }
        let connection = self.connection()?;
        self.rebuild_session_search(&connection).await?;
        let predicates = std::iter::repeat_n("instr(lower(body), ?) > 0", terms.len())
            .collect::<Vec<_>>()
            .join(" AND ");
        let sql = format!(
            "SELECT session_id, archive_id, revision, body FROM agent_session_search WHERE {predicates} ORDER BY revision DESC, archive_id = '' DESC LIMIT ?"
        );
        let mut parameters: Vec<turso::Value> =
            terms.iter().cloned().map(turso::Value::from).collect();
        parameters.push(turso::Value::from(
            i64::try_from(limit).map_err(|_| AgentFailure::InvalidInput)?,
        ));
        let mut rows = connection.query(&sql, parameters).await.map_err(storage)?;
        let mut hits = Vec::new();
        while let Some(row) = rows.next().await.map_err(storage)? {
            let session_id =
                Uuid::parse_str(&row.get::<String>(0).map_err(storage)?).map_err(unavailable)?;
            let archive_id = row.get::<String>(1).map_err(storage)?;
            let session_revision = u64::try_from(row.get::<i64>(2).map_err(storage)?)
                .map_err(|_| AgentFailure::VaultUnavailable)?;
            let recovery = if archive_id.is_empty() {
                None
            } else {
                Some(
                    self.recovery_pointer(Uuid::parse_str(&archive_id).map_err(unavailable)?)
                        .await?,
                )
            };
            hits.push(SessionSearchHit {
                session_id,
                session_revision,
                recovery,
                excerpt: excerpt(&row.get::<String>(3).map_err(storage)?, &terms),
            });
        }
        self.check_access()?;
        Ok(hits)
    }

    pub async fn compact_session(
        &self,
        session_id: Uuid,
        expected_revision: u64,
        through_turn_id: Uuid,
        summary: String,
    ) -> Result<SessionCompactionResult, AgentFailure> {
        let summary = summary.trim().to_owned();
        if summary.is_empty() || summary.len() > MAX_COMPACTION_SUMMARY_BYTES {
            return Err(AgentFailure::InvalidInput);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(storage)?;
        let result = async {
            let source = self.session_on(&transaction, session_id).await?;
            if source.revision != expected_revision {
                return Err(AgentFailure::Conflict);
            }
            if source.active_turn.is_some() || source.pending_output.is_some() {
                return Err(AgentFailure::Conflict);
            }
            let split = source
                .messages
                .iter()
                .rposition(|message| message.turn_id() == through_turn_id)
                .map(|position| position + 1)
                .ok_or(AgentFailure::NotFound)?;
            let archived_turns: HashSet<_> = source.messages[..split]
                .iter()
                .map(AgentMessage::turn_id)
                .collect();
            let archive_id = Uuid::new_v4();
            let recovery = SessionRecoveryPointer {
                archive_id,
                source_revision: source.revision,
                through_turn_id,
                archived_message_count: split,
            };
            let archive_payload = serde_json::to_string(&source).map_err(storage)?;
            transaction.execute(
                "INSERT INTO agent_session_archives (id, session_id, source_revision, through_turn_id, message_count, payload) VALUES (?, ?, ?, ?, ?, ?)",
                (
                    archive_id.to_string(),
                    session_id.to_string(),
                    integer(source.revision)?,
                    through_turn_id.to_string(),
                    i64::try_from(split).map_err(|_| AgentFailure::BudgetExceeded)?,
                    archive_payload,
                ),
            ).await.map_err(storage)?;

            let mut session = source.clone();
            session.revision = session.revision.checked_add(1).ok_or(AgentFailure::Conflict)?;
            session.messages = std::iter::once(AgentMessage::Compaction {
                turn_id: through_turn_id,
                summary,
                recovery: recovery.clone(),
            })
            .chain(source.messages[split..].iter().cloned())
            .collect();
            session.model_attempts.retain(|record| !archived_turns.contains(&record.turn_id));
            session.capability_executions.retain(|record| !archived_turns.contains(&record.turn_id));
            session.delegation_executions.retain(|record| !archived_turns.contains(&record.turn_id));
            let payload = self.payload(&session)?;
            let changed = transaction.execute(
                "UPDATE agent_sessions SET revision = ?, payload = ? WHERE id = ? AND revision = ?",
                (
                    integer(session.revision)?,
                    payload,
                    session_id.to_string(),
                    integer(expected_revision)?,
                ),
            ).await.map_err(storage)?;
            if changed != 1 {
                return Err(AgentFailure::Conflict);
            }
            transaction.execute(
                "DELETE FROM agent_session_search WHERE session_id = ? AND archive_id = ''",
                [session_id.to_string()],
            ).await.map_err(storage)?;
            self.index_session_on(&transaction, &session, "").await?;
            self.index_session_on(&transaction, &source, &archive_id.to_string())
                .await?;
            self.check_access()?;
            Ok(SessionCompactionResult { session, recovery })
        }.await;
        match result {
            Ok(result) => {
                transaction.commit().await.map_err(storage)?;
                Ok(result)
            }
            Err(error) => {
                let _ = transaction.rollback().await;
                Err(error)
            }
        }
    }

    pub async fn recover_session(
        &self,
        recovery: &SessionRecoveryPointer,
    ) -> Result<AgentSession, AgentFailure> {
        let connection = self.connection()?;
        let mut rows = connection.query(
            "SELECT session_id, source_revision, through_turn_id, message_count, payload FROM agent_session_archives WHERE id = ?",
            [recovery.archive_id.to_string()],
        ).await.map_err(storage)?;
        let row = rows
            .next()
            .await
            .map_err(storage)?
            .ok_or(AgentFailure::NotFound)?;
        let session: AgentSession =
            serde_json::from_str(&row.get::<String>(4).map_err(storage)?).map_err(unavailable)?;
        if session.id.to_string() != row.get::<String>(0).map_err(storage)?
            || session.revision
                != u64::try_from(row.get::<i64>(1).map_err(storage)?).map_err(unavailable)?
            || recovery.source_revision != session.revision
            || recovery.through_turn_id.to_string() != row.get::<String>(2).map_err(storage)?
            || recovery.archived_message_count
                != usize::try_from(row.get::<i64>(3).map_err(storage)?).map_err(unavailable)?
        {
            return Err(AgentFailure::VaultUnavailable);
        }
        self.payload(&session)?;
        self.check_access()?;
        Ok(session)
    }

    async fn recovery_pointer(
        &self,
        archive_id: Uuid,
    ) -> Result<SessionRecoveryPointer, AgentFailure> {
        let connection = self.connection()?;
        let mut rows = connection.query(
            "SELECT source_revision, through_turn_id, message_count FROM agent_session_archives WHERE id = ?",
            [archive_id.to_string()],
        ).await.map_err(storage)?;
        let row = rows
            .next()
            .await
            .map_err(storage)?
            .ok_or(AgentFailure::VaultUnavailable)?;
        Ok(SessionRecoveryPointer {
            archive_id,
            source_revision: u64::try_from(row.get::<i64>(0).map_err(storage)?)
                .map_err(unavailable)?,
            through_turn_id: Uuid::parse_str(&row.get::<String>(1).map_err(storage)?)
                .map_err(unavailable)?,
            archived_message_count: usize::try_from(row.get::<i64>(2).map_err(storage)?)
                .map_err(unavailable)?,
        })
    }

    async fn rebuild_session_search(
        &self,
        connection: &turso::Connection,
    ) -> Result<(), AgentFailure> {
        connection
            .execute("DELETE FROM agent_session_search", ())
            .await
            .map_err(storage)?;
        let mut rows = connection
            .query("SELECT payload FROM agent_sessions", ())
            .await
            .map_err(storage)?;
        while let Some(row) = rows.next().await.map_err(storage)? {
            let session: AgentSession =
                serde_json::from_str(&row.get::<String>(0).map_err(storage)?)
                    .map_err(unavailable)?;
            self.index_session_on(connection, &session, "").await?;
        }
        drop(rows);
        let mut rows = connection
            .query("SELECT id, payload FROM agent_session_archives", ())
            .await
            .map_err(storage)?;
        while let Some(row) = rows.next().await.map_err(storage)? {
            let archive_id = row.get::<String>(0).map_err(storage)?;
            let session: AgentSession =
                serde_json::from_str(&row.get::<String>(1).map_err(storage)?)
                    .map_err(unavailable)?;
            self.index_session_on(connection, &session, &archive_id)
                .await?;
        }
        Ok(())
    }

    async fn index_session_on(
        &self,
        connection: &turso::Connection,
        session: &AgentSession,
        archive_id: &str,
    ) -> Result<(), AgentFailure> {
        let body = session
            .messages
            .iter()
            .map(searchable_message)
            .collect::<Vec<_>>()
            .join("\n");
        if !body.is_empty() {
            connection.execute(
                "INSERT INTO agent_session_search (session_id, archive_id, revision, body) VALUES (?, ?, ?, ?)",
                (session.id.to_string(), archive_id.to_owned(), integer(session.revision)?, body),
            ).await.map_err(storage)?;
        }
        Ok(())
    }
}

fn searchable_message(message: &AgentMessage) -> String {
    match message {
        AgentMessage::Compaction { summary, .. }
        | AgentMessage::Preamble { text: summary, .. }
        | AgentMessage::User { text: summary, .. }
        | AgentMessage::Assistant { text: summary, .. } => summary.clone(),
        AgentMessage::Capability {
            capability_id,
            input,
            result,
            ..
        } => {
            format!("{capability_id}\n{input}\n{result:?}")
        }
        AgentMessage::Delegation { task, .. } => serde_json::to_string(task).unwrap_or_default(),
    }
}

fn search_terms(query: &str) -> Result<Vec<String>, AgentFailure> {
    if query.trim().is_empty() || query.len() > MAX_SEARCH_QUERY_BYTES {
        return Err(AgentFailure::InvalidInput);
    }
    let terms: Vec<_> = query
        .split(|character: char| !character.is_alphanumeric())
        .filter(|term| !term.is_empty())
        .map(str::to_lowercase)
        .collect();
    if terms.is_empty() {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(terms)
}

fn excerpt(body: &str, terms: &[String]) -> String {
    let matched_line = body
        .lines()
        .find(|line| {
            let line = line.to_lowercase();
            terms.iter().any(|term| line.contains(term))
        })
        .unwrap_or(body);
    let mut excerpt: String = matched_line.chars().take(160).collect();
    if matched_line.chars().count() > 160 {
        excerpt.push_str(" …");
    }
    excerpt
}

fn integer(value: u64) -> Result<i64, AgentFailure> {
    i64::try_from(value).map_err(|_| AgentFailure::BudgetExceeded)
}
