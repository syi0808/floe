import 'package:floe_client/app/runtime/app_runtime.dart';
import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/app/runtime/owner_operation.dart'
    show ownerCommand, ownerQuery;
import 'package:floe_client/features/actions/application/calendar_action_gateway.dart';
import 'package:floe_client/features/actions/domain/calendar_action.dart';
import 'package:floe_client/features/conversation/application/agent_request_id.dart';

/// AppWire client for Conversation's projection of Access-owned policy.
final class AppWireOperationAuthorizationGateway implements OperationAuthorizationGateway {
  AppWireOperationAuthorizationGateway(this._transport);

  final AppWireTransport _transport;

  @override
  Future<ActionAuthority> loadAuthority() async {
    final result = await _query({'kind': 'conversation.calendar_policy.get'});
    _expectResult(result, 'conversation.calendar_policy', const {'policy'});
    return ActionAuthority.fromJson(_object(result['policy'], 'operation policy'));
  }

  @override
  Future<ActionAuthority> setAuthority({
    required String commandId,
    required ActionAuthorityMode mode,
    required int expectedRevision,
  }) async {
    if (expectedRevision <= 0) {
      throw const FormatException('Invalid operation policy revision.');
    }
    final result = await _command(commandId, {
      'kind': 'conversation.calendar_policy.set',
      'mode': mode.name,
      'expected_revision': expectedRevision,
    });
    _expectResult(result, 'conversation.calendar_policy', const {'policy'});
    return ActionAuthority.fromJson(_object(result['policy'], 'operation policy'));
  }

  Future<Map<String, dynamic>> _command(
    String commandId,
    Map<String, Object?> command,
  ) async {
    try {
      return await ownerCommand(_transport, commandId, command);
    } on AppWireTransportException catch (error) {
      throw _runtimeError(error);
    }
  }

  Future<Map<String, dynamic>> _query(Map<String, Object?> query) async {
    try {
      return await ownerQuery(_transport, newAgentRequestId(), query);
    } on AppWireTransportException catch (error) {
      throw _runtimeError(error);
    }
  }

  AppRuntimeException _runtimeError(AppWireTransportException error) =>
      AppRuntimeException(
        error.metadata['reason_code'] ??
            error.metadata['owner_code'] ??
            error.code,
        error.message,
        field: error.field,
        metadata: error.metadata,
        ownerFailure: error.ownerFailure,
        commandOutcome: error.commandOutcome,
      );
}

void _expectResult(
  Map<String, dynamic> result,
  String kind,
  Set<String> payloadKeys,
) {
  final keys = {'kind', ...payloadKeys};
  if (result.length != keys.length ||
      !result.keys.toSet().containsAll(keys) ||
      result['kind'] != kind) {
    throw FormatException('Invalid $kind result.');
  }
}

Map<String, dynamic> _object(Object? value, String field) {
  if (value is! Map) throw FormatException('Invalid $field.');
  try {
    return Map<String, dynamic>.from(value);
  } on Object {
    throw FormatException('Invalid $field.');
  }
}
