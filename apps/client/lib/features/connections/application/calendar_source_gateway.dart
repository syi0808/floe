import 'package:floe_client/features/connections/domain/calendar_source_connection.dart';

abstract interface class CalendarSourceGateway {
  Future<CalendarSourceConnection?> inspectNative(String personId);

  Future<CalendarSourceConnection> establishNative(
    String personId, {
    required String resourceMode,
    required List<CalendarSourceResource> resources,
  });

  Future<CalendarSourceConnection> configureNative(
    String personId, {
    required CalendarSourceConnection current,
    required String resourceMode,
    required List<CalendarSourceResource> resources,
  });

  Future<CalendarSourceConnection> reconcileNativeInventory(
    String personId, {
    required CalendarSourceConnection current,
    required List<CalendarSourceResource> resources,
  });

  Future<CalendarSourceConnection> disconnectNative(
    String personId, {
    required CalendarSourceConnection current,
  });

  Future<List<CalendarSourceConnection>> inspectRemote(String personId);

  Future<CalendarSourceConnection> bindRemote(
    String personId, {
    required String connectorId,
    required String connectionId,
    required List<CalendarSourceResource> resources,
    CalendarSourceConnection? current,
  });

  Future<CalendarSourceConnection> disconnectRemote(
    String personId, {
    required CalendarSourceConnection current,
  });
}
