//! Shared static source-processing projection checks. Access still revalidates
//! live source, memory and exact Gateway authority at dispatch and release.
use floe_agent_contract::{AgentFailure, DataClass, ProcessingBoundary};
use floe_context_contract::{DependencyCoverage, SourceAccessRequirement};

pub(super) fn source_processing_requirements(
    principal: &str,
    device_id: &str,
    boundary: ProcessingBoundary,
    coverage: &DependencyCoverage,
    input_data_classes: &[DataClass],
) -> Result<Vec<SourceAccessRequirement>, AgentFailure> {
    let mut blockers = Vec::new();
    match coverage {
        DependencyCoverage::Unknown
            if boundary == floe_agent_contract::ProcessingBoundary::Gateway =>
        {
            return Err(AgentFailure::PolicyDenied);
        }
        DependencyCoverage::Dependent { dependencies } => {
            for dependency in dependencies {
                if dependency.person_id().to_string() != principal
                    || dependency.expires_at() <= chrono::Utc::now()
                {
                    return Err(AgentFailure::PolicyDenied);
                }
                if dependency.source().connector().as_str() == floe_access::WELLBEING_CONNECTOR {
                    dependency.validate_health_transform(device_id, chrono::Utc::now())?;
                    if !input_data_classes.contains(&DataClass::HighlySensitive) {
                        return Err(AgentFailure::PolicyDenied);
                    }
                }
                if boundary == floe_agent_contract::ProcessingBoundary::Gateway
                    && !dependency
                        .processing()
                        .admits_gateway(dependency.categories())
                {
                    let blocker =
                        floe_context_contract::SourceAccessRequirement::from_processing_dependency(
                            dependency,
                        )
                        .map_err(|_| AgentFailure::InvalidInput)?;
                    if !blockers.contains(&blocker) {
                        blockers.push(blocker);
                    }
                }
            }
        }
        _ => {}
    }
    Ok(blockers)
}
