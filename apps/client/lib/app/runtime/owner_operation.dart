import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/features/conversation/application/agent_request_id.dart';

/// Send an immutable product command with its caller-owned v4 identity.
Future<Map<String, dynamic>> ownerCommand(
  AppWireTransport transport,
  String operationId,
  Map<String, Object?> command,
) => transport.commandV2({
  'schema_version': appWireProtocolVersion,
  'request_id': newAgentRequestId(),
  'command_id': operationId,
  'command': command,
});

/// Send a pure AppWire query. Query IDs are fresh v4 correlation identities.
Future<Map<String, dynamic>> ownerQuery(
  AppWireTransport transport,
  String requestId,
  Map<String, Object?> query,
) => transport.queryV2({
  'schema_version': appWireProtocolVersion,
  'request_id': requestId,
  'query': query,
});
