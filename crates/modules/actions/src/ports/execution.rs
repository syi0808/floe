use floe_agent_contract::{DependencyCoverage, TaskExecutionReceiptRef};
use floe_connections::SourceConnection;
use floe_context_contract::{ConnectionId, ContextDependency};
use floe_day::Event;
use floe_execution::{BoxFuture, ExecutionScope};
use floe_kernel::{AgentFailure, OwnerActor, PersonId};
use uuid::Uuid;

use super::repository::DispatchAdmission;
use crate::ExpertCalendarProposal;
use crate::domain::record::*;

/// Source facts and the outstanding-operation barrier. This port grants no write permission.
pub trait ActionSourceReader: Send + Sync {
    fn list_calendar_sources<'a>(
        &'a self,
        person_id: PersonId,
    ) -> BoxFuture<'a, Result<Vec<SourceConnection>, AgentFailure>>;
    fn load<'a>(
        &'a self,
        person_id: PersonId,
        connection_id: &'a ConnectionId,
    ) -> BoxFuture<'a, Result<Option<SourceConnection>, AgentFailure>>;
    fn source_is_fenced<'a>(
        &'a self,
        person_id: PersonId,
        connection_id: &'a ConnectionId,
    ) -> BoxFuture<'a, Result<bool, AgentFailure>>;
    fn read_reservation_fence<'a>(
        &'a self,
        person_id: PersonId,
        connection_id: &'a ConnectionId,
    ) -> BoxFuture<'a, Result<floe_connections::SourceReservationFence, AgentFailure>>;
}

#[derive(Clone, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarDestinationObservation {
    pub calendar_id: String,
    pub calendar_name: String,
    pub can_modify: bool,
}

#[derive(Clone, Debug)]
pub struct ExpertProposalEvidence {
    pub receipt: TaskExecutionReceiptRef,
    pub artifact_id: Uuid,
    pub proposal: ExpertCalendarProposal,
    /// The exact Calendar contributor claimed by the proposal artifact.
    pub dependency: ContextDependency,
    /// Full authenticated terminal Task coverage, including inherited context.
    pub coverage: DependencyCoverage,
    pub installation_id: Uuid,
    pub assignment_id: Uuid,
    pub definition_revision: u64,
    pub invocation_id: Uuid,
}

/// Concrete storage adapter reloads the exact Task receipt/artifact and invokes
/// the Experts-owned provenance validator before returning these values.
pub trait ExpertProposalReader: Send + Sync {
    fn read<'a>(
        &'a self,
        actor: &'a OwnerActor,
        receipt: &'a TaskExecutionReceiptRef,
        artifact_id: Uuid,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<ExpertProposalEvidence, AgentFailure>>;
}

/// UTC millisecond timestamps match the native receipt codec without rounding a
/// fast prewrite rejection to a time before its immutable dispatch admission.
pub trait ActionsClock: Send + Sync {
    fn now(&self) -> chrono::DateTime<chrono::Utc>;
}
pub struct SystemActionsClock;
impl ActionsClock for SystemActionsClock {
    fn now(&self) -> chrono::DateTime<chrono::Utc> {
        let now = chrono::Utc::now();
        now - chrono::Duration::nanoseconds(i64::from(now.timestamp_subsec_nanos() % 1_000_000))
    }
}

/// Preparation is bounded external inspection. It never invokes save/remove.
/// Its returned capability retains exact source, event and host observations.
pub trait ActionCalendarExecutor: Send + Sync {
    fn destinations<'a>(
        &'a self,
        actor: &'a OwnerActor,
        source: &'a ActionSourceFence,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<Vec<CalendarDestinationObservation>, ActionBlockedReason>>;
    fn prepare<'a>(
        &'a self,
        actor: &'a OwnerActor,
        record: &'a ActionRecord,
        dependencies: &'a [crate::ActionDependencySourceFence],
        local_events: &'a [Event],
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<Box<dyn PreparedCalendarEffect>, ActionBlockedReason>>;
    /// Recovery is lookup-only, including exact historical native receipt readback.
    /// Missing receipt and matching Update/Delete postconditions stay Unknown.
    fn recover<'a>(
        &'a self,
        actor: &'a OwnerActor,
        intent: &'a ExecutionIntent,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, CalendarEffectOutcome>;
}

pub trait PreparedCalendarEffect: Send {
    fn executor_generation(&self) -> u64;
    /// Ownership consumption prevents reusing a prepared adapter write capability.
    /// Only a freshly committed dispatch admission reaches this method.
    fn dispatch(
        self: Box<Self>,
        admission: DispatchAdmission,
        scope: ExecutionScope,
    ) -> BoxFuture<'static, CalendarEffectOutcome>;
}
