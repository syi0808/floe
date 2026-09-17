//! Keeping the Person's builtin Expert setup in step with the sources that can
//! answer for it.
//!
//! Which sources a device can serve is the caller's observation; what that means
//! for the registry — whether to install, refresh, or leave it alone, and
//! whether a failure to do so stops the run that needed it — is the registry's
//! own judgment.

use floe_agent_contract::AgentFailure;
use uuid::Uuid;

use crate::calendar_access::BoxFuture;
use crate::registry::{
    BuiltinExpertSetup, BuiltinExpertSetupResult, BuiltinSourceBinding, BuiltinSourceState,
};

/// What a device knows about whatever serves one builtin source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuiltinSourceEvidence {
    /// Something is serving the source right now.
    Serving,
    /// The Person has it, but has turned it off or taken it back.
    Withheld,
    /// Nothing serves it on this device.
    Absent,
}

impl BuiltinSourceEvidence {
    /// The state a binding carries for a source under this evidence.
    ///
    /// A withheld source is disabled rather than unavailable: the Person still
    /// has it, and turning it back on is theirs to do.
    pub const fn state(self) -> BuiltinSourceState {
        match self {
            Self::Serving => BuiltinSourceState::Available,
            Self::Withheld => BuiltinSourceState::Disabled,
            Self::Absent => BuiltinSourceState::Unavailable,
        }
    }
}

/// When a setup that does not exist yet may be created.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuiltinExpertRefresh {
    /// Install the setup when the Person has none yet.
    InstallIfAbsent,
    /// Only bring an existing setup up to date. A turn is not where a Person's
    /// Experts are first installed.
    ExistingOnly,
}

/// The Person's builtin Expert setup, as a refresh reads and writes it.
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

    fn refresh<'a>(
        &'a self,
        expected_revision: u64,
        sources: Vec<BuiltinSourceBinding>,
    ) -> BoxFuture<'a, Result<BuiltinExpertSetupResult, AgentFailure>>;
}

/// Bring the Person's builtin setup in line with the sources now serving them.
///
/// A setup already naming these sources is left alone. A racing writer that
/// moved the revision underneath is not an error the caller has to handle: the
/// setup is read again and brought up to date against what it now says.
pub async fn ensure_builtin_experts(
    store: &impl BuiltinExpertStore,
    sources: Vec<BuiltinSourceBinding>,
    expected_assignments: usize,
    when: BuiltinExpertRefresh,
) -> Result<(), AgentFailure> {
    let existing = store.overview().await?;
    if existing.is_none() && when == BuiltinExpertRefresh::ExistingOnly {
        return Ok(());
    }
    let ensured = match &existing {
        Some(existing) if existing.setup.sources == sources => Ok(existing.clone()),
        Some(existing) => {
            store
                .refresh(existing.registry.revision, sources.clone())
                .await
        }
        None => {
            store
                .install(BuiltinExpertSetup {
                    instance_id: store.instance_id(),
                    expected_revision: store.registry_revision().await?,
                    setup_id: store.setup_id(),
                    sources: sources.clone(),
                })
                .await
        }
    };
    let result = match ensured {
        Ok(result) => result,
        Err(AgentFailure::Conflict) => {
            let latest = store.overview().await?.ok_or(AgentFailure::Conflict)?;
            if latest.setup.sources == sources {
                latest
            } else {
                store.refresh(latest.registry.revision, sources).await?
            }
        }
        Err(failure) => return Err(failure),
    };
    if result.setup.assignments.len() != expected_assignments {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(())
}

/// What a failed refresh means for the run that needed it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExpertRefreshOutcome {
    /// The registry says what the run needs it to say.
    Ready,
    /// The registry is stale, but the run can go on without the refresh.
    Degraded(AgentFailure),
    /// Nothing the run does can stand on this registry.
    Fatal(AgentFailure),
}

/// Whether a refresh failure stops the run that needed it.
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
