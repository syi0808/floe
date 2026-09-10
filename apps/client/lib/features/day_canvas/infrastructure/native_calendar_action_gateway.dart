import '../../../infrastructure/native/native_transport.dart';
import '../application/calendar_action_gateway.dart';
import '../domain/calendar_action.dart';

final class NativeCalendarActionGateway
    implements CalendarActionExecutionGateway, CalendarDirectActionGateway {
  NativeCalendarActionGateway(this._request);

  final Future<Map<String, dynamic>> Function(
    String operation,
    Map<String, dynamic> request,
  )
  _request;

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
    final data = await _request('calendar_actions', {
      'schema_version': nativeProtocolVersion,
      'person_id': personId,
      'operation': {'kind': 'capabilities'},
    });
    return data['writes_enabled'] == true;
  }

  @override
  Future<ActionAuthority> loadActionAuthority(String personId) async {
    final data = await _request('calendar_actions', {
      'schema_version': nativeProtocolVersion,
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
      'schema_version': nativeProtocolVersion,
      'person_id': personId,
      'operation': {'kind': 'set_authority', 'calendar_create': mode.name},
    });
    return ActionAuthority.fromJson(_asMap(data['authority']));
  }

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
  ) async {
    final data = await _request('calendar_actions', {
      'schema_version': nativeProtocolVersion,
      'person_id': personId,
      'operation': operation,
    });
    return (data['actions']! as List)
        .map((value) => CalendarAction.fromJson(_asMap(value)))
        .toList(growable: false);
  }
}

Map<String, dynamic> _asMap(Object? value) =>
    Map<String, dynamic>.from(value! as Map);
