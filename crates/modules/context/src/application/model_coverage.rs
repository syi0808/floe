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
/// anything; a dependency that fails re-admission with `PolicyDenied` or
/// `AccessReviewRequired` denies its turn rather than failing the read. A
/// revoked, paused, or never-reviewed grant no longer authorizes showing the
/// derived content, but the turn itself can still proceed without it.
/// Cancellation, deadlines, and infrastructure failures still fail the read.
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
                    Err(
                        AgentFailure::PolicyDenied | AgentFailure::AccessReviewRequired,
                    ) => Ok(false),
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
    use chrono::{Duration, Utc};
    use floe_context_contract::PersonId;
    use floe_context_contract::{
        ConnectionId, ConnectorId, ConsumerPolicyAuthority, ExecutionOwnerId, GrantAuthority,
        GrantConsumer, GrantDataCategory, GrantId, GrantOperation, GrantPurpose,
        GrantSourceBinding, ProcessingRestriction, ResourceHandle, SourceAuthority,
    };
    use floe_execution::Cancellation;
    use tokio::time::Instant;

    use super::*;

    fn dependency(
        person: PersonId,
        connector: &str,
        processing: ProcessingRestriction,
    ) -> ContextDependency {
        let source = GrantSourceBinding::try_new(
            person,
            ConnectionId::try_new("connection").unwrap(),
            ConnectorId::try_new(connector).unwrap(),
            ExecutionOwnerId::try_new("owner").unwrap(),
            SourceAuthority::new(),
        )
        .unwrap();
        let now = Utc::now();
        ContextDependency::try_new(
            person,
            GrantId::new(),
            GrantAuthority::new(),
            source,
            vec![ResourceHandle::try_new("resource").unwrap()],
            vec![GrantDataCategory::Metadata],
            GrantOperation::Read,
            GrantPurpose::Assistant,
            GrantConsumer::builtin("assistant").unwrap(),
            processing,
            ConsumerPolicyAuthority::new(),
            Uuid::new_v4(),
            b"fingerprint".to_vec(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            now - Duration::minutes(1),
            now + Duration::minutes(5),
        )
        .unwrap()
    }

    fn personal_dependency(person: PersonId) -> ContextDependency {
        dependency(person, "contacts.apple", ProcessingRestriction::LocalOnly)
    }

    fn remote_dependency(person: PersonId) -> ContextDependency {
        dependency(
            person,
            "mail.remote",
            ProcessingRestriction::ApprovedRecipient {
                recipient: "gateway-local".into(),
                categories: vec![GrantDataCategory::Metadata],
            },
        )
    }

    fn authorization() -> DependencyAuthorization {
        DependencyAuthorization {
            deadline: Instant::now() + std::time::Duration::from_secs(30),
            cancellation: Cancellation::default(),
        }
    }

    struct StaticReader {
        coverage: DependencyCoverage,
    }

    impl EvidenceReader for StaticReader {
        fn read_turn_coverage(
            &self,
            _session_id: Uuid,
            _turn_id: Uuid,
        ) -> impl std::future::Future<Output = Result<DependencyCoverage, AgentFailure>> + Send
        {
            let coverage = self.coverage.clone();
            async move { Ok(coverage) }
        }
    }

    struct AcceptAll;

    impl DependencyResolver for AcceptAll {
        fn authorize<'a>(
            &'a self,
            _dependency: &'a ContextDependency,
            _request: &'a DependencyAuthorization,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), AgentFailure>> + Send + 'a>>
        {
            Box::pin(async move { Ok(()) })
        }
    }

    struct DenyAll;

    impl DependencyResolver for DenyAll {
        fn authorize<'a>(
            &'a self,
            _dependency: &'a ContextDependency,
            _request: &'a DependencyAuthorization,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), AgentFailure>> + Send + 'a>>
        {
            Box::pin(async move { Err(AgentFailure::PolicyDenied) })
        }
    }

    struct NeedsReview;

    impl DependencyResolver for NeedsReview {
        fn authorize<'a>(
            &'a self,
            _dependency: &'a ContextDependency,
            _request: &'a DependencyAuthorization,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), AgentFailure>> + Send + 'a>>
        {
            // A paused, revoked, or never-reviewed personal grant no longer
            // authorizes its derived history.
            Box::pin(async move { Err(AgentFailure::AccessReviewRequired) })
        }
    }

    #[tokio::test]
    async fn one_route_neutral_authorization_reauthorizes_personal_and_remote() {
        let person = PersonId::new();
        let personal = personal_dependency(person);
        let remote = remote_dependency(person);
        let coverage = DependencyCoverage::dependent(personal.clone())
            .unwrap()
            .merge(&DependencyCoverage::dependent(remote.clone()).unwrap())
            .unwrap();
        let reader = StaticReader { coverage };
        let session_id = Uuid::new_v4();
        let turn_id = Uuid::new_v4();

        let decisions =
            project_history(&reader, session_id, [turn_id], Some(&AcceptAll), &authorization())
                .await
                .unwrap();

        let decision = decisions.get(&turn_id).unwrap();
        assert!(decision.retain_derived);
        assert_eq!(decision.authorized_dependencies.len(), 2);
        assert!(decision.authorized_dependencies.contains(&personal));
        assert!(decision.authorized_dependencies.contains(&remote));
    }

    #[tokio::test]
    async fn revoked_dependency_denies_its_turn_without_failing_projection() {
        let person = PersonId::new();
        let coverage =
            DependencyCoverage::dependent(personal_dependency(person)).unwrap();
        let reader = StaticReader { coverage };
        let session_id = Uuid::new_v4();
        let turn_id = Uuid::new_v4();

        let decisions =
            project_history(&reader, session_id, [turn_id], Some(&DenyAll), &authorization())
                .await
                .unwrap();

        let decision = decisions.get(&turn_id).unwrap();
        assert!(!decision.retain_derived);
        assert!(decision.authorized_dependencies.is_empty());
    }

    #[tokio::test]
    async fn review_required_denies_its_turn_without_failing_projection() {
        let person = PersonId::new();
        let coverage =
            DependencyCoverage::dependent(personal_dependency(person)).unwrap();
        let reader = StaticReader { coverage };
        let session_id = Uuid::new_v4();
        let turn_id = Uuid::new_v4();

        let decisions =
            project_history(&reader, session_id, [turn_id], Some(&NeedsReview), &authorization())
                .await
                .unwrap();

        let decision = decisions.get(&turn_id).unwrap();
        assert!(!decision.retain_derived);
        assert!(decision.authorized_dependencies.is_empty());
    }
}
