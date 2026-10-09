import 'package:floe_client/features/actions/domain/calendar_action.dart';

/// Access-owned policy for Expert-proposed Calendar changes.
abstract interface class OperationAuthorizationGateway {
  Future<ActionAuthority> loadAuthority();

  Future<ActionAuthority> setAuthority({
    required String commandId,
    required ActionAuthorityMode mode,
    required int expectedRevision,
  });
}
