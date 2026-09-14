use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{APP_WIRE_VERSION, AppCommandReceiptDto, AppRunSnapshotDto};

pub const MAX_APP_EVENTS_PER_READ: u16 = 256;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AppEventsRequestDto {
    pub schema_version: u32,
    pub request_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_epoch: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<u64>,
    pub limit: u16,
}

impl AppEventsRequestDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != APP_WIRE_VERSION {
            return Err("schema_version");
        }
        if self.request_id.is_nil() {
            return Err("request_id");
        }
        if self.limit == 0 || self.limit > MAX_APP_EVENTS_PER_READ {
            return Err("limit");
        }
        if self.runtime_epoch.is_some() != self.cursor.is_some() || self.runtime_epoch == Some(0) {
            return Err("cursor");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AppEventsResultDto {
    Events {
        runtime_epoch: u64,
        next_cursor: u64,
        events: Vec<AppEventDto>,
    },
    ResyncRequired {
        runtime_epoch: u64,
        snapshot_cursor: u64,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AppEventDto {
    pub cursor: u64,
    pub aggregate_revision: u64,
    pub runtime_epoch: u64,
    pub event: AppEventKindDto,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AppEventKindDto {
    CommandUpdated { receipt: AppCommandReceiptDto },
    RunUpdated { run: AppRunSnapshotDto },
}
