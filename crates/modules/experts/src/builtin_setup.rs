//! Keeping the Person's builtin Expert setup installed.
//!
//! Registry setup is source-independent: it records package topology only.
//! Source availability is discovered at read time by Context/Access.

use floe_agent_contract::AgentFailure;
use uuid::Uuid;

use crate::calendar_access::BoxFuture;
use crate::registry::{BuiltinExpertSetup, BuiltinExpertSetupResult};

/// When a setup that does not exist yet may be created.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuiltinExpertRefresh {
    /// Install the setup when the Person has none yet.
    InstallIfAbsent,
    /// Only validate an existing setup. A turn is not where a Person's
    /// Experts are first installed.
    ExistingOnly,
}

/// The Person's builtin Expert setup, as ensured and read.
pub trait BuiltinExpertStore: Sync {
    /// The registry instance this Person's setup belongs to.
    fn instance_id(&self) -> Uuid;

    /// The identity this Person's builtin setup is installed under.
    fn setup_id(&self) -> Uuid;

    fn overview<'a>(
        &'a self,
    ) -> BoxFuture<'a, Result<Option<BuiltinExpertSetupResult>, AgentFailure>>;

    /// The revision the registry is at, or zero when there is no registry yet.
    fn registry_revision<'a>(&'a self) -> BoxFuture<'a, Result<u64, AgentFailure>>;

    fn install<'a>(
        &'a self,
        setup: BuiltinExpertSetup,
    ) -> BoxFuture<'a, Result<BuiltinExpertSetupResult, AgentFailure>>;
}

/// Ensure the Person's builtin setup exists and matches the expected topology.
///
/// An existing setup is left alone. A racing writer that moved the revision
/// underneath is not an error the caller has to handle: the setup is read
/// again and validated against what it now says.
pub async fn ensure_builtin_experts(
    store: &impl BuiltinExpertStore,
    expected_assignments: usize,
    when: BuiltinExpertRefresh,
) -> Result<(), AgentFailure> {
    let existing = store.overview().await?;
    if existing.is_none() && when == BuiltinExpertRefresh::ExistingOnly {
        return Ok(());
    }
    let ensured = match &existing {
        Some(existing) => Ok(existing.clone()),
        None => {
            store
                .install(BuiltinExpertSetup {
                    instance_id: store.instance_id(),
                    expected_revision: store.registry_revision().await?,
                    setup_id: store.setup_id(),
                })
                .await
        }
    };
    let result = match ensured {
        Ok(result) => result,
        Err(AgentFailure::Conflict) => store
            .overview()
            .await?
            .ok_or(AgentFailure::Conflict)?,
        Err(failure) => return Err(failure),
    };
    if result.setup.assignments.len() != expected_assignments {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(())
}

/// What a failed ensure means for the run that needed it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExpertRefreshOutcome {
    /// The registry says what the run needs it to say.
    Ready,
    /// The registry is stale, but the run can go on without the refresh.
    Degraded(AgentFailure),
    /// Nothing the run does can stand on this registry.
    Fatal(AgentFailure),
}

/// Whether an ensure failure stops the run that needed it.
///
/// A cancelled run and an unreachable vault are the run's own ground giving
/// way. Everything else leaves the Person's Experts as they were, which is a
/// worse answer rather than no answer.
pub fn expert_refresh_outcome(result: Result<(), AgentFailure>) -> ExpertRefreshOutcome {
    match result {
        Ok(()) => ExpertRefreshOutcome::Ready,
        Err(
            failure @ (AgentFailure::Cancelled
            | AgentFailure::VaultUnavailable
            | AgentFailure::StorageUnavailable),
        ) => ExpertRefreshOutcome::Fatal(failure),
        Err(failure) => ExpertRefreshOutcome::Degraded(failure),
    }
}
