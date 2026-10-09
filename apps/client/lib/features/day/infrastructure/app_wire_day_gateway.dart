import 'dart:convert';

import 'package:floe_client/app/runtime/app_runtime.dart';
import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/app/runtime/owner_operation.dart';
import 'package:floe_client/features/conversation/application/agent_request_id.dart';
import 'package:floe_client/features/day/application/day_gateway.dart';
import 'package:floe_client/features/day/domain/day_models.dart';

const _maxDayItems = 10000;
const _maxDaySnapshotBytes = 4 * 1024 * 1024;
const _maxRevision = 0x7fffffffffffffff;
final _uuidPattern = RegExp(
  r'^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$',
);
final _timestampPattern = RegExp(
  r'^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$',
);

/// The Day timeline, safe Calendar coverage, and durable refresh owner client.
final class AppWireDayGateway implements DayGateway, DayRefreshGateway {
  AppWireDayGateway(this._runtime, {DateTime Function()? clock})
    : _clock = clock ?? DateTime.now;

  final AppRuntime _runtime;
  final DateTime Function() _clock;
  _PendingDayMutation? _pendingMutation;
  _PendingExternalCalendarOperation? _pendingExternalCalendarOperation;
  final Map<String, _PendingRefreshCommand> _pendingRefreshes = {};

  @override
  Future<DayRefreshSnapshot> refreshDay({
    required String commandId,
    required DayQuery query,
  }) async {
    final intent = _queryIntent(query);
    var pending = _pendingRefreshes[intent];
    if (pending == null) {
      if (_pendingRefreshes.length >= 4096) {
        throw StateError('Too many unconfirmed Day refresh commands.');
      }
      if (_pendingRefreshes.values.any(
        (value) => value.commandId == commandId,
      )) {
        throw StateError('A Day refresh command changed its query.');
      }
      pending = _PendingRefreshCommand(
        commandId,
        query,
        Map<String, dynamic>.unmodifiable(_day(query)),
      );
      _pendingRefreshes[intent] = pending;
    }
    final active = pending;
    final previouslySubmitted = active.submitted;
    active.submitted = true;
    try {
      final result = await ownerCommand(
        _runtime.wireTransport,
        active.commandId,
        {'kind': 'day.refresh', 'day': active.day},
      );
      final snapshot = _refreshSnapshot(result);
      _acceptRefresh(intent, active, snapshot, result);
      return snapshot;
    } on AppWireTransportException catch (error) {
      if (mayDiscardPendingCommand(
            error,
            previouslySubmitted: previouslySubmitted,
          ) &&
          identical(_pendingRefreshes[intent], active)) {
        _pendingRefreshes.remove(intent);
      }
      // The original body stays available when this screen is replaced.
      throw _runtimeError(error);
    }
  }

  @override
  Future<DayRefreshSnapshot> observeDayRefresh(String operationRef) async {
    final result = await ownerQuery(
      _runtime.wireTransport,
      newAgentRequestId(),
      {'kind': 'day.refresh.get', 'operation_ref': operationRef},
    );
    final snapshot = _refreshSnapshot(result);
    if (snapshot.operationRef != operationRef) {
      throw const FormatException('Day refresh identity changed.');
    }
    for (final entry in _pendingRefreshes.entries) {
      if (entry.value.observed?.operationRef == operationRef) {
        _acceptRefresh(entry.key, entry.value, snapshot, result);
        break;
      }
    }
    return snapshot;
  }

  void _acceptRefresh(
    String intent,
    _PendingRefreshCommand pending,
    DayRefreshSnapshot next,
    Map<String, dynamic> wire,
  ) {
    final encoded = jsonEncode(_canonicalJson(wire));
    final previous = pending.observed;
    if (previous != null &&
        (next.operationRef != previous.operationRef ||
            next.revision < previous.revision ||
            (next.revision == previous.revision &&
                pending.observedWire != encoded) ||
            (previous.terminal && next.revision != previous.revision) ||
            (previous is RunningDayRefresh && next is PendingDayRefresh))) {
      throw const FormatException('Day refresh observation regressed.');
    }
    if (next is CompletedDayRefresh &&
        (next.day.personId != pending.query.personId ||
            _date(next.day.date) != _date(pending.query.date) ||
            next.day.timezoneOffsetSeconds !=
                pending.query.timezoneOffsetSeconds)) {
      throw const FormatException('Day refresh returned a different query.');
    }
    pending.observed = next;
    pending.observedWire = encoded;
    if (next.terminal && identical(_pendingRefreshes[intent], pending)) {
      _pendingRefreshes.remove(intent);
    }
  }

  @override
  Future<DaySnapshot> loadDay(DayQuery query) async {
    final json = await _load(query);
    return _decodeSnapshot(
      json,
      expectedPersonId: query.personId,
      expectedQuery: query,
    );
  }

  @override
  Future<List<ManualCalendarDestination>> loadExternalCalendarDestinations()
  async {
    final result = await ownerQuery(
      _runtime.wireTransport,
      newAgentRequestId(),
      const {'kind': 'day.calendar_destinations'},
    );
    _requireKeys(result, const {'kind', 'destinations'}, 'Day destinations');
    if (result['kind'] != 'day.calendar_destinations' ||
        result['destinations'] is! List ||
        (result['destinations'] as List).length > 64) {
      throw const FormatException('Invalid Day Calendar destinations.');
    }
    final raw = result['destinations'] as List;
    final destinations = raw.map((value) {
      final item = _asMap(value, 'Calendar destination');
      _requireKeys(
        item,
        const {'destination_ref', 'label'},
        'Calendar destination',
      );
      if (item['destination_ref'] is! String ||
          item['label'] is! String ||
          (item['label'] as String).trim().isEmpty ||
          (item['label'] as String).length > 512) {
        throw const FormatException('Invalid Day Calendar destination.');
      }
      return ManualCalendarDestination(
        destinationRef: item['destination_ref'] as String,
        label: item['label'] as String,
      );
    }).toList(growable: false);
    if (destinations.map((value) => value.destinationRef).toSet().length !=
        destinations.length) {
      throw const FormatException('Duplicate Day Calendar destinations.');
    }
    return List.unmodifiable(destinations);
  }

