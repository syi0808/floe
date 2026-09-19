use chrono::Utc;
use floe_context_contract::{DataClass, DependencyCoverage, ModelPlacement, ProcessingRestriction};
use floe_kernel::AgentFailure;

use crate::ports::dependency_authorization::{DependencyAuthorization, DependencyResolver};
use crate::ports::model_dispatch::{
    ModelDispatchRecipientAuthority, ModelDispatchRequest, ModelDispatchTarget,
};

/// Admitted but not yet handed off to transport.
#[must_use = "a dispatch permit must be consumed at the provider handoff fence"]
pub struct ModelDispatchPermit<'resolver, 'authority, Resolver, Authority> {
    request: ModelDispatchRequest,
    resolver: &'resolver Resolver,
    authority: &'authority Authority,
}

/// Consumed immediately before transport; retained for post-response revalidation.
#[must_use = "a dispatch fence must be revalidated before the response is released"]
pub struct ModelDispatchFence<'resolver, 'authority, Resolver, Authority> {
    request: ModelDispatchRequest,
    resolver: &'resolver Resolver,
    authority: &'authority Authority,
}

pub async fn admit_model_dispatch<'resolver, 'authority, Resolver, Authority>(
    request: ModelDispatchRequest,
    resolver: &'resolver Resolver,
    authority: &'authority Authority,
) -> Result<ModelDispatchPermit<'resolver, 'authority, Resolver, Authority>, AgentFailure>
where
    Resolver: DependencyResolver,
    Authority: ModelDispatchRecipientAuthority,
{
    authorize_request(&request, resolver, authority).await?;
    Ok(ModelDispatchPermit {
        request,
        resolver,
        authority,
    })
}

pub async fn consume_model_dispatch<'resolver, 'authority, Resolver, Authority>(
    permit: ModelDispatchPermit<'resolver, 'authority, Resolver, Authority>,
) -> Result<ModelDispatchFence<'resolver, 'authority, Resolver, Authority>, AgentFailure>
where
    Resolver: DependencyResolver,
    Authority: ModelDispatchRecipientAuthority,
{
    let ModelDispatchPermit {
        request,
        resolver,
        authority,
    } = permit;
    // Repeats current authority/dependency/recipient checks immediately
    // before the provider handoff.
    authorize_request(&request, resolver, authority).await?;
    Ok(ModelDispatchFence {
        request,
        resolver,
        authority,
    })
}

pub async fn revalidate_model_dispatch<Resolver, Authority>(
    fence: &ModelDispatchFence<'_, '_, Resolver, Authority>,
) -> Result<(), AgentFailure>
where
    Resolver: DependencyResolver,
    Authority: ModelDispatchRecipientAuthority,
{
    // After the response, before it leaves Inference.
    authorize_request(&fence.request, fence.resolver, fence.authority).await
}

async fn authorize_request<Resolver, Authority>(
    request: &ModelDispatchRequest,
    resolver: &Resolver,
    authority: &Authority,
) -> Result<(), AgentFailure>
where
    Resolver: DependencyResolver,
    Authority: ModelDispatchRecipientAuthority,
{
    request.validate()?;
    if request.cancellation.is_cancelled() {
        return Err(AgentFailure::Cancelled);
    }
    if tokio::time::Instant::now() >= request.deadline {
        return Err(AgentFailure::DeadlineExceeded);
    }
    deny_forbidden_data_classes(request)?;
    match &request.target {
        ModelDispatchTarget::Device => authorize_device(request, resolver).await,
        ModelDispatchTarget::External { recipient } => {
            authorize_external(request, recipient, resolver, authority).await
        }
    }
}

