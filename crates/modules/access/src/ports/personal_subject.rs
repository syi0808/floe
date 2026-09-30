use floe_execution::Cancellation;
use floe_kernel::{AgentFailure, PersonId};
use tokio::time::Instant;
use uuid::Uuid;

use crate::ports::remote_grants::BoxFuture;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersonalSubjectEvidence {
    pub before: String,
    pub after: String,
}

pub enum PersonalSubjectProbe {
    Attention,
    People { selected_handles: Vec<String> },
    Wellbeing,
}

pub trait PersonalSubjectInspector: Sync {
    fn inspect<'a>(
        &'a self,
        person_id: PersonId,
        device_id: &'a str,
        probe: PersonalSubjectProbe,
        expected_native_subject_fingerprint: Option<String>,
        deadline: Option<Instant>,
        cancellation: Cancellation,
    ) -> BoxFuture<'a, Result<PersonalSubjectEvidence, AgentFailure>>;

    fn attention_presence(&self, person_id: PersonId, device_id: &str) -> Option<Uuid>;
}
