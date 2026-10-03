use crate::{
    DependencyAuthorization, DependencyResolver, GatewayAdmission, ModelDispatchRequest,
    ModelDispatchTarget,
};
use chrono::Utc;
use floe_context_contract::{DataClass, DependencyCoverage};
use floe_execution::ExecutionScope;
use floe_kernel::AgentFailure;

#[must_use = "consume the permit at the provider handoff fence"]
pub struct ModelDispatchPermit<'resolver, 'authority, 'scope, Resolver, Authority> {
    request: ModelDispatchRequest,
    resolver: &'resolver Resolver,
    authority: &'authority Authority,
    scope: &'scope ExecutionScope,
}
#[must_use = "revalidate the fence before releasing the response"]
pub struct ModelDispatchFence<'resolver, 'authority, 'scope, Resolver, Authority> {
    request: ModelDispatchRequest,
    resolver: &'resolver Resolver,
    authority: &'authority Authority,
    scope: &'scope ExecutionScope,
}
impl<Resolver, Authority> ModelDispatchFence<'_, '_, '_, Resolver, Authority> {
    pub fn target(&self) -> (&[u8; 32], &ModelDispatchTarget) {
        (&self.request.binding_digest, &self.request.target)
    }
}

pub async fn admit_model_dispatch<'r, 'a, 's, R: DependencyResolver, A: GatewayAdmission>(
    request: ModelDispatchRequest,
    resolver: &'r R,
    authority: &'a A,
    scope: &'s ExecutionScope,
) -> Result<ModelDispatchPermit<'r, 'a, 's, R, A>, AgentFailure> {
    authorize_request(&request, resolver, authority, scope).await?;
    Ok(ModelDispatchPermit {
        request,
        resolver,
        authority,
        scope,
    })
}
pub async fn consume_model_dispatch<'r, 'a, 's, R: DependencyResolver, A: GatewayAdmission>(
    permit: ModelDispatchPermit<'r, 'a, 's, R, A>,
) -> Result<ModelDispatchFence<'r, 'a, 's, R, A>, AgentFailure> {
    let ModelDispatchPermit {
        request,
        resolver,
        authority,
        scope,
    } = permit;
    authorize_request(&request, resolver, authority, scope).await?;
    Ok(ModelDispatchFence {
        request,
        resolver,
        authority,
        scope,
    })
}
pub async fn revalidate_model_dispatch<R: DependencyResolver, A: GatewayAdmission>(
    fence: &ModelDispatchFence<'_, '_, '_, R, A>,
) -> Result<(), AgentFailure> {
    authorize_request(&fence.request, fence.resolver, fence.authority, fence.scope).await
}
async fn authorize_request<R: DependencyResolver, A: GatewayAdmission>(
    request: &ModelDispatchRequest,
    resolver: &R,
    authority: &A,
    scope: &ExecutionScope,
) -> Result<(), AgentFailure> {
    request.validate()?;
    if request.cancellation.is_cancelled() || scope.cancellation().is_cancelled() {
        return Err(AgentFailure::Cancelled);
    }
    if request.deadline > scope.deadline() {
        return Err(AgentFailure::InvalidInput);
    }
    if tokio::time::Instant::now() >= request.deadline
        || tokio::time::Instant::now() >= scope.deadline()
    {
        return Err(AgentFailure::DeadlineExceeded);
    }
    if request.input_data_classes.iter().any(|class| {
        matches!(class, DataClass::Credential | DataClass::DeviceOnlyRaw)
            || (*class == DataClass::TemporaryAiContext && request.target.is_gateway())
    }) {
        return Err(AgentFailure::PolicyDenied);
    }
    match &request.coverage {
        DependencyCoverage::Unknown if request.target.is_gateway() => {
            Err(AgentFailure::PolicyDenied)
        }
        DependencyCoverage::Unknown | DependencyCoverage::Independent => Ok(()),
        DependencyCoverage::Dependent { dependencies } => {
            let authorization = DependencyAuthorization {
                deadline: request.deadline,
                cancellation: request.cancellation.clone(),
            };
            for dependency in dependencies {
                if dependency.person_id() != request.person_id
                    || dependency.expires_at() <= Utc::now()
                {
                    return Err(AgentFailure::PolicyDenied);
                }
                resolver.authorize(dependency, &authorization).await?;
                if dependency.source().connector().as_str() == crate::WELLBEING_CONNECTOR {
                    dependency
                        .validate_health_transform(&request.device_id, Utc::now())
                        .map_err(|_| AgentFailure::PolicyDenied)?;
                    if !request
                        .input_data_classes
                        .contains(&DataClass::HighlySensitive)
                    {
                        return Err(AgentFailure::PolicyDenied);
                    }
                }
                if request.target.is_gateway()
                    && !dependency
                        .processing()
                        .admits_gateway(dependency.categories())
                {
                    return Err(AgentFailure::PolicyDenied);
                }
            }
            Ok(())
        }
    }?;
    // Source revalidation may await native/provider I/O. Reload the exact model
    // transport after those awaits, immediately at this handoff/release fence.
    if let ModelDispatchTarget::Gateway { expected } = &request.target {
        if authority.admit(expected, scope).await? != *expected {
            return Err(AgentFailure::PolicyDenied);
        }
    }
    if request.cancellation.is_cancelled() || scope.cancellation().is_cancelled() {
        return Err(AgentFailure::Cancelled);
    }
    if tokio::time::Instant::now() >= request.deadline
        || tokio::time::Instant::now() >= scope.deadline()
    {
        return Err(AgentFailure::DeadlineExceeded);
    }
    Ok(())
}
