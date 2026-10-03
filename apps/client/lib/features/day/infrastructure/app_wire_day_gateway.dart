import 'package:floe_client/app/runtime/owner_failure.dart';
import 'package:floe_client/app/runtime/app_runtime.dart';
import 'package:floe_client/app/runtime/native_transport.dart'
    show NativeTransportException, nativeProtocolVersion;
import 'package:floe_client/app/runtime/owner_operation.dart';
import 'package:floe_client/features/conversation/application/agent_request_id.dart';
import 'package:floe_client/features/day/application/day_gateway.dart';
import 'package:floe_client/features/day/domain/day_models.dart';

/// The Day timeline and its calendar mirror.
///
/// The transport, runtime client and read model live in [AppRuntime]; proposals
/// live in the Actions feature and the sample conversation in Conversation.
final class AppWireDayGateway implements DayGateway, DayRefreshGateway {
  AppWireDayGateway(this._runtime, {DateTime Function()? clock}) : _clock = clock ?? DateTime.now;
  final AppRuntime _runtime;
  final DateTime Function() _clock;

  @override
  Future<DayRefreshSnapshot> refreshDay({required String commandId, required DayQuery query}) async {
    final result = await ownerCommand(_runtime.wireTransport, commandId, {
      'kind': 'day.refresh', 'day': _day(query),
    });
    return _refreshSnapshot(result);
  }

  @override
  Future<DayRefreshSnapshot> observeDayRefresh(String operationRef) async {
    final result = await ownerQuery(_runtime.wireTransport, newAgentRequestId(), {
      'kind': 'day.refresh.get', 'operation_ref': operationRef,
    });
    final snapshot = _refreshSnapshot(result);
    if (snapshot.operationRef != operationRef) throw const FormatException('Day refresh identity changed.');
    return snapshot;
  }

  @override
  Future<DaySnapshot> loadDay(DayQuery query) async {
    final envelope = await _load(query);
    return _decodeSnapshot(envelope);
  }

  @override
  Future<CaptureReceipt> submitCapture(String input, DayQuery query) async {
    final data = await _mutate(query, {
      'type': 'submit_capture',
      'input': input,
      'occurred_at': _timestamp(_clock()),
    });
    return _decodeCapture(_asMap(data['capture']));
  }

  @override
  Future<DaySnapshot> classifyCapture(
    CaptureReceipt capture,
    ClassificationDraft classification,
    DayQuery query,
  ) async {
    final data = await _mutate(query, {
      'type': 'classify_capture',
      'capture_id': capture.id,
      'expected_revision': capture.revision,
      'classification': _classification(classification),
      'occurred_at': _timestamp(_clock()),
    });
    return _decodeSnapshot(_asMap(data['snapshot']));
  }

  @override
  Future<DaySnapshot> setTaskCompleted(
    TaskItem task,
    bool completed,
    DayQuery query,
  ) async {
    final data = await _mutate(query, {
      'type': 'set_task_completion',
      'task_id': task.id,
      'expected_revision': task.revision,
      'completed': completed,
      'occurred_at': _timestamp(_clock()),
    });
    return _decodeSnapshot(_asMap(data['snapshot']));
  }

  @override
  Future<DaySnapshot> deleteItem(DayItem item, DayQuery query) async {
    final data = await _mutate(query, {
      'type': 'delete_item',
      'target': {'kind': item.kind.name, 'id': item.id},
      'expected_revision': item.revision,
      'occurred_at': _timestamp(_clock()),
    });
    return _decodeSnapshot(_asMap(data['snapshot']));
  }

  Future<Map<String, dynamic>> _load(DayQuery query) async {
    try {
      final result = await ownerQuery(
        _runtime.wireTransport,
        newAgentRequestId(),
        {'kind': 'day.snapshot', 'day': _day(query)},
      );
      if (result['kind'] != 'day_snapshot') {
        throw const FormatException('Invalid Day result');
      }
      return _asMap(result['snapshot']);
    } on NativeTransportException catch (error) {
      throw AppRuntimeException(
        error.metadata['owner_code'] ?? error.code,
        error.message,
        field: error.field,
        metadata: error.metadata,
      );
    }
  }

