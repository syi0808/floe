import 'package:flutter/foundation.dart';

import '../domain/calendar_action.dart';
import '../domain/day_models.dart';
import 'calendar_action_gateway.dart';

final class CalendarActionController extends ChangeNotifier {
  CalendarActionController({
    required this.gateway,
    required this.personId,
    this.collect,
  });

  final CalendarActionGateway gateway;
  final String personId;
  final Future<void> Function(CalendarAction)? collect;
  bool writesEnabled = false;
  ActionAuthority authority = const ActionAuthority(
    calendarCreate: ActionAuthorityMode.ask,
  );
  String? phase;
  final Map<String, String> collection = {};
  List<CalendarAction> actions = const [];
  bool busy = false;
  bool needsReload = true;
  bool failed = false;
  bool _disposed = false;

  CalendarAction? find(String id) {
    for (final action in actions) {
      if (action.id == id) return action;
    }
    return null;
  }

  bool canApprove(
    CalendarAction action,
    CalendarConnection? connection,
    DateTime now, {
    bool approved = false,
  }) =>
      !busy &&
      !needsReload &&
      action.personId == personId &&
      (approved
          ? action.status == CalendarActionStatus.approved
          : action.status.canDecide) &&
      !now.isBefore(action.createdAt) &&
      now.isBefore(action.expiresAt) &&
      connection != null &&
      connection.error == null &&
      connection.lastSuccessAt != null &&
      connection.provider == action.provider &&
      connection.revision == action.connectionRevision &&
      connection.selectedCalendarIds.contains(action.calendarId) &&
      !connection.calendars.any(
        (calendar) =>
            calendar.id == action.calendarId &&
            (calendar.error != null || calendar.lastSuccessAt == null),
      );

  Future<void> load() async {
    if (busy || _disposed) return;
    busy = true;
    failed = false;
    notifyListeners();
    try {
      final result = await gateway.loadCalendarActions(personId);
      final enabled = gateway is CalendarActionExecutionGateway
          ? await (gateway as CalendarActionExecutionGateway)
                .calendarWritesEnabled(personId)
          : false;
      final loadedAuthority = gateway is CalendarActionExecutionGateway
          ? await (gateway as CalendarActionExecutionGateway)
                .loadActionAuthority(personId)
          : authority;
      if (_disposed) return;
      if (result.any((action) => action.personId != personId)) {
        throw StateError('Unexpected proposal owner');
      }
      actions = List.unmodifiable(result);
      writesEnabled = enabled;
      authority = loadedAuthority;
      needsReload = false;
    } on Object {
      if (_disposed) return;
      failed = true;
      needsReload = true;
    } finally {
      if (!_disposed) {
        busy = false;
        phase = null;
        notifyListeners();
      }
    }
  }

  Future<void> decide(
    String id,
    CalendarActionDecision decision,
    CalendarConnection? connection,
    DateTime now,
  ) async {
    final action = find(id);
    if (_disposed ||
        busy ||
        needsReload ||
        action == null ||
        !action.status.canDecide) {
      return;
    }
    if (decision == CalendarActionDecision.approve &&
        !canApprove(action, connection, now)) {
      return;
    }
    busy = true;
    failed = false;
    notifyListeners();
    try {
      final result = await gateway.decideCalendarAction(
        personId: personId,
        actionId: id,
        decision: decision,
      );
      if (_disposed) return;
      if (result.id != id ||
          result.personId != personId ||
          result.executionId != action.executionId) {
        throw StateError('Unexpected proposal result');
      }
      actions = List.unmodifiable(
        actions.map((entry) => entry.id == id ? result : entry),
      );
      if (decision == CalendarActionDecision.approve &&
          writesEnabled &&
          result.status == CalendarActionStatus.approved) {
        phase = 'executing';
        notifyListeners();
        final executed = await (gateway as CalendarActionExecutionGateway)
            .executeCalendarAction(personId, id);
        if (_disposed) return;
        _replace(executed, action);
        if (executed.status == CalendarActionStatus.succeeded) {
          await _collect(executed);
        }
      }
    } on Object {
      if (_disposed) return;
      failed = true;
      needsReload = true;
    } finally {
      if (!_disposed) {
        busy = false;
        phase = null;
        notifyListeners();
      }
    }
  }

  bool get canPropose =>
      gateway is CalendarActionExecutionGateway &&
      authority.calendarCreate != ActionAuthorityMode.deny &&
      !busy &&
      !needsReload &&
      !actions.any(
        (action) =>
            action.status == CalendarActionStatus.unknown ||
            action.status == CalendarActionStatus.executing,
      );

