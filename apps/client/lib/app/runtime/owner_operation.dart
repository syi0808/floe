import 'dart:convert';

import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/app/runtime/agent_vault_gateway.dart';
import 'package:floe_client/app/runtime/native_transport.dart';
import 'package:floe_client/features/conversation/application/agent_request_id.dart';
import 'package:floe_client/infrastructure/diagnostics/app_diagnostics.dart';

Future<Map<String, dynamic>> ownerCommand(
  AppWireTransport transport,
  String operationId,
  Map<String, Object?> command,
) => transport.commandV2({
  'schema_version': appWireProtocolVersion,
  'request_id': operationId,
  'command_id': operationId,
  'command': command,
});

Future<Map<String, dynamic>> ownerQuery(
  AppWireTransport transport,
  String operationId,
  Map<String, Object?> query,
) => transport.queryV2({
  'schema_version': appWireProtocolVersion,
  'request_id': operationId,
  'query': query,
});

Future<Map<String, dynamic>> ownerResult(
  AppWireTransport transport,
  String kind,
  String operationId,
  bool release,
) => ownerQuery(transport, newAgentRequestId(), {
  'kind': kind,
  'operation_id': operationId,
  'release': release,
});

final class OwnerOperationObserver {
  OwnerOperationObserver({this.timeout = const Duration(seconds: 35)});
  final Duration timeout;
  _PendingOwnerOperation? _pending;
  bool _observing = false;

  Future<T> observe<T>({
    required String scope,
    required String intent,
    required String stage,
    required String resultKind,
    required Future<Map<String, dynamic>> Function(String) start,
    required Future<Map<String, dynamic>> Function(String, bool) read,
    required T Function(Map<String, dynamic>) decode,
  }) async {
    if (_observing) throw const AgentVaultException('conflict');
    _observing = true;
    try {
      final previous = _pending;
      if (previous != null) {
        if (previous.scope != scope) {
          throw const AgentVaultException('conflict');
        }
        final recovered = await _finish(previous);
        if (previous.intent == intent && previous.resultKind == resultKind) {
          return recovered as T;
        }
      }
      final pending = _PendingOwnerOperation(
        scope: scope,
        intent: intent,
        correlation: _OwnerCorrelation(newAgentRequestId(), stage),
        resultKind: resultKind,
        start: start,
        read: read,
        decode: decode,
      );
      _pending = pending;
      return await _finish(pending) as T;
    } on NativeTransportException catch (error) {
      throw AgentVaultException(
        error.metadata['agent_failure'] ?? error.code,
        requestId: _pending?.correlation.id,
        stage: stage,
        metadata: error.metadata,
      );
    } finally {
      _observing = false;
    }
  }

  Future<Object?> _finish(_PendingOwnerOperation pending) async {
    if (!pending.decoded) {
      final elapsed = Stopwatch()..start();
      var result = pending.started
          ? await pending.read(pending.correlation.id, false)
          : await pending.start(pending.correlation.id);
      pending.started = true;
      while (true) {
        if (result['kind'] != pending.resultKind) {
          throw const FormatException('Invalid owner result kind');
        }
        final body = result;
        if (body['operation_id'] != pending.correlation.id ||
            body['done'] is! bool) {
          throw const FormatException('Invalid owner operation correlation');
        }
        _validateFailure(body['failure'], pending.correlation);
        if (body['done'] == true) {
          if (body['failure'] != null) {
            pending.failure = _failureException(
              body['failure'],
              pending.correlation,
            );
            final failure = pending.failure!;
            AppDiagnostics.error(
              component: 'owner_gateway',
              operation: pending.correlation.stage,
              error: failure,
              stackTrace: StackTrace.current,
              failure: failure.failure,
              failureDomain: failure.domain,
              failureCategory: failure.category,
              reasonCode: failure.reasonCode,
              incidentId: failure.incidentId,
              safeActions: failure.safeActions,
              requestId: pending.correlation.id,
              retryable: failure.retryable,
            );
          } else {
            pending.value = pending.decode(body);
          }
          pending.decoded = true;
          break;
        }
        if (elapsed.elapsed >= timeout) {
          throw AgentVaultException(
            'deadline_exceeded',
            requestId: pending.correlation.id,
            stage: pending.correlation.stage,
          );
        }
        await Future<void>.delayed(const Duration(milliseconds: 80));
        result = await pending.read(pending.correlation.id, false);
      }
    }
    try {
      final released = await pending.read(pending.correlation.id, true);
      if (released['kind'] != pending.resultKind) {
        throw const FormatException('Invalid owner release result');
      }
      final body = released;
      if (body['operation_id'] != pending.correlation.id ||
          body['done'] != true) {
        throw const FormatException('Invalid owner release correlation');
      }
      _validateFailure(body['failure'], pending.correlation);
    } on NativeTransportException catch (error) {
      if (!pending.releaseAttempted || error.code != 'not_found') rethrow;
    } finally {
      pending.releaseAttempted = true;
    }
    _pending = null;
    if (pending.failure case final failure?) throw failure;
    return pending.value;
  }
}

