import 'dart:convert';
import 'dart:developer' as developer;
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:path_provider/path_provider.dart';

enum DiagnosticLevel { debug, info, warning, error }

final class DiagnosticRecord {
  const DiagnosticRecord({
    required this.observedAt,
    required this.level,
    required this.component,
    required this.operation,
    this.errorId,
    this.failure,
    this.requestId,
    this.sessionId,
    this.invocationId,
    this.elapsedMilliseconds,
    this.retryable,
    this.errorType,
    this.stackTrace,
  });

  final DateTime observedAt;
  final DiagnosticLevel level;
  final String component;
  final String operation;
  final String? errorId;
  final String? failure;
  final String? requestId;
  final String? sessionId;
  final String? invocationId;
  final int? elapsedMilliseconds;
  final bool? retryable;
  final String? errorType;
  final String? stackTrace;

  Map<String, Object?> toJson() => {
    'observed_at': observedAt.toUtc().toIso8601String(),
    'level': level.name,
    'component': component,
    'operation': operation,
    'error_id': ?errorId,
    'failure': ?failure,
    'request_id': ?requestId,
    'session_id': ?sessionId,
    'invocation_id': ?invocationId,
    'elapsed_ms': ?elapsedMilliseconds,
    'retryable': ?retryable,
    'error_type': ?errorType,
    if (stackTrace != null) 'stack_trace': stackTrace,
  };
}

final class AppDiagnostics {
  AppDiagnostics._();

  static const int _capacity = 500;
  static final List<DiagnosticRecord> _records = [];
  static int _nextErrorId = 0;

  static List<DiagnosticRecord> get records => List.unmodifiable(_records);

  static void event({
    required String component,
    required String operation,
    DiagnosticLevel level = DiagnosticLevel.info,
    String? requestId,
    String? sessionId,
    String? invocationId,
    int? elapsedMilliseconds,
  }) {
    _append(
      DiagnosticRecord(
        observedAt: DateTime.now(),
        level: level,
        component: component,
        operation: operation,
        requestId: requestId,
        sessionId: sessionId,
        invocationId: invocationId,
        elapsedMilliseconds: elapsedMilliseconds,
      ),
    );
  }

  static String error({
    required String component,
    required String operation,
    required Object error,
    StackTrace? stackTrace,
    String? failure,
    String? requestId,
    String? sessionId,
    String? invocationId,
    int? elapsedMilliseconds,
    bool? retryable,
  }) {
    final errorId = _newErrorId();
    _append(
      DiagnosticRecord(
        observedAt: DateTime.now(),
        level: DiagnosticLevel.error,
        component: component,
        operation: operation,
        errorId: errorId,
        failure: failure,
        requestId: requestId,
        sessionId: sessionId,
        invocationId: invocationId,
        elapsedMilliseconds: elapsedMilliseconds,
        retryable: retryable,
        errorType: error.runtimeType.toString(),
        stackTrace: stackTrace?.toString(),
      ),
    );
    return errorId;
  }

  static Future<File> exportBundle() async {
    final directory = await getTemporaryDirectory();
    final observedAt = DateTime.now().toUtc();
    final file = File(
      '${directory.path}/floe-diagnostics-${observedAt.millisecondsSinceEpoch}.json',
    );
    await file.writeAsString(
      const JsonEncoder.withIndent('  ').convert({
        'schema_version': 1,
        'created_at': observedAt.toIso8601String(),
        'platform': Platform.operatingSystem,
        'platform_version': Platform.operatingSystemVersion,
        'dart_version': Platform.version,
        'records': _records.map((record) => record.toJson()).toList(),
      }),
      flush: true,
    );
    return file;
  }

  @visibleForTesting
  static void clear() {
    _records.clear();
    _nextErrorId = 0;
  }

  static void _append(DiagnosticRecord record) {
    if (_records.length == _capacity) _records.removeAt(0);
    _records.add(record);
    developer.log(
      jsonEncode(record.toJson()),
      name: 'floe.${record.component}',
      level: switch (record.level) {
        DiagnosticLevel.debug => 500,
        DiagnosticLevel.info => 800,
        DiagnosticLevel.warning => 900,
        DiagnosticLevel.error => 1000,
      },
    );
  }

  static String _newErrorId() {
    _nextErrorId += 1;
    final timestamp = DateTime.now().toUtc().microsecondsSinceEpoch;
    return 'floe-$timestamp-${_nextErrorId.toRadixString(36)}';
  }
}
