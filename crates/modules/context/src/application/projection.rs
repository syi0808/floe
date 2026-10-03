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
