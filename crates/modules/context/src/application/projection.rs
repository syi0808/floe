use std::future::Future;

use floe_agent_contract::AgentFailure;
use floe_context_contract::{ContextDependency, DependencyCoverage};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoverageProjection {
    retain_derived: bool,
    authorized_dependencies: Vec<ContextDependency>,
}

impl CoverageProjection {
    pub fn retain_derived(&self) -> bool {
        self.retain_derived
    }

    pub fn authorized_dependencies(&self) -> &[ContextDependency] {
        &self.authorized_dependencies
    }
}

pub async fn project_coverage<Authorize, ProjectionFuture>(
    coverage: &DependencyCoverage,
    mut authorize: Authorize,
) -> Result<CoverageProjection, AgentFailure>
where
    Authorize: FnMut(ContextDependency) -> ProjectionFuture,
    ProjectionFuture: Future<Output = Result<bool, AgentFailure>>,
{
    coverage
        .validate()
        .map_err(|_| AgentFailure::InvalidInput)?;
    match coverage {
        DependencyCoverage::Independent => Ok(CoverageProjection {
            retain_derived: true,
            authorized_dependencies: vec![],
        }),
        DependencyCoverage::Unknown => Ok(CoverageProjection {
            retain_derived: false,
            authorized_dependencies: vec![],
        }),
        DependencyCoverage::Dependent { dependencies } => {
            let mut authorized_dependencies = Vec::with_capacity(dependencies.len());
            let mut retain_derived = true;
            for dependency in dependencies {
                if authorize(dependency.clone()).await? {
                    authorized_dependencies.push(dependency.clone());
                } else {
                    retain_derived = false;
                }
            }
            if !retain_derived {
                authorized_dependencies.clear();
            }
            Ok(CoverageProjection {
                retain_derived,
                authorized_dependencies,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use chrono::{TimeZone, Utc};
    use floe_context_contract::PersonId;
    use floe_context_contract::{
        ConsumerPolicyAuthority, GrantAuthority, GrantConsumer, GrantDataCategory, GrantId,
        GrantOperation, GrantPurpose, GrantSourceBinding, ProcessingRestriction, ResourceHandle,
    };
    use uuid::Uuid;

    use super::*;

    fn dependency(observation_id: Uuid) -> ContextDependency {
        let person = PersonId::new();
        let source = GrantSourceBinding::try_new(
            person,
            floe_context_contract::ConnectionId::try_new("connection").unwrap(),
            floe_context_contract::ConnectorId::try_new("connector").unwrap(),
            floe_context_contract::ExecutionOwnerId::try_new("owner").unwrap(),
            floe_context_contract::SourceAuthority::new(),
        )
        .unwrap();
        ContextDependency::try_new(
            person,
            GrantId::new(),
            GrantAuthority::new(),
            source,
            vec![ResourceHandle::try_new("calendar/a").unwrap()],
            vec![GrantDataCategory::Metadata],
            GrantOperation::Read,
            GrantPurpose::Scheduling,
            GrantConsumer::builtin("calendar").unwrap(),
            ProcessingRestriction::LocalOnly,
            ConsumerPolicyAuthority::new(),
            observation_id,
            b"history".to_vec(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2026, 1, 1, 0, 5, 0).unwrap(),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn independent_and_unknown_never_invoke_authorizer() {
        let calls = Arc::new(Mutex::new(0));
        let independent = project_coverage(&DependencyCoverage::Independent, {
            let calls = Arc::clone(&calls);
            move |_| {
                let calls = Arc::clone(&calls);
                async move {
                    *calls.lock().unwrap() += 1;
                    Ok(true)
                }
            }
        })
        .await
        .unwrap();
        let unknown = project_coverage(&DependencyCoverage::Unknown, {
            let calls = Arc::clone(&calls);
            move |_| {
                let calls = Arc::clone(&calls);
                async move {
                    *calls.lock().unwrap() += 1;
                    Ok(true)
                }
            }
        })
        .await
        .unwrap();
        assert!(independent.retain_derived());
        assert!(independent.authorized_dependencies().is_empty());
        assert!(!unknown.retain_derived());
        assert!(unknown.authorized_dependencies().is_empty());
        assert_eq!(*calls.lock().unwrap(), 0);
    }

    #[tokio::test]
    async fn all_dependencies_must_authorize_before_projection_retains_them() {
        let first = dependency(Uuid::new_v4());
        let second = dependency(Uuid::new_v4());
        let first_id = first.observation_id();
        let second_id = second.observation_id();
        let coverage = DependencyCoverage::dependent(first.clone())
            .unwrap()
            .merge(&DependencyCoverage::dependent(second.clone()).unwrap())
            .unwrap();
        let allowed = project_coverage(&coverage, |_| async { Ok(true) })
            .await
            .unwrap();
        assert!(allowed.retain_derived());
        let DependencyCoverage::Dependent { dependencies } = &coverage else {
            panic!("fixture must remain dependent");
        };
        assert_eq!(allowed.authorized_dependencies(), dependencies);
        let seen = Arc::new(Mutex::new(Vec::new()));
        let projection = project_coverage(&coverage, {
            let seen = Arc::clone(&seen);
            move |dependency| {
                let seen = Arc::clone(&seen);
                async move {
                    seen.lock().unwrap().push(dependency.observation_id());
                    Ok(dependency.observation_id() == first_id)
                }
            }
        })
        .await
        .unwrap();
        assert!(!projection.retain_derived());
        assert!(projection.authorized_dependencies().is_empty());
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 2);
        assert!(seen.contains(&first_id));
        assert!(seen.contains(&second_id));
    }

    #[tokio::test]
    async fn fatal_authorization_error_after_denial_is_returned() {
        let coverage = DependencyCoverage::dependent(dependency(Uuid::new_v4()))
            .unwrap()
            .merge(&DependencyCoverage::dependent(dependency(Uuid::new_v4())).unwrap())
            .unwrap();
        let mut calls = 0;
        assert_eq!(
            project_coverage(&coverage, |_| {
                calls += 1;
                std::future::ready(if calls == 1 {
                    Ok(false)
                } else {
                    Err(AgentFailure::VaultUnavailable)
                })
            })
            .await,
            Err(AgentFailure::VaultUnavailable)
        );
        assert_eq!(calls, 2);
    }

    #[tokio::test]
    async fn invalid_coverage_is_rejected_before_authorization() {
        let coverage = DependencyCoverage::Dependent {
            dependencies: vec![],
        };
        assert_eq!(
            project_coverage(&coverage, |_| async { panic!("must not authorize") }).await,
            Err(AgentFailure::InvalidInput)
        );
    }
}
