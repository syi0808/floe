use floe_agent_contract::{PackageKind, PackageRef};
use floe_context_contract::CalendarProvider;
use floe_context_contract::DataClass;
use floe_day::PersonId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::CalendarAction;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentActionOrigin {
    pub schema_version: u32,
    pub instance_id: Uuid,
    pub session_id: Uuid,
    pub invocation_id: Uuid,
    pub assignment_id: Uuid,
    pub package: PackageRef,
    pub view_handle: Uuid,
    pub state_revision: u64,
    pub data_class: DataClass,
    pub automatic: bool,
}

impl AgentActionOrigin {
    pub fn valid_for(&self, action: &CalendarAction) -> bool {
        self.schema_version == 1
            && self.package.kind == PackageKind::Expert
            && self.invocation_id == action.id
            && self.state_revision > 0
            && !action.direct
            && action.mutation.is_none()
            && matches!(
                (self.data_class, action.provider),
                (DataClass::Synthetic, CalendarProvider::Fixture)
                    | (
                        DataClass::Personal,
                        CalendarProvider::EventKit
                            | CalendarProvider::Android
                            | CalendarProvider::Google
                            | CalendarProvider::Microsoft,
                    )
            )
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertProposalReference {
    pub person_id: PersonId,
    pub session_id: Uuid,
    pub invocation_id: Uuid,
}