  @override
  Future<ManualCalendarOperationPage> loadExternalCalendarOperations({
    String? cursor,
    int limit = 100,
  }) async {
    if (limit < 1 || limit > 100) {
      throw ArgumentError.value(limit, 'limit');
    }
    if (cursor != null && !_uuidPattern.hasMatch(cursor)) {
      throw ArgumentError.value(cursor, 'cursor');
    }
    final result = await ownerQuery(
      _runtime.wireTransport,
      newAgentRequestId(),
      {'kind': 'day.external_calendar_operations', 'cursor': cursor, 'limit': limit},
    );
    _requireKeys(result, const {'kind', 'operations'}, 'Day operations');
    if (result['kind'] != 'day.external_calendar_operations' ||
        result['operations'] is! Map) {
      throw const FormatException('Invalid Day Calendar operations.');
    }
    final payload = _asMap(result['operations'], 'Day operations');
    _requireKeys(payload, const {'operations', 'next_cursor'}, 'Day operations');
    final raw = payload['operations'];
    if (raw is! List || raw.length > limit) {
      throw const FormatException('Invalid Day Calendar operation list.');
    }
    final values = raw
        .map((item) => ManualCalendarOperationReceipt.fromJson(
              _asMap(item, 'Day operation'),
            ))
        .toList(growable: false);
    if (values.map((value) => value.operationRef).toSet().length != values.length) {
      throw const FormatException('Duplicate Day Calendar operation.');
    }
    final nextCursor = payload['next_cursor'] == null
        ? null
        : _requiredUuid(payload['next_cursor'], 'operations.next_cursor');
    if (nextCursor != null &&
        (values.isEmpty || values.last.operationRef != nextCursor)) {
      throw const FormatException('Invalid Day Calendar operation cursor.');
    }
    return ManualCalendarOperationPage(
      operations: List.unmodifiable(values),
      nextCursor: nextCursor,
    );
  }

  @override
  Future<ManualCalendarOperationReceipt> inspectExternalCalendarOperation(
    String operationRef,
  ) async {
    if (!_uuidPattern.hasMatch(operationRef)) {
      throw ArgumentError.value(operationRef, 'operationRef');
    }
    final result = await ownerQuery(
      _runtime.wireTransport,
      newAgentRequestId(),
      {'kind': 'day.external_calendar_operation.get', 'operation_ref': operationRef},
    );
    _requireKeys(result, const {'kind', 'operation'}, 'Day operation');
    if (result['kind'] != 'day.external_calendar_operation') {
      throw const FormatException('Invalid Day Calendar operation kind.');
    }
    final receipt = ManualCalendarOperationReceipt.fromJson(
      _asMap(result['operation'], 'Day operation'),
    );
    if (receipt.operationRef != operationRef) {
      throw const FormatException('Day Calendar operation identity changed.');
    }
    return receipt;
  }

  @override
  Future<ManualCalendarOperationReceipt> reconcileExternalCalendarOperation(
    String operationRef,
    int expectedRevision,
  ) async {
    if (!_uuidPattern.hasMatch(operationRef) || expectedRevision <= 0 ||
        expectedRevision > _maxRevision) {
      throw ArgumentError('Invalid Day Calendar operation identity or revision.');
    }
    final result = await ownerCommand(
      _runtime.wireTransport,
      newAgentRequestId(),
      {
        'kind': 'day.external_calendar_operation.reconcile',
        'operation_ref': operationRef,
        'expected_revision': expectedRevision,
      },
    );
    _requireKeys(result, const {'kind', 'operation'}, 'Day operation');
    if (result['kind'] != 'day.external_calendar_operation') {
      throw const FormatException('Invalid Day Calendar operation kind.');
    }
    final receipt = ManualCalendarOperationReceipt.fromJson(
      _asMap(result['operation'], 'Day operation'),
    );
    if (receipt.operationRef != operationRef) {
      throw const FormatException('Day Calendar operation identity changed.');
    }
    return receipt;
  }