String ownerIntent(Map<String, Object?> value) => jsonEncode(value);

final class _PendingOwnerOperation {
  _PendingOwnerOperation({
    required this.scope,
    required this.intent,
    required this.correlation,
    required this.resultKind,
    required this.start,
    required this.read,
    required this.decode,
  });
  final String scope;
  final String intent;
  final _OwnerCorrelation correlation;
  final String resultKind;
  final Future<Map<String, dynamic>> Function(String) start;
  final Future<Map<String, dynamic>> Function(String, bool) read;
  final Object? Function(Map<String, dynamic>) decode;
  bool started = false;
  bool decoded = false;
  bool releaseAttempted = false;
  Object? value;
  AgentVaultException? failure;
}

final class _OwnerCorrelation {
  _OwnerCorrelation(this.id, this.stage);
  final String id;
  final String stage;
}

AgentVaultException _failureException(Object? raw, _OwnerCorrelation job) {
  final envelope = _failureEnvelope(raw, job);
  return AgentVaultException(
    envelope.reasonCode,
    requestId: envelope.correlationRequestId,
    stage: envelope.stage,
    recoveryAction: envelope.recoveryAction,
    affectedRefs: envelope.affectedRefs,
    correlationRequestId: envelope.correlationRequestId,
    retryableOverride: envelope.retryable,
    domain: envelope.domain,
    category: envelope.category,
    reasonCode: envelope.reasonCode,
    safeActions: envelope.safeActions,
    incidentId: envelope.incidentId,
    retryPolicy: envelope.retryPolicy,
    reloadRequired: envelope.reloadRequired,
    sealSession: envelope.sealSession,
  );
}

void _validateFailure(Object? raw, _OwnerCorrelation job) {
  if (raw != null) _failureEnvelope(raw, job);
}

