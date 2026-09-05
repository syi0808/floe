import '../domain/calendar_action.dart';

abstract interface class CalendarActionGateway {
  Future<List<CalendarAction>> loadCalendarActions(String personId);

  Future<CalendarAction> decideCalendarAction({
    required String personId,
    required String actionId,
    required CalendarActionDecision decision,
  });
}