  @override
  Future<ManualCalendarOperationReceipt> executeExternalCalendarOperation(
    ManualCalendarOperationIntent operation,
  ) async {
    final encoded = jsonEncode(_canonicalJson(operation.toJson(_timestamp)));
    var active = _pendingExternalCalendarOperation;
    if (active == null) {
      if (_pendingExternalCalendarOperation != null) {
        throw StateError('An external Calendar command is unresolved.');
      }
      active = _PendingExternalCalendarOperation(
        commandId: newAgentRequestId(),
        intent: encoded,
        operation: Map<String, dynamic>.unmodifiable(
          jsonDecode(encoded) as Map<String, dynamic>,
        ),
      );
      _pendingExternalCalendarOperation = active;
    } else if (active.intent != encoded) {
      throw StateError(
        'A prior Calendar command is unresolved; retry that exact command first.',
      );
    }
    final previouslySubmitted = active.submitted;
    active.submitted = true;
    try {
      final result = await ownerCommand(
        _runtime.wireTransport,
        active.commandId,
        {
          'kind': 'day.external_calendar_operation',
          'operation': active.operation,
        },
      );
      _requireKeys(result, const {'kind', 'operation'}, 'Day Calendar result');
      if (result['kind'] != 'day.external_calendar_operation') {
        throw const FormatException('Invalid Day Calendar result kind.');
      }
      final receipt = ManualCalendarOperationReceipt.fromJson(
        _asMap(result['operation'], 'Day Calendar operation'),
      );
      _pendingExternalCalendarOperation = null;
      return receipt;
    } on AppWireTransportException catch (error) {
      if (mayDiscardPendingCommand(
        error,
        previouslySubmitted: previouslySubmitted,
      )) {
        _pendingExternalCalendarOperation = null;
      }
      throw _runtimeError(error);
    }
  }

  @override
  Future<CaptureReceipt> submitCapture(String input, DayQuery query) => _mutate(
    query,
    {
      'type': 'submit_capture',
      'input': input,
      'occurred_at': _timestamp(_clock()),
    },
    (result) {
      final capture = result.capture;
      if (capture == null) {
        throw const FormatException(
          'Day capture acknowledgement omitted its receipt.',
        );
      }
      return capture;
    },
  );

  @override
  Future<DaySnapshot> classifyCapture(
    CaptureReceipt capture,
    ClassificationDraft classification,
    DayQuery query,
  ) => _mutate(query, {
    'type': 'classify_capture',
    'capture_id': capture.id,
    'expected_revision': capture.revision,
    'classification': _classification(classification),
    'occurred_at': _timestamp(_clock()),
  }, (result) => result.snapshot);

  @override
  Future<DaySnapshot> setTaskCompleted(
    TaskItem task,
    bool completed,
    DayQuery query,
  ) => _mutate(query, {
    'type': 'set_task_completion',
    'task_id': task.id,
    'expected_revision': task.revision,
    'completed': completed,
    'occurred_at': _timestamp(_clock()),
  }, (result) => result.snapshot);

  @override
  Future<DaySnapshot> deleteItem(DayItem item, DayQuery query) {
    if (item is EventItem && item.source is CalendarDayItemSource) {
      throw StateError(
        'External Calendar events must be deleted through Calendar activity.',
      );
    }
    return _mutate(query, {
      'type': 'delete_item',
      'target': {'kind': item.kind.name, 'id': item.id},
      'expected_revision': item.revision,
      'occurred_at': _timestamp(_clock()),
    }, (result) => result.snapshot);
  }

  Future<Map<String, dynamic>> _load(DayQuery query) async {
    try {
      final result = await _runtime.wireTransport.queryV2({
        'schema_version': appWireProtocolVersion,
        'request_id': newAgentRequestId(),
        'query': {'kind': 'day.snapshot', 'day': _day(query)},
      }, timeout: const Duration(seconds: 35));
      _requireKeys(result, const {'kind', 'snapshot'}, 'Day snapshot result');
      if (result['kind'] != 'day_snapshot') {
        throw const FormatException('Invalid Day result kind.');
      }
      return _asMap(result['snapshot'], 'Day snapshot');
    } on AppWireTransportException catch (error) {
      throw _runtimeError(error);
    }
  }

  Future<T> _mutate<T>(
    DayQuery query,
    Map<String, dynamic> mutation,
    T Function(_DecodedMutation result) decode,
  ) async {
    final day = _day(query);
    final queryIntent = _queryIntent(query);
    final mutationIntent = _mutationIntent(mutation);
    final pending = _pendingMutation;
    late final _PendingDayMutation active;
    late final String commandId;
    late final Map<String, dynamic> requestDay;
    late final Map<String, dynamic> requestMutation;
    if (pending == null) {
      commandId = newAgentRequestId();
      requestDay = Map<String, dynamic>.unmodifiable(day);
      requestMutation = Map<String, dynamic>.unmodifiable(
        jsonDecode(jsonEncode(mutation)) as Map<String, dynamic>,
      );
      active = _PendingDayMutation(
        commandId: commandId,
        queryIntent: queryIntent,
        mutationIntent: mutationIntent,
        day: requestDay,
        mutation: requestMutation,
      );
      _pendingMutation = active;
    } else {
      if (pending.queryIntent != queryIntent ||
          pending.mutationIntent != mutationIntent) {
        throw StateError(
          'A prior Day mutation acknowledgement is unresolved; retry that exact mutation first.',
        );
      }
      commandId = pending.commandId;
      requestDay = pending.day;
      requestMutation = pending.mutation;
      active = pending;
    }

    final previouslySubmitted = active.submitted;
    active.submitted = true;
    try {
      final result = await ownerCommand(_runtime.wireTransport, commandId, {
        'kind': 'day.mutate',
        'day': requestDay,
        'mutation': requestMutation,
      });
      _requireKeys(result, const {
        'kind',
        'command_id',
        'mutation',
      }, 'Day mutation result');
      if (result['kind'] != 'day_mutation' ||
          result['command_id'] != commandId) {
        throw const FormatException('Invalid Day mutation correlation.');
      }
      final payload = _decodeMutationResult(
        _asMap(result['mutation'], 'Day mutation'),
        query,
      );
      final decoded = decode(payload);
      if (_pendingMutation?.commandId == commandId) _pendingMutation = null;
      return decoded;
    } on AppWireTransportException catch (error) {
      if (mayDiscardPendingCommand(
            error,
            previouslySubmitted: previouslySubmitted,
          ) &&
          identical(_pendingMutation, active)) {
        _pendingMutation = null;
      }
      throw _runtimeError(error);
    }
  }

