use chrono::Utc;
use floe_context_contract::{
    DataClass, DependencyCoverage, ProcessingRequirement, ProcessingRestriction,
    ProcessingSourceScope,
};
use floe_kernel::AgentFailure;

use crate::ports::dependency_authorization::{DependencyAuthorization, DependencyResolver};
use crate::ports::model_dispatch::{
    ModelDispatchDenial, ModelDispatchRecipientAuthority, ModelDispatchRequest,
    ModelDispatchTarget, RecipientCheckOutcome,
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
) -> Result<ModelDispatchPermit<'resolver, 'authority, Resolver, Authority>, ModelDispatchDenial>
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
) -> Result<ModelDispatchFence<'resolver, 'authority, Resolver, Authority>, ModelDispatchDenial>
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
    // After the response, before it leaves Inference. Any denial here
    // suppresses the already-transmitted response; a missing consent never
    // becomes a re-review for transmitted bytes.
    authorize_request(&fence.request, fence.resolver, fence.authority)
        .await
        .map_err(ModelDispatchDenial::into_hard)
}

async fn authorize_request<Resolver, Authority>(
    request: &ModelDispatchRequest,
    resolver: &Resolver,
    authority: &Authority,
) -> Result<(), ModelDispatchDenial>
where
    Resolver: DependencyResolver,
    Authority: ModelDispatchRecipientAuthority,
{
    request.validate().map_err(ModelDispatchDenial::Hard)?;
    if request.cancellation.is_cancelled() {
        return Err(ModelDispatchDenial::Hard(AgentFailure::Cancelled));
    }
    if tokio::time::Instant::now() >= request.deadline {
        return Err(ModelDispatchDenial::Hard(AgentFailure::DeadlineExceeded));
    }
    deny_forbidden_data_classes(request).map_err(ModelDispatchDenial::Hard)?;
    match &request.target {
        ModelDispatchTarget::Device => authorize_device(request, resolver)
            .await
            .map_err(ModelDispatchDenial::Hard),
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
                deadline: request.deadline,
                cancellation: request.cancellation.clone(),
            };
            for dependency in dependencies {
                check_dependency_identity(request, dependency)?;
                resolver.authorize(dependency, &authorization).await?;
                check_device_processing(dependency)?;
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
) -> Result<(), ModelDispatchDenial>
where
    Resolver: DependencyResolver,
    Authority: ModelDispatchRecipientAuthority,
{
    // Exact-recipient authority first: a hard denial (revoked recipient,
    // failed pairing, missing lineage, store failure) denies before
    // dependency detail is consulted.
    let granted = match authority.check_recipient(request).await {
        Ok(RecipientCheckOutcome::Granted) => true,
        Ok(RecipientCheckOutcome::Missing) => false,
        Err(failure) => return Err(ModelDispatchDenial::Hard(failure)),
    };
    // A missing consent is reviewable only when the route is otherwise
    // admissible: validate the current source/dependency and class
    // restrictions before deriving any requirement. Dependency failures
    // keep their original hard failure; only the consent absence itself
    // is recoverable.
    match &request.coverage {
        DependencyCoverage::Unknown => return Err(hard()),
        DependencyCoverage::Independent => {}
        DependencyCoverage::Dependent { dependencies } => {
            let authorization = DependencyAuthorization {
                deadline: request.deadline,
                cancellation: request.cancellation.clone(),
            };
            for dependency in dependencies {
                check_dependency_identity(request, dependency)
                    .map_err(ModelDispatchDenial::Hard)?;
                resolver
                    .authorize(dependency, &authorization)
                    .await
                    .map_err(ModelDispatchDenial::Hard)?;
                check_processing_restriction(dependency, recipient)
                    .map_err(ModelDispatchDenial::Hard)?;
            }
        }
    }
    if granted {
        return Ok(());
    }
    let Some(lineage) = request.lineage else {
        // No lineage, no review: callers without Conversation lineage
        // (learner, provider smoke) fail closed without a card.
        return Err(hard());
    };
    let requirement = processing_requirement(request, recipient, lineage).map_err(|_| hard())?;
    Err(ModelDispatchDenial::NeedsConsent(requirement))
}

fn hard() -> ModelDispatchDenial {
    ModelDispatchDenial::Hard(AgentFailure::PolicyDenied)
}

/// The reviewable requirement for one otherwise admissible dispatch,
/// derived from the actual request: exact recipient, reviewed profile,
/// purpose/consumer, data classes, source scopes, projection identity
/// (audit), and lineage. Never called for prohibited input.
fn processing_requirement(
    request: &ModelDispatchRequest,
    recipient: &str,
    lineage: floe_context_contract::RecipientLineage,
) -> Result<ProcessingRequirement, AgentFailure> {
    let scopes = match &request.coverage {
        DependencyCoverage::Unknown | DependencyCoverage::Independent => vec![],
        DependencyCoverage::Dependent { dependencies } => dependencies
            .iter()
            .map(ProcessingSourceScope::from_dependency)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| AgentFailure::InvalidInput)?,
    };
    ProcessingRequirement::try_new(
        recipient.to_owned(),
        request.profile_id.clone(),
        request.purpose.clone(),
        request.consumer.clone(),
        request.input_data_classes.clone(),
        scopes,
        request.projection_ref,
        request.projection_revision,
        lineage,
    )
    .map_err(|_| AgentFailure::InvalidInput)
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

/// A Device target admits only device-local processing: reauthorization is
/// route-neutral, so dispatch itself denies anything approved for an external
/// recipient.
fn check_device_processing(
    dependency: &floe_context_contract::ContextDependency,
) -> Result<(), AgentFailure> {
    match dependency.processing() {
        ProcessingRestriction::LocalOnly => Ok(()),
        ProcessingRestriction::ApprovedRecipient { .. } => Err(AgentFailure::PolicyDenied),
    }
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
        calls: std::sync::atomic::AtomicUsize,
    }

    impl TestResolver {
        fn live() -> Self {
            Self {
                live: AtomicBool::new(true),
                calls: std::sync::atomic::AtomicUsize::new(0),
            }
        }

        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }

        fn revoke(&self) {
            self.live.store(false, Ordering::SeqCst);
        }
    }

    impl DependencyResolver for TestResolver {
        fn authorize<'a>(
            &'a self,
            _dependency: &'a ContextDependency,
            _request: &'a DependencyAuthorization,
        ) -> std::pin::Pin<Box<dyn Future<Output = Result<(), AgentFailure>> + Send + 'a>> {
            Box::pin(async move {
                if !self.live.load(Ordering::SeqCst) {
                    return Err(AgentFailure::PolicyDenied);
                }
                // Route-neutral: the resolver learns the deadline window only,
                // never the Device/External target being dispatched to.
                self.calls.fetch_add(1, Ordering::SeqCst);
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
        fn check_recipient<'a>(
            &'a self,
            request: &'a ModelDispatchRequest,
        ) -> std::pin::Pin<
            Box<dyn Future<Output = Result<RecipientCheckOutcome, AgentFailure>> + Send + 'a>,
        > {
            Box::pin(async move {
                let recipient = request.target.recipient().unwrap_or_default();
                if recipient.trim().is_empty() {
                    return Err(AgentFailure::InvalidInput);
                }
                if self.revoked.load(Ordering::SeqCst) {
                    return Err(AgentFailure::PolicyDenied);
                }
                Ok(RecipientCheckOutcome::Granted)
            })
        }
    }

    struct MissingAuthority;

    impl ModelDispatchRecipientAuthority for MissingAuthority {
        fn check_recipient<'a>(
            &'a self,
            _request: &'a ModelDispatchRequest,
        ) -> std::pin::Pin<
            Box<dyn Future<Output = Result<RecipientCheckOutcome, AgentFailure>> + Send + 'a>,
        > {
            Box::pin(async move { Ok(RecipientCheckOutcome::Missing) })
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
            profile_id: "server-model".into(),
            target,
            lineage: Some(
                floe_context_contract::RecipientLineage::try_new(Uuid::new_v4(), Uuid::new_v4())
                    .unwrap(),
            ),
            deadline: Instant::now() + std::time::Duration::from_secs(30),
            cancellation: Cancellation::default(),
        }
    }

    async fn admit_consume_revalidate(
        request: ModelDispatchRequest,
        resolver: &TestResolver,
        authority: &TestAuthority,
    ) -> Result<(), ModelDispatchDenial> {
        let permit = admit_model_dispatch(request, resolver, authority).await?;
        let fence = consume_model_dispatch(permit).await?;
        revalidate_model_dispatch(&fence)
            .await
            .map_err(ModelDispatchDenial::Hard)
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
        assert_eq!(resolver.calls(), 0);
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
        // Admit, consume and post-response revalidation each reauthorize.
        assert_eq!(resolver.calls(), 3);
    }

    #[tokio::test]
    async fn device_local_only_dependency_is_accepted() {
        let person_id = person();
        let resolver = TestResolver::live();
        let authority = TestAuthority::live();
        let mut request = base_request(person_id, ModelDispatchTarget::Device);
        request.coverage = DependencyCoverage::dependent(dependency(
            person_id,
            ProcessingRestriction::LocalOnly,
            vec![GrantDataCategory::Metadata],
        ))
        .unwrap();
        admit_consume_revalidate(request, &resolver, &authority)
            .await
            .unwrap();
        assert_eq!(resolver.calls(), 3);
    }

    #[tokio::test]
    async fn device_approved_recipient_dependency_is_denied() {
        let person_id = person();
        let resolver = TestResolver::live();
        let authority = TestAuthority::live();
        let mut request = base_request(person_id, ModelDispatchTarget::Device);
        request.coverage =
            DependencyCoverage::dependent(approved_dependency(person_id, "gateway-local")).unwrap();
        assert_eq!(
            admit_model_dispatch(request, &resolver, &authority)
                .await
                .err(),
            Some(ModelDispatchDenial::Hard(AgentFailure::PolicyDenied))
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
            Some(ModelDispatchDenial::Hard(AgentFailure::PolicyDenied))
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
            Some(ModelDispatchDenial::Hard(AgentFailure::PolicyDenied))
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
            vec![GrantDataCategory::Metadata, GrantDataCategory::Content],
        ))
        .unwrap();
        assert_eq!(
            admit_model_dispatch(request, &resolver, &authority)
                .await
                .err(),
            Some(ModelDispatchDenial::Hard(AgentFailure::PolicyDenied))
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
            Some(ModelDispatchDenial::Hard(AgentFailure::PolicyDenied))
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
            Some(ModelDispatchDenial::Hard(AgentFailure::PolicyDenied))
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
                    Some(ModelDispatchDenial::Hard(AgentFailure::PolicyDenied)),
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
            Some(ModelDispatchDenial::Hard(AgentFailure::PolicyDenied))
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
            Some(ModelDispatchDenial::Hard(AgentFailure::PolicyDenied))
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
            Some(ModelDispatchDenial::Hard(AgentFailure::PolicyDenied))
        );

        // Original device permit consumes without recipient authority.
        let fence = consume_model_dispatch(permit).await.unwrap();
        revalidate_model_dispatch(&fence).await.unwrap();
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

    #[tokio::test]
    async fn missing_consent_on_admissible_independent_route_is_reviewable() {
        let person_id = person();
        let resolver = TestResolver::live();
        let authority = MissingAuthority;
        let request = base_request(
            person_id,
            ModelDispatchTarget::External {
                recipient: "gateway-local".into(),
            },
        );
        let denial = admit_model_dispatch(request, &resolver, &authority)
            .await
            .err()
            .unwrap();
        let ModelDispatchDenial::NeedsConsent(requirement) = denial else {
            panic!("admissible route must be reviewable: {denial:?}");
        };
        assert_eq!(requirement.recipient(), "gateway-local");
        assert_eq!(requirement.profile_id(), "server-model");
        assert_eq!(requirement.purpose(), "everyday_assistance");
        assert_eq!(requirement.consumer(), "conversation.root");
        assert!(requirement.source_scopes().is_empty());
        assert!(requirement.validate().is_ok());
    }

    #[tokio::test]
    async fn missing_consent_on_admissible_dependent_route_binds_scope() {
        let person_id = person();
        let resolver = TestResolver::live();
        let authority = MissingAuthority;
        let mut request = base_request(
            person_id,
            ModelDispatchTarget::External {
                recipient: "gateway-local".into(),
            },
        );
        request.coverage =
            DependencyCoverage::dependent(approved_dependency(person_id, "gateway-local")).unwrap();
        let denial = admit_model_dispatch(request, &resolver, &authority)
            .await
            .err()
            .unwrap();
        let ModelDispatchDenial::NeedsConsent(requirement) = denial else {
            panic!("admissible route must be reviewable: {denial:?}");
        };
        assert_eq!(requirement.source_scopes().len(), 1);
        let scope = &requirement.source_scopes()[0];
        assert_eq!(scope.connection_id().as_str(), "connection");
        assert_eq!(scope.resources().len(), 1);
    }

    #[tokio::test]
    async fn prohibited_routes_stay_hard_when_consent_is_missing() {
        let person_id = person();
        let resolver = TestResolver::live();
        let authority = MissingAuthority;
        // LocalOnly-to-external is never reviewable.
        let mut local_only = base_request(
            person_id,
            ModelDispatchTarget::External {
                recipient: "gateway-local".into(),
            },
        );
        local_only.coverage = DependencyCoverage::dependent(dependency(
            person_id,
            ProcessingRestriction::LocalOnly,
            vec![GrantDataCategory::Metadata],
        ))
        .unwrap();
        assert_eq!(
            admit_model_dispatch(local_only, &resolver, &authority)
                .await
                .err(),
            Some(ModelDispatchDenial::Hard(AgentFailure::PolicyDenied))
        );
        // Recipient mismatch is never reviewable.
        let mut mismatch = base_request(
            person_id,
            ModelDispatchTarget::External {
                recipient: "other-recipient".into(),
            },
        );
        mismatch.coverage =
            DependencyCoverage::dependent(approved_dependency(person_id, "gateway-local")).unwrap();
        assert_eq!(
            admit_model_dispatch(mismatch, &resolver, &authority)
                .await
                .err(),
            Some(ModelDispatchDenial::Hard(AgentFailure::PolicyDenied))
        );
        // Unknown coverage is never reviewable.
        let mut unknown = base_request(
            person_id,
            ModelDispatchTarget::External {
                recipient: "gateway-local".into(),
            },
        );
        unknown.coverage = DependencyCoverage::Unknown;
        assert_eq!(
            admit_model_dispatch(unknown, &resolver, &authority)
                .await
                .err(),
            Some(ModelDispatchDenial::Hard(AgentFailure::PolicyDenied))
        );
        // Missing lineage is never reviewable.
        let mut unlined = base_request(
            person_id,
            ModelDispatchTarget::External {
                recipient: "gateway-local".into(),
            },
        );
        unlined.lineage = None;
        assert_eq!(
            admit_model_dispatch(unlined, &resolver, &authority)
                .await
                .err(),
            Some(ModelDispatchDenial::Hard(AgentFailure::PolicyDenied))
        );
        // Forbidden classes are never reviewable.
        let mut forbidden = base_request(
            person_id,
            ModelDispatchTarget::External {
                recipient: "gateway-local".into(),
            },
        );
        forbidden.input_data_classes = vec![DataClass::HighlySensitive];
        assert_eq!(
            admit_model_dispatch(forbidden, &resolver, &authority)
                .await
                .err(),
            Some(ModelDispatchDenial::Hard(AgentFailure::PolicyDenied))
        );
    }

    #[tokio::test]
    async fn missing_consent_after_handoff_suppresses_without_review() {
        let person_id = person();
        let resolver = TestResolver::live();
        let granted = TestAuthority::live();
        let mut request = base_request(
            person_id,
            ModelDispatchTarget::External {
                recipient: "gateway-local".into(),
            },
        );
        request.coverage =
            DependencyCoverage::dependent(approved_dependency(person_id, "gateway-local")).unwrap();
        let permit = admit_model_dispatch(request, &resolver, &granted)
            .await
            .unwrap();
        // Handoff fence holds the granted authority; revalidation below uses
        // a revoked authority to prove suppression maps to hard denial.
        let fence = consume_model_dispatch(permit).await.unwrap();
        granted.revoke();
        assert_eq!(
            revalidate_model_dispatch(&fence).await.err(),
            Some(AgentFailure::PolicyDenied)
        );
    }
}
