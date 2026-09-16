//! The record one capability execution leaves behind, and where it is written.
//!
//! A capability call is durable intent before it is a result: the caller must
//! know a call was dispatched even if the answer never arrives. The record is
//! role-neutral, so a root Run and a delegated Expert leave the same shape.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{AgentFailure, BoxFuture};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityExecution {
    pub scope_id: Uuid,
    pub turn_id: Uuid,
    pub call_id: Uuid,
    pub capability_id: String,
    pub input: String,
    pub state: CapabilityExecutionState,
    pub result: Option<Result<String, AgentFailure>>,
    pub replay: Option<ProviderReplay>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityExecutionState {
    Started,
    Settled,
    Interrupted,
}

/// What the provider was actually asked, kept so the same call is never made
/// twice against an external system.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderReplay {
    pub call_ids: Vec<String>,
    pub preamble: String,
    pub gateway: String,
    pub purpose: String,
    pub external: bool,
    pub source: String,
    pub provider_call_id: String,
    pub items: serde_json::Value,
}

#[derive(Clone, Debug)]
pub struct ModelReplay {
    pub call_id: Uuid,
    pub replay: ProviderReplay,
}

/// Where a capability execution is durably recorded before it is dispatched.
///
/// The owner that keeps the record implements this; a journal that accepts
/// nothing still lets the call run, but it cannot claim the call was recorded.
pub trait CapabilityJournal: Send + Sync {
    fn record<'a>(
        &'a self,
        record: CapabilityExecution,
    ) -> BoxFuture<'a, Result<(), AgentFailure>>;
}
