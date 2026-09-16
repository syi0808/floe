import 'dart:async';

import 'package:flutter/services.dart';

import 'package:floe_client/app/local_identity.dart';
import 'package:floe_client/app/runtime/app_runtime.dart';
import 'package:floe_client/app/runtime/native_transport.dart' show nativeProtocolVersion;
import 'package:floe_client/features/day/application/calendar_gateway.dart';
import 'package:floe_client/features/day/application/calendar_observation_publisher.dart';
import 'package:floe_client/features/day/application/day_gateway.dart';
import 'package:floe_client/features/day/domain/day_models.dart';

const localPersonId = defaultLocalPersonId;

/// The Day timeline and its calendar mirror.
///
/// The transport, runtime client and read model live in [AppRuntime]; proposals
/// live in the Actions feature and the sample conversation in Conversation.
final class NativeDayGateway
    implements DayGateway, CalendarGateway, CalendarSystemAccessGateway {
  NativeDayGateway(
    this._runtime,
    this._calendarAdapter, {
    DateTime Function()? clock,
  }) : _clock = clock ?? DateTime.now,
       _deviceId = _runtime.deviceId,
       _calendarObservationPublisher = CalendarObservationPublisher(
         transport: _runtime.localContextTransport,
         deviceId: _runtime.deviceId,
       );

  final AppRuntime _runtime;
  final DateTime Function() _clock;
  final CalendarAdapter _calendarAdapter;
  final CalendarObservationPublisher _calendarObservationPublisher;
  final String _deviceId;
  Future<void> _calendarOperationTail = Future.value();

  /// Drains any calendar operation still in flight. Transport shutdown belongs
  /// to [AppRuntime].
  Future<void> drain() => _calendarOperationTail;

  @override
  Future<DaySnapshot> loadDay(DayQuery query) async {
    final envelope = await _runtime.request('load_day', _loadRequest(query));
    return _decodeSnapshot(envelope);
  }

  @override
  Future<CaptureReceipt> submitCapture(String input, DayQuery query) async {
    final data = await _runtime.request(
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
    final data = await _runtime.request(
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
    final data = await _runtime.request(
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
    final data = await _runtime.request(
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

  @override
  Future<List<CalendarChoice>> calendars() => _calendarAdapter.calendars();

  @override
  Future<CalendarSystemAccess> inspectCalendarAccess() async {
    try {
      await _calendarAdapter.calendars(requestAccess: false);
      return CalendarSystemAccess.allowed;
    } on PlatformException catch (error) {
      if (error.code == 'permission_denied') {
        return CalendarSystemAccess.denied;
      }
      return CalendarSystemAccess.unavailable;
    } on Object {
      return CalendarSystemAccess.unavailable;
    }
  }

  @override
  Future<void> openCalendarSettings() => _calendarAdapter.openSettings();

  @override
  Future<DaySnapshot> selectCalendar(CalendarChoice calendar, DayQuery query) =>
      selectCalendars([calendar], query);

  @override
  Future<DaySnapshot> selectCalendars(
    List<CalendarChoice> calendars,
    DayQuery query, {
    bool includeAll = false,
  }) async {
    if (calendars.isEmpty ||
        calendars.any(
          (calendar) => calendar.provider != calendars.first.provider,
        )) {
      throw ArgumentError('Select calendars from one provider');
    }
    final current = await loadDay(query);
    final connectionId = _deviceId.startsWith('local-')
        ? _deviceId.substring('local-'.length)
        : _deviceId;
    final data = await _runtime.request(
      'execute',
      _commandRequest(query, {
        'type': 'set_calendar_scope',
        'connection_id': connectionId,
        'connection_revision': (current.calendar?.revision ?? 0) + 1,
        'device_id': _deviceId,
        'scope': includeAll ? 'all' : 'selected',
        'provider': calendars.first.provider,
        'calendars': [
          for (final calendar in calendars)
            {'calendar_id': calendar.id, 'calendar_name': calendar.name},
        ],
      }),
    );
    return _decodeSnapshot(_asMap(data['snapshot']));
  }

  @override
  Future<DaySnapshot> bindCalendarConnection({
    required String connectionId,
    required int connectionRevision,
    required String deviceId,
    required String provider,
    required List<CalendarChoice> calendars,
    required DayQuery query,
  }) async {
    if (connectionId.isEmpty ||
        connectionRevision <= 0 ||
        deviceId.isEmpty ||
        calendars.isEmpty ||
        calendars.any((calendar) => calendar.provider != provider)) {
      throw ArgumentError('Invalid Calendar connection binding');
    }
    final data = await _runtime.request(
      'execute',
      _commandRequest(query, {
        'type': 'set_calendar_scope',
        'connection_id': connectionId,
        'connection_revision': connectionRevision,
        'device_id': deviceId,
        'scope': 'selected',
        'provider': provider,
        'calendars': [
          for (final calendar in calendars)
            {'calendar_id': calendar.id, 'calendar_name': calendar.name},
        ],
      }),
    );
    return _decodeSnapshot(_asMap(data['snapshot']));
  }

  @override
  Future<DaySnapshot> syncCalendar(DayQuery query) =>
      _runCalendarOperation(() => _syncCalendar(query));

  Future<DaySnapshot> _syncCalendar(DayQuery query) async {
    final current = await loadDay(query);
    var connection = current.calendar;
    if (connection == null) return current;
    final provider = connection.provider;
    try {
      final inventory = await _calendarAdapter
          .calendars(requestAccess: false)
          .timeout(const Duration(seconds: 20));
      final available = inventory
          .where((calendar) => calendar.provider == provider)
          .map((calendar) => calendar.id)
          .toSet();
      if (connection.includeAll) {
        final discovered = await _runtime.request(
          'execute',
          _commandRequest(query, {
            'type': 'discover_calendars',
            'expected_revision': connection.revision,
            'calendars': [
              for (final calendar in inventory.where(
                (calendar) => calendar.provider == provider,
              ))
                {'calendar_id': calendar.id, 'calendar_name': calendar.name},
            ],
          }),
        );
        connection = _decodeSnapshot(_asMap(discovered['snapshot'])).calendar!;
      }
      final active = connection;
      final batches = await Future.wait(
        active.selectedCalendarIds.map((calendarId) async {
          try {
            if (!available.contains(calendarId)) {
              throw PlatformException(code: 'calendar_unavailable');
            }
            final records = await _calendarAdapter
                .read(calendarId, query)
                .timeout(const Duration(seconds: 20));
            return {
              'calendar_id': calendarId,
              'records': [
                for (final record in records)
                  {...record, 'calendar_id': calendarId},
              ],
              'failure': null,
            };
          } on Object catch (error) {
            return {
              'calendar_id': calendarId,
              'records': <Object>[],
              'failure': _calendarFailure(error),
            };
          }
        }),
      );
      final data = await _runtime.request(
        'execute',
        _commandRequest(query, {
          'type': 'import_calendar_sources',
          'expected_revision': active.revision,
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
            'end_timezone_offset_seconds': query.endTimezoneOffsetSeconds,
          },
          'batches': batches,
        }),
      );
      final snapshot = _decodeSnapshot(_asMap(data['snapshot']));
      final syncedConnection = snapshot.calendar;
      if (syncedConnection != null) {
        connection = syncedConnection;
        final permissionRevoked =
            batches.isNotEmpty &&
            batches.every((batch) => batch['failure'] == 'permission_denied');
        await _updateCalendarObservation(
          query: query,
          connection: syncedConnection,
          batches: batches,
          permissionRevoked: permissionRevoked,
        );
      }
      return snapshot;
    } on Object catch (error) {
      if (_calendarObservationPublisher.supports(provider)) {
        await _revokeCalendarObservation(query.personId);
      }
      if (error is AppRuntimeException && error.code != 'validation') {
        rethrow;
      }
      final data = await _runtime.request(
        'execute',
        _commandRequest(query, {
          'type': 'calendar_failed',
          'expected_revision': connection!.revision,
          'failure': _calendarFailure(error),
        }),
      );
      return _decodeSnapshot(_asMap(data['snapshot']));
    }
  }

  Future<void> _updateCalendarObservation({
    required DayQuery query,
    required CalendarConnection connection,
    required List<Map<String, dynamic>> batches,
    required bool permissionRevoked,
  }) async {
    try {
      if (permissionRevoked) {
        await _calendarObservationPublisher.revoke(personId: query.personId);
      } else {
        await _calendarObservationPublisher.publish(
          personId: query.personId,
          connection: connection,
          observedAt: _clock().toUtc(),
          rangeStart: query.startsAt,
          rangeEnd: query.endsAt,
          batches: batches,
        );
      }
    } on Object {
      return;
    }
  }

  Future<void> _revokeCalendarObservation(String personId) async {
    try {
      await _calendarObservationPublisher.revoke(personId: personId);
    } on Object {
      return;
    }
  }

  @override
  Future<DaySnapshot> disconnectCalendar(DayQuery query) =>
      _runCalendarOperation(() => _disconnectCalendar(query));

  Future<DaySnapshot> _disconnectCalendar(DayQuery query) async {
    final current = await loadDay(query);
    if (current.calendar == null) return current;
    if (_calendarObservationPublisher.supports(current.calendar!.provider)) {
      await _calendarObservationPublisher.revoke(personId: query.personId);
    }
    final data = await _runtime.request(
      'execute',
      _commandRequest(query, {
        'type': 'disconnect_calendar',
        'expected_revision': current.calendar!.revision,
      }),
    );
    return _decodeSnapshot(_asMap(data['snapshot']));
  }

  Future<T> _runCalendarOperation<T>(Future<T> Function() operation) {
    final completer = Completer<T>();
    _calendarOperationTail = _calendarOperationTail
        .catchError((Object _) {})
        .then((_) async {
          try {
            completer.complete(await operation());
          } on Object catch (error, stackTrace) {
            completer.completeError(error, stackTrace);
          }
        });
    return completer.future;
  }



  Map<String, dynamic> _loadRequest(DayQuery query) => {
    'schema_version': nativeProtocolVersion,
    'person_id': query.personId,
    'day': _day(query),
  };

  Map<String, dynamic> _commandRequest(
    DayQuery query,
    Map<String, dynamic> command,
  ) => {
    'schema_version': nativeProtocolVersion,
    'person_id': query.personId,
    'day': _day(query),
    'command': command,
  };

  Map<String, dynamic> _day(DayQuery query) {
    final now = _clock();
    return {
      'date': _date(query.date),
      'timezone_offset_seconds': query.timezoneOffsetSeconds,
      'end_timezone_offset_seconds': query.endTimezoneOffsetSeconds,
      'now': _timestamp(now),
    };
  }
}

String _calendarFailure(Object error) {
  final code = error is PlatformException ? error.code : 'provider_unavailable';
  return ['permission_denied', 'calendar_unavailable'].contains(code)
      ? code
      : 'provider_unavailable';
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

CalendarConnection _decodeCalendar(Map<String, dynamic> json) =>
    CalendarConnection(
      connectionId: json['connection_id']! as String,
      deviceId: json['device_id']! as String,
      calendars: (json['calendars']! as List)
          .map(
            (calendar) => ConnectedCalendar(
              id: calendar['calendar_id'] as String,
              name: calendar['calendar_name'] as String,
              error:
                  (json['source_statuses']
                          as Map?)?[calendar['calendar_id']]?['error']
                      as String?,
              lastSuccessAt: _optionalTimestamp(
                (json['source_statuses']
                    as Map?)?[calendar['calendar_id']]?['last_success_at'],
              ),
            ),
          )
          .toList(),
      provider: json['provider']! as String,
      includeAll: json['scope'] == 'all',
      revision: json['revision']! as int,
      sourceAuthority: _optionalSourceAuthority(json['source_authority']),
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

CalendarSourceAuthority? _optionalSourceAuthority(Object? value) {
  if (value == null) return null;
  try {
    return CalendarSourceAuthority.fromJson(value);
  } on FormatException {
    return null;
  }
}

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
