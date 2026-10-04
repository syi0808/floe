import 'package:floe_client/app/runtime/app_runtime.dart';
import 'package:floe_client/features/actions/application/calendar_action_gateway.dart';
import 'package:floe_client/features/actions/domain/calendar_action.dart';
import 'package:floe_client/features/actions/infrastructure/native_calendar_action_gateway.dart';

/// The single concrete Actions facade injected into product surfaces.
final class CalendarActionFacade implements CalendarActionGateway {
  CalendarActionFacade(AppRuntime runtime)
    : _gateway = NativeCalendarActionGateway(runtime.wireTransport);

  final NativeCalendarActionGateway _gateway;

  @override
  Future<List<ActionDestinationChoice>> loadDestinations() =>
      _gateway.loadDestinations();

  @override
  Future<ActionProposalPreview> loadProposalPreview(
    TaskExecutionReceiptReference receipt,
    String artifactId,
  ) => _gateway.loadProposalPreview(receipt, artifactId);

  @override
  Future<ActionAuthority> loadAuthority() => _gateway.loadAuthority();

  @override
  Future<ActionAuthority> setAuthority({
    required String commandId,
    required ActionAuthorityMode mode,
    required int expectedRevision,
  }) => _gateway.setAuthority(
    commandId: commandId,
    mode: mode,
    expectedRevision: expectedRevision,
  );

  @override
  Future<CalendarAction> submit({
    required String commandId,
    required ActionIntent intent,
  }) => _gateway.submit(commandId: commandId, intent: intent);

  @override
  Future<CalendarAction> decide({
    required String commandId,
    required CalendarAction action,
    required CalendarActionDecision decision,
  }) =>
      _gateway.decide(commandId: commandId, action: action, decision: decision);

  @override
  Future<CalendarAction> reconcile({
    required String commandId,
    required CalendarAction action,
  }) => _gateway.reconcile(commandId: commandId, action: action);

  @override
  Future<CalendarAction> inspect(String actionRef) =>
      _gateway.inspect(actionRef);

  @override
  Future<ActionsPage> list({String? cursor, int limit = 100}) =>
      _gateway.list(cursor: cursor, limit: limit);
}
