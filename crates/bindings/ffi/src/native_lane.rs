use crate::{app_wire, bridge::FloeNativeHostLane};
use floe_protocol::*;

fn structural(_error: ErrorDto) -> AppWireErrorDto {
    app_wire::validation("native_host")
}

fn lane_error(error: floe_app::NativeHostLaneError) -> AppWireErrorDto {
    match error {
        floe_app::NativeHostLaneError::Host(error) => app_wire::host_failure(error),
        floe_app::NativeHostLaneError::Native(error) => app_wire::agent_failure(error),
    }
}
pub(crate) fn command(
    handle: &FloeNativeHostLane,
    request: AppCommandRequestDto,
) -> app_wire::AppWireResult<AppCommandResultDto> {
    request.validate().map_err(app_wire::request_validation)?;
    let AppCommandDto::NativeHost(command) = request.command else {
        return Err(app_wire::validation("command"));
    };
    let command = crate::context_wire::command(command).map_err(structural)?;
    let result = handle
        .lane
        .command(request.request_id.get(), command)
        .map_err(lane_error)?;
    crate::conversion::native::native_host_command_result(result).map_err(structural)
}
pub(crate) fn query(
    handle: &FloeNativeHostLane,
    request: AppQueryRequestDto,
) -> app_wire::AppWireResult<AppQueryResultDto> {
    request.validate().map_err(app_wire::request_validation)?;
    let AppQueryDto::NativeHost(query) = request.query else {
        return Err(app_wire::validation("query"));
    };
    let query = crate::context_wire::query(query).map_err(structural)?;
    let result = handle
        .lane
        .query(request.request_id.get(), query)
        .map_err(lane_error)?;
    crate::conversion::native::native_host_query_result(result).map_err(structural)
}