  String _queryIntent(DayQuery query) => jsonEncode({
    'person_id': query.personId,
    'date': _date(query.date),
    'timezone_offset_seconds': query.timezoneOffsetSeconds,
    'end_timezone_offset_seconds':
        query.endTimezoneOffsetSeconds ?? query.timezoneOffsetSeconds,
  });

  String _mutationIntent(Map<String, dynamic> mutation) {
    final stable = Map<String, dynamic>.from(mutation)..remove('occurred_at');
    return jsonEncode(stable);
  }

  Map<String, dynamic> _day(DayQuery query) => {
    'date': _date(query.date),
    'timezone_offset_seconds': query.timezoneOffsetSeconds,
    'end_timezone_offset_seconds': query.endTimezoneOffsetSeconds,
    'now': _timestamp(query.now),
  };
}

final class _PendingRefreshCommand {
  _PendingRefreshCommand(this.commandId, this.query, this.day);
  final String commandId;
  final DayQuery query;
  final Map<String, dynamic> day;
  DayRefreshSnapshot? observed;
  String? observedWire;
  bool submitted = false;
}

final class _PendingDayMutation {
  _PendingDayMutation({
    required this.commandId,
    required this.queryIntent,
    required this.mutationIntent,
    required this.day,
    required this.mutation,
  });

  final String commandId;
  final String queryIntent;
  final String mutationIntent;
  final Map<String, dynamic> day;
  final Map<String, dynamic> mutation;
  bool submitted = false;
}

final class _PendingExternalCalendarOperation {
  _PendingExternalCalendarOperation({
    required this.commandId,
    required this.intent,
    required this.operation,
  });

  final String commandId;
  final String intent;
  final Map<String, dynamic> operation;
  bool submitted = false;
}

final class _DecodedMutation {
  const _DecodedMutation({
    required this.snapshot,
    required this.changedItem,
    required this.capture,
  });

  final DaySnapshot snapshot;
  final DayItem? changedItem;
  final CaptureReceipt? capture;
}

AppRuntimeException _runtimeError(AppWireTransportException error) =>
    AppRuntimeException(
      error.metadata['owner_code'] ?? error.code,
      error.message,
      field: error.field,
      metadata: error.metadata,
      ownerFailure: error.ownerFailure,
      commandOutcome: error.commandOutcome,
    );

_DecodedMutation _decodeMutationResult(
  Map<String, dynamic> json,
  DayQuery query,
) {
  _requireKeys(json, const {
    'snapshot',
    'changed_item',
    'capture',
  }, 'Day mutation payload');
  final snapshot = _decodeSnapshot(
    _asMap(json['snapshot'], 'Day mutation snapshot'),
    expectedPersonId: query.personId,
    expectedQuery: query,
  );
  final changedItem = json['changed_item'] == null
      ? null
      : _decodeItem(
          _asMap(json['changed_item'], 'Day changed item'),
          expectedPersonId: snapshot.personId,
        );
  final capture = json['capture'] == null
      ? null
      : _decodeCapture(
          _asMap(json['capture'], 'Day capture'),
          expectedPersonId: query.personId,
        );
  return _DecodedMutation(
    snapshot: snapshot,
    changedItem: changedItem,
    capture: capture,
  );
}

DayRefreshSnapshot _refreshSnapshot(Map<String, dynamic> result) {
  _requireKeys(result, const {'kind', 'refresh'}, 'Day refresh result');
  if (result['kind'] != 'day.refresh') {
    throw const FormatException('Invalid Day refresh result kind.');
  }
  final value = _asMap(result['refresh'], 'Day refresh snapshot');
  final state = value['state'];
  final expectedKeys = <String>{'operation_ref', 'revision', 'state'};
  if (state == 'completed') expectedKeys.add('day');
  if (state == 'failed' || state == 'interrupted') {
    expectedKeys.add('failure');
  }
  _requireKeys(value, expectedKeys, 'Day refresh snapshot');
  final operationRef = _requiredUuid(value['operation_ref'], 'operation_ref');
  final revision = _requiredRevision(value['revision'], 'refresh.revision');
  return switch (state) {
    'pending' => PendingDayRefresh(
      operationRef: operationRef,
      revision: revision,
    ),
    'running' => RunningDayRefresh(
      operationRef: operationRef,
      revision: revision,
    ),
    'completed' => CompletedDayRefresh(
      operationRef: operationRef,
      revision: revision,
      day: _decodeSnapshot(_asMap(value['day'], 'completed Day snapshot')),
    ),
    'failed' => FailedDayRefresh(
      operationRef: operationRef,
      revision: revision,
      failure: _refreshFailure(value['failure']),
    ),
    'interrupted' => InterruptedDayRefresh(
      operationRef: operationRef,
      revision: revision,
      failure: _refreshFailure(value['failure']),
    ),
    _ => throw const FormatException('Unknown Day refresh state.'),
  };
}

DayRefreshFailure _refreshFailure(Object? raw) =>
    _enumValue(raw, DayRefreshFailure.values, 'Day refresh failure');

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