fn deny_forbidden_data_classes(request: &ModelDispatchRequest) -> Result<(), AgentFailure> {
    if request
        .input_data_classes
        .iter()
        .any(|class| matches!(class, DataClass::Credential | DataClass::DeviceOnlyRaw))
    {
        return Err(AgentFailure::PolicyDenied);
    }
    if request.target.is_external()
        && request
            .input_data_classes
            .iter()
            .any(|class| matches!(class, DataClass::HighlySensitive))
    {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(())
}

async fn authorize_device<Resolver>(
    request: &ModelDispatchRequest,
    resolver: &Resolver,
) -> Result<(), AgentFailure>
where
    Resolver: DependencyResolver,
{
    match &request.coverage {
        DependencyCoverage::Unknown | DependencyCoverage::Independent => Ok(()),
        DependencyCoverage::Dependent { dependencies } => {
            let authorization = DependencyAuthorization {
                allowed_placements: vec![ModelPlacement::DeviceLocal],
                deadline: request.deadline,
                cancellation: request.cancellation.clone(),
            };
            for dependency in dependencies {
                check_dependency_identity(request, dependency)?;
                resolver.authorize(dependency, &authorization).await?;
            }
            Ok(())
        }
    }
}

async fn authorize_external<Resolver, Authority>(
    request: &ModelDispatchRequest,
    recipient: &str,
    resolver: &Resolver,
    authority: &Authority,
) -> Result<(), AgentFailure>
where
    Resolver: DependencyResolver,
    Authority: ModelDispatchRecipientAuthority,
{
    // Exact-recipient authority first: a revoked recipient denies before
    // dependency detail is consulted.
    authority.check_recipient(recipient)?;
    match &request.coverage {
        DependencyCoverage::Unknown => Err(AgentFailure::PolicyDenied),
        DependencyCoverage::Independent => Ok(()),
        DependencyCoverage::Dependent { dependencies } => {
            let authorization = DependencyAuthorization {
                allowed_placements: vec![ModelPlacement::Remote],
                deadline: request.deadline,
                cancellation: request.cancellation.clone(),
            };
            for dependency in dependencies {
                check_dependency_identity(request, dependency)?;
                resolver.authorize(dependency, &authorization).await?;
                check_processing_restriction(dependency, recipient)?;
            }
            Ok(())
        }
    }
}

fn check_dependency_identity(
    request: &ModelDispatchRequest,
    dependency: &floe_context_contract::ContextDependency,
) -> Result<(), AgentFailure> {
    if dependency.person_id() != request.person_id || dependency.expires_at() <= Utc::now() {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(())
}

fn check_processing_restriction(
    dependency: &floe_context_contract::ContextDependency,
    recipient: &str,
) -> Result<(), AgentFailure> {
    match dependency.processing() {
        ProcessingRestriction::LocalOnly => Err(AgentFailure::PolicyDenied),
        ProcessingRestriction::ApprovedRecipient {
            recipient: approved,
            categories,
        } => {
            if approved != recipient {
                return Err(AgentFailure::PolicyDenied);
            }
            let admitted = dependency
                .categories()
                .iter()
                .all(|category| categories.contains(category));
            admitted.then_some(()).ok_or(AgentFailure::PolicyDenied)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::sync::atomic::{AtomicBool, Ordering};

    use chrono::{Duration, Utc};
    use floe_context_contract::{
        ConnectionId, ConnectorId, ConsumerPolicyAuthority, ContextDependency, ExecutionOwnerId,
        GrantAuthority, GrantConsumer, GrantDataCategory, GrantId, GrantOperation, GrantPurpose,
        GrantSourceBinding, ProcessingRestriction, ResourceHandle, SourceAuthority,
    };
    use floe_execution::Cancellation;
    use floe_kernel::PersonId;
    use tokio::time::Instant;
    use uuid::Uuid;

    use super::*;
    use crate::ports::dependency_authorization::DependencyAuthorization;

    struct TestResolver {
        live: AtomicBool,
        seen: std::sync::Mutex<Vec<Vec<ModelPlacement>>>,
    }

    impl TestResolver {
        fn live() -> Self {
            Self {
                live: AtomicBool::new(true),
                seen: std::sync::Mutex::new(Vec::new()),
            }
        }

        fn revoke(&self) {
            self.live.store(false, Ordering::SeqCst);
        }
    }

    impl DependencyResolver for TestResolver {
        fn authorize<'a>(
            &'a self,
            _dependency: &'a ContextDependency,
            request: &'a DependencyAuthorization,
        ) -> std::pin::Pin<Box<dyn Future<Output = Result<(), AgentFailure>> + Send + 'a>>
        {
            Box::pin(async move {
                if !self.live.load(Ordering::SeqCst) {
                    return Err(AgentFailure::PolicyDenied);
                }
                self.seen
                    .lock()
                    .unwrap()
                    .push(request.allowed_placements.clone());
                Ok(())
            })
        }
    }

    struct TestAuthority {
        revoked: AtomicBool,
    }

    impl TestAuthority {
        fn live() -> Self {
            Self {
                revoked: AtomicBool::new(false),
            }
        }

        fn revoke(&self) {
            self.revoked.store(true, Ordering::SeqCst);
        }
    }

    impl ModelDispatchRecipientAuthority for TestAuthority {
        fn check_recipient(&self, recipient: &str) -> Result<(), AgentFailure> {
            if recipient.trim().is_empty() {
                return Err(AgentFailure::InvalidInput);
            }
            if self.revoked.load(Ordering::SeqCst) {
                return Err(AgentFailure::PolicyDenied);
            }
            Ok(())
        }
    }

    fn person() -> PersonId {
        PersonId::new()
    }

    fn dependency(
        person_id: PersonId,
        processing: ProcessingRestriction,
        categories: Vec<GrantDataCategory>,
    ) -> ContextDependency {
        let source = GrantSourceBinding::try_new(
            person_id,
            ConnectionId::try_new("connection").unwrap(),
            ConnectorId::try_new("connector").unwrap(),
            ExecutionOwnerId::try_new("owner").unwrap(),
            SourceAuthority::new(),
        )
        .unwrap();
        let now = Utc::now();
        ContextDependency::try_new(
            person_id,
            GrantId::new(),
            GrantAuthority::new(),
            source,
            vec![ResourceHandle::try_new("resource").unwrap()],
            categories,
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

    fn approved_dependency(person_id: PersonId, recipient: &str) -> ContextDependency {
        dependency(
            person_id,
            ProcessingRestriction::ApprovedRecipient {
                recipient: recipient.into(),
                categories: vec![GrantDataCategory::Metadata],
            },
            vec![GrantDataCategory::Metadata],
        )
    }

    fn base_request(person_id: PersonId, target: ModelDispatchTarget) -> ModelDispatchRequest {
        ModelDispatchRequest {
            person_id,
            projection_ref: Uuid::new_v4(),
            projection_revision: 1,
            coverage: DependencyCoverage::Independent,
            input_data_classes: vec![DataClass::Personal],
            purpose: "everyday_assistance".into(),
            consumer: "conversation.root".into(),
            target,
            deadline: Instant::now() + std::time::Duration::from_secs(30),
            cancellation: Cancellation::default(),
        }
    }

    async fn admit_consume_revalidate(
        request: ModelDispatchRequest,
        resolver: &TestResolver,
        authority: &TestAuthority,
    ) -> Result<(), AgentFailure> {
        let permit = admit_model_dispatch(request, resolver, authority).await?;
        let fence = consume_model_dispatch(permit).await?;
        revalidate_model_dispatch(&fence).await
    }

    #[tokio::test]
    async fn device_independent_is_accepted() {
        let person_id = person();
        let resolver = TestResolver::live();
        let authority = TestAuthority::live();
        let request = base_request(person_id, ModelDispatchTarget::Device);
        admit_consume_revalidate(request, &resolver, &authority)
            .await
            .unwrap();
        assert!(resolver.seen.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn external_exact_approved_recipient_is_accepted() {
        let person_id = person();
        let recipient = "gateway-local";
        let resolver = TestResolver::live();
        let authority = TestAuthority::live();
        let mut request = base_request(
            person_id,
            ModelDispatchTarget::External {
                recipient: recipient.into(),
            },
        );
        request.coverage =
            DependencyCoverage::dependent(approved_dependency(person_id, recipient)).unwrap();
        admit_consume_revalidate(request, &resolver, &authority)
            .await
            .unwrap();
        assert_eq!(
            resolver.seen.lock().unwrap().as_slice(),
            &[
                vec![ModelPlacement::Remote],
                vec![ModelPlacement::Remote],
                vec![ModelPlacement::Remote],
            ]
        );
    }

    #[tokio::test]
    async fn external_local_only_is_denied() {
        let person_id = person();
        let resolver = TestResolver::live();
        let authority = TestAuthority::live();
        let mut request = base_request(
            person_id,
            ModelDispatchTarget::External {
                recipient: "gateway-local".into(),
            },
        );
        request.coverage = DependencyCoverage::dependent(dependency(
            person_id,
            ProcessingRestriction::LocalOnly,
            vec![GrantDataCategory::Metadata],
        ))
        .unwrap();
        assert_eq!(
            admit_model_dispatch(request, &resolver, &authority)
                .await
                .err(),
            Some(AgentFailure::PolicyDenied)
        );
    }

    #[tokio::test]
    async fn external_recipient_mismatch_is_denied() {
        let person_id = person();
        let resolver = TestResolver::live();
        let authority = TestAuthority::live();
        let mut request = base_request(
            person_id,
            ModelDispatchTarget::External {
                recipient: "other-recipient".into(),
            },
        );
        request.coverage =
            DependencyCoverage::dependent(approved_dependency(person_id, "gateway-local")).unwrap();
        assert_eq!(
            admit_model_dispatch(request, &resolver, &authority)
                .await
                .err(),
            Some(AgentFailure::PolicyDenied)
        );
    }

    #[tokio::test]
    async fn external_category_mismatch_is_denied() {
        let person_id = person();
        let resolver = TestResolver::live();
        let authority = TestAuthority::live();
        let mut request = base_request(
            person_id,
            ModelDispatchTarget::External {
                recipient: "gateway-local".into(),
            },
        );
        // Dependency carries Content but the approved set only admits Metadata.
        request.coverage = DependencyCoverage::dependent(dependency(
            person_id,
            ProcessingRestriction::ApprovedRecipient {
                recipient: "gateway-local".into(),
                categories: vec![GrantDataCategory::Metadata],
            },
            vec![
                GrantDataCategory::Metadata,
                GrantDataCategory::Content,
            ],
        ))
        .unwrap();
        assert_eq!(
            admit_model_dispatch(request, &resolver, &authority)
                .await
                .err(),
            Some(AgentFailure::PolicyDenied)
        );
    }

    #[tokio::test]
    async fn stale_dependency_is_denied() {
        let person_id = person();
        let resolver = TestResolver::live();
        resolver.revoke();
        let authority = TestAuthority::live();
        let mut request = base_request(
            person_id,
            ModelDispatchTarget::External {
                recipient: "gateway-local".into(),
            },
        );
        request.coverage =
            DependencyCoverage::dependent(approved_dependency(person_id, "gateway-local")).unwrap();
        assert_eq!(
            admit_model_dispatch(request, &resolver, &authority)
                .await
                .err(),
            Some(AgentFailure::PolicyDenied)
        );
    }

    #[tokio::test]
    async fn external_unknown_coverage_is_denied() {
        let person_id = person();
        let resolver = TestResolver::live();
        let authority = TestAuthority::live();
        let mut request = base_request(
            person_id,
            ModelDispatchTarget::External {
                recipient: "gateway-local".into(),
            },
        );
        request.coverage = DependencyCoverage::Unknown;
        assert_eq!(
            admit_model_dispatch(request, &resolver, &authority)
                .await
                .err(),
            Some(AgentFailure::PolicyDenied)
        );
    }

    #[tokio::test]
    async fn credential_and_device_only_raw_are_denied() {
        for class in [DataClass::Credential, DataClass::DeviceOnlyRaw] {
            for target in [
                ModelDispatchTarget::Device,
                ModelDispatchTarget::External {
                    recipient: "gateway-local".into(),
                },
            ] {
                let person_id = person();
                let resolver = TestResolver::live();
                let authority = TestAuthority::live();
                let mut request = base_request(person_id, target);
                request.input_data_classes = vec![class];
                assert_eq!(
                    admit_model_dispatch(request, &resolver, &authority)
                        .await
                        .err(),
                    Some(AgentFailure::PolicyDenied),
                    "class {class:?} must never reach a model"
                );
            }
        }
    }

    #[tokio::test]
    async fn external_highly_sensitive_is_denied() {
        let person_id = person();
        let resolver = TestResolver::live();
        let authority = TestAuthority::live();
        let mut request = base_request(
            person_id,
            ModelDispatchTarget::External {
                recipient: "gateway-local".into(),
            },
        );
        request.input_data_classes = vec![DataClass::HighlySensitive];
        assert_eq!(
            admit_model_dispatch(request, &resolver, &authority)
                .await
                .err(),
            Some(AgentFailure::PolicyDenied)
        );
    }

    #[tokio::test]
    async fn revoke_between_admit_and_consume_is_denied() {
        let person_id = person();
        let resolver = TestResolver::live();
        let authority = TestAuthority::live();
        let request = base_request(person_id, ModelDispatchTarget::Device);
        let permit = admit_model_dispatch(request, &resolver, &authority)
            .await
            .unwrap();
        authority.revoke();
        // Device never consults recipient authority, so revoke the resolver instead.
        resolver.revoke();
        let mut dependent = base_request(person_id, ModelDispatchTarget::Device);
        dependent.coverage = DependencyCoverage::dependent(dependency(
            person_id,
            ProcessingRestriction::LocalOnly,
            vec![GrantDataCategory::Metadata],
        ))
        .unwrap();
        assert_eq!(
            admit_model_dispatch(dependent, &resolver, &authority)
                .await
                .err(),
            Some(AgentFailure::PolicyDenied)
        );

        // The original Independent device permit still consumes because it has
        // no dependency to reauthorize; exercise the external revoke path instead.
        let external_person = person();
        let external_resolver = TestResolver::live();
        let external_authority = TestAuthority::live();
        let mut external = base_request(
            external_person,
            ModelDispatchTarget::External {
                recipient: "gateway-local".into(),
            },
        );
        external.coverage =
            DependencyCoverage::dependent(approved_dependency(external_person, "gateway-local"))
                .unwrap();
        let external_permit =
            admit_model_dispatch(external, &external_resolver, &external_authority)
                .await
                .unwrap();
        external_authority.revoke();
        assert_eq!(
            consume_model_dispatch(external_permit).await.err(),
            Some(AgentFailure::PolicyDenied)
        );

        // Original device permit consumes without recipient authority.
        consume_model_dispatch(permit).await.unwrap();
    }

    #[tokio::test]
    async fn revoke_after_handoff_before_response_is_denied() {
        let person_id = person();
        let resolver = TestResolver::live();
        let authority = TestAuthority::live();
        let mut request = base_request(
            person_id,
            ModelDispatchTarget::External {
                recipient: "gateway-local".into(),
            },
        );
        request.coverage =
            DependencyCoverage::dependent(approved_dependency(person_id, "gateway-local")).unwrap();
        let permit = admit_model_dispatch(request, &resolver, &authority)
            .await
            .unwrap();
        let fence = consume_model_dispatch(permit).await.unwrap();
        authority.revoke();
        assert_eq!(
            revalidate_model_dispatch(&fence).await.err(),
            Some(AgentFailure::PolicyDenied)
        );
    }
}
