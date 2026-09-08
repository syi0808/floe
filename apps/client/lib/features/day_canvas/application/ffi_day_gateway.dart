import 'dart:async';
import 'dart:convert';
import 'dart:ffi';
import 'dart:io';
import 'dart:isolate';

import 'package:ffi/ffi.dart';
import 'package:path_provider/path_provider.dart';
import 'package:flutter/services.dart';

import '../domain/day_models.dart';
import '../domain/calendar_action.dart';
import '../infrastructure/floe_native_bindings.dart';
import 'day_gateway.dart';
import 'calendar_gateway.dart';
import 'calendar_action_gateway.dart';
import '../../server/local_server_client.dart';
import '../../agent/agent_fixture_gateway.dart';
import '../../agent/agent_vault_gateway.dart';

const _protocolVersion = 1;
const localPersonId = '00000000-0000-4000-8000-000000000001';

final class FfiDayGatewayException implements Exception {
  const FfiDayGatewayException(this.code, this.message);

  final String code;
  final String message;

  @override
  String toString() => message;
}

final class FfiDayGateway
    implements
        DayGateway,
        CalendarGateway,
        CalendarActionExecutionGateway,
        CalendarDirectActionGateway,
        AgentFixtureStreamingGateway {
  FfiDayGateway._(
    this._isolate,
    this._commands,
    this._clock,
    this._calendarAdapter,
    this.serverClient,
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
  final LocalServerClient serverClient;
  bool _closed = false;
  late final AgentVaultGateway secureAgent = NativeAgentVaultGateway(
    _vaultRequest,
    resolveRemoteRoute: _remoteRoute,
  );

  Future<Map<String, Object?>?> _remoteRoute() async {
    try {
      final connection = await serverClient.connection();
      if (connection == null) return null;
      final availability = await serverClient.purposes(connection);
      final route = availability[InferencePurpose.everydayAssistance];
      if (route == null || !route.available) return null;
      return {
        'base_url': connection.address,
        'bearer_token': connection.token,
        'purpose': InferencePurpose.everydayAssistance.wireName,
        'external': route.requiresExternalConsent,
        'allow_external': connection.allowExternal,
      };
    } on ServerConnectionException {
      return null;
    }
  }

  Future<Map<String, dynamic>> _vaultRequest(
    Map<String, Object?> request,
  ) async {
    try {
      return await _request('agent_vault', request);
    } on FfiDayGatewayException catch (error) {
      throw AgentVaultException(error.code);
    }
  }

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
    LocalServerClient? serverClient,
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
      serverClient ?? LocalServerClient.shared,
    );
  }

  static String resolveLibraryPath() {
    final override = Platform.environment['FLOE_CORE_LIBRARY_PATH'];
    if (override != null && override.isNotEmpty) return override;
    if (!Platform.isMacOS) {
      throw UnsupportedError('FfiDayGateway currently supports macOS only.');
    }
    final executableDirectory = File(Platform.resolvedExecutable).parent.path;
    return '$executableDirectory/../Frameworks/libfloe_ffi.dylib';
  }

  @override
  Future<AgentFixtureResult> startAgentFixture(String personId) =>
      _agentFixture(personId, {'kind': 'start'});

  @override
  Future<AgentFixtureResult> resumeAgentFixture(String personId) =>
      _agentFixture(personId, {'kind': 'resume'});

  @override
  Future<AgentRunUpdate> beginAgentFixtureRun(
    AgentSession session,
    AgentFixturePrompt prompt,
  ) => _agentRun(session, {'kind': 'begin', 'prompt': prompt.wireName});

  @override
  Future<AgentRunUpdate> pollAgentFixtureRun(
    AgentSession session,
    int afterSequence,
  ) => _agentRun(session, {'kind': 'poll', 'after_sequence': afterSequence});

  @override
  Future<AgentRunUpdate> stopAgentFixtureRun(AgentSession session) =>
      _agentRun(session, {'kind': 'stop'});

  @override
  Future<AgentRunUpdate> releaseAgentFixtureRun(AgentSession session) =>
      _agentRun(session, {'kind': 'release'});

  Future<AgentRunUpdate> _agentRun(
    AgentSession session,
    Map<String, Object?> operation,
  ) async => AgentRunUpdate.fromJson(
    await _request('agent_fixture_run', {
      'schema_version': agentSchemaVersion,
      'person_id': session.personId,
      'session_id': session.id,
      'expected_revision': session.revision,
      'operation': operation,
    }),
  );

  @override
  Future<AgentFixtureResult> loadAgentFixture(
    String personId,
    String sessionId,
  ) => _agentFixture(personId, {'kind': 'get', 'session_id': sessionId});

  @override
  Future<AgentFixtureResult> runAgentFixture(
    AgentSession session,
    AgentFixturePrompt prompt,
  ) => _agentFixture(session.personId, {
    'kind': 'turn',
    'session_id': session.id,
    'expected_revision': session.revision,
    'prompt': prompt.wireName,
  });

  @override
  Future<AgentFixtureResult> recoverAgentFixture(AgentSession session) =>
      _agentFixture(session.personId, {
        'kind': 'recover',
        'session_id': session.id,
        'expected_revision': session.revision,
      });

  Future<AgentFixtureResult> _agentFixture(
    String personId,
    Map<String, Object?> operation,
  ) async => AgentFixtureResult.fromJson(
    await _request('agent_fixture', {
      'schema_version': agentSchemaVersion,
      'person_id': personId,
      'operation': operation,
    }),
  );

  @override
  Future<List<CalendarAction>> loadCalendarActions(String personId) =>
      _calendarActions(personId, {'kind': 'list'});

  Future<CalendarAction> loadCalendarAction(
    String personId,
    String actionId,
  ) async => (await _calendarActions(personId, {
    'kind': 'get',
    'action_id': actionId,
  })).single;

  @override
  Future<CalendarAction> submitDirectCalendarAction({
    required String personId,
    required String calendarId,
    required String title,
    required DateTime startsAt,
    required DateTime endsAt,
    required String timezone,
    String? eventId,
    int? eventRevision,
    bool delete = false,
  }) async => (await _calendarActions(personId, {
    'kind': 'direct',
    'calendar_id': calendarId,
    'title': title,
    'starts_at': startsAt.toUtc().toIso8601String(),
    'ends_at': endsAt.toUtc().toIso8601String(),
    'timezone': timezone,
    'event_id': eventId,
    'event_revision': eventRevision,
    'delete': delete,
  })).single;

  @override
  Future<CalendarAction> proposeCalendarAction({
    required String personId,
    required String calendarId,
    required String title,
    required DateTime startsAt,
    required DateTime endsAt,
    required String timezone,
  }) async => (await _calendarActions(personId, {
    'kind': 'propose',
    'calendar_id': calendarId,
    'title': title,
    'starts_at': startsAt.toUtc().toIso8601String(),
    'ends_at': endsAt.toUtc().toIso8601String(),
    'timezone': timezone,
  })).single;

  @override
  Future<CalendarAction> decideCalendarAction({
    required String personId,
    required String actionId,
    required CalendarActionDecision decision,
  }) async => (await _calendarActions(personId, {
    'kind': 'decide',
    'action_id': actionId,
    'decision': decision.name,
  })).single;

  Future<List<CalendarAction>> _calendarActions(
    String personId,
    Map<String, dynamic> operation,
  ) async {
    final data = await _request('calendar_actions', {
      'schema_version': _protocolVersion,
      'person_id': personId,
      'operation': operation,
    });
    return (data['actions']! as List)
        .map((value) => CalendarAction.fromJson(_asMap(value)))
        .toList(growable: false);
  }

  @override
  Future<bool> calendarWritesEnabled(String personId) async {
    final data = await _request('calendar_actions', {
      'schema_version': _protocolVersion,
      'person_id': personId,
      'operation': {'kind': 'capabilities'},
    });
    return data['writes_enabled'] == true;
  }

  @override
  Future<ActionAuthority> loadActionAuthority(String personId) async {
    final data = await _request('calendar_actions', {
      'schema_version': _protocolVersion,
      'person_id': personId,
      'operation': {'kind': 'get_authority'},
    });
    return ActionAuthority.fromJson(_asMap(data['authority']));
  }

  @override
  Future<ActionAuthority> setCalendarCreateAuthority(
    String personId,
    ActionAuthorityMode mode,
  ) async {
    final data = await _request('calendar_actions', {
      'schema_version': _protocolVersion,
      'person_id': personId,
      'operation': {'kind': 'set_authority', 'calendar_create': mode.name},
    });
    return ActionAuthority.fromJson(_asMap(data['authority']));
  }

  @override
  Future<CalendarAction> executeCalendarAction(
    String personId,
    String actionId,
  ) async => (await _calendarActions(personId, {
    'kind': 'execute',
    'action_id': actionId,
  })).single;

  @override
  Future<CalendarAction> recoverCalendarAction(
    String personId,
    String actionId,
  ) async => (await _calendarActions(personId, {
    'kind': 'recover',
    'action_id': actionId,
  })).single;

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
    final data = await _request(
      'execute',
      _commandRequest(query, {
        'type': 'set_calendar_scope',
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
  Future<DaySnapshot> syncCalendar(DayQuery query) async {
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
        final discovered = await _request(
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
      final data = await _request(
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
      return _decodeSnapshot(_asMap(data['snapshot']));
    } on Object catch (error) {
      if (error is FfiDayGatewayException && error.code != 'validation') {
        rethrow;
      }
      final data = await _request(
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

  @override
  Future<DaySnapshot> disconnectCalendar(DayQuery query) async {
    final current = await loadDay(query);
    if (current.calendar == null) return current;
    final data = await _request(
      'execute',
      _commandRequest(query, {
        'type': 'disconnect_calendar',
        'expected_revision': current.calendar!.revision,
      }),
    );
    return _decodeSnapshot(_asMap(data['snapshot']));
  }

  Future<Map<String, dynamic>> _request(
    String operation,
    Map<String, dynamic> request,
  ) async {
    if (_closed) throw StateError('FfiDayGateway is already closed.');
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
        result['message']?.toString() ?? 'The Rust core request failed.',
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

Map<String, dynamic> _unwrapEnvelope(String source) {
  final envelope = _asMap(jsonDecode(source));
  if (envelope['schema_version'] != _protocolVersion) {
    throw const FfiDayGatewayException(
      'unsupported_version',
      'Unsupported Rust protocol version.',
    );
  }
  if (envelope['status'] == 'error') {
    throw _exceptionFromEnvelope(envelope);
  }
  if (envelope['status'] != 'ok') {
    throw const FormatException('Unknown Rust response status.');
  }
  return _asMap(envelope['data']);
}

FfiDayGatewayException _exceptionFromEnvelope(Map<String, dynamic> envelope) {
  final error = envelope['error'] is Map ? _asMap(envelope['error']) : envelope;
  return FfiDayGatewayException(
    error['code']?.toString() ?? 'internal',
    error['message']?.toString() ?? 'Could not open Rust core.',
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
  final source = ['calendar', 'external'].contains(provenance['kind'])
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
    calendarId: source?['calendar_id'] as String?,
    calendarName: source == null
        ? null
        : source['calendar_name'] as String? ?? 'Previous calendar connection',
    externalId: source?['external_id'] as String?,
    canModify: source?['can_modify'] as bool? ?? false,
    provider: source?['provider'] as String?,
    timezone: schedule['timezone'] as String?,
  );
}

CalendarConnection _decodeCalendar(Map<String, dynamic> json) =>
    CalendarConnection(
      id: json['calendar_id']! as String,
      name: json['calendar_name']! as String,
      calendarIds:
          (json['calendars'] as List?)
              ?.map((calendar) => calendar['calendar_id'] as String)
              .toList() ??
          const [],
      calendars:
          (json['calendars'] as List?)
              ?.map(
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
              .toList() ??
          const [],
      provider: json['provider']! as String,
      includeAll: json['scope'] == 'all',
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

Future<void> _ffiWorkerMain(Map<String, Object?> configuration) async {
  final ready = configuration['ready']! as SendPort;
  FloeNativeBindings? bindings;
  Pointer<Void> handle = nullptr;
  try {
    bindings = FloeNativeBindings(configuration['library_path']! as String);
    if (bindings.protocolVersion() != _protocolVersion) {
      throw StateError('Rust protocol version does not match Flutter.');
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
            ? StateError('Could not open Rust core.')
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
        output = switch (operation) {
          'load_day' => bindings.loadDay(handle, input),
          'execute' => bindings.execute(handle, input),
          'calendar_actions' => bindings.calendarActions(handle, input),
          'agent_fixture' => bindings.agentFixture(handle, input),
          'agent_fixture_run' => bindings.agentFixtureRun(handle, input),
          'agent_vault' => bindings.agentVault(handle, input),
          _ => throw StateError('Unknown core operation: $operation'),
        };
        if (output == nullptr) {
          throw StateError('Rust core returned an empty response.');
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