DaySnapshot _decodeSnapshot(
  Map<String, dynamic> json, {
  String? expectedPersonId,
  DayQuery? expectedQuery,
}) {
  _requireKeys(json, const {
    'schema_version',
    'person_id',
    'date',
    'generated_at',
    'timezone_offset_seconds',
    'now_event_id',
    'next_event_id',
    'overdue_task_count',
    'items',
    'calendar',
    'calendar_mirror_revision',
  }, 'Day snapshot');
  if (json['schema_version'] != 1) {
    throw const FormatException('Unsupported Day snapshot version.');
  }
  final byteLength = _jsonByteLength(json, 'Day snapshot');
  if (byteLength > _maxDaySnapshotBytes) {
    throw const FormatException('Day snapshot exceeds its byte limit.');
  }
  final personId = _requiredUuid(json['person_id'], 'person_id');
  if (expectedPersonId != null && personId != expectedPersonId) {
    throw const FormatException('Day snapshot owner identity changed.');
  }
  final date = _civilDate(json['date'], 'day.date');
  final generatedAt = _utcTimestamp(json['generated_at'], 'generated_at');
  final timezoneOffset = _requiredOffset(
    json['timezone_offset_seconds'],
    'timezone_offset_seconds',
  );
  if (expectedQuery != null &&
      (date.year != expectedQuery.date.year ||
          date.month != expectedQuery.date.month ||
          date.day != expectedQuery.date.day ||
          timezoneOffset != expectedQuery.timezoneOffsetSeconds)) {
    throw const FormatException('Day snapshot query identity changed.');
  }
  final overdueCount = json['overdue_task_count'];
  if (overdueCount is! int || overdueCount < 0 || overdueCount > 0xffffffff) {
    throw const FormatException('Invalid Day overdue task count.');
  }
  final rawItems = json['items'];
  if (rawItems is! List || rawItems.length > _maxDayItems) {
    throw const FormatException('Day snapshot exceeds its item limit.');
  }
  final items = <DayItem>[];
  final identities = <String>{};
  final eventIds = <String>{};
  for (final raw in rawItems) {
    final item = _decodeItem(
      _asMap(raw, 'Day item'),
      expectedPersonId: personId,
    );
    if (!identities.add('${item.kind.name}:${item.id}')) {
      throw const FormatException('Duplicate Day item identity.');
    }
    if (item case EventItem()) eventIds.add(item.id);
    items.add(item);
  }
  final nowEventId = _optionalUuid(json['now_event_id'], 'now_event_id');
  final nextEventId = _optionalUuid(json['next_event_id'], 'next_event_id');
  if ((nowEventId != null && !eventIds.contains(nowEventId)) ||
      (nextEventId != null && !eventIds.contains(nextEventId))) {
    throw const FormatException(
      'Day snapshot contains an invalid event reference.',
    );
  }
  final mirrorRevision = json['calendar_mirror_revision'];
  if (mirrorRevision != null &&
      (mirrorRevision is! int ||
          mirrorRevision < 1 ||
          mirrorRevision > _maxRevision)) {
    throw const FormatException('Invalid Day calendar revision field.');
  }
  final rawCoverage = json['calendar'];
  final coverage = rawCoverage == null
      ? null
      : _decodeCoverage(_asMap(rawCoverage, 'Day calendar coverage'));
  return DaySnapshot(
    personId: personId,
    date: date,
    generatedAt: generatedAt,
    timezoneOffsetSeconds: timezoneOffset,
    items: List.unmodifiable(items),
    nowEventId: nowEventId,
    nextEventId: nextEventId,
    overdueTaskCount: overdueCount,
    calendarCoverage: coverage,
  );
}

DayItem _decodeItem(
  Map<String, dynamic> json, {
  required String expectedPersonId,
}) {
  final kind = json['kind'];
  switch (kind) {
    case 'event':
      _requireKeys(json, const {
        'kind',
        'id',
        'person_id',
        'title',
        'schedule',
        'source',
        'created_at',
        'updated_at',
        'revision',
        'deleted_at',
        'action_target',
      }, 'Day event');
      final id = _requiredUuid(json['id'], 'event.id');
      final personId = _requiredUuid(json['person_id'], 'event.person_id');
      if (personId != expectedPersonId) {
        throw const FormatException('Day event owner identity changed.');
      }
      final source = _decodeSource(json['source']);
      final schedule = _decodeEventSchedule(json['schedule']);
      final revision = _requiredRevision(json['revision'], 'event.revision');
      _utcTimestamp(json['updated_at'], 'event.updated_at');
      _optionalUtcTimestamp(json['deleted_at'], 'event.deleted_at');
      final title = _displayText(json['title'], 'event.title');
      final actionTarget = json['action_target'] == null
          ? null
          : _decodeEventTarget(json['action_target']);
      if (source is CalendarDayItemSource) {
        if (actionTarget == null ||
            actionTarget.eventId != id ||
            actionTarget.expectedRevision != revision) {
          throw const FormatException('Invalid safe Day event target.');
        }
      } else if (actionTarget != null) {
        throw const FormatException(
          'Non-Calendar Day event has an action target.',
        );
      }
      return EventItem(
        id: id,
        title: title,
        revision: revision,
        createdAt: _utcTimestamp(json['created_at'], 'event.created_at'),
        source: source,
        startsAt: schedule.startsAt,
        endsAt: schedule.endsAt,
        isAllDay: schedule.isAllDay,
        timezone: schedule.timezone,
        actionTarget: actionTarget,
      );
    case 'task':
      _requireKeys(json, const {
        'kind',
        'id',
        'person_id',
        'title',
        'deadline',
        'priority',
        'completed_at',
        'source',
        'created_at',
        'updated_at',
        'revision',
        'deleted_at',
      }, 'Day task');
      final id = _requiredUuid(json['id'], 'task.id');
      final personId = _requiredUuid(json['person_id'], 'task.person_id');
      if (personId != expectedPersonId) {
        throw const FormatException('Day task owner identity changed.');
      }
      final priority = switch (json['priority']) {
        'low' => TaskPriority.low,
        'normal' => TaskPriority.normal,
        'high' => TaskPriority.high,
        _ => throw const FormatException('Unknown Day task priority.'),
      };
      final revision = _requiredRevision(json['revision'], 'task.revision');
      _utcTimestamp(json['updated_at'], 'task.updated_at');
      _optionalUtcTimestamp(json['deleted_at'], 'task.deleted_at');
      return TaskItem(
        id: id,
        title: _displayText(json['title'], 'task.title'),
        revision: revision,
        createdAt: _utcTimestamp(json['created_at'], 'task.created_at'),
        source: _decodeSource(json['source']),
        deadline: _optionalUtcTimestamp(json['deadline'], 'task.deadline'),
        completedAt: _optionalUtcTimestamp(
          json['completed_at'],
          'task.completed_at',
        ),
        priority: priority,
      );
    case 'note':
      _requireKeys(json, const {
        'kind',
        'id',
        'person_id',
        'content',
        'source',
        'created_at',
        'updated_at',
        'revision',
        'deleted_at',
      }, 'Day note');
      final id = _requiredUuid(json['id'], 'note.id');
      final personId = _requiredUuid(json['person_id'], 'note.person_id');
      if (personId != expectedPersonId) {
        throw const FormatException('Day note owner identity changed.');
      }
      _utcTimestamp(json['updated_at'], 'note.updated_at');
      _optionalUtcTimestamp(json['deleted_at'], 'note.deleted_at');
      return NoteItem(
        id: id,
        title: _displayText(json['content'], 'note.content'),
        revision: _requiredRevision(json['revision'], 'note.revision'),
        createdAt: _utcTimestamp(json['created_at'], 'note.created_at'),
        source: _decodeSource(json['source']),
      );
    default:
      throw const FormatException('Unknown Day timeline item kind.');
  }
}

