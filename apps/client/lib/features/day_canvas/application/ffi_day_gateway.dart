import 'dart:async';
import 'dart:convert';
import 'dart:ffi';
import 'dart:io';
import 'dart:isolate';

import 'package:ffi/ffi.dart';
import 'package:path_provider/path_provider.dart';
import 'package:flutter/services.dart';

import '../domain/day_models.dart';
import '../infrastructure/floe_native_bindings.dart';
import 'day_gateway.dart';
import 'calendar_gateway.dart';

const _protocolVersion = 1;
const localPersonId = '00000000-0000-4000-8000-000000000001';

final class FfiDayGatewayException implements Exception {
  const FfiDayGatewayException(this.code, this.message);

  final String code;
  final String message;

  @override
  String toString() => message;
}

final class FfiDayGateway implements DayGateway, CalendarGateway {
  FfiDayGateway._(
    this._isolate,
    this._commands,
    this._clock,
    this._calendarAdapter,
  ) {
    _finalizer.attach(this, _commands, detach: this);
  }

  static final Finalizer<SendPort> _finalizer = Finalizer(
    (commands) => commands.send(const {'operation': 'close'}),
  );

  final Isolate _isolate;
  final SendPort _commands;
  final DateTime Function() _clock;
  final CalendarAdapter _calendarAdapter;
  bool _closed = false;

  static Future<FfiDayGateway> openDefault() async {
    final supportDirectory = await getApplicationSupportDirectory();
    final databaseDirectory = Directory(
      '${supportDirectory.path}/people/$localPersonId',
    );
    await databaseDirectory.create(recursive: true);
    return open(
      libraryPath: resolveLibraryPath(),
      databasePath: '${databaseDirectory.path}/floe.db',
    );
  }

  static Future<FfiDayGateway> open({
    required String libraryPath,
    required String databasePath,
    DateTime Function()? clock,
    CalendarAdapter calendarAdapter = const EventKitCalendarAdapter(),
  }) async {
    final ready = ReceivePort();
    final isolate = await Isolate.spawn(_ffiWorkerMain, {
      'ready': ready.sendPort,
      'library_path': libraryPath,
      'database_path': databasePath,
    });
    final result = _asMap(await ready.first);
    ready.close();
    if (result['status'] != 'ok') {
      isolate.kill(priority: Isolate.immediate);
      throw _exceptionFromEnvelope(_asMap(result['error']));
    }
    return FfiDayGateway._(
      isolate,
      result['commands']! as SendPort,
      clock ?? DateTime.now,
      calendarAdapter,
    );
  }

  static String resolveLibraryPath() {
    final override = Platform.environment['FLOE_CORE_LIBRARY_PATH'];
    if (override != null && override.isNotEmpty) return override;
    if (!Platform.isMacOS) {
      throw UnsupportedError('FfiDayGateway는 현재 macOS만 지원합니다.');
    }
    final executableDirectory = File(Platform.resolvedExecutable).parent.path;
    return '$executableDirectory/../Frameworks/libfloe_ffi.dylib';
  }

  @override
  Future<DaySnapshot> loadDay(DayQuery query) async {
    final envelope = await _request('load_day', _loadRequest(query));
    return _decodeSnapshot(envelope);
  }

  @override
  Future<CaptureReceipt> submitCapture(String input, DayQuery query) async {
    final data = await _request(
      'execute',
      _commandRequest(query, {
        'type': 'submit_capture',
        'input': input,
        'occurred_at': _timestamp(_clock()),
      }),
    );
    return _decodeCapture(_asMap(data['capture']));
  }

  @override
  Future<DaySnapshot> classifyCapture(
    CaptureReceipt capture,
    ClassificationDraft classification,
    DayQuery query,
  ) async {
    final data = await _request(
      'execute',
      _commandRequest(query, {
        'type': 'classify_capture',
        'capture_id': capture.id,
        'expected_revision': capture.revision,
        'classification': _classification(classification),
        'occurred_at': _timestamp(_clock()),
      }),
    );
    return _decodeSnapshot(_asMap(data['snapshot']));
  }

  @override
  Future<DaySnapshot> setTaskCompleted(
    TaskItem task,
    bool completed,
    DayQuery query,
  ) async {
    final data = await _request(
      'execute',
      _commandRequest(query, {
        'type': 'set_task_completion',
        'task_id': task.id,
        'expected_revision': task.revision,
        'completed': completed,
        'occurred_at': _timestamp(_clock()),
      }),
    );
    return _decodeSnapshot(_asMap(data['snapshot']));
  }

