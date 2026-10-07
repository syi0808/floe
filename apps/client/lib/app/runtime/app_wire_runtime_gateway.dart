import 'package:floe_client/app/runtime/app_owner_exception.dart';
import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/app/runtime/owner_failure.dart';
import 'package:floe_client/app/runtime/runtime_gateway.dart';
import 'package:floe_client/features/conversation/application/agent_request_id.dart';

final class AppWireRuntimeGateway implements RuntimeGateway {
  AppWireRuntimeGateway(this._transport);

  final AppWireTransport _transport;

  @override
  Future<RuntimeReadinessSnapshot> readiness(String requestId) async {
    try {
      final value = await _transport.queryV2({
        'schema_version': appWireProtocolVersion,
        'request_id': requestId,
        'query': const {'kind': 'runtime.readiness'},
      });
      if (value['kind'] != 'runtime.readiness' ||
          value['failure'] != null && value['failure'] is! Map) {
        throw const FormatException('Invalid Runtime readiness result.');
      }
      const states = {
        'ready': RuntimeReadinessState.ready,
        'preparation_required': RuntimeReadinessState.preparationRequired,
        'unavailable': RuntimeReadinessState.unavailable,
      };
      final state = states[value['state']];
      if (state == null) {
        throw const FormatException('Invalid Runtime readiness state.');
      }
      final rawFailure = value['failure'];
      return RuntimeReadinessSnapshot(
        state: state,
        failure: rawFailure == null ? null : OwnerFailure.fromJson(rawFailure),
      );
    } on AppWireTransportException catch (error) {
      throw _translate(error, requestId, 'runtime_readiness');
    }
  }

  @override
  Future<RuntimePreparationResult> prepare(String operationId) => _command(
    operationId,
    const {'kind': 'runtime.prepare'},
    'runtime_prepare',
  );

  @override
  Future<RuntimePreparationResult> acknowledge(String operationId) => _command(
    operationId,
    const {'kind': 'runtime.preparation.acknowledge'},
    'runtime_preparation_acknowledge',
  );

  @override
  Future<RuntimePreparationResult> getPreparation(String operationId) async {
    final requestId = newAgentRequestId();
    try {
      final value = await _transport.queryV2({
        'schema_version': appWireProtocolVersion,
        'request_id': requestId,
        'query': {
          'kind': 'runtime.preparation.get',
          'operation_id': operationId,
        },
      });
      return _decodePreparation(value, operationId);
    } on AppWireTransportException catch (error) {
      throw _translate(error, operationId, 'runtime_preparation_get');
    }
  }

  Future<RuntimePreparationResult> _command(
    String operationId,
    Map<String, Object?> command,
    String stage,
  ) async {
    try {
      final value = await _transport.commandV2({
        'schema_version': appWireProtocolVersion,
        'request_id': newAgentRequestId(),
        'command_id': operationId,
        'command': command,
      });
      return _decodePreparation(value, operationId);
    } on AppWireTransportException catch (error) {
      throw _translate(error, operationId, stage);
    }
  }

  RuntimePreparationResult _decodePreparation(
    Map<String, dynamic> value,
    String operationId,
  ) {
    final failure = value['failure'];
    if (value['kind'] != 'runtime.preparation' ||
        value['operation_id'] != operationId ||
        value['done'] is! bool ||
        failure != null && failure is! String) {
      throw const FormatException('Invalid Runtime preparation result.');
    }
    return RuntimePreparationResult(
      operationId: operationId,
      done: value['done'] as bool,
      failure: failure as String?,
    );
  }

  AppOwnerException _translate(
    AppWireTransportException error,
    String requestId,
    String stage,
  ) => AppOwnerException.fromAppWire(
    error.metadata['reason_code'] ??
        error.metadata['agent_failure'] ??
        error.code,
    requestId: requestId,
    stage: stage,
    metadata: error.metadata,
    ownerFailure: error.ownerFailure,
    commandOutcome: error.commandOutcome,
  );
}
