//! What a recorded turn's coverage says about showing it to a model again.
//!
//! Context reads the coverage a turn was committed under, re-admits every
//! dependency it names, and states the decision. Applying that decision to a
//! transcript — which messages survive, whether the replay still holds — is the
//! Session owner's, not this.

use std::collections::BTreeMap;

use floe_access::{DependencyAuthorization, DependencyResolver};
use floe_agent_contract::AgentFailure;
use floe_context_contract::{ContextDependency, DependencyCoverage};
use uuid::Uuid;

use crate::application::history::read_history_coverage;
use crate::application::projection::project_coverage;
use crate::ports::evidence_reader::EvidenceReader;

/// What one recorded turn may still contribute to a model input.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TurnCoverageDecision {
    /// Whether messages derived from a source may be shown again.
    pub retain_derived: bool,
    /// The dependencies that re-admitted, and so must be recorded against the
    /// current turn.
    pub authorized_dependencies: Vec<ContextDependency>,
}

/// Decide, per turn, what a history projection may keep.
///
/// A turn with no recorded coverage is `Unknown`, and `Unknown` never authorizes
/// anything; a dependency that fails re-admission with `PolicyDenied` denies its
/// turn rather than failing the read.
pub async fn project_history(
    reader: &impl EvidenceReader,
    session_id: Uuid,
    turn_ids: impl IntoIterator<Item = Uuid>,
    resolver: Option<&dyn DependencyResolver>,
    authorization: &DependencyAuthorization,
) -> Result<BTreeMap<Uuid, TurnCoverageDecision>, AgentFailure> {
    let turn_ids: Vec<_> = turn_ids.into_iter().collect();
    let coverage_by_turn =
        read_history_coverage(reader, session_id, turn_ids.iter().copied()).await?;
    let mut decisions = BTreeMap::new();
    for turn_id in turn_ids {
        if decisions.contains_key(&turn_id) {
            continue;
        }
        let coverage = coverage_by_turn
            .get(&turn_id)
            .cloned()
            .unwrap_or(DependencyCoverage::Unknown);
        let projection = project_coverage(&coverage, |dependency| async move {
            match resolver {
                Some(resolver) => match resolver.authorize(&dependency, authorization).await {
                    Ok(()) => Ok(true),
                    Err(AgentFailure::PolicyDenied) => Ok(false),
                    Err(error) => Err(error),
                },
                None => Ok(false),
            }
        })
        .await?;
        decisions.insert(
            turn_id,
            TurnCoverageDecision {
                retain_derived: projection.retain_derived(),
                authorized_dependencies: projection.authorized_dependencies().to_vec(),
            },
        );
    }
    Ok(decisions)
}

/// Re-admit the coverage a turn already committed, without changing it.
pub async fn revalidate_turn_coverage(
    coverage: DependencyCoverage,
    resolver: &dyn DependencyResolver,
    authorization: &DependencyAuthorization,
) -> Result<(), AgentFailure> {
    match coverage {
        DependencyCoverage::Independent => Ok(()),
        DependencyCoverage::Unknown => Err(AgentFailure::PolicyDenied),
        DependencyCoverage::Dependent { dependencies } => {
            for dependency in dependencies {
                resolver.authorize(&dependency, authorization).await?;
            }
            Ok(())
        }
    }
}
