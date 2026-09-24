use std::{future::Future, pin::Pin};

use floe_agent_contract::{AgentFailure, Cancellation};
use floe_context_contract::{
    AuthorizedSourceBinding, ContextDependency, GrantConsumer, GrantPurpose, GrantScope, PersonId,
    SourceReadOutcome,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::time::Instant;
use uuid::Uuid;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SourceKey(String);

impl SourceKey {
    pub fn try_new(value: impl Into<String>) -> Result<Self, AgentFailure> {
        let value = value.into();
        if value.trim() != value || value.is_empty() || value.len() > 128 {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

pub struct SourceReadRequest {
    person_id: PersonId,
    source: SourceKey,
    consumer: GrantConsumer,
    purpose: GrantPurpose,
    query: Value,
    deadline: Instant,
    cancellation: Cancellation,
    process_incarnation_id: Uuid,
    query_fingerprint: Vec<u8>,
}

pub(crate) struct SourceReadRequestParts {
    pub person_id: PersonId,
    pub source_id: String,
    pub consumer: GrantConsumer,
    pub purpose: GrantPurpose,
    pub query: Value,
    pub deadline: Instant,
    pub cancellation: Cancellation,
    pub process_incarnation_id: Uuid,
}

impl SourceReadRequest {
    pub(crate) fn try_new(parts: SourceReadRequestParts) -> Result<Self, AgentFailure> {
        let source = SourceKey::try_new(parts.source_id)?;
        let query_bytes =
            serde_json::to_vec(&parts.query).map_err(|_| AgentFailure::InvalidInput)?;
        if parts.person_id.0.is_nil()
            || parts.consumer.identifier().is_empty()
            || query_bytes.len() > floe_context_contract::MAX_QUERY_FINGERPRINT_BYTES
            || parts.process_incarnation_id.is_nil()
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(Self {
            person_id: parts.person_id,
            source,
            consumer: parts.consumer,
            purpose: parts.purpose,
            query: parts.query,
            deadline: parts.deadline,
            cancellation: parts.cancellation,
            process_incarnation_id: parts.process_incarnation_id,
            query_fingerprint: Sha256::digest(query_bytes).to_vec(),
        })
    }

    pub fn person_id(&self) -> PersonId {
        self.person_id
    }

    pub fn source(&self) -> &SourceKey {
        &self.source
    }

    pub fn consumer(&self) -> &GrantConsumer {
        &self.consumer
    }

    pub fn purpose(&self) -> GrantPurpose {
        self.purpose
    }

    pub fn query(&self) -> &Value {
        &self.query
    }

    pub fn deadline(&self) -> Instant {
        self.deadline
    }

    pub fn cancellation(&self) -> &Cancellation {
        &self.cancellation
    }

    pub fn process_incarnation_id(&self) -> Uuid {
        self.process_incarnation_id
    }

    pub fn query_fingerprint(&self) -> &[u8] {
        &self.query_fingerprint
    }
}

#[derive(Debug)]
pub struct SourceRead {
    source: SourceKey,
    payload: Value,
    bindings: Vec<AuthorizedSourceBinding>,
}

impl SourceRead {
    pub fn new(
        source: SourceKey,
        payload: Value,
        dependency: ContextDependency,
        scope: GrantScope,
    ) -> Self {
        Self {
            source,
            payload,
            bindings: vec![AuthorizedSourceBinding { dependency, scope }],
        }
    }

    pub fn with_bindings(
        source: SourceKey,
        payload: Value,
        bindings: Vec<AuthorizedSourceBinding>,
    ) -> Self {
        Self {
            source,
            payload,
            bindings,
        }
    }

    pub fn payload(&self) -> &Value {
        &self.payload
    }

    pub fn source(&self) -> &SourceKey {
        &self.source
    }

    pub fn into_parts(self) -> (Value, Vec<AuthorizedSourceBinding>) {
        (self.payload, self.bindings)
    }

    pub fn bindings(&self) -> &[AuthorizedSourceBinding] {
        &self.bindings
    }
}

pub trait SourceReader: Send + Sync {
    /// Read one source view, or report why it cannot be read without the
    /// Person. A recoverable blocker keeps its typed requirement (and, for a
    /// Ready read, every authorizing binding); only hard failures raise.
    fn read<'a>(
        &'a self,
        request: &'a SourceReadRequest,
    ) -> Pin<
        Box<dyn Future<Output = Result<SourceReadOutcome<SourceRead>, AgentFailure>> + Send + 'a>,
    >;
}
