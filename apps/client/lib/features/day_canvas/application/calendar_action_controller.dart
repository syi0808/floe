import 'package:flutter/foundation.dart';

import '../domain/calendar_action.dart';
import '../domain/day_models.dart';
import 'calendar_action_gateway.dart';

final class CalendarActionController extends ChangeNotifier {
  CalendarActionController({required this.gateway, required this.personId});

  final CalendarActionGateway gateway;
  final String personId;
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
    DateTime now,
  ) =>
      !busy &&
      !needsReload &&
      action.personId == personId &&
      action.status.canDecide &&
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
      if (_disposed) return;
      if (result.any((action) => action.personId != personId)) {
        throw StateError('Unexpected proposal owner');
      }
      actions = List.unmodifiable(result);
      needsReload = false;
    } on Object {
      if (_disposed) return;
      failed = true;
      needsReload = true;
    } finally {
      if (!_disposed) {
        busy = false;
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
    } on Object {
      if (_disposed) return;
      failed = true;
      needsReload = true;
    } finally {
      if (!_disposed) {
        busy = false;
        notifyListeners();
      }
    }
  }

  @override
  void dispose() {
    _disposed = true;
    super.dispose();
  }
}