  Future<Map<String, dynamic>> _mutate(
    DayQuery query,
    Map<String, dynamic> mutation,
  ) async {
    final commandId = newAgentRequestId();
    try {
      final result = await ownerCommand(_runtime.wireTransport, commandId, {
        'kind': 'day.mutate',
        'day': _day(query),
        'mutation': mutation,
      });
      if (result['kind'] != 'day_mutation' ||
          result['command_id'] != commandId) {
        throw const FormatException('Invalid Day mutation correlation');
      }
      return _asMap(result['mutation']);
    } on NativeTransportException catch (error) {
      throw AppRuntimeException(
        error.metadata['owner_code'] ?? error.code,
        error.message,
        field: error.field,
        metadata: error.metadata,
      );
    }
  }

  Map<String, dynamic> _day(DayQuery query) {
    final now = query.now;
    return {
      'date': _date(query.date),
      'timezone_offset_seconds': query.timezoneOffsetSeconds,
      'end_timezone_offset_seconds': query.endTimezoneOffsetSeconds,
      'now': _timestamp(now),
    };
  }
}

DayRefreshSnapshot _refreshSnapshot(Map<String, dynamic> result) {
  if (result.length != 2 || result['kind'] != 'day.refresh' || result['refresh'] is! Map) {
    throw const FormatException('Invalid Day refresh result.');
  }
  final value = _asMap(result['refresh']);
  final state = value['state'];
  final fields = {'operation_ref','revision','state',
    if (state == 'completed') 'day',
    if (state == 'failed' || state == 'interrupted') 'failure'};
  final operation = value['operation_ref'];
  final revision = value['revision'];
  if (value.length != fields.length || !value.keys.toSet().containsAll(fields) ||
      operation is! String || !RegExp(r'^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$').hasMatch(operation) ||
      revision is! int || revision < 1) {
    throw const FormatException('Invalid Day refresh snapshot.');
  }
  return switch (state) {
    'pending' => PendingDayRefresh(operationRef: operation, revision: revision),
    'running' => RunningDayRefresh(operationRef: operation, revision: revision),
    'completed' => CompletedDayRefresh(operationRef: operation, revision: revision, day: _decodeSnapshot(_asMap(value['day']))),
    'failed' => FailedDayRefresh(operationRef: operation, revision: revision, failure: OwnerFailure.fromJson(value['failure'])),
    'interrupted' => InterruptedDayRefresh(operationRef: operation, revision: revision, failure: OwnerFailure.fromJson(value['failure'])),
    _ => throw const FormatException('Unknown Day refresh state.'),
  };
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

CaptureReceipt _decodeCapture(Map<String, dynamic> json) => CaptureReceipt(
  id: json['id']! as String,
  originalInput: json['original_input']! as String,
  capturedAt: DateTime.parse(json['captured_at']! as String),
  revision: json['revision']! as int,
);

DaySnapshot _decodeSnapshot(Map<String, dynamic> json) {
  if (json['schema_version'] != nativeProtocolVersion) {
    throw const AppRuntimeException(
      'unsupported_version',
      'Unsupported DaySnapshot version.',
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
        : _decodeCalendarMirror(_asMap(json['calendar'])),
    calendarMirrorRevision: json['calendar_mirror_revision'] as int?,
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
    _ => throw FormatException('Unknown timeline item kind: ${json['kind']}'),
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
    calendarId: source == null ? null : source['calendar_id'] as String,
    calendarName: source == null ? null : source['calendar_name'] as String,
    externalId: source == null ? null : source['external_id'] as String,
    canModify: source == null ? false : source['can_modify'] as bool,
    provider: source == null ? null : source['provider'] as String,
    timezone: schedule['timezone'] as String?,
  );
}

CalendarMirrorState _decodeCalendarMirror(Map<String, dynamic> json) =>
    CalendarMirrorState(
      sourceConnectionId: json['source_connection_id']! as String,
      provider: json['provider']! as String,
      sourceStatuses: {
        for (final entry in _asMap(json['source_statuses']).entries)
          entry.key: CalendarSyncStatus(
            error: _asMap(entry.value)['error'] as String?,
            lastSuccessAt: _optionalTimestamp(
              _asMap(entry.value)['last_success_at'],
            ),
          ),
      },
      lastSuccessAt: _optionalTimestamp(json['last_success_at']),
      error: json['error'] as String?,
      rangeStart: (json['last_range'] as Map?)?['start_date'] as String?,
      rangeEnd: (json['last_range'] as Map?)?['end_date_exclusive'] as String?,
    );

TaskPriority _priority(String value) => switch (value) {
  'low' => TaskPriority.low,
  'normal' => TaskPriority.normal,
  'high' => TaskPriority.high,
  _ => throw FormatException('Unknown task priority: $value'),
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
