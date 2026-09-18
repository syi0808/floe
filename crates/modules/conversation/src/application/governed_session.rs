//! The Session store a governed turn runs against.
//!
//! Conversation owns the Session: which messages a model may see, whether the
//! replay still holds, and what the turn commits. Context decides what the
//! recorded coverage still authorizes; the repository performs the one atomic
//! commit. None of those three does the other's work.

use std::collections::{BTreeMap, HashMap};

use floe_agent_contract::{AgentFailure, BoxFuture, SessionProtection};
use floe_agent_contract::{ContextDependency, DependencyCoverage};
use floe_context::{
    CoverageMessageFact, CoverageRegistry, DependencyAuthorization, DependencyLiveness,
    DependencyResolver, EvidenceReader,
};
use floe_kernel::PersonId;
use uuid::Uuid;

use crate::turn::{AgentMessage, AgentSession, ModelRequest, SessionStore};

/// The storage a governed Session turn needs, and nothing more.
///
/// Reading a turn's committed coverage and committing the Session together with
/// the coverage it produced are one owner's business; the adapter behind this
/// port keeps its transaction, its CAS and its authority checks.
pub trait GovernedSessionRepository: Send + Sync {
    fn protection(&self) -> SessionProtection;

    /// The Person this store is bound to.
    fn person_id(&self) -> PersonId;

    fn load<'a>(
        &'a self,
        person_id: PersonId,
        session_id: Uuid,
    ) -> BoxFuture<'a, Result<AgentSession, AgentFailure>>;

    fn read_turn_coverage<'a>(
        &'a self,
        session_id: Uuid,
        turn_id: Uuid,
    ) -> BoxFuture<'a, Result<DependencyCoverage, AgentFailure>>;

    /// Commit the Session and the coverage it produced in one transaction.
    fn commit_session_with_coverage<'a>(
        &'a self,
        session: &'a AgentSession,
        previous_revision: u64,
        coverage: &'a BTreeMap<Uuid, DependencyCoverage>,
        liveness: Option<&'a dyn DependencyLiveness>,
    ) -> BoxFuture<'a, Result<(), AgentFailure>>;
}

/// One invocation's Session store, with the coverage it is accumulating.
pub struct GovernedSessionStore<'a, Repository> {
    repository: &'a Repository,
    session_id: Uuid,
    coverage: CoverageRegistry,
    liveness: Option<&'a dyn DependencyLiveness>,
}

/// The committed coverage of this Session's turns, as Context reads it.
struct SessionEvidence<'a, Repository> {
    repository: &'a Repository,
    session_id: Uuid,
}

impl<Repository: GovernedSessionRepository> EvidenceReader for SessionEvidence<'_, Repository> {
    async fn read_turn_coverage(
        &self,
        session_id: Uuid,
        turn_id: Uuid,
    ) -> Result<DependencyCoverage, AgentFailure> {
        if session_id.is_nil() || turn_id.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        if session_id != self.session_id {
            return Err(AgentFailure::Conflict);
        }
        self.repository
            .read_turn_coverage(session_id, turn_id)
            .await
    }
}

impl<'a, Repository: GovernedSessionRepository> GovernedSessionStore<'a, Repository> {
    pub fn new(repository: &'a Repository, session_id: Uuid) -> Self {
        Self {
            repository,
            session_id,
            coverage: CoverageRegistry::new(),
            liveness: None,
        }
    }

    pub fn with_liveness(mut self, liveness: &'a dyn DependencyLiveness) -> Self {
        self.liveness = Some(liveness);
        self
    }

    pub fn session_id(&self) -> Uuid {
        self.session_id
    }

    pub async fn record_dependency(
        &self,
        turn_id: Uuid,
        dependency: ContextDependency,
    ) -> Result<(), AgentFailure> {
        if turn_id.is_nil() || dependency.observation_id().is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        let stored = if self.coverage.has_turn(turn_id)? {
            None
        } else {
            let session = self
                .repository
                .load(self.repository.person_id(), self.session_id)
                .await?;
            if session
                .messages
                .iter()
                .any(|message| message.turn_id() == turn_id)
            {
                Some(
                    self.repository
                        .read_turn_coverage(self.session_id, turn_id)
                        .await?,
                )
            } else {
                None
            }
        };
        self.coverage.record_dependency(turn_id, dependency, stored)
    }

    pub fn record_result_dependency(
        &self,
        turn_id: Uuid,
        result_id: Uuid,
        dependency: ContextDependency,
    ) -> Result<(), AgentFailure> {
        if turn_id.is_nil() || result_id.is_nil() || dependency.observation_id().is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        self.coverage
            .record_result_dependency(turn_id, result_id, dependency)
    }

    pub fn record_result_independent(
        &self,
        turn_id: Uuid,
        result_id: Uuid,
    ) -> Result<(), AgentFailure> {
        if turn_id.is_nil() || result_id.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        self.coverage.record_result_independent(turn_id, result_id)
    }

    pub fn result_coverage(
        &self,
        turn_id: Uuid,
        result_id: Uuid,
    ) -> Result<Option<DependencyCoverage>, AgentFailure> {
        if turn_id.is_nil() || result_id.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        self.coverage.result_coverage(turn_id, result_id)
    }

