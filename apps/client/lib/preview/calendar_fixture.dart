import '../features/day_canvas/application/calendar_action_gateway.dart';
import '../features/day_canvas/application/day_gateway.dart';
import '../features/day_canvas/application/fake_day_gateway.dart';
import '../features/day_canvas/domain/calendar_action.dart';
import '../features/day_canvas/domain/day_models.dart';

final calendarPreviewDate = DateTime.utc(2026, 9, 4);
final calendarPreviewQuery = DayQuery(
  personId: 'visual-preview',
  date: calendarPreviewDate,
  now: DateTime.utc(2026, 9, 4, 14, 28),
  timezoneOffsetSeconds: 0,
);

DayGateway calendarPreviewGateway() => CalendarPreviewGateway(
  items: [
    for (final entry in [
      ('making', 'A day for making', 'Personal'),
      ('research', 'Research week', 'Product team'),
    ])
      EventItem(
        id: entry.$1,
        title: entry.$2,
        revision: 1,
        createdAt: calendarPreviewDate,
        startsAt: calendarPreviewDate,
        endsAt: calendarPreviewDate.add(const Duration(days: 1)),
        isAllDay: true,
        calendarName: entry.$3,
        externalId: 'fixture:${entry.$1}',
        provider: 'fixture',
        timezone: 'UTC',
      ),
    for (final entry in [
      ('standup', 'A little alignment', 570, 600, 'Work'),
      ('review', 'Make room for the details', 660, 720, 'Work'),
      ('planning', 'Plan the next step', 675, 735, 'Product team'),
      ('personal-call', 'A quick personal call', 690, 705, 'Personal'),
      ('reset', 'Take a breath', 750, 755, 'Personal'),
      ('zones', 'Afternoon catch-up', 960, 1005, 'Product team'),
    ])
      EventItem(
        id: entry.$1,
        title: entry.$2,
        revision: 1,
        createdAt: calendarPreviewDate,
        startsAt: calendarPreviewDate.add(Duration(minutes: entry.$3)),
        endsAt: calendarPreviewDate.add(Duration(minutes: entry.$4)),
        calendarName: entry.$5,
        externalId: 'fixture:${entry.$1}',
        provider: 'fixture',
        timezone: 'UTC',
      ),
    TaskItem(
      id: 'feedback',
      title: 'Prepare launch brief',
      revision: 1,
      createdAt: calendarPreviewDate,
      deadline: calendarPreviewDate,
    ),
    NoteItem(
      id: 'launch-plan',
      title: 'Launch plan',
      revision: 1,
      createdAt: calendarPreviewDate,
    ),
  ],
);

