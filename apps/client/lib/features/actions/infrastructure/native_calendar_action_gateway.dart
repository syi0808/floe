import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/app/runtime/owner_operation.dart';
import 'package:floe_client/features/actions/application/calendar_action_gateway.dart';
import 'package:floe_client/features/actions/domain/calendar_action.dart';

final class NativeCalendarActionGateway
    implements CalendarActionExecutionGateway, CalendarDirectActionGateway {
  NativeCalendarActionGateway(this._transport);
  final AppWireTransport _transport;
  final OwnerOperationObserver _operations = OwnerOperationObserver(
    timeout: const Duration(seconds: 40),
  );

  @override
  Future<List<CalendarAction>> loadCalendarActions(String personId) =>
      _actions(personId, {'kind': 'list'});

  Future<CalendarAction> loadCalendarAction(
    String personId,
    String actionId,
  ) async =>
      (await _actions(personId, {'kind': 'get', 'action_id': actionId})).single;

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
  }) async => (await _actions(personId, {
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
  }) async => (await _actions(personId, {
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
  }) async => (await _actions(personId, {
    'kind': 'decide',
    'action_id': actionId,
    'decision': decision.name,
  })).single;

  @override
  Future<bool> calendarWritesEnabled(String personId) async {
    return _invoke(personId, {
      'kind': 'capabilities',
    }, (data) => data['writes_enabled'] == true);
  }

  @override
  Future<ActionAuthority> loadActionAuthority(String personId) => _invoke(
    personId,
    {'kind': 'get_authority'},
    (data) => ActionAuthority.fromJson(_asMap(data['authority'])),
  );

  @override
  Future<ActionAuthority> setCalendarCreateAuthority(
    String personId,
    ActionAuthorityMode mode,
  ) => _invoke(personId, {
    'kind': 'set_authority',
    'calendar_create': mode.name,
  }, (data) => ActionAuthority.fromJson(_asMap(data['authority'])));

  @override
  Future<CalendarAction> executeCalendarAction(
    String personId,
    String actionId,
  ) async => (await _actions(personId, {
    'kind': 'execute',
    'action_id': actionId,
  })).single;

  @override
  Future<CalendarAction> recoverCalendarAction(
    String personId,
    String actionId,
  ) async => (await _actions(personId, {
    'kind': 'recover',
    'action_id': actionId,
  })).single;

  Future<List<CalendarAction>> _actions(
    String personId,
    Map<String, dynamic> operation,
  ) => _invoke(personId, operation, (data) {
    final values = data['actions'];
    if (values is! List) throw const FormatException('Invalid Actions result');
    final actions = values
        .map((value) => CalendarAction.fromJson(_asMap(value)))
        .toList(growable: false);
    if (actions.any((action) => action.personId != personId)) {
      throw const FormatException('Actions Person mismatch');
    }
    final expected = operation['action_id'];
    if (expected != null &&
        (actions.length != 1 || actions.single.id != expected)) {
      throw const FormatException('Action identity mismatch');
    }
    return actions;
  });

  Future<T> _invoke<T>(
    String personId,
    Map<String, dynamic> operation,
    T Function(Map<String, dynamic>) decode,
  ) {
    final query = switch (operation['kind']) {
      'capabilities' => <String, Object?>{'kind': 'actions.capabilities'},
      'get_authority' => <String, Object?>{'kind': 'actions.authority'},
      'list' => <String, Object?>{'kind': 'actions.list'},
      'get' => <String, Object?>{
        'kind': 'actions.get',
        'action_id': operation['action_id'],
      },
      _ => null,
    };
    final intent =
        query ?? {'kind': 'actions.calendar', 'operation': operation};
    return _operations.observe(
      scope: personId,
      intent: ownerIntent(intent),
      stage: 'calendar_action',
      resultKind: 'action_operation',
      start: (operationId) => query == null
          ? ownerCommand(_transport, operationId, intent)
          : ownerQuery(_transport, operationId, intent),
      read: (operationId, release) =>
          ownerResult(_transport, 'actions.read_result', operationId, release),
      decode: (result) => decode(_asMap(result['calendar_actions'])),
    );
  }
}

Map<String, dynamic> _asMap(Object? value) =>
    Map<String, dynamic>.from(value! as Map);
