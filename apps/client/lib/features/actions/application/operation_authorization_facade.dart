import 'package:floe_client/app/runtime/app_runtime.dart';
import 'package:floe_client/features/actions/application/calendar_action_gateway.dart';
import 'package:floe_client/features/actions/domain/calendar_action.dart';
import 'package:floe_client/features/actions/infrastructure/app_wire_operation_authorization_gateway.dart';

/// App-lifetime client for the Access-owned Expert operation policy.
final class OperationAuthorizationFacade implements OperationAuthorizationGateway {
  OperationAuthorizationFacade(AppRuntime runtime)
    : _gateway = AppWireOperationAuthorizationGateway(runtime.wireTransport);

  final AppWireOperationAuthorizationGateway _gateway;

  @override
  Future<ActionAuthority> loadAuthority() => _gateway.loadAuthority();

  @override
  Future<ActionAuthority> setAuthority({
    required String commandId,
    required ActionAuthorityMode mode,
    required int expectedRevision,
  }) => _gateway.setAuthority(
    commandId: commandId,
    mode: mode,
    expectedRevision: expectedRevision,
  );
}
