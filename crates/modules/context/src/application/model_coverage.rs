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
/// anything. A dependency that fails re-admission with `StaleContext` or
/// `AccessReviewRequired` denies only that historical turn's derived content.
/// `PolicyDenied`, malformed proof, cancellation, deadlines, and infrastructure
/// failures retain their failure semantics and fail the read.
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
                    Err(AgentFailure::StaleContext | AgentFailure::AccessReviewRequired) => {
                        Ok(false)
                    }
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

#[cfg(test)]
mod tests {
    use super::{project_history, revalidate_turn_coverage};
    use crate::ports::evidence_reader::EvidenceReader;
    use floe_access::{
        ConnectionId, DependencyAuthorization, DependencyResolver, ExecutionOwnerId,
        GrantAuthority, GrantConsumer, GrantDataCategory, GrantId, GrantOperation, GrantPurpose,
        GrantSourceBinding, ProcessingRestriction, ResourceHandle, SourceAuthority,
    };
    use floe_agent_contract::AgentFailure;
    use floe_context_contract::{ContextDependency, DependencyCoverage};
    use floe_execution::BoxFuture;
    use floe_kernel::PersonId;
    use std::{collections::BTreeMap, time::Duration};
    use uuid::Uuid;

    struct ScriptedEvidence(BTreeMap<Uuid, DependencyCoverage>);

    impl EvidenceReader for ScriptedEvidence {
        fn read_turn_coverage<'a>(
            &'a self,
            _session_id: Uuid,
            turn_id: Uuid,
        ) -> BoxFuture<'a, Result<DependencyCoverage, AgentFailure>> {
            Box::pin(async move {
                Ok(self
                    .0
                    .get(&turn_id)
                    .cloned()
                    .unwrap_or(DependencyCoverage::Unknown))
            })
        }
    }

    struct ScriptedResolver(AgentFailure);

    impl DependencyResolver for ScriptedResolver {
        fn authorize<'a>(
            &'a self,
            _dependency: &'a ContextDependency,
            _request: &'a DependencyAuthorization,
        ) -> BoxFuture<'a, Result<(), AgentFailure>> {
            Box::pin(async move { Err(self.0.clone()) })
        }
    }

    fn dependency(person_id: PersonId) -> ContextDependency {
        let source = GrantSourceBinding::try_new(
            person_id,
            ConnectionId::try_new("fixture.connection").expect("valid connection"),
            floe_access::ConnectorId::try_new("fixture.connector").expect("valid connector"),
            ExecutionOwnerId::try_new("fixture.owner").expect("valid execution owner"),
        )
        .expect("valid source binding");
        let now = chrono::Utc::now();
        ContextDependency::try_new(
            person_id,
            GrantId::new(),
            GrantAuthority::new(),
            source,
            vec![ResourceHandle::try_new("fixture.resource").expect("valid resource")],
            SourceAuthority::new(),
            vec![
                ResourceHandle::try_new("fixture.source-resource").expect("valid source resource"),
            ],
            vec![GrantDataCategory::Content],
            GrantOperation::Read,
            GrantPurpose::Assistant,
            GrantConsumer::builtin("fixture.manager").expect("valid consumer"),
            ProcessingRestriction::DeviceOnly,
            Uuid::new_v4(),
            vec![b'x'; 16],
            Uuid::new_v4(),
            Uuid::new_v4(),
            now,
            now + chrono::Duration::minutes(5),
        )
        .expect("valid source dependency")
    }

    fn authorization() -> DependencyAuthorization {
        DependencyAuthorization {
            deadline: tokio::time::Instant::now() + Duration::from_secs(5),
            cancellation: floe_execution::Cancellation::new(),
        }
    }

    fn dependent(dependency: ContextDependency) -> DependencyCoverage {
        DependencyCoverage::dependent(dependency).expect("valid dependent coverage")
    }

    #[tokio::test]
    async fn historical_stale_dependency_is_omitted_per_turn() {
        let stale_turn = Uuid::new_v4();
        let independent_turn = Uuid::new_v4();
        let reader = ScriptedEvidence(BTreeMap::from([
            (stale_turn, dependent(dependency(PersonId::new()))),
            (independent_turn, DependencyCoverage::Independent),
        ]));
        let resolver = ScriptedResolver(AgentFailure::StaleContext);
        let projected = project_history(
            &reader,
            Uuid::new_v4(),
            [stale_turn, independent_turn],
            Some(&resolver),
            &authorization(),
        )
        .await
        .expect("a stale historical observation removes only its own derived turn");

        assert!(!projected[&stale_turn].retain_derived);
        assert!(projected[&stale_turn].authorized_dependencies.is_empty());
        assert!(projected[&independent_turn].retain_derived);
    }

    #[tokio::test]
    async fn current_turn_required_source_failures_are_never_history_omissions() {
        for failure in [
            AgentFailure::StaleContext,
            AgentFailure::AccessReviewRequired,
            AgentFailure::PolicyDenied,
            AgentFailure::StorageUnavailable,
        ] {
            let resolver = ScriptedResolver(failure.clone());
            let actual = revalidate_turn_coverage(
                dependent(dependency(PersonId::new())),
                &resolver,
                &authorization(),
            )
            .await;
            assert_eq!(actual, Err(failure));
        }
    }

    #[tokio::test]
    async fn ambiguous_history_authority_and_storage_failures_are_not_suppressed() {
        let turn_id = Uuid::new_v4();
        let reader = ScriptedEvidence(BTreeMap::from([(
            turn_id,
            dependent(dependency(PersonId::new())),
        )]));
        for failure in [AgentFailure::PolicyDenied, AgentFailure::StorageUnavailable] {
            let resolver = ScriptedResolver(failure.clone());
            let actual = project_history(
                &reader,
                Uuid::new_v4(),
                [turn_id],
                Some(&resolver),
                &authorization(),
            )
            .await;
            assert_eq!(actual, Err(failure));
        }
    }
}
