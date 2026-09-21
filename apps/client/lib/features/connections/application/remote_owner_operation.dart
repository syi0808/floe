import 'dart:async';
import 'dart:convert';

import 'package:floe_client/app/runtime/agent_vault_gateway.dart';
import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/app/runtime/native_transport.dart';
import 'package:floe_client/features/conversation/application/agent_request_id.dart';

typedef RemoteOwnerRequest = Future<Map<String, dynamic>> Function(
  Map<String, dynamic> request,
);

final class RemoteOperationPending implements Exception {
  const RemoteOperationPending(this.operationId, this.phase);
  final String operationId;
  final String phase;

  @override
  String toString() =>
      'Remote operation $operationId requires $phase reconciliation';
}

final class RemoteOwnerOperation {
  RemoteOwnerOperation(
    this.request, {
    required this.resultFields,
    this.pollInterval = const Duration(milliseconds: 150),
    this.deadline = const Duration(seconds: 15),
  });

  final RemoteOwnerRequest request;
  final Set<String> resultFields;
  final Duration pollInterval;
  final Duration deadline;
  final Map<String, _RemoteJob> _jobs = {};

  Future<Result> perform<Result>(
    Map<String, Object?> operation,
    Result Function(Map<String, dynamic>) decode, {
    bool Function(Result)? retain,
  }) async {
    final key = jsonEncode(operation);
    if (!_jobs.containsKey(key) && _jobs.length >= 32) {
      throw StateError('Reconcile retained remote operations first.');
    }
    final job = _jobs.putIfAbsent(key, () => _RemoteJob(newAgentRequestId()));
    final running = job.running;
    if (running != null) return await running as Result;
    final future = _perform(job, key, operation, decode, retain);
    job.running = future;
    try {
      return await future;
    } finally {
      job.running = null;
    }
  }

  Future<Result> _perform<Result>(
    _RemoteJob job,
    String key,
    Map<String, Object?> operation,
    Result Function(Map<String, dynamic>) decode,
    bool Function(Result)? retain,
  ) async {
    final timer = Stopwatch()..start();
    if (!job.submitted) {
      job.submitted = true;
      try {
        job.result = await _call(job.id, operation, job.id);
      } on NativeTransportException catch (error) {
        if (!{'timeout', 'ffi'}.contains(error.code)) {
          _jobs.remove(key);
          rethrow;
        }
      } on TimeoutException {
        job.result = null;
      }
    }
    while (job.result?['done'] != true) {
      if (timer.elapsed >= deadline) {
        throw RemoteOperationPending(job.id, 'read_result');
      }
      await Future<void>.delayed(pollInterval);
      try {
        job.result = await _read(job.id, false);
      } on NativeTransportException {
        throw RemoteOperationPending(job.id, 'read_result');
      } on TimeoutException {
        throw RemoteOperationPending(job.id, 'read_result');
      }
    }
    final result = job.result!;
    final failure = result['failure'];
    if (failure != null) {
      final exception = _failure(failure, job.id);
      await release(job.id);
      throw exception;
    }
    final decoded = decode(result);
    if (!(retain?.call(decoded) ?? false)) await release(job.id);
    return decoded;
  }

  Future<void> release(String operationId) async {
    final entries = _jobs.entries.where(
      (entry) => entry.value.id == operationId,
    );
    if (entries.isEmpty) return;
    final entry = entries.single;
    if (entry.value.result?['done'] != true) {
      throw RemoteOperationPending(operationId, 'read_result');
    }
    final previouslyAttempted = entry.value.releaseAttempted;
    entry.value.releaseAttempted = true;
    try {
      final result = await _read(operationId, true);
      if (result['done'] != true) {
        throw const FormatException('Incomplete release');
      }
    } on NativeTransportException catch (error) {
      if (!previouslyAttempted || error.code != 'not_found') {
        throw RemoteOperationPending(operationId, 'release');
      }
    } on TimeoutException {
      throw RemoteOperationPending(operationId, 'release');
    }
    _jobs.remove(entry.key);
  }

  Future<Map<String, dynamic>> _read(String operationId, bool release) => _call(
    newAgentRequestId(),
    {'kind': 'read_result', 'operation_id': operationId, 'release': release},
    operationId,
  );

  Future<Map<String, dynamic>> _call(
    String requestId,
    Map<String, Object?> operation,
    String operationId,
  ) async {
    final result = await request({
      'schema_version': appWireProtocolVersion,
      'request_id': requestId,
      'operation': operation,
    });
    final fields = {'operation_id', 'done', 'failure', ...resultFields};
    if (result['operation_id'] != operationId ||
        result['done'] is! bool ||
        result.keys.any((key) => !fields.contains(key)) ||
        result['done'] == false &&
            fields
                .where((key) => key != 'operation_id' && key != 'done')
                .any((key) => result[key] != null)) {
      throw const FormatException('Invalid remote owner result');
    }
    return result;
  }
}

final class _RemoteJob {
  _RemoteJob(this.id);
  final String id;
  bool submitted = false;
  bool releaseAttempted = false;
  Map<String, dynamic>? result;
  Future<Object?>? running;
}

AgentVaultException _failure(Object raw, String operationId) {
  if (raw is! Map) throw const FormatException('Invalid owner failure');
  final value = Map<String, Object?>.from(raw);
  const textFields = {
    'domain',
    'category',
    'reason_code',
    'kind',
    'stage',
    'incident_id',
    'retry_policy',
    'recovery_action',
    'correlation_request_id',
  };
  const boolFields = {'retryable', 'reload_required', 'seal_session'};
  const listFields = {'safe_actions', 'affected_refs'};
  const fields = {
    'schema_version',
    ...textFields,
    ...boolFields,
    ...listFields,
  };
  if (value.length != fields.length ||
      value.keys.any((key) => !fields.contains(key)) ||
      value['schema_version'] != 1 ||
      value['correlation_request_id'] != operationId ||
      textFields.any(
        (key) => value[key] is! String || (value[key] as String).isEmpty,
      ) ||
      boolFields.any((key) => value[key] is! bool) ||
      listFields.any(
        (key) =>
            value[key] is! List ||
            (value[key] as List).any((item) => item is! String),
      )) {
    throw const FormatException('Invalid owner failure');
  }
  return AgentVaultException(
    value['reason_code'] as String,
    metadata: {'kind': value['kind'] as String},
    requestId: operationId,
    stage: value['stage'] as String,
    domain: value['domain'] as String,
    category: value['category'] as String,
    reasonCode: value['reason_code'] as String,
    incidentId: value['incident_id'] as String,
    retryPolicy: value['retry_policy'] as String,
    retryableOverride: value['retryable'] as bool,
    recoveryAction: value['recovery_action'] as String,
    reloadRequired: value['reload_required'] as bool,
    sealSession: value['seal_session'] as bool,
    correlationRequestId: operationId,
    safeActions: List.unmodifiable(
      (value['safe_actions'] as List).cast<String>(),
    ),
    affectedRefs: List.unmodifiable(
      (value['affected_refs'] as List).cast<String>(),
    ),
  );
}
