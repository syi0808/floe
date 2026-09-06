import '../domain/calendar_action.dart';

abstract interface class CalendarDirectActionGateway {
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
  });
}

abstract interface class CalendarActionGateway {
  Future<List<CalendarAction>> loadCalendarActions(String personId);

  Future<CalendarAction> decideCalendarAction({
    required String personId,
    required String actionId,
    required CalendarActionDecision decision,
  });
}

abstract interface class CalendarActionExecutionGateway
    implements CalendarActionGateway {
  Future<ActionAuthority> loadActionAuthority(String personId);
  Future<ActionAuthority> setCalendarCreateAuthority(
    String personId,
    ActionAuthorityMode mode,
  );
  Future<bool> calendarWritesEnabled(String personId);
  Future<CalendarAction> executeCalendarAction(
    String personId,
    String actionId,
  );
  Future<CalendarAction> recoverCalendarAction(
    String personId,
    String actionId,
  );
  Future<CalendarAction> proposeCalendarAction({
    required String personId,
    required String calendarId,
    required String title,
    required DateTime startsAt,
    required DateTime endsAt,
    required String timezone,
  });
}
