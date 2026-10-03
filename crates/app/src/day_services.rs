//! Mechanical host forwarding to the retained Day owner.
use crate::{AppComposition, CallerContext, CoreError};
use floe_day::{DayQuery, DayMutationRequest, DayMutationResult, DaySnapshot, DayRefreshSnapshot};
use floe_execution::Cancellation;
use uuid::Uuid;

pub trait DayCommands {
    fn mutate_day(&self, caller: &CallerContext, request: DayMutationRequest) -> Result<DayMutationResult, CoreError>;
    fn refresh_day(&self, caller: &CallerContext, command_id: Uuid, query: DayQuery) -> Result<DayRefreshSnapshot, CoreError>;
}
pub trait DayQueries {
    fn read_day(&self, caller: &CallerContext, request: DayQuery) -> Result<DaySnapshot, CoreError>;
    fn get_day_refresh(&self, caller: &CallerContext, operation_ref: Uuid) -> Result<DayRefreshSnapshot, CoreError>;
}
impl DayQueries for AppComposition {
    fn read_day(&self, caller: &CallerContext, request: DayQuery) -> Result<DaySnapshot, CoreError> {
        let scope = crate::host_scope(Uuid::new_v4(), Cancellation::new(), std::time::Duration::from_secs(30));
        self.runtime.block_on(self.core.day.snapshot(&caller.owner_actor(), request, &scope)).map_err(crate::core::day_error)
    }
    fn get_day_refresh(&self, caller: &CallerContext, operation_ref: Uuid) -> Result<DayRefreshSnapshot, CoreError> {
        let scope = crate::host_scope(Uuid::new_v4(), Cancellation::new(), std::time::Duration::from_secs(30));
        self.runtime.block_on(self.core.day.get_refresh(&caller.owner_actor(), operation_ref, &scope)).map_err(crate::core::day_error)
    }
}
impl DayCommands for AppComposition {
    fn mutate_day(&self, caller: &CallerContext, request: DayMutationRequest) -> Result<DayMutationResult, CoreError> {
        let scope = crate::host_scope(request.command_id, Cancellation::new(), std::time::Duration::from_secs(30));
        self.runtime.block_on(self.core.day.mutate(&caller.owner_actor(), request, &scope)).map_err(crate::core::day_error)
    }
    fn refresh_day(&self, caller: &CallerContext, command_id: Uuid, query: DayQuery) -> Result<DayRefreshSnapshot, CoreError> {
        let scope = crate::host_scope(command_id, Cancellation::new(), std::time::Duration::from_secs(30));
        self.runtime.block_on(self.core.day.refresh_day(&caller.owner_actor(), command_id, query, &scope)).map_err(crate::core::day_error)
    }
}
