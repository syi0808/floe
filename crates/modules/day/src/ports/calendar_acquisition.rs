use chrono::{DateTime, Utc};
use floe_execution::{BoxFuture, ExecutionScope};
use crate::{CalendarAcquisition, CalendarRefreshError, CalendarRefreshRequest};

pub trait DayClock: Send + Sync { fn now(&self) -> DateTime<Utc>; }
pub struct SystemDayClock;
impl DayClock for SystemDayClock { fn now(&self) -> DateTime<Utc> { Utc::now() } }

pub trait CalendarAcquisitionPort: Send + Sync {
    fn inspect_sources<'a>(&'a self, actor: &'a floe_kernel::OwnerActor, scope: &'a ExecutionScope) -> BoxFuture<'a, Result<crate::CalendarCacheInspection, CalendarRefreshError>>;
    fn acquire<'a>(&'a self, request: CalendarRefreshRequest, scope: &'a ExecutionScope) -> BoxFuture<'a, Result<CalendarAcquisition, CalendarRefreshError>>;
}
