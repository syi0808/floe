//! Applying Context's history decision to one model input.
//!
//! Context says, per recorded turn, what may still be shown; this is the
//! Session owner applying that: a Person's own message always stays, anything
//! derived from a turn that no longer re-admits is dropped, and a replay that
//! stood on the transcript that just changed goes with it.

use std::collections::BTreeMap;

use floe_agent_contract::{AgentFailure, ContextDependency, SourceHistoryBoundary};
use floe_context::TurnCoverageDecision;
use uuid::Uuid;

use crate::turn::{AgentMessage, ModelRequest};

/// What a projection is applied against.
pub struct HistoryProjection<'a> {
    pub decisions: &'a BTreeMap<Uuid, TurnCoverageDecision>,
    /// The turn being produced now. Its own messages are never projected away:
    /// the run is writing them, not recalling them.
    pub current_turn: Option<Uuid>,
}

/// A turn that re-admitted no dependency but still carries source-derived
/// results is not independent: whatever it says came from somewhere this run
/// may no longer read.
///
/// Which results carry source data is the owner's to say, through `boundary`.
pub fn narrow_by_source_boundary(
    decisions: &mut BTreeMap<Uuid, TurnCoverageDecision>,
    messages: &[AgentMessage],
    boundary: &dyn SourceHistoryBoundary,
) {
    for (turn_id, decision) in decisions.iter_mut() {
        if !decision.authorized_dependencies.is_empty() {
            continue;
        }
        let crosses = messages
            .iter()
            .filter(|message| message.turn_id() == *turn_id)
            .any(|message| crate::turn::carries_source_history(std::slice::from_ref(message), boundary));
        if crosses {
            decision.retain_derived = false;
        }
    }
}

/// Apply the decision to the transcript, and report every dependency the
/// current turn now stands on.
///
/// The replay is always discarded: a receipt describes the request that
/// produced it, and this request is not that one.
pub fn project_history_into(
    request: &mut ModelRequest,
    projection: HistoryProjection<'_>,
    mut record: impl FnMut(Uuid, &[ContextDependency]) -> Result<(), AgentFailure>,
) -> Result<bool, AgentFailure> {
    let messages = std::mem::take(&mut request.messages);
    let mut retained = Vec::with_capacity(messages.len());
    let mut filtered = false;
    let mut recorded: Vec<Uuid> = Vec::new();
    for message in messages {
        let turn_id = message.turn_id();
        if projection.current_turn == Some(turn_id) {
            retained.push(message);
            continue;
        }
        let decision = projection.decisions.get(&turn_id);
        let retain_derived = decision.is_some_and(|decision| decision.retain_derived);
        if retain_derived {
            if let Some(decision) = decision
                && !recorded.contains(&turn_id)
            {
                recorded.push(turn_id);
                record(turn_id, &decision.authorized_dependencies)?;
            }
            retained.push(message);
        } else if matches!(message, AgentMessage::User { .. }) {
            retained.push(message);
        } else {
            filtered = true;
        }
    }
    request.messages = retained;
    request.replay.clear();
    Ok(filtered)
}
