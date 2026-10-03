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
