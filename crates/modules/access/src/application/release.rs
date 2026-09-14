use chrono::Utc;
use floe_context_contract::DependencyCoverage;
use floe_kernel::{AgentFailure, PersonId};
use uuid::Uuid;

use crate::ports::CurrentAuthority;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReleaseRecipient {
    Storage { person_id: PersonId, vault_id: Uuid },
}

impl ReleaseRecipient {
    fn validate(self, session_id: Uuid, session_revision: u64) -> Result<(), AgentFailure> {
        let Self::Storage {
            person_id,
            vault_id,
        } = self;
        if !person_id.is_valid()
            || vault_id.is_nil()
            || session_id.is_nil()
            || session_revision == 0
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }

    fn person_id(self) -> PersonId {
        match self {
            Self::Storage { person_id, .. } => person_id,
        }
    }
}

#[must_use = "a release permit must be consumed at the storage commit fence"]
pub struct ReleasePermit<'authority, Authority: CurrentAuthority + ?Sized> {
    recipient: ReleaseRecipient,
    session_id: Uuid,
    session_revision: u64,
    coverage: DependencyCoverage,
    authority: &'authority Authority,
}

pub async fn admit_release<'authority, Authority: CurrentAuthority + ?Sized>(
    coverage: &DependencyCoverage,
    recipient: ReleaseRecipient,
    session_id: Uuid,
    session_revision: u64,
    authority: &'authority Authority,
) -> Result<ReleasePermit<'authority, Authority>, AgentFailure> {
    recipient.validate(session_id, session_revision)?;
    authority.validate_target(recipient, session_id, session_revision)?;
    validate_release_coverage(coverage, recipient.person_id(), authority).await?;
    Ok(ReleasePermit {
        recipient,
        session_id,
        session_revision,
        coverage: coverage.clone(),
        authority,
    })
}

pub async fn consume_release<Authority: CurrentAuthority + ?Sized>(
    permit: ReleasePermit<'_, Authority>,
) -> Result<(), AgentFailure> {
    permit
        .recipient
        .validate(permit.session_id, permit.session_revision)?;
    permit.authority.validate_target(
        permit.recipient,
        permit.session_id,
        permit.session_revision,
    )?;
    validate_release_coverage(
        &permit.coverage,
        permit.recipient.person_id(),
        permit.authority,
    )
    .await
}

async fn validate_release_coverage<Authority: CurrentAuthority + ?Sized>(
    coverage: &DependencyCoverage,
    person_id: PersonId,
    authority: &Authority,
) -> Result<(), AgentFailure> {
    coverage
        .validate()
        .map_err(|_| AgentFailure::PolicyDenied)?;
    let DependencyCoverage::Dependent { dependencies } = coverage else {
        return Ok(());
    };
    for dependency in dependencies {
        if dependency.person_id() != person_id || dependency.expires_at() <= Utc::now() {
            return Err(AgentFailure::PolicyDenied);
        }
        authority.validate(dependency).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        future::Future,
        pin::Pin,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
    };

    use chrono::{Duration, Utc};
    use floe_context_contract::{
        ConnectionId, ConsumerPolicyAuthority, ContextDependency, ExecutionOwnerId, GrantAuthority,
        GrantConsumer, GrantDataCategory, GrantId, GrantOperation, GrantPurpose,
        GrantSourceBinding, ProcessingRestriction, ResourceHandle, SourceAuthority,
    };

    use super::*;

    struct Authority {
        target_ok: Arc<AtomicBool>,
        current_ok: Arc<AtomicBool>,
    }

    impl CurrentAuthority for Authority {
        fn validate_target(
            &self,
            _: ReleaseRecipient,
            _: Uuid,
            _: u64,
        ) -> Result<(), AgentFailure> {
            self.target_ok
                .load(Ordering::SeqCst)
                .then_some(())
                .ok_or(AgentFailure::VaultUnavailable)
        }

        fn validate<'a>(
            &'a self,
            _: &'a ContextDependency,
        ) -> Pin<Box<dyn Future<Output = Result<(), AgentFailure>> + Send + 'a>> {
            Box::pin(async move {
                self.current_ok
                    .load(Ordering::SeqCst)
                    .then_some(())
                    .ok_or(AgentFailure::PolicyDenied)
            })
        }
    }

    fn target() -> ReleaseRecipient {
        ReleaseRecipient::Storage {
            person_id: PersonId::new(),
            vault_id: Uuid::new_v4(),
        }
    }

    fn dependency(target: ReleaseRecipient) -> ContextDependency {
        let ReleaseRecipient::Storage { person_id, .. } = target;
        let source = GrantSourceBinding::try_new(
            person_id,
            ConnectionId::try_new("connection").unwrap(),
            floe_context_contract::ConnectorId::try_new("connector").unwrap(),
            ExecutionOwnerId::try_new("owner").unwrap(),
            SourceAuthority::new(),
        )
        .unwrap();
        ContextDependency::try_new(
            person_id,
            GrantId::new(),
            GrantAuthority::new(),
            source,
            vec![ResourceHandle::try_new("resource").unwrap()],
            vec![GrantDataCategory::Metadata],
            GrantOperation::Read,
            GrantPurpose::Assistant,
            GrantConsumer::builtin("assistant").unwrap(),
            ProcessingRestriction::LocalOnly,
            ConsumerPolicyAuthority::new(),
            Uuid::new_v4(),
            vec![1],
            Uuid::new_v4(),
            Uuid::new_v4(),
            Utc::now() - Duration::minutes(1),
            Utc::now() + Duration::minutes(5),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn consume_rechecks_the_same_authority() {
        let recipient = target();
        let authority = Authority {
            target_ok: Arc::new(AtomicBool::new(true)),
            current_ok: Arc::new(AtomicBool::new(true)),
        };
        let coverage = DependencyCoverage::dependent(dependency(recipient)).unwrap();
        let permit = admit_release(&coverage, recipient, Uuid::new_v4(), 1, &authority)
            .await
            .unwrap();
        authority.current_ok.store(false, Ordering::SeqCst);
        assert_eq!(
            consume_release(permit).await,
            Err(AgentFailure::PolicyDenied)
        );
    }

    #[tokio::test]
    async fn independent_and_unknown_are_storage_bound() {
        let recipient = target();
        let authority = Authority {
            target_ok: Arc::new(AtomicBool::new(true)),
            current_ok: Arc::new(AtomicBool::new(true)),
        };
        for coverage in [DependencyCoverage::Independent, DependencyCoverage::Unknown] {
            let permit = admit_release(&coverage, recipient, Uuid::new_v4(), 1, &authority)
                .await
                .unwrap();
            authority.target_ok.store(false, Ordering::SeqCst);
            assert_eq!(
                consume_release(permit).await,
                Err(AgentFailure::VaultUnavailable)
            );
            authority.target_ok.store(true, Ordering::SeqCst);
        }
    }

    #[tokio::test]
    async fn target_mismatch_is_denied_before_coverage() {
        let recipient = target();
        let authority = Authority {
            target_ok: Arc::new(AtomicBool::new(false)),
            current_ok: Arc::new(AtomicBool::new(true)),
        };
        let result = admit_release(
            &DependencyCoverage::Independent,
            recipient,
            Uuid::new_v4(),
            1,
            &authority,
        )
        .await;
        assert!(matches!(result, Err(AgentFailure::VaultUnavailable)));
    }
}
