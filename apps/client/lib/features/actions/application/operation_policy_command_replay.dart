import 'package:floe_client/features/actions/application/calendar_action_gateway.dart';
import 'package:floe_client/features/conversation/application/agent_request_id.dart';

/// App-gateway-lifetime correlation only. An uncertain command retains its
/// exact payload identity when a screen/controller is disposed and recreated.
final class OperationPolicyCommandReplay {
  OperationPolicyCommandReplay._();
  static final Expando<OperationPolicyCommandReplay> _byGateway =
      Expando<OperationPolicyCommandReplay>('Access operation policy replay');
  static const _maximumPending = 128;
  final Map<String, String> _pending = {};

  static OperationPolicyCommandReplay forGateway(OperationAuthorizationGateway gateway) =>
      _byGateway[gateway] ??= OperationPolicyCommandReplay._();

  String retain(String exactPayload) {
    final previous = _pending[exactPayload];
    if (previous != null) return previous;
    if (_pending.length >= _maximumPending) {
      throw StateError(
        'Too many unresolved operation policy commands. Refresh the current policy.',
      );
    }
    return _pending[exactPayload] = newAgentRequestId();
  }

  void acknowledge(String exactPayload, String commandId) {
    if (_pending[exactPayload] == commandId) _pending.remove(exactPayload);
  }
}