  @override
  Future<DaySnapshot> deleteItem(DayItem item, DayQuery query) async {
    final data = await _request(
      'execute',
      _commandRequest(query, {
        'type': 'delete_item',
        'target': {'kind': item.kind.name, 'id': item.id},
        'expected_revision': item.revision,
        'occurred_at': _timestamp(_clock()),
      }),
    );
    return _decodeSnapshot(_asMap(data['snapshot']));
  }

  Future<void> close() async {
    if (_closed) return;
    _closed = true;
    _finalizer.detach(this);
    final reply = ReceivePort();
    _commands.send({'operation': 'close', 'reply': reply.sendPort});
    await reply.first;
    reply.close();
    _isolate.kill(priority: Isolate.immediate);
  }

  @override
  Future<List<CalendarChoice>> calendars() => _calendarAdapter.calendars();

  @override
  Future<void> openCalendarSettings() => _calendarAdapter.openSettings();

  @override
  Future<DaySnapshot> selectCalendar(
    CalendarChoice calendar,
    DayQuery query,
  ) async {
    final data = await _request(
      'execute',
      _commandRequest(query, {
        'type': 'select_calendar',
        'provider': calendar.provider,
        'calendar_id': calendar.id,
        'calendar_name': calendar.name,
      }),
    );
    return _decodeSnapshot(_asMap(data['snapshot']));
  }

  @override
  Future<DaySnapshot> syncCalendar(DayQuery query) async {
    final current = await loadDay(query);
    final connection = current.calendar;
    if (connection == null) return current;
    try {
      final records = await _calendarAdapter
          .read(connection.id, query)
          .timeout(const Duration(seconds: 20));
      final data = await _request(
        'execute',
        _commandRequest(query, {
          'type': 'import_calendar',
          'expected_revision': connection.revision,
          'occurred_at': _timestamp(_clock()),
          'range': {
            'start_date': _date(query.date),
            'end_date_exclusive': _date(
              DateTime.utc(
                query.date.year,
                query.date.month,
                query.date.day + 1,
              ),
            ),
            'timezone_offset_seconds': query.timezoneOffsetSeconds,
          },
          'records': records,
        }),
      );
      return _decodeSnapshot(_asMap(data['snapshot']));
    } on Object catch (error) {
      if (error is FfiDayGatewayException && error.code != 'validation') {
        rethrow;
      }
      final code = error is PlatformException
          ? error.code
          : 'provider_unavailable';
      final data = await _request(
        'execute',
        _commandRequest(query, {
          'type': 'calendar_failed',
          'expected_revision': connection.revision,
          'failure':
              ['permission_denied', 'calendar_unavailable'].contains(code)
              ? code
              : 'provider_unavailable',
        }),
      );
      return _decodeSnapshot(_asMap(data['snapshot']));
    }
  }

  Future<Map<String, dynamic>> _request(
    String operation,
    Map<String, dynamic> request,
  ) async {
    if (_closed) throw StateError('FfiDayGateway가 이미 닫혔습니다.');
    final reply = ReceivePort();
    _commands.send({
      'operation': operation,
      'request': jsonEncode(request),
      'reply': reply.sendPort,
    });
    final result = _asMap(await reply.first);
    reply.close();
    if (result['status'] != 'ok') {
      throw FfiDayGatewayException(
        'ffi',
        result['message']?.toString() ?? 'Rust core 호출에 실패했습니다.',
      );
    }
    return _unwrapEnvelope(result['response']! as String);
  }

  Map<String, dynamic> _loadRequest(DayQuery query) => {
    'schema_version': _protocolVersion,
    'person_id': query.personId,
    'day': _day(query),
  };

  Map<String, dynamic> _commandRequest(
    DayQuery query,
    Map<String, dynamic> command,
  ) => {
    'schema_version': _protocolVersion,
    'person_id': query.personId,
    'day': _day(query),
    'command': command,
  };

  Map<String, dynamic> _day(DayQuery query) {
    final now = _clock();
    return {
      'date': _date(query.date),
      'timezone_offset_seconds': query.timezoneOffsetSeconds,
      'now': _timestamp(now),
    };
  }
}

