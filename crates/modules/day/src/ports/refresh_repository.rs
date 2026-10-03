use floe_execution::BoxFuture;
use crate::{DayError, RefreshAdmission, RefreshAdmissionResult, RefreshCommit, RefreshExecutorReplacement, RefreshLookup, RefreshRecord, RefreshTransition};

/// Every method is a short local transaction. No source/provider/native I/O.
pub trait DayRefreshRepository: Send + Sync {
    /// Rejoin by Person/device/command/digest before observing the mirror.
    /// For New, capture proven mirror absence or exact revision in the same transaction.
    fn admit_refresh<'a>(&'a self, admission: RefreshAdmission) -> BoxFuture<'a, Result<RefreshAdmissionResult, DayError>>;
    fn read_refresh<'a>(&'a self, lookup: RefreshLookup) -> BoxFuture<'a, Result<Option<RefreshRecord>, DayError>>;
    fn transition_refresh<'a>(&'a self, transition: RefreshTransition) -> BoxFuture<'a, Result<RefreshRecord, DayError>>;
    /// Compare complete current configured Calendar inventory, each successful
    /// source's exact record digest and absence of a source-operation fence,
    /// mirror expectation and refresh generation/revision. Atomically install
    /// mirror plus Completed. Any mismatch leaves the entire mirror unchanged.
    fn commit_refresh<'a>(&'a self, commit: RefreshCommit) -> BoxFuture<'a, Result<RefreshRecord, DayError>>;
    /// Mark old nonterminal work Interrupted; never reacquire or replay it.
    fn interrupt_refreshes<'a>(&'a self, replacement: RefreshExecutorReplacement) -> BoxFuture<'a, Result<Vec<RefreshRecord>, DayError>>;
}
