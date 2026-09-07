use floe_agent::{AgentOutcome, AgentSessionScope, CalendarExpertSetupReceipt, Cancellation};
use turso::transaction::TransactionBehavior;

use super::*;

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub async fn create_calendar_session(
        &self,
        setup_id: Uuid,
        cancellation: Cancellation,
    ) -> Result<AgentSession, AgentFailure> {
        check_cancelled(&cancellation)?;
        let scope = self.calendar_session_scope(setup_id).await?;
        let mut session = AgentSession::new(self.person_id);
        session.scope = Some(scope);
        session.data_classes = vec![scope.data_class()];
        let payload = self.payload(&session)?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(storage)?;
        let result = async {
            check_cancelled(&cancellation)?;
            transaction
                .execute(
                    "INSERT INTO agent_sessions (id, revision, payload) VALUES (?, 0, ?)",
                    (session.id.to_string(), payload),
                )
                .await
                .map_err(storage)?;
            self.check_access()?;
            check_cancelled(&cancellation)
        }
        .await;
        match result {
            Ok(()) => transaction.commit().await.map_err(storage)?,
            Err(failure) => {
                let _ = transaction.rollback().await;
                return Err(failure);
            }
        }
        self.check_access()?;
        check_cancelled(&cancellation)?;
        Ok(session)
    }

    pub async fn resume_calendar_session(
        &self,
        setup_id: Uuid,
        cancellation: Cancellation,
    ) -> Result<AgentSession, AgentFailure> {
        check_cancelled(&cancellation)?;
        self.calendar_session_scope(setup_id).await?;
        let connection = self.connection()?;
        let mut rows = connection.query(
            "SELECT id FROM agent_sessions WHERE json_extract(payload, '$.scope.kind') = 'calendar' AND json_extract(payload, '$.scope.setup_id') = ? ORDER BY rowid DESC LIMIT 1",
            [setup_id.to_string()],
        ).await.map_err(storage)?;
        let id = rows
            .next()
            .await
            .map_err(storage)?
            .map(|row| {
                Uuid::parse_str(&row.get::<String>(0).map_err(storage)?).map_err(unavailable)
            })
            .transpose()?;
        drop(rows);
        match id {
            Some(id) => {
                let session = self.calendar_session(id).await?;
                check_cancelled(&cancellation)?;
                Ok(session)
            }
            None => self.create_calendar_session(setup_id, cancellation).await,
        }
    }

    pub async fn calendar_session(&self, session_id: Uuid) -> Result<AgentSession, AgentFailure> {
        let session = self.load(self.person_id, session_id).await?;
        let Some(AgentSessionScope::Calendar { setup_id, .. }) = session.scope else {
            return Err(AgentFailure::PolicyDenied);
        };
        if session.scope != Some(self.calendar_session_scope(setup_id).await?) {
            return Err(AgentFailure::Conflict);
        }
        self.check_access()?;
        Ok(session)
    }

    pub async fn recover_calendar_session(
        &self,
        session_id: Uuid,
        expected_revision: u64,
        cancellation: Cancellation,
    ) -> Result<AgentSession, AgentFailure> {
        check_cancelled(&cancellation)?;
        let mut session = self.calendar_session(session_id).await?;
        if session.revision != expected_revision {
            return Err(AgentFailure::Conflict);
        }
        if session.active_turn.is_some() {
            check_cancelled(&cancellation)?;
            session.active_turn = None;
            session.last_outcome = Some(AgentOutcome::Halted {
                reason: AgentFailure::Interrupted,
            });
            session.revision = expected_revision
                .checked_add(1)
                .ok_or(AgentFailure::Conflict)?;
            self.compare_and_swap(&session, expected_revision).await?;
        }
        self.check_access()?;
        check_cancelled(&cancellation)?;
        Ok(session)
    }

    pub async fn calendar_session_setup(
        &self,
        session: &AgentSession,
    ) -> Result<CalendarExpertSetupReceipt, AgentFailure> {
        let saved = self.calendar_session(session.id).await?;
        if &saved != session {
            return Err(AgentFailure::Conflict);
        }
        let Some(AgentSessionScope::Calendar { setup_id, .. }) = saved.scope else {
            return Err(AgentFailure::PolicyDenied);
        };
        self.calendar_expert_overview()
            .await?
            .setups
            .into_iter()
            .find(|setup| setup.setup_id == setup_id)
            .ok_or(AgentFailure::NotFound)
    }

    async fn calendar_session_scope(
        &self,
        setup_id: Uuid,
    ) -> Result<AgentSessionScope, AgentFailure> {
        let overview = self.calendar_expert_overview().await?;
        let setup = overview
            .setups
            .iter()
            .find(|setup| setup.setup_id == setup_id)
            .ok_or(AgentFailure::NotFound)?;
        let view = overview
            .views
            .iter()
            .find(|view| view.handle == setup.view_handle)
            .ok_or(AgentFailure::Conflict)?;
        Ok(AgentSessionScope::Calendar {
            setup_id,
            provider: view.provider,
        })
    }
}

fn check_cancelled(cancellation: &Cancellation) -> Result<(), AgentFailure> {
    if cancellation.is_cancelled() {
        Err(AgentFailure::Cancelled)
    } else {
        Ok(())
    }
}