Map<String, dynamic> _classification(ClassificationDraft value) =>
    switch (value) {
      EventDraft(:final title, :final startsAt, :final endsAt) => {
        'kind': 'event',
        'title': title,
        'schedule': {
          'kind': 'timed',
          'starts_at': _timestamp(startsAt),
          'ends_at': _timestamp(endsAt),
          'timezone': _timezone(startsAt.timeZoneOffset),
        },
      },
      TaskDraft(:final title, :final deadline) => {
        'kind': 'task',
        'title': title,
        'deadline': deadline == null ? null : _timestamp(deadline),
        'priority': 'normal',
      },
      NoteDraft(:final content) => {'kind': 'note', 'content': content},
    };

Map<String, dynamic> _unwrapEnvelope(String source) {
  final envelope = _asMap(jsonDecode(source));
  if (envelope['schema_version'] != _protocolVersion) {
    throw const FfiDayGatewayException(
      'unsupported_version',
      '지원하지 않는 Rust protocol 버전입니다.',
    );
  }
  if (envelope['status'] == 'error') {
    throw _exceptionFromEnvelope(envelope);
  }
  if (envelope['status'] != 'ok') {
    throw const FormatException('알 수 없는 Rust response status입니다.');
  }
  return _asMap(envelope['data']);
}

FfiDayGatewayException _exceptionFromEnvelope(Map<String, dynamic> envelope) {
  final error = envelope['error'] is Map ? _asMap(envelope['error']) : envelope;
  return FfiDayGatewayException(
    error['code']?.toString() ?? 'internal',
    error['message']?.toString() ?? 'Rust core를 열 수 없습니다.',
  );
}

CaptureReceipt _decodeCapture(Map<String, dynamic> json) => CaptureReceipt(
  id: json['id']! as String,
  originalInput: json['original_input']! as String,
  capturedAt: DateTime.parse(json['captured_at']! as String),
  revision: json['revision']! as int,
);

DaySnapshot _decodeSnapshot(Map<String, dynamic> json) {
  if (json['schema_version'] != _protocolVersion) {
    throw const FfiDayGatewayException(
      'unsupported_version',
      '지원하지 않는 DaySnapshot 버전입니다.',
    );
  }
  return DaySnapshot(
    personId: json['person_id']! as String,
    date: DateTime.parse(json['date']! as String),
    generatedAt: DateTime.parse(json['generated_at']! as String),
    timezoneOffsetSeconds: json['timezone_offset_seconds']! as int,
    items: (json['items']! as List<Object?>)
        .map((item) => _decodeItem(_asMap(item)))
        .toList(growable: false),
    nowEventId: json['now_event_id'] as String?,
    nextEventId: json['next_event_id'] as String?,
    overdueTaskCount: json['overdue_task_count']! as int,
    calendar: json['calendar'] == null
        ? null
        : _decodeCalendar(_asMap(json['calendar'])),
  );
}

DayItem _decodeItem(Map<String, dynamic> json) {
  final createdAt = DateTime.parse(json['created_at']! as String);
  return switch (json['kind']) {
    'event' => _decodeEvent(json, createdAt),
    'task' => TaskItem(
      id: json['id']! as String,
      title: json['title']! as String,
      revision: json['revision']! as int,
      createdAt: createdAt,
      deadline: _optionalTimestamp(json['deadline']),
      completedAt: _optionalTimestamp(json['completed_at']),
      priority: _priority(json['priority']! as String),
    ),
    'note' => NoteItem(
      id: json['id']! as String,
      title: json['content']! as String,
      revision: json['revision']! as int,
      createdAt: createdAt,
    ),
    _ => throw FormatException('알 수 없는 timeline item kind: ${json['kind']}'),
  };
}

EventItem _decodeEvent(Map<String, dynamic> json, DateTime createdAt) {
  final schedule = _asMap(json['schedule']);
  final isAllDay = schedule['kind'] == 'all_day';
  final provenance = _asMap(json['source']);
  final source = provenance['kind'] == 'calendar'
      ? _asMap(provenance['source'])
      : null;
  return EventItem(
    id: json['id']! as String,
    title: json['title']! as String,
    revision: json['revision']! as int,
    createdAt: createdAt,
    startsAt: DateTime.parse(
      (isAllDay ? schedule['start_date'] : schedule['starts_at'])! as String,
    ),
    endsAt: DateTime.parse(
      (isAllDay ? schedule['end_date_exclusive'] : schedule['ends_at'])!
          as String,
    ),
    isAllDay: isAllDay,
    calendarName: source?['calendar_name'] as String?,
    externalId: source?['external_id'] as String?,
    provider: source?['provider'] as String?,
    timezone: schedule['timezone'] as String?,
  );
}