final class _DecodedEventSchedule {
  const _DecodedEventSchedule({
    required this.startsAt,
    required this.endsAt,
    required this.isAllDay,
    this.timezone,
  });

  final DateTime startsAt;
  final DateTime endsAt;
  final bool isAllDay;
  final String? timezone;
}

_DecodedEventSchedule _decodeEventSchedule(Object? raw) {
  final json = _asMap(raw, 'Day event schedule');
  switch (json['kind']) {
    case 'timed':
      _requireKeys(json, const {
        'kind',
        'starts_at',
        'ends_at',
        'timezone',
      }, 'Timed Day schedule');
      final startsAt = _utcTimestamp(json['starts_at'], 'schedule.starts_at');
      final endsAt = _utcTimestamp(json['ends_at'], 'schedule.ends_at');
      if (!endsAt.isAfter(startsAt)) {
        throw const FormatException('Invalid Day event schedule interval.');
      }
      final timezone = json['timezone'];
      if (timezone is! String) {
        throw const FormatException('Invalid Day event timezone.');
      }
      return _DecodedEventSchedule(
        startsAt: startsAt,
        endsAt: endsAt,
        isAllDay: false,
        timezone: timezone,
      );
    case 'all_day':
      _requireKeys(json, const {
        'kind',
        'start_date',
        'end_date_exclusive',
      }, 'All-day Day schedule');
      final startsAt = _civilDate(json['start_date'], 'schedule.start_date');
      final endsAt = _civilDate(
        json['end_date_exclusive'],
        'schedule.end_date_exclusive',
      );
      if (!endsAt.isAfter(startsAt)) {
        throw const FormatException('Invalid all-day Day schedule interval.');
      }
      return _DecodedEventSchedule(
        startsAt: startsAt,
        endsAt: endsAt,
        isAllDay: true,
      );
    default:
      throw const FormatException('Unknown Day event schedule kind.');
  }
}

DayEventTarget _decodeEventTarget(Object? raw) {
  final json = _asMap(raw, 'Day event action target');
  _requireKeys(json, const {
    'event_id',
    'expected_revision',
  }, 'Day action target');
  return DayEventTarget(
    eventId: _requiredUuid(json['event_id'], 'action_target.event_id'),
    expectedRevision: _requiredRevision(
      json['expected_revision'],
      'action_target.expected_revision',
    ),
  );
}

DayItemSource _decodeSource(Object? raw) {
  final json = _asMap(raw, 'Day item source');
  switch (json['kind']) {
    case 'manual':
      _requireKeys(json, const {'kind'}, 'Manual Day source');
      return const ManualDayItemSource();
    case 'capture':
      _requireKeys(json, const {'kind', 'capture_id'}, 'Capture Day source');
      return CaptureDayItemSource(
        captureId: _requiredUuid(json['capture_id'], 'source.capture_id'),
      );
    case 'calendar':
      _requireKeys(json, const {
        'kind',
        'source_ref',
        'calendar_ref',
        'calendar_label',
      }, 'Calendar Day source');
      return CalendarDayItemSource(
        sourceRef: _requiredUuid(json['source_ref'], 'source.source_ref'),
        calendarRef: _requiredUuid(json['calendar_ref'], 'source.calendar_ref'),
        calendarLabel: _calendarLabel(json['calendar_label']),
      );
    default:
      throw const FormatException('Unknown Day item source kind.');
  }
}

