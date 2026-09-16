import 'package:floe_client/app/runtime/app_runtime.dart';
import 'package:floe_client/features/actions/application/calendar_action_gateway.dart';
import 'package:floe_client/features/actions/domain/calendar_action.dart';
import 'package:floe_client/features/actions/infrastructure/native_calendar_action_gateway.dart';

/// Proposal, approval, execution and recovery of calendar actions.
///
/// Approval state and the uncertain-result outcome are owned by Rust Actions;
/// this class only carries the request and the settled result.
final class CalendarActionFacade
    implements CalendarActionExecutionGateway, CalendarDirectActionGateway {
  CalendarActionFacade(AppRuntime runtime)
    : _gateway = NativeCalendarActionGateway(runtime.request);

  final NativeCalendarActionGateway _gateway;

  @override
  Future<List<CalendarAction>> loadCalendarActions(String personId) =>
      _gateway.loadCalendarActions(personId);

  Future<CalendarAction> loadCalendarAction(String personId, String actionId) =>
      _gateway.loadCalendarAction(personId, actionId);

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
  }) => _gateway.submitDirectCalendarAction(
    personId: personId,
    calendarId: calendarId,
    title: title,
    startsAt: startsAt,
    endsAt: endsAt,
    timezone: timezone,
    eventId: eventId,
    eventRevision: eventRevision,
    delete: delete,
  );

  @override
  Future<CalendarAction> proposeCalendarAction({
    required String personId,
    required String calendarId,
    required String title,
    required DateTime startsAt,
    required DateTime endsAt,
    required String timezone,
  }) => _gateway.proposeCalendarAction(
    personId: personId,
    calendarId: calendarId,
    title: title,
    startsAt: startsAt,
    endsAt: endsAt,
    timezone: timezone,
  );

  @override
  Future<CalendarAction> decideCalendarAction({
    required String personId,
    required String actionId,
    required CalendarActionDecision decision,
  }) => _gateway.decideCalendarAction(
    personId: personId,
    actionId: actionId,
    decision: decision,
  );

  @override
  Future<bool> calendarWritesEnabled(String personId) =>
      _gateway.calendarWritesEnabled(personId);

  @override
  Future<ActionAuthority> loadActionAuthority(String personId) =>
      _gateway.loadActionAuthority(personId);

  @override
  Future<ActionAuthority> setCalendarCreateAuthority(
    String personId,
    ActionAuthorityMode mode,
  ) => _gateway.setCalendarCreateAuthority(personId, mode);

  @override
  Future<CalendarAction> executeCalendarAction(
    String personId,
    String actionId,
  ) => _gateway.executeCalendarAction(personId, actionId);

  @override
  Future<CalendarAction> recoverCalendarAction(
    String personId,
    String actionId,
  ) => _gateway.recoverCalendarAction(personId, actionId);
}