    /// The committed coverage of one recorded turn, for projection inputs that
    /// quote history. This reads what was committed, never the live registry.
    pub async fn committed_turn_coverage(
        &self,
        turn_id: Uuid,
    ) -> Result<DependencyCoverage, AgentFailure> {
        if turn_id.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        self.repository
            .read_turn_coverage(self.session_id, turn_id)
            .await
    }

    pub async fn project_model_request(
        &self,
        request: &mut ModelRequest,
        resolver: Option<&dyn DependencyResolver>,
    ) -> Result<(), AgentFailure> {
        self.project_model_request_with_coverage(request, resolver)
            .await
            .map(|_| ())
    }

    /// Re-admit the coverage the current turn already stands on.
    pub async fn revalidate_current_coverage(
        &self,
        request: &ModelRequest,
        resolver: &dyn DependencyResolver,
    ) -> Result<(), AgentFailure> {
        if request.session_id != self.session_id {
            return Err(AgentFailure::Conflict);
        }
        let coverage = match self.coverage.turn_coverage(request.turn_id)? {
            Some(value) => value,
            None => {
                self.repository
                    .read_turn_coverage(self.session_id, request.turn_id)
                    .await?
            }
        };
        floe_context::revalidate_turn_coverage(coverage, resolver, &authorization(request)).await
    }

    /// Project the transcript this model attempt may see.
    ///
    /// Context decides what each recorded turn still authorizes; this applies
    /// that decision: a Person's own message always stays, anything derived
    /// from a source that no longer re-admits is dropped, and any replay is
    /// discarded once the transcript it replayed against has changed.
    pub async fn project_model_request_with_coverage(
        &self,
        request: &mut ModelRequest,
        resolver: Option<&dyn DependencyResolver>,
    ) -> Result<bool, AgentFailure> {
        if request.session_id != self.session_id {
            return Err(AgentFailure::Conflict);
        }
        let evidence = SessionEvidence {
            repository: self.repository,
            session_id: self.session_id,
        };
        let decisions = floe_context::project_history(
            &evidence,
            self.session_id,
            request.messages.iter().map(AgentMessage::turn_id),
            resolver,
            &authorization(request),
        )
        .await?;
        let current_turn = request.turn_id;
        super::history_projection::project_history_into(
            request,
            super::history_projection::HistoryProjection {
                decisions: &decisions,
                current_turn: None,
            },
            |_, dependencies| {
                for dependency in dependencies {
                    self.coverage
                        .record_dependency(current_turn, dependency.clone(), None)
                        .map_err(|error| match error {
                            AgentFailure::InvalidInput => AgentFailure::PolicyDenied,
                            error => error,
                        })?;
                }
                Ok(())
            },
        )
    }
}

fn authorization(request: &ModelRequest) -> DependencyAuthorization {
    DependencyAuthorization {
        allowed_placements: request.policy.allowed_placements.clone(),
        deadline: request.deadline,
        cancellation: request.cancellation.clone(),
    }
}

impl<Repository: GovernedSessionRepository> SessionStore for GovernedSessionStore<'_, Repository> {
    fn protection(&self) -> SessionProtection {
        self.repository.protection()
    }

    async fn load(
        &self,
        person_id: PersonId,
        session_id: Uuid,
    ) -> Result<AgentSession, AgentFailure> {
        self.repository.load(person_id, session_id).await
    }

    async fn compare_and_swap(
        &self,
        session: &AgentSession,
        previous_revision: u64,
    ) -> Result<(), AgentFailure> {
        if session.id != self.session_id {
            return Err(AgentFailure::Conflict);
        }
        let stored = self.repository.load(session.person_id, session.id).await?;
        if stored.revision != previous_revision || session.messages.len() < stored.messages.len() {
            return Err(AgentFailure::Conflict);
        }
        let appended = &session.messages[stored.messages.len()..];
        let mut initial = BTreeMap::new();
        let mut stored_turns = HashMap::new();
        for message in &stored.messages {
            stored_turns.insert(message.turn_id(), ());
        }
        for turn_id in appended.iter().map(AgentMessage::turn_id) {
            if stored_turns.contains_key(&turn_id) && !initial.contains_key(&turn_id) {
                initial.insert(
                    turn_id,
                    self.repository
                        .read_turn_coverage(session.id, turn_id)
                        .await?,
                );
            }
        }
        let facts = appended
            .iter()
            .map(|message| CoverageMessageFact {
                turn_id: message.turn_id(),
                existing_turn: stored_turns.contains_key(&message.turn_id()),
                is_user: matches!(message, AgentMessage::User { .. }),
                result_id: match message {
                    AgentMessage::Capability { call_id, .. } => Some(*call_id),
                    AgentMessage::Delegation { task, .. } => Some(task.id),
                    _ => None,
                },
            })
            .collect::<Vec<_>>();
        let snapshot = self.coverage.fold_messages(&facts, &initial)?;
        self.repository
            .commit_session_with_coverage(session, previous_revision, &snapshot, self.liveness)
            .await
    }
}
