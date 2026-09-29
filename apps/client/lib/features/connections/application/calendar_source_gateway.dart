import 'package:floe_client/features/connections/domain/source_connection.dart';

abstract interface class CalendarSourceGateway {
  Future<SourceConnection?> inspectNative(String personId);

  Future<SourceConnection> establishNative(
    String personId, {
    required String resourceMode,
    required List<SourceResource> resources,
  });

  Future<SourceConnection> configureNative(
    String personId, {
    required SourceConnection current,
    required String resourceMode,
    required List<SourceResource> resources,
  });

  Future<SourceConnection> reconcileNativeInventory(
    String personId, {
    required SourceConnection current,
    required List<SourceResource> resources,
  });

  Future<SourceConnection> disconnectNative(
    String personId, {
    required SourceConnection current,
  });

  Future<List<SourceConnection>> inspectRemote(String personId);

  Future<SourceConnection> bindRemote(
    String personId, {
    required String connectorId,
    required String connectionId,
    required List<SourceResource> resources,
    SourceConnection? current,
  });

  Future<SourceConnection> disconnectRemote(
    String personId, {
    required SourceConnection current,
  });
}
