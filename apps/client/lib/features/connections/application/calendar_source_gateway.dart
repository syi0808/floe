import 'package:floe_client/features/connections/domain/source_connection.dart';

/// Frozen S2 Calendar Action display dependency. No S1 product adapter is composed.
abstract interface class CalendarSourceGateway {
  Future<SourceConnection?> inspectNative(String personId);
  Future<List<SourceConnection>> inspectRemote(String personId);
}
