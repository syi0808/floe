import 'package:floe_client/features/actions/domain/calendar_action.dart';

abstract interface class CalendarActionGateway {
  Future<List<ActionDestinationChoice>> loadDestinations();
  Future<ActionProposalPreview> loadProposalPreview(
    TaskExecutionReceiptReference receipt,
    String artifactId,
  );

  Future<ActionAuthority> loadAuthority();

  Future<ActionAuthority> setAuthority({
    required String commandId,
    required ActionAuthorityMode mode,
    required int expectedRevision,
  });

  Future<CalendarAction> submit({
    required String commandId,
    required ActionIntent intent,
  });

  Future<CalendarAction> decide({
    required String commandId,
    required CalendarAction action,
    required CalendarActionDecision decision,
  });

  Future<CalendarAction> reconcile({
    required String commandId,
    required CalendarAction action,
  });

  Future<CalendarAction> inspect(String actionRef);

  Future<ActionsPage> list({String? cursor, int limit = 100});
}
