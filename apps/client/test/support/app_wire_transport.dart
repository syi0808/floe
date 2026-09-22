import 'package:floe_client/app/runtime/app_wire_transport.dart';

abstract class TestAppWireTransport implements AppWireTransport {
  Future<Map<String, dynamic>> call(Map<String, dynamic> request);

  @override
  Future<Map<String, dynamic>> commandV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) => call(request);
  @override
  Future<Map<String, dynamic>> queryV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) => call(request);
  @override
  Future<Map<String, dynamic>> eventsV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) => throw UnimplementedError();
  @override
  Future<void> close() async {}
}

final class CallbackAppWireTransport extends TestAppWireTransport {
  CallbackAppWireTransport(this.callback);
  final Future<Map<String, dynamic>> Function(Map<String, dynamic>) callback;
  @override
  Future<Map<String, dynamic>> call(Map<String, dynamic> request) =>
      callback(request);
}

String ownerOperationId(Map<String, dynamic> request) =>
    (request['query'] as Map?)?['operation_id'] as String? ??
    request['command_id'] as String? ??
    request['request_id'] as String;

Map<String, Object?> ownerFailure(
  Map<String, dynamic> request,
  String kind,
  String stage, {
  String recovery = 'none',
}) => {
  'schema_version': 1,
  'domain': 'vault',
  'category': 'internal',
  'reason_code': kind,
  'kind': kind,
  'stage': stage,
  'safe_actions': <String>[],
  'affected_refs': <String>[],
  'incident_id': ownerOperationId(request),
  'retry_policy': 'never',
  'retryable': false,
  'recovery_action': recovery,
  'reload_required': recovery == 'reopen_vault',
  'seal_session': recovery == 'reopen_vault',
  'correlation_request_id': ownerOperationId(request),
};