_OwnerFailureEnvelope _failureEnvelope(Object? raw, _OwnerCorrelation job) {
  if (raw is! Map) {
    throw const FormatException('Invalid vault failure envelope');
  }
  late final Map<String, Object?> value;
  try {
    value = Map<String, Object?>.from(raw);
  } on Object {
    throw const FormatException('Invalid vault failure envelope');
  }
  const fields = {
    'schema_version',
    'domain',
    'category',
    'reason_code',
    'kind',
    'stage',
    'safe_actions',
    'affected_refs',
    'incident_id',
    'retry_policy',
    'retryable',
    'recovery_action',
    'correlation_request_id',
    'reload_required',
    'seal_session',
  };
  if (value.length != fields.length ||
      !value.keys.toSet().containsAll(fields)) {
    throw const FormatException('Invalid vault failure envelope fields');
  }
  if (value['schema_version'] != 1 ||
      value['domain'] is! String ||
      value['category'] is! String ||
      value['reason_code'] is! String ||
      value['kind'] is! String ||
      value['stage'] != job.stage ||
      (value['stage'] as String).isEmpty ||
      (value['stage'] as String).length > 64 ||
      (value['kind'] as String).trim().isEmpty ||
      (value['kind'] as String).length > 128 ||
      (value['reason_code'] as String).trim().isEmpty ||
      (value['reason_code'] as String).length > 128 ||
      value['incident_id'] is! String ||
      (value['incident_id'] as String).isEmpty ||
      (value['incident_id'] as String).length > 128 ||
      value['retryable'] is! bool ||
      value['reload_required'] is! bool ||
      value['seal_session'] is! bool ||
      value['retry_policy'] is! String ||
      value['recovery_action'] is! String ||
      value['correlation_request_id'] != job.id) {
    throw const FormatException('Invalid vault failure envelope');
  }
  if (!const {
    'source',
    'capability',
    'turn',
    'session',
    'vault',
    'app',
  }.contains(value['domain'])) {
    throw const FormatException('Invalid vault failure domain');
  }
  if (!const {
    'user_configuration',
    'transient',
    'integrity',
    'security',
    'internal',
  }.contains(value['category'])) {
    throw const FormatException('Invalid vault failure category');
  }
  final refs = value['affected_refs'];
  if (refs is! List ||
      refs.length > 32 ||
      refs.any((ref) => ref is! String || ref.isEmpty || ref.length > 256)) {
    throw const FormatException('Invalid vault failure references');
  }
  final action = value['recovery_action']! as String;
  if (!const {
    'none',
    'retry_read',
    'refresh_session',
    'refresh_context',
    'review_source',
    'reopen_vault',
    'reconcile',
  }.contains(action)) {
    throw const FormatException('Invalid vault recovery action');
  }
  final safeActions = value['safe_actions'];
  if (safeActions is! List ||
      safeActions.length > 16 ||
      safeActions.any(
        (item) =>
            item is! String ||
            !const {
              'continue_without_source',
              'review_source',
              'retry',
              'refresh_session',
              'start_new_session',
              'reopen_vault',
              'reset_local_agent_state',
              'export_diagnostics',
            }.contains(item),
      ) ||
      safeActions.toSet().length != safeActions.length) {
    throw const FormatException('Invalid vault failure safe actions');
  }
  final retryPolicy = value['retry_policy']! as String;
  if (!const {'never', 'immediate', 'backoff'}.contains(retryPolicy)) {
    throw const FormatException('Invalid vault failure retry policy');
  }
  final retryable = value['retryable']! as bool;
  if (retryable != (action == 'retry_read') ||
      retryable != (retryPolicy != 'never') ||
      retryable != safeActions.contains('retry')) {
    throw const FormatException('Invalid vault recovery retry contract');
  }
  return _OwnerFailureEnvelope(
    domain: value['domain']! as String,
    category: value['category']! as String,
    reasonCode: value['reason_code']! as String,
    kind: value['kind']! as String,
    stage: value['stage']! as String,
    safeActions: List.unmodifiable(safeActions.cast<String>()),
    affectedRefs: List.unmodifiable(refs.cast<String>()),
    retryable: retryable,
    incidentId: value['incident_id']! as String,
    retryPolicy: retryPolicy,
    recoveryAction: action,
    reloadRequired: value['reload_required']! as bool,
    sealSession: value['seal_session']! as bool,
    correlationRequestId: value['correlation_request_id']! as String,
  );
}

final class _OwnerFailureEnvelope {
  const _OwnerFailureEnvelope({
    required this.domain,
    required this.category,
    required this.reasonCode,
    required this.kind,
    required this.stage,
    required this.safeActions,
    required this.affectedRefs,
    required this.retryable,
    required this.incidentId,
    required this.retryPolicy,
    required this.recoveryAction,
    required this.reloadRequired,
    required this.sealSession,
    required this.correlationRequestId,
  });

  final String domain;
  final String category;
  final String reasonCode;
  final String kind;
  final String stage;
  final List<String> safeActions;
  final List<String> affectedRefs;
  final bool retryable;
  final String incidentId;
  final String retryPolicy;
  final String recoveryAction;
  final bool reloadRequired;
  final bool sealSession;
  final String correlationRequestId;
}
