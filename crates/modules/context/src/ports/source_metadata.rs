//! Native Calendar metadata acquisition. This port cannot read events or
//! request permission; Connections and Context own the reviewed selection.
use floe_connections::SourceConnection;
use floe_execution::{BoxFuture, ExecutionScope};
use floe_kernel::{AgentFailure, OwnerActor};

use crate::NativeSubjectObservation;

pub trait SourceMetadataTransport: Send + Sync {
    fn calendar_subject<'a>(
        &'a self,
        actor: &'a OwnerActor,
        source: &'a SourceConnection,
        calendar_ids: &'a [String],
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<NativeSubjectObservation, AgentFailure>>;
}