DayCalendarCoverage _decodeCoverage(Map<String, dynamic> json) {
  _requireKeys(json, const {'sources'}, 'Day calendar coverage');
  final rawSources = json['sources'];
  if (rawSources is! List || rawSources.length > 64) {
    throw const FormatException('Day calendar source limit exceeded.');
  }
  final sources = <DayCalendarSourceCoverage>[];
  final sourceRefs = <String>{};
  final resourceRefs = <String>{};
  var resourceCount = 0;
  for (final rawSource in rawSources) {
    final source = _decodeSourceCoverage(
      _asMap(rawSource, 'Day calendar source coverage'),
    );
    if (!sourceRefs.add(source.sourceRef)) {
      throw const FormatException('Duplicate Day calendar source reference.');
    }
    resourceCount += source.resources.length;
    if (resourceCount > 256) {
      throw const FormatException('Day calendar resource limit exceeded.');
    }
    for (final resource in source.resources) {
      if (!resourceRefs.add(resource.resourceRef)) {
        throw const FormatException(
          'Duplicate Day calendar resource reference.',
        );
      }
    }
    sources.add(source);
  }
  return DayCalendarCoverage(sources: List.unmodifiable(sources));
}

DayCalendarSourceCoverage _decodeSourceCoverage(Map<String, dynamic> json) {
  _requireKeys(json, const {
    'source_ref',
    'label',
    'state',
    'last_success_at',
    'last_range',
    'failure',
    'failure_at',
    'resources',
  }, 'Day calendar source coverage');
  final rawResources = json['resources'];
  if (rawResources is! List) {
    throw const FormatException('Invalid Day calendar resource coverage.');
  }
  return DayCalendarSourceCoverage(
    sourceRef: _requiredUuid(json['source_ref'], 'coverage.source_ref'),
    label: _calendarLabel(json['label']),
    state: _enumValue(json['state'], DayCoverageState.values, 'coverage.state'),
    lastSuccessAt: _optionalUtcTimestamp(
      json['last_success_at'],
      'coverage.last_success_at',
    ),
    lastRange: _optionalCalendarRange(json['last_range']),
    failure: _optionalEnumValue(
      json['failure'],
      DayCalendarFailure.values,
      'coverage.failure',
    ),
    failureAt: _optionalUtcTimestamp(json['failure_at'], 'coverage.failure_at'),
    resources: List.unmodifiable(
      rawResources.map(
        (resource) => _decodeResourceCoverage(
          _asMap(resource, 'Day calendar resource coverage'),
        ),
      ),
    ),
  );
}

DayCalendarResourceCoverage _decodeResourceCoverage(Map<String, dynamic> json) {
  _requireKeys(json, const {
    'resource_ref',
    'label',
    'state',
    'last_success_at',
    'last_range',
    'failure',
    'failure_at',
  }, 'Day calendar resource coverage');
  return DayCalendarResourceCoverage(
    resourceRef: _requiredUuid(json['resource_ref'], 'coverage.resource_ref'),
    label: _calendarLabel(json['label']),
    state: _enumValue(json['state'], DayCoverageState.values, 'coverage.state'),
    lastSuccessAt: _optionalUtcTimestamp(
      json['last_success_at'],
      'coverage.last_success_at',
    ),
    lastRange: _optionalCalendarRange(json['last_range']),
    failure: _optionalEnumValue(
      json['failure'],
      DayCalendarFailure.values,
      'coverage.failure',
    ),
    failureAt: _optionalUtcTimestamp(json['failure_at'], 'coverage.failure_at'),
  );
}

DayCalendarRange? _optionalCalendarRange(Object? raw) =>
    raw == null ? null : _decodeCalendarRange(_asMap(raw, 'Calendar range'));

DayCalendarRange _decodeCalendarRange(Map<String, dynamic> json) {
  _requireKeys(json, const {
    'start_date',
    'end_date_exclusive',
    'timezone_offset_seconds',
    'end_timezone_offset_seconds',
  }, 'Calendar range');
  final startDate = _civilDate(json['start_date'], 'range.start_date');
  final endDate = _civilDate(
    json['end_date_exclusive'],
    'range.end_date_exclusive',
  );
  final startOffset = _requiredOffset(
    json['timezone_offset_seconds'],
    'range.timezone_offset_seconds',
  );
  final endOffset = json['end_timezone_offset_seconds'] == null
      ? null
      : _requiredOffset(
          json['end_timezone_offset_seconds'],
          'range.end_timezone_offset_seconds',
        );
  final days = endDate.difference(startDate).inDays;
  final elapsedSeconds =
      days * 86400 + startOffset - (endOffset ?? startOffset);
  if (days < 1 || days > 31 || elapsedSeconds <= 0) {
    throw const FormatException('Invalid Calendar coverage range.');
  }
  return DayCalendarRange(
    startDate: startDate,
    endDateExclusive: endDate,
    timezoneOffsetSeconds: startOffset,
    endTimezoneOffsetSeconds: endOffset,
  );
}

CaptureReceipt _decodeCapture(
  Map<String, dynamic> json, {
  required String expectedPersonId,
}) {
  _requireKeys(json, const {
    'id',
    'person_id',
    'original_input',
    'captured_at',
    'source',
    'processing',
    'revision',
  }, 'Day capture receipt');
  final id = _requiredUuid(json['id'], 'capture.id');
  if (_requiredUuid(json['person_id'], 'capture.person_id') !=
      expectedPersonId) {
    throw const FormatException('Day capture owner identity changed.');
  }
  final input = json['original_input'];
  if (input is! String || utf8.encode(input).length > 1048576) {
    throw const FormatException('Invalid Day capture input.');
  }
  if (!const {'typed', 'voice'}.contains(json['source'])) {
    throw const FormatException('Unknown Day capture source.');
  }
  _decodeCaptureProcessing(json['processing']);
  return CaptureReceipt(
    id: id,
    originalInput: input,
    capturedAt: _utcTimestamp(json['captured_at'], 'capture.captured_at'),
    revision: _requiredRevision(json['revision'], 'capture.revision'),
  );
}

