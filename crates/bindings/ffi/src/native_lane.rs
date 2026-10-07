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

fn native_command_request(
    request: AppCommandRequestDto,
) -> app_wire::AppWireResult<(uuid::Uuid, NativeHostCommandDto)> {
    request.validate().map_err(app_wire::request_validation)?;
    let request_id = request.request_id.get();
    let AppCommandDto::NativeHost(command) = request.command else {
        return Err(app_wire::validation("command"));
    };
    Ok((request_id, command))
}

fn native_query_request(
    request: AppQueryRequestDto,
) -> app_wire::AppWireResult<(uuid::Uuid, NativeHostQueryDto)> {
    request.validate().map_err(app_wire::request_validation)?;
    let request_id = request.request_id.get();
    let AppQueryDto::NativeHost(query) = request.query else {
        return Err(app_wire::validation("query"));
    };
    Ok((request_id, query))
}

pub(crate) fn command(
    handle: &FloeNativeHostLane,
    request: AppCommandRequestDto,
) -> app_wire::AppWireResult<AppCommandResultDto> {
    let (request_id, command) = native_command_request(request)?;
    let command = crate::context_wire::command(command).map_err(structural)?;
    let result = handle
        .lane
        .command(request_id, command)
        .map_err(lane_error)?;
    crate::conversion::native::native_host_command_result(result).map_err(structural)
}
pub(crate) fn query(
    handle: &FloeNativeHostLane,
    request: AppQueryRequestDto,
) -> app_wire::AppWireResult<AppQueryResultDto> {
    let (request_id, query) = native_query_request(request)?;
    let query = crate::context_wire::query(query).map_err(structural)?;
    let result = handle.lane.query(request_id, query).map_err(lane_error)?;
    crate::conversion::native::native_host_query_result(result).map_err(structural)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_callback_lane_rejects_runtime_command_and_query() {
        let runtime_command = AppCommandRequestDto {
            schema_version: APP_WIRE_VERSION,
            request_id: RequestIdDto::new(uuid::Uuid::new_v4()).expect("request UUID"),
            command_id: CommandIdDto::new(uuid::Uuid::new_v4()).expect("command UUID"),
            command: AppCommandDto::Runtime(RuntimeCommandDto::Prepare {}),
        };
        assert!(native_command_request(runtime_command).is_err());

        let runtime_query = AppQueryRequestDto {
            schema_version: APP_WIRE_VERSION,
            request_id: RequestIdDto::new(uuid::Uuid::new_v4()).expect("request UUID"),
            query: AppQueryDto::Runtime(RuntimeQueryDto::Readiness {}),
        };
        assert!(native_query_request(runtime_query).is_err());
    }
}
