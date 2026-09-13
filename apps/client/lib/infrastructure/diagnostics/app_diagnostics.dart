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
    this.failureDomain,
    this.failureCategory,
    this.reasonCode,
    this.incidentId,
    this.safeActions = const [],
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
  final String? failureDomain;
  final String? failureCategory;
  final String? reasonCode;
  final String? incidentId;
  final List<String> safeActions;
  final String? requestId;
  final String? sessionId;
  final String? invocationId;
  final int? elapsedMilliseconds;
  final bool? retryable;
  final String? errorType;
  final String? stackTrace;

  static DiagnosticRecord? fromJson(Map<String, Object?> json) {
    final observedAt = DateTime.tryParse(json['observed_at'] as String? ?? '');
    final levelName = json['level'] as String?;
    final component = json['component'] as String?;
    final operation = json['operation'] as String?;
    if (observedAt == null || component == null || operation == null) {
      return null;
    }
    final level = DiagnosticLevel.values.firstWhere(
      (value) => value.name == levelName,
      orElse: () => DiagnosticLevel.info,
    );
    return DiagnosticRecord(
      observedAt: observedAt,
      level: level,
      component: component,
      operation: operation,
      errorId: json['error_id'] as String?,
      failure: json['failure'] as String?,
      failureDomain: json['failure_domain'] as String?,
      failureCategory: json['failure_category'] as String?,
      reasonCode: json['reason_code'] as String?,
      incidentId: json['incident_id'] as String?,
      safeActions: List.unmodifiable(
        (json['safe_actions'] as List? ?? const []).cast<String>(),
      ),
      requestId: json['request_id'] as String?,
      sessionId: json['session_id'] as String?,
      invocationId: json['invocation_id'] as String?,
      elapsedMilliseconds: json['elapsed_ms'] as int?,
      retryable: json['retryable'] as bool?,
      errorType: json['error_type'] as String?,
      stackTrace: json['stack_trace'] as String?,
    );
  }

  Map<String, Object?> toJson() => {
    'observed_at': observedAt.toUtc().toIso8601String(),
    'level': level.name,
    'component': component,
    'operation': operation,
    'error_id': ?errorId,
    'failure': ?failure,
    'failure_domain': ?failureDomain,
    'failure_category': ?failureCategory,
    'reason_code': ?reasonCode,
    'incident_id': ?incidentId,
    if (safeActions.isNotEmpty) 'safe_actions': safeActions,
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
  static const int _defaultJournalFileBytes = 5 * 1024 * 1024;
  static const int _defaultJournalFileCount = 5;
  static const String _journalFileName = 'incidents.ndjson';
  static final List<DiagnosticRecord> _records = [];
  static final List<DiagnosticRecord> _pendingPersistence = [];
  static Future<void> _writeQueue = Future<void>.value();
  static Directory? _journalDirectory;
  static int _journalFileBytes = _defaultJournalFileBytes;
  static int _journalFileCount = _defaultJournalFileCount;
  static bool _initialized = false;
  static int _nextErrorId = 0;

  static List<DiagnosticRecord> get records => List.unmodifiable(_records);

  static Future<void> initialize({
    Directory? directory,
    int maxFileBytes = _defaultJournalFileBytes,
    int maxFiles = _defaultJournalFileCount,
  }) async {
    _initialized = false;
    await _writeQueue;
    _journalFileBytes = maxFileBytes > 0
        ? maxFileBytes
        : _defaultJournalFileBytes;
    _journalFileCount = maxFiles > 0 ? maxFiles : _defaultJournalFileCount;
    try {
      final root = directory ?? await getApplicationSupportDirectory();
      final journalDirectory = directory == null
          ? Directory('${root.path}/diagnostics')
          : root;
      await journalDirectory.create(recursive: true);
      _journalDirectory = journalDirectory;
      final retained = await _readJournal(journalDirectory);
      final pending = List<DiagnosticRecord>.from(_pendingPersistence);
      _pendingPersistence.clear();
      _records
        ..clear()
        ..addAll(retained);
      for (final record in pending) {
        _addToMemory(record);
        _enqueuePersistence(record);
      }
      _initialized = true;
    } on Object {
      _journalDirectory = null;
      _pendingPersistence.clear();
      _initialized = true;
    }
  }

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
    String? failureDomain,
    String? failureCategory,
    String? reasonCode,
    String? incidentId,
    List<String> safeActions = const [],
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
        failureDomain: failureDomain,
        failureCategory: failureCategory,
        reasonCode: reasonCode,
        incidentId: incidentId,
        safeActions: List.unmodifiable(safeActions),
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
    await _writeQueue;
    var exportRecords = List<DiagnosticRecord>.from(_records);
    final journalDirectory = _journalDirectory;
    if (journalDirectory != null) {
      final persisted = await _readJournal(journalDirectory, limit: null);
      final counts = <String, int>{};
      for (final record in persisted) {
        final key = jsonEncode(_sanitizedJson(record));
        counts[key] = (counts[key] ?? 0) + 1;
      }
      exportRecords = persisted;
      for (final record in _records) {
        final key = jsonEncode(_sanitizedJson(record));
        final count = counts[key] ?? 0;
        if (count > 0) {
          counts[key] = count - 1;
        } else {
          exportRecords.add(record);
        }
      }
    }
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
        'records': exportRecords.map(_sanitizedJson).toList(),
      }),
      flush: true,
    );
    return file;
  }

  @visibleForTesting
  static void clear() {
    _records.clear();
    _pendingPersistence.clear();
    _nextErrorId = 0;
    if (_initialized) {
      _enqueue(() async {
        final directory = _journalDirectory;
        if (directory == null) return;
        for (var index = 0; index < _journalFileCount; index++) {
          final suffix = index == 0 ? '' : '.$index';
          try {
            await File('${directory.path}/$_journalFileName$suffix').delete();
          } on FileSystemException catch (error) {
            _ignore(error);
          }
        }
      });
    }
  }

  static Future<void> deleteJournal() async {
    clear();
    await _writeQueue;
  }

  @visibleForTesting
  static Future<void> flush() => _writeQueue;

  static void _append(DiagnosticRecord record) {
    _addToMemory(record);
    if (_initialized && _journalDirectory != null) {
      _enqueuePersistence(record);
    } else {
      _pendingPersistence.add(record);
    }
    try {
      developer.log(
        jsonEncode(_sanitizedJson(record)),
        name: 'floe.${record.component}',
        level: switch (record.level) {
          DiagnosticLevel.debug => 500,
          DiagnosticLevel.info => 800,
          DiagnosticLevel.warning => 900,
          DiagnosticLevel.error => 1000,
        },
      );
    } on Object catch (error) {
      _ignore(error);
    }
  }

  static void _addToMemory(DiagnosticRecord record) {
    if (_records.length == _capacity) _records.removeAt(0);
    _records.add(record);
  }

  static void _enqueuePersistence(DiagnosticRecord record) {
    _enqueue(() async {
      final directory = _journalDirectory;
      if (directory == null) return;
      final line = '${jsonEncode(_sanitizedJson(record))}\n';
      final file = File('${directory.path}/$_journalFileName');
      try {
        final length = await file.exists() ? await file.length() : 0;
        if (length + utf8.encode(line).length > _journalFileBytes) {
          await _rotateJournal(directory);
        }
        await file.writeAsString(line, mode: FileMode.append, flush: true);
      } on Object catch (error) {
        _ignore(error);
      }
    });
  }

  static void _enqueue(Future<void> Function() operation) {
    _writeQueue = _writeQueue.then((_) async {
      try {
        await operation();
      } on Object catch (error) {
        _ignore(error);
      }
    });
  }

  static Future<void> _rotateJournal(Directory directory) async {
    for (var index = _journalFileCount - 2; index >= 1; index--) {
      final source = File('${directory.path}/$_journalFileName.$index');
      final destination = File(
        '${directory.path}/$_journalFileName.${index + 1}',
      );
      try {
        if (await destination.exists()) await destination.delete();
        if (await source.exists()) await source.rename(destination.path);
      } on Object catch (error) {
        _ignore(error);
      }
    }
    if (_journalFileCount <= 1) {
      try {
        final current = File('${directory.path}/$_journalFileName');
        if (await current.exists()) await current.delete();
      } on Object catch (error) {
        _ignore(error);
      }
      return;
    }
    final current = File('${directory.path}/$_journalFileName');
    try {
      if (await current.exists()) {
        final firstBackup = File('${directory.path}/$_journalFileName.1');
        if (await firstBackup.exists()) await firstBackup.delete();
        await current.rename(firstBackup.path);
      }
    } on Object catch (error) {
      _ignore(error);
    }
  }

  static Future<List<DiagnosticRecord>> _readJournal(
    Directory directory, {
    int? limit = _capacity,
  }) async {
    final records = <DiagnosticRecord>[];
    for (var index = _journalFileCount - 1; index >= 0; index--) {
      final suffix = index == 0 ? '' : '.$index';
      final file = File('${directory.path}/$_journalFileName$suffix');
      try {
        if (!await file.exists()) continue;
        for (final line in await file.readAsLines()) {
          try {
            final decoded = jsonDecode(line);
            if (decoded is Map) {
              final record = DiagnosticRecord.fromJson(
                Map<String, Object?>.from(decoded),
              );
              if (record != null) records.add(record);
            }
          } on Object catch (error) {
            _ignore(error);
          }
        }
      } on Object catch (error) {
        _ignore(error);
      }
    }
    return limit == null || records.length <= limit
        ? records
        : records.sublist(records.length - limit);
  }

  static Map<String, Object?> _sanitizedJson(DiagnosticRecord record) {
    final json = record.toJson();
    for (final key in [
      'failure',
      'failure_domain',
      'failure_category',
      'reason_code',
      'incident_id',
    ]) {
      final value = json[key];
      if (value is String && !_safeToken(value)) json.remove(key);
    }
    if (json['safe_actions'] case final List<Object?> actions) {
      final sanitized = actions.whereType<String>().where(_safeToken).toList();
      if (sanitized.isEmpty) {
        json.remove('safe_actions');
      } else {
        json['safe_actions'] = sanitized;
      }
    }
    return json;
  }

  static bool _safeToken(String value) =>
      RegExp(r'^[a-zA-Z0-9_.:-]{1,128}$').hasMatch(value);

  static void _ignore(Object _) {}

  static String _newErrorId() {
    _nextErrorId += 1;
    final timestamp = DateTime.now().toUtc().microsecondsSinceEpoch;
    return 'floe-$timestamp-${_nextErrorId.toRadixString(36)}';
  }
}
