use chrono::{DateTime, Utc};
use floe_execution::{BoxFuture, ExecutionScope};
use crate::{CalendarAcquisition, CalendarRefreshError, CalendarRefreshRequest};

pub trait DayClock: Send + Sync { fn now(&self) -> DateTime<Utc>; }
pub struct SystemDayClock;
impl DayClock for SystemDayClock { fn now(&self) -> DateTime<Utc> { Utc::now() } }

pub trait CalendarAcquisitionPort: Send + Sync {
    fn acquire<'a>(&'a self, request: CalendarRefreshRequest, scope: &'a ExecutionScope) -> BoxFuture<'a, Result<CalendarAcquisition, CalendarRefreshError>>;
}