void _decodeCaptureProcessing(Object? raw) {
  final json = _asMap(raw, 'Capture processing');
  switch (json['status']) {
    case 'pending':
      _requireKeys(json, const {'status'}, 'Pending capture processing');
      return;
    case 'classified':
      _requireKeys(json, const {
        'status',
        'target',
        'classified_at',
      }, 'Classified capture processing');
      _decodeDomainRef(json['target']);
      _utcTimestamp(json['classified_at'], 'capture.classified_at');
      return;
    case 'dismissed':
      _requireKeys(json, const {
        'status',
        'dismissed_at',
      }, 'Dismissed capture processing');
      _utcTimestamp(json['dismissed_at'], 'capture.dismissed_at');
      return;
    default:
      throw const FormatException('Unknown Day capture processing state.');
  }
}

void _decodeDomainRef(Object? raw) {
  final json = _asMap(raw, 'Capture domain reference');
  _requireKeys(json, const {'kind', 'id'}, 'Capture domain reference');
  if (!const {'event', 'task', 'note'}.contains(json['kind'])) {
    throw const FormatException('Unknown capture domain reference kind.');
  }
  _requiredUuid(json['id'], 'capture.target.id');
}

DateTime _civilDate(Object? value, String field) {
  if (value is! String || !RegExp(r'^\d{4}-\d{2}-\d{2}$').hasMatch(value)) {
    throw FormatException('Invalid $field.');
  }
  final parsed = DateTime.tryParse(value);
  if (parsed == null || _date(parsed) != value) {
    throw FormatException('Invalid $field.');
  }
  return DateTime.utc(parsed.year, parsed.month, parsed.day);
}

DateTime _utcTimestamp(Object? value, String field) {
  if (value is! String ||
      utf8.encode(value).length > 64 ||
      !_timestampPattern.hasMatch(value)) {
    throw FormatException('Invalid $field.');
  }
  final zone = value.endsWith('Z') ? 'Z' : value.substring(value.length - 6);
  if (zone != 'Z' &&
      (zone.substring(1, 3) != '00' || zone.substring(4, 6) != '00')) {
    throw FormatException('Invalid $field timezone.');
  }
  final parsed = DateTime.tryParse(value);
  if (parsed == null) throw FormatException('Invalid $field.');
  return parsed.toUtc();
}

DateTime? _optionalUtcTimestamp(Object? value, String field) =>
    value == null ? null : _utcTimestamp(value, field);

int _requiredOffset(Object? value, String field) {
  if (value is! int || value.abs() >= 86400) {
    throw FormatException('Invalid $field.');
  }
  return value;
}

int _requiredRevision(Object? value, String field) {
  if (value is! int || value < 1 || value > _maxRevision) {
    throw FormatException('Invalid $field.');
  }
  return value;
}

String _requiredUuid(Object? value, String field) {
  if (value is! String ||
      !_uuidPattern.hasMatch(value) ||
      value == '00000000-0000-0000-0000-000000000000') {
    throw FormatException('Invalid $field.');
  }
  return value;
}

String? _optionalUuid(Object? value, String field) =>
    value == null ? null : _requiredUuid(value, field);

String _displayText(Object? value, String field) {
  if (value is! String || utf8.encode(value).length > _maxDaySnapshotBytes) {
    throw FormatException('Invalid $field.');
  }
  return value;
}

String _calendarLabel(Object? value) {
  if (value is! String || value.isEmpty || utf8.encode(value).length > 256) {
    throw const FormatException('Invalid Day Calendar label.');
  }
  return value;
}

T _enumValue<T extends Enum>(Object? raw, List<T> values, String field) {
  if (raw is! String) throw FormatException('Invalid $field.');
  for (final value in values) {
    if (value.name == raw) return value;
  }
  throw FormatException('Unknown $field.');
}

T? _optionalEnumValue<T extends Enum>(
  Object? raw,
  List<T> values,
  String field,
) => raw == null ? null : _enumValue(raw, values, field);

Map<String, dynamic> _asMap(Object? value, String field) {
  if (value is! Map) throw FormatException('Invalid $field.');
  try {
    return Map<String, dynamic>.from(value);
  } on Object {
    throw FormatException('Invalid $field.');
  }
}

void _requireKeys(
  Map<String, dynamic> value,
  Set<String> expected,
  String field,
) {
  if (value.length != expected.length ||
      !value.keys.toSet().containsAll(expected)) {
    throw FormatException('Invalid $field fields.');
  }
}

int _jsonByteLength(Map<String, dynamic> value, String field) {
  try {
    return utf8.encode(jsonEncode(value)).length;
  } on Object {
    throw FormatException('Invalid $field encoding.');
  }
}

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

// Object member order is not part of an immutable JSON value.
Object? _canonicalJson(Object? value) {
  if (value is Map<String, dynamic>) {
    final keys = value.keys.toList()..sort();
    return <String, Object?>{
      for (final key in keys) key: _canonicalJson(value[key]),
    };
  }
  if (value is List) return value.map(_canonicalJson).toList(growable: false);
  return value;
}