final class CalendarPreviewGateway
    implements DayGateway, CalendarActionExecutionGateway {
  CalendarPreviewGateway({required List<DayItem> items})
    : _days = FakeDayGateway(initialItems: items);

  final FakeDayGateway _days;
  final List<CalendarAction> _actions = [];
  int _nextActionId = 1;

  CalendarConnection get _connection => CalendarConnection(
    id: 'preview',
    name: 'Preview calendars',
    provider: 'fixture',
    revision: 1,
    lastSuccessAt: calendarPreviewQuery.now,
    calendars: [
      for (final (id, name) in [
        ('personal', 'Personal'),
        ('work', 'Work'),
        ('product', 'Product team'),
      ])
        ConnectedCalendar(
          id: id,
          name: name,
          lastSuccessAt: calendarPreviewQuery.now,
        ),
    ],
  );

  DaySnapshot _connected(DaySnapshot snapshot) => DaySnapshot(
    personId: snapshot.personId,
    date: snapshot.date,
    generatedAt: snapshot.generatedAt,
    timezoneOffsetSeconds: snapshot.timezoneOffsetSeconds,
    items: snapshot.items,
    nowEventId: snapshot.nowEventId,
    nextEventId: snapshot.nextEventId,
    overdueTaskCount: snapshot.overdueTaskCount,
    calendar: _connection,
  );

  @override
  Future<DaySnapshot> loadDay(DayQuery query) async =>
      _connected(await _days.loadDay(query));

  @override
  Future<CaptureReceipt> submitCapture(String input, DayQuery query) =>
      _days.submitCapture(input, query);

  @override
  Future<DaySnapshot> classifyCapture(
    CaptureReceipt capture,
    ClassificationDraft classification,
    DayQuery query,
  ) async =>
      _connected(await _days.classifyCapture(capture, classification, query));

  @override
  Future<DaySnapshot> setTaskCompleted(
    TaskItem task,
    bool completed,
    DayQuery query,
  ) async => _connected(await _days.setTaskCompleted(task, completed, query));

  @override
  Future<DaySnapshot> deleteItem(DayItem item, DayQuery query) async =>
      _connected(await _days.deleteItem(item, query));

  @override
  Future<List<CalendarAction>> loadCalendarActions(String personId) async =>
      List.unmodifiable(_actions);

  @override
  Future<ActionAuthority> loadActionAuthority(String personId) async =>
      const ActionAuthority(calendarCreate: ActionAuthorityMode.ask);

  @override
  Future<ActionAuthority> setCalendarCreateAuthority(
    String personId,
    ActionAuthorityMode mode,
  ) async => ActionAuthority(calendarCreate: mode);

  @override
  Future<bool> calendarWritesEnabled(String personId) async => false;

  @override
  Future<CalendarAction> proposeCalendarAction({
    required String personId,
    required String calendarId,
    required String title,
    required DateTime startsAt,
    required DateTime endsAt,
    required String timezone,
  }) async {
    final createdAt = DateTime.now().toUtc();
    final action = CalendarAction.fromJson({
      'id': 'preview-action-${_nextActionId++}',
      'person_id': personId,
      'provider': _connection.provider,
      'calendar_id': calendarId,
      'calendar_name': _connection.connectedCalendars
          .firstWhere((calendar) => calendar.id == calendarId)
          .name,
      'title': title,
      'connection_revision': _connection.revision,
      'created_at': createdAt.toIso8601String(),
      'expires_at': createdAt
          .add(const Duration(minutes: 15))
          .toIso8601String(),
      'approved_at': null,
      'execution_id': 'preview-execution-${_nextActionId - 1}',
      'schedule': {
        'starts_at': startsAt.toUtc().toIso8601String(),
        'ends_at': endsAt.toUtc().toIso8601String(),
        'timezone': timezone,
      },
      'state': {'status': 'pending', 'reason': null},
    });
    _actions.insert(0, action);
    return action;
  }

  @override
  Future<CalendarAction> decideCalendarAction({
    required String personId,
    required String actionId,
    required CalendarActionDecision decision,
  }) async {
    final index = _actions.indexWhere((action) => action.id == actionId);
    final action = _actions[index];
    final decided = _copyAction(
      action,
      status: decision == CalendarActionDecision.approve
          ? CalendarActionStatus.approved
          : CalendarActionStatus.rejected,
      approvedAt: decision == CalendarActionDecision.approve
          ? DateTime.now().toUtc()
          : null,
    );
    _actions[index] = decided;
    return decided;
  }

  @override
  Future<CalendarAction> executeCalendarAction(
    String personId,
    String actionId,
  ) => throw UnsupportedError('Calendar writes are disabled in preview.');

  @override
  Future<CalendarAction> recoverCalendarAction(
    String personId,
    String actionId,
  ) async => _actions.singleWhere((action) => action.id == actionId);
}

CalendarAction _copyAction(
  CalendarAction action, {
  required CalendarActionStatus status,
  DateTime? approvedAt,
}) => CalendarAction.fromJson({
  'id': action.id,
  'person_id': action.personId,
  'provider': action.provider,
  'calendar_id': action.calendarId,
  'calendar_name': action.calendarName,
  'title': action.title,
  'connection_revision': action.connectionRevision,
  'created_at': action.createdAt.toIso8601String(),
  'expires_at': action.expiresAt.toIso8601String(),
  'approved_at': approvedAt?.toIso8601String(),
  'execution_id': action.executionId,
  'schedule': {
    'starts_at': action.startsAt.toIso8601String(),
    'ends_at': action.endsAt.toIso8601String(),
    'timezone': action.timezone,
  },
  'state': {'status': status.name, 'reason': null},
});