  Future<CalendarAction?> propose({
    required String calendarId,
    required String title,
    required DateTime startsAt,
    required DateTime endsAt,
    required String timezone,
  }) async {
    if (!canPropose || _disposed) return null;
    busy = true;
    failed = false;
    notifyListeners();
    try {
      final result = await (gateway as CalendarActionExecutionGateway)
          .proposeCalendarAction(
            personId: personId,
            calendarId: calendarId,
            title: title,
            startsAt: startsAt,
            endsAt: endsAt,
            timezone: timezone,
          );
      if (_disposed) return null;
      if (result.personId != personId ||
          result.status != CalendarActionStatus.pending) {
        throw StateError('Unexpected proposal');
      }
      actions = List.unmodifiable([result, ...actions]);
      if (authority.calendarCreate == ActionAuthorityMode.allow &&
          writesEnabled) {
        final approved = await (gateway as CalendarActionExecutionGateway)
            .decideCalendarAction(
              personId: personId,
              actionId: result.id,
              decision: CalendarActionDecision.approve,
            );
        _replace(approved, result);
        phase = 'executing';
        notifyListeners();
        final executed = await (gateway as CalendarActionExecutionGateway)
            .executeCalendarAction(personId, result.id);
        _replace(executed, approved);
        if (executed.status == CalendarActionStatus.succeeded) {
          await _collect(executed);
        }
        return executed;
      }
      return result;
    } on Object {
      if (!_disposed) {
        failed = true;
        needsReload = true;
      }
      return null;
    } finally {
      if (!_disposed) {
        busy = false;
        phase = null;
        notifyListeners();
      }
    }
  }

  Future<void> setCalendarCreateAuthority(ActionAuthorityMode mode) async {
    if (_disposed || busy || gateway is! CalendarActionExecutionGateway) return;
    busy = true;
    failed = false;
    notifyListeners();
    try {
      authority = await (gateway as CalendarActionExecutionGateway)
          .setCalendarCreateAuthority(personId, mode);
    } on Object {
      if (!_disposed) failed = true;
    } finally {
      if (!_disposed) {
        busy = false;
        notifyListeners();
      }
    }
  }

  void _replace(CalendarAction result, CalendarAction original) {
    if (result.id != original.id ||
        result.personId != personId ||
        result.executionId != original.executionId) {
      throw StateError('Unexpected execution result');
    }
    actions = List.unmodifiable(
      actions.map((entry) => entry.id == result.id ? result : entry),
    );
    notifyListeners();
  }

  Future<void> run(
    String id, {
    bool recover = false,
    CalendarConnection? connection,
  }) async {
    final action = find(id);
    if (_disposed ||
        busy ||
        needsReload ||
        action == null ||
        gateway is! CalendarActionExecutionGateway) {
      return;
    }
    if (recover) {
      if (action.status != CalendarActionStatus.executing &&
          action.status != CalendarActionStatus.unknown) {
        return;
      }
    } else if (!writesEnabled ||
        !canApprove(action, connection, DateTime.now(), approved: true)) {
      return;
    }
    busy = true;
    failed = false;
    phase = recover ? 'recovering' : 'executing';
    notifyListeners();
    try {
      final executor = gateway as CalendarActionExecutionGateway;
      final result = recover
          ? await executor.recoverCalendarAction(personId, id)
          : await executor.executeCalendarAction(personId, id);
      if (_disposed) return;
      _replace(result, action);
      if (result.status == CalendarActionStatus.succeeded) {
        await _collect(result);
      }
    } on Object {
      if (!_disposed) {
        failed = true;
        needsReload = true;
      }
    } finally {
      if (!_disposed) {
        busy = false;
        phase = null;
        notifyListeners();
      }
    }
  }

  Future<void> _collect(CalendarAction action) async {
    collection[action.id] = 'collecting';
    phase = 'collecting';
    notifyListeners();
    try {
      if (collect == null) throw StateError('Calendar read unavailable');
      await collect!(action);
      if (!_disposed) collection[action.id] = 'collected';
    } on Object {
      if (!_disposed) collection[action.id] = 'failed';
    }
  }

  Future<void> retryRead(String id) async {
    final action = find(id);
    if (_disposed || busy || action?.status != CalendarActionStatus.succeeded) {
      return;
    }
    busy = true;
    notifyListeners();
    await _collect(action!);
    if (!_disposed) {
      busy = false;
      phase = null;
      notifyListeners();
    }
  }

  @override
  void dispose() {
    _disposed = true;
    super.dispose();
  }
}
