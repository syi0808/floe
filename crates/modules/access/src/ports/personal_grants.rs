//! What reviewing a personal grant needs from outside Access.
//!
//! Two things: the device, to say which subject it is answering for right now,
//! and the Person's own store, to report what they already granted and to commit
//! what they just reviewed. Neither decides whether the review is admissible.

use floe_context_contract::{GrantAuthority, GrantId, GrantScope, GrantSourceBinding};
use floe_execution::Cancellation;
use floe_kernel::{AgentFailure, PersonId};
use tokio::time::Instant;
use uuid::Uuid;

use crate::application::personal_read::FeasibilityGrantQuery;
use crate::data_access_grant::DataAccessGrant;
use crate::ports::remote_grants::BoxFuture;

/// The subject the device reported on either side of one inspection.
///
/// A subject that moved between the two is a subject the Person did not review.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersonalSubjectEvidence {
    pub before: String,
    pub after: String,
}

/// Which personal source is being inspected, and under what query.
pub enum PersonalSubjectProbe<'a> {
    Attention,
    People { selected_handles: Vec<String> },
    Feasibility { query: &'a FeasibilityGrantQuery },
    Wellbeing,
}

/// The device this Person is reviewing access on.
pub trait PersonalSubjectInspector: Sync {
    /// Ask the device which subject it would answer for, without reading it.
    fn inspect<'a>(
        &'a self,
        person_id: PersonId,
        device_id: &'a str,
        probe: PersonalSubjectProbe<'a>,
        expected_native_subject_fingerprint: Option<String>,
        deadline: Option<Instant>,
        cancellation: Cancellation,
    ) -> BoxFuture<'a, Result<PersonalSubjectEvidence, AgentFailure>>;

    /// The process this device's live attention observation belongs to, if any.
    fn attention_presence(&self, person_id: PersonId, device_id: &str) -> Option<Uuid>;
}

/// The grant the Person already left, and where a reviewed one is committed.
pub trait PersonalGrantStore: Sync {
    fn grants<'a>(&'a self, limit: usize)
    -> BoxFuture<'a, Result<Vec<DataAccessGrant>, AgentFailure>>;

    fn reviewed_subject<'a>(&'a self, grant: GrantId)
    -> BoxFuture<'a, Result<String, AgentFailure>>;

    fn selected_handles<'a>(
        &'a self,
        grant: GrantId,
    ) -> BoxFuture<'a, Result<Vec<String>, AgentFailure>>;

    fn feasibility_query<'a>(
        &'a self,
        grant: GrantId,
    ) -> BoxFuture<'a, Result<FeasibilityGrantQuery, AgentFailure>>;

    /// Commit the review against the authority it expects, atomically.
    fn review_grant<'a>(
        &'a self,
        source: GrantSourceBinding,
        scope: GrantScope,
        native_subject_fingerprint: &'a str,
        expected: Option<(GrantId, GrantAuthority)>,
    ) -> BoxFuture<'a, Result<DataAccessGrant, AgentFailure>>;

    fn review_grant_with_selection<'a>(
        &'a self,
        source: GrantSourceBinding,
        scope: GrantScope,
        native_subject_fingerprint: &'a str,
        expected: Option<(GrantId, GrantAuthority)>,
        selected_handles: &'a [String],
    ) -> BoxFuture<'a, Result<DataAccessGrant, AgentFailure>>;

    fn review_grant_with_feasibility_query<'a>(
        &'a self,
        source: GrantSourceBinding,
        scope: GrantScope,
        native_subject_fingerprint: &'a str,
        expected: Option<(GrantId, GrantAuthority)>,
        query: FeasibilityGrantQuery,
    ) -> BoxFuture<'a, Result<DataAccessGrant, AgentFailure>>;

    fn pause_grant<'a>(
        &'a self,
        grant: GrantId,
        authority: GrantAuthority,
    ) -> BoxFuture<'a, Result<DataAccessGrant, AgentFailure>>;
}