CalendarConnection _decodeCalendar(Map<String, dynamic> json) =>
    CalendarConnection(
      id: json['calendar_id']! as String,
      name: json['calendar_name']! as String,
      provider: json['provider']! as String,
      revision: json['revision']! as int,
      lastSuccessAt: _optionalTimestamp(json['last_success_at']),
      error: json['error'] as String?,
      rangeStart: (json['last_range'] as Map?)?['start_date'] as String?,
      rangeEnd: (json['last_range'] as Map?)?['end_date_exclusive'] as String?,
    );

TaskPriority _priority(String value) => switch (value) {
  'low' => TaskPriority.low,
  'normal' => TaskPriority.normal,
  'high' => TaskPriority.high,
  _ => throw FormatException('알 수 없는 task priority: $value'),
};

DateTime? _optionalTimestamp(Object? value) =>
    value == null ? null : DateTime.parse(value as String);

Map<String, dynamic> _asMap(Object? value) =>
    Map<String, dynamic>.from(value! as Map);

String _timestamp(DateTime value) => value.toUtc().toIso8601String();

String _date(DateTime value) =>
    '${value.year.toString().padLeft(4, '0')}-${value.month.toString().padLeft(2, '0')}-${value.day.toString().padLeft(2, '0')}';

String _timezone(Duration offset) {
  final sign = offset.isNegative ? '-' : '+';
  final minutes = offset.inMinutes.abs();
  final hours = (minutes ~/ 60).toString().padLeft(2, '0');
  final remainder = (minutes % 60).toString().padLeft(2, '0');
  return 'UTC$sign$hours:$remainder';
}

Future<void> _ffiWorkerMain(Map<String, Object?> configuration) async {
  final ready = configuration['ready']! as SendPort;
  FloeNativeBindings? bindings;
  Pointer<Void> handle = nullptr;
  try {
    bindings = FloeNativeBindings(configuration['library_path']! as String);
    if (bindings.protocolVersion() != _protocolVersion) {
      throw StateError('Rust protocol 버전이 Flutter와 일치하지 않습니다.');
    }
    final path = (configuration['database_path']! as String).toNativeUtf8();
    final error = calloc<Pointer<Utf8>>();
    try {
      handle = bindings.open(path, error);
      if (handle == nullptr) {
        final pointer = error.value;
        final source = pointer == nullptr ? null : pointer.toDartString();
        if (pointer != nullptr) bindings.freeString(pointer);
        throw source == null
            ? StateError('Rust core를 열 수 없습니다.')
            : _exceptionFromEnvelope(_asMap(jsonDecode(source)));
      }
    } finally {
      calloc.free(error);
      calloc.free(path);
    }
  } on Object catch (error) {
    ready.send({
      'status': 'error',
      'error': {'code': 'ffi_open', 'message': error.toString()},
    });
    return;
  }

  final commands = ReceivePort();
  ready.send({'status': 'ok', 'commands': commands.sendPort});
  await for (final raw in commands) {
    final message = _asMap(raw);
    final operation = message['operation'];
    if (operation == 'close') {
      bindings.freeCore(handle);
      (message['reply'] as SendPort?)?.send(true);
      commands.close();
      return;
    }
    final reply = message['reply']! as SendPort;
    try {
      final input = (message['request']! as String).toNativeUtf8();
      Pointer<Utf8> output = nullptr;
      try {
        output = operation == 'load_day'
            ? bindings.loadDay(handle, input)
            : bindings.execute(handle, input);
        if (output == nullptr) {
          throw StateError('Rust core가 빈 response를 반환했습니다.');
        }
        reply.send({'status': 'ok', 'response': output.toDartString()});
      } finally {
        if (output != nullptr) bindings.freeString(output);
        calloc.free(input);
      }
    } on Object catch (error) {
      reply.send({'status': 'error', 'message': error.toString()});
    }
  }
}
