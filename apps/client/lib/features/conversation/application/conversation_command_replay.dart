import 'package:floe_client/features/conversation/application/agent_conversation_gateway.dart';
import 'package:floe_client/features/conversation/application/agent_request_id.dart';

/// Retain only exact pending command identities for the lifetime of the
/// Conversation gateway. A transport-uncertain retry keeps the same payload.
final class ConversationCommandReplay {
  ConversationCommandReplay._();
  static final Expando<ConversationCommandReplay> _byGateway =
      Expando<ConversationCommandReplay>('Conversation command replay');
  static const _maximumPending = 128;
  final Map<String, String> _pending = {};

  static ConversationCommandReplay forGateway(AgentConversationGateway gateway) =>
      _byGateway[gateway] ??= ConversationCommandReplay._();

  String retain(String exactPayload) {
    final previous = _pending[exactPayload];
    if (previous != null) return previous;
    if (_pending.length >= _maximumPending) {
      throw StateError('Too many unresolved Conversation commands.');
    }
    return _pending[exactPayload] = newAgentRequestId();
  }

  void acknowledge(String exactPayload, String commandId) {
    if (_pending[exactPayload] == commandId) _pending.remove(exactPayload);
  }
}
