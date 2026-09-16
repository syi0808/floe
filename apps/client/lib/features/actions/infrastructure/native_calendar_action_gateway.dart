import 'package:floe_client/app/runtime/native_transport.dart';
import 'package:floe_client/features/conversation/application/agent_request_id.dart';
import 'package:floe_client/app/runtime/agent_vault_gateway.dart';
import 'package:floe_client/features/actions/application/calendar_action_gateway.dart';
import 'package:floe_client/features/actions/domain/calendar_action.dart';

final class NativeCalendarActionGateway
    implements CalendarActionExecutionGateway, CalendarDirectActionGateway {
  NativeCalendarActionGateway(this._request);

  final Future<Map<String, dynamic>> Function(
    String operation,
    Map<String, dynamic> request,
  )
  _request;
  ({String personId, String requestId})? _pendingAction;

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
    final data = await _vaultAction(personId, {'kind': 'get_authority'});
    return ActionAuthority.fromJson(_asMap(data['authority']));
  }

  @override
  Future<ActionAuthority> setCalendarCreateAuthority(
    String personId,
    ActionAuthorityMode mode,
  ) async {
    final data = await _vaultAction(personId, {
      'kind': 'set_authority',
      'calendar_create': mode.name,
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
    if (const {
      'get',
      'decide',
      'execute',
      'recover',
    }.contains(operation['kind'])) {
      try {
        final inspected = await _vaultAction(personId, {
          'kind': 'get',
          'action_id': operation['action_id'],
        });
        final data = operation['kind'] == 'get'
            ? inspected
            : await _vaultAction(personId, operation);
        return (data['actions']! as List)
            .map((value) => CalendarAction.fromJson(_asMap(value)))
            .toList(growable: false);
      } on AgentVaultException catch (error) {
        if (error.failure != 'not_found') rethrow;
      }
      final inspected = await _request('calendar_actions', {
        'schema_version': nativeProtocolVersion,
        'person_id': personId,
        'operation': {'kind': 'get', 'action_id': operation['action_id']},
      });
      final actions = inspected['actions'] as List;
      if (actions.length == 1 &&
          _asMap(actions.single)['agent_origin'] != null) {
        throw const AgentVaultException(
          'conflict',
          stage: 'calendar_action',
          recoveryAction: 'reconcile',
        );
      }
      if (operation['kind'] == 'get') {
        return actions
            .map((value) => CalendarAction.fromJson(_asMap(value)))
            .toList(growable: false);
      }
    }
    final data = await _request('calendar_actions', {
      'schema_version': nativeProtocolVersion,
      'person_id': personId,
      'operation': operation,
    });
    return (data['actions']! as List)
        .map((value) => CalendarAction.fromJson(_asMap(value)))
        .toList(growable: false);
  }

  Future<Map<String, dynamic>> _vaultAction(
    String personId,
    Map<String, dynamic> operation,
  ) async {
    final pending = _pendingAction;
    if (pending != null) {
      final previous = await _request('agent_vault', {
        'schema_version': nativeProtocolVersion,
        'person_id': pending.personId,
        'request_id': pending.requestId,
        'operation': {'kind': 'poll', 'after_sequence': 0},
      });
      if (previous['done'] != true) {
        throw AgentVaultException(
          'conflict',
          requestId: pending.requestId,
          stage: 'calendar_action',
          recoveryAction: 'reconcile',
        );
      }
      await _request('agent_vault', {
        'schema_version': nativeProtocolVersion,
        'person_id': pending.personId,
        'request_id': pending.requestId,
        'operation': {'kind': 'release'},
      });
      _pendingAction = null;
    }
    final requestId = newAgentRequestId();
    Future<Map<String, dynamic>> call(Map<String, dynamic> action) =>
        _request('agent_vault', {
          'schema_version': nativeProtocolVersion,
          'person_id': personId,
          'request_id': requestId,
          'operation': action,
        });
    var result = await call({
      'kind': 'submit',
      'action': {'kind': 'calendar_action', 'operation': operation},
    });
    _pendingAction = (personId: personId, requestId: requestId);
    final elapsed = Stopwatch()..start();
    while (result['done'] != true) {
      if (elapsed.elapsed >= const Duration(seconds: 40)) {
        await call({'kind': 'stop'});
        throw AgentVaultException(
          'deadline_exceeded',
          requestId: requestId,
          stage: 'calendar_action',
          recoveryAction: 'reconcile',
        );
      }
      await Future<void>.delayed(const Duration(milliseconds: 20));
      result = await call({'kind': 'poll', 'after_sequence': 0});
    }
    await call({'kind': 'release'});
    _pendingAction = null;
    final failure = result['failure'];
    if (failure != null) {
      final envelope = failure is Map ? _asMap(failure) : null;
      throw AgentVaultException(
        envelope?['kind'] as String? ?? failure.toString(),
        requestId: requestId,
        stage: 'calendar_action',
        recoveryAction: envelope?['recovery_action'] as String?,
      );
    }
    return _asMap(result['calendar_actions']);
  }
}

Map<String, dynamic> _asMap(Object? value) =>
    Map<String, dynamic>.from(value! as Map);
