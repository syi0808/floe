//! What an authorized read of a Person's own device source needs from its host.
//!
//! Context decides what a read is allowed to be and what its result owes its
//! provenance to. Where the grant record lives, and which driver asks the
//! device, are the host's; both reach this module as ports so the read itself
//! names neither a vault nor a native bridge.

use floe_access::{ConsumerPolicyAuthority, DataAccessGrant, FeasibilityGrantQuery, GrantId};
use floe_agent_contract::{AgentFailure, BoxFuture, PersonId};
use floe_execution::Cancellation;
use serde_json::Value;
use tokio::time::Instant;
use uuid::Uuid;

/// The grant records a personal read runs against.
pub trait PersonalGrantRecords: Sync {
    fn grants<'a>(&'a self) -> BoxFuture<'a, Result<Vec<DataAccessGrant>, AgentFailure>>;

    /// The device subject the Person reviewed this grant against.
    fn reviewed_subject<'a>(&'a self, grant: GrantId) -> BoxFuture<'a, Result<String, AgentFailure>>;

    /// The consumer policy recorded with the grant.
    fn consumer_policy<'a>(
        &'a self,
        grant: GrantId,
    ) -> BoxFuture<'a, Result<ConsumerPolicyAuthority, AgentFailure>>;

    /// The query a feasibility grant admits.
    fn feasibility_query<'a>(
        &'a self,
        grant: GrantId,
    ) -> BoxFuture<'a, Result<FeasibilityGrantQuery, AgentFailure>>;
}

/// Which of the Person's own domains a read asks for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PersonalDomain {
    People,
    Feasibility,
    Wellbeing,
}

/// One acquisition the device driver is asked to perform.
pub struct PersonalAcquisition<'a> {
    pub person_id: PersonId,
    pub device_id: &'a str,
    pub host_epoch: String,
    pub domain: PersonalDomain,
    pub selected_handles: Vec<String>,
    pub feasibility: Option<&'a FeasibilityGrantQuery>,
    /// The device subject this read insists answered it.
    pub expected_subject: String,
    pub deadline: Instant,
}

/// One attention acquisition.
pub struct AttentionAcquisition<'a> {
    pub person_id: PersonId,
    pub device_id: &'a str,
    pub host_epoch: String,
    pub expected_subject: String,
    pub deadline: Instant,
}

/// What the device answered, and which subject answered it — before the read
/// and after it.
pub struct AcquiredSource {
    pub view: Option<Value>,
    pub subject_before: String,
    pub subject_after: String,
}

/// The device driver one personal read asks, and the observations it keeps.
pub trait PersonalSourceDriver: Sync {
    /// The acquisition host this process is registered as.
    fn personal_host_epoch(&self, person_id: PersonId) -> Result<String, AgentFailure>;

    fn attention_host_epoch(&self, person_id: PersonId) -> Result<String, AgentFailure>;

    /// This host process, as every observation it commits is bound to it.
    fn process_incarnation(&self) -> Uuid;

    fn acquire<'a>(
        &'a self,
        request: PersonalAcquisition<'a>,
        cancellation: Cancellation,
    ) -> BoxFuture<'a, Result<AcquiredSource, AgentFailure>>;

    fn acquire_attention<'a>(
        &'a self,
        request: AttentionAcquisition<'a>,
        cancellation: Cancellation,
    ) -> BoxFuture<'a, Result<AcquiredSource, AgentFailure>>;

    /// Record the observation a personal read depends on, so a later turn can
    /// ask whether it still holds.
    #[allow(clippy::too_many_arguments)]
    fn commit_personal_observation(
        &self,
        person_id: PersonId,
        device_id: &str,
        observation_id: Uuid,
        process_incarnation_id: Uuid,
        native_subject_fingerprint: &str,
        observed_at_unix_ms: i64,
        expires_at_unix_ms: i64,
        query_fingerprint: Vec<u8>,
    ) -> Result<(), AgentFailure>;

    /// Record the attention projection this read depends on, returning the
    /// observation and process it was recorded under.
    fn commit_attention_projection(
        &self,
        person_id: PersonId,
        host_epoch: &str,
        device_id: &str,
        view: &crate::AttentionView,
        native_subject_fingerprint: &str,
    ) -> Result<(Uuid, Uuid), AgentFailure>;
}
