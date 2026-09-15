import 'agent_fixture_gateway.dart';

final class AgentConversationTurnRequest {
  const AgentConversationTurnRequest({
    required this.session,
    required this.text,
    this.continuation = false,
    this.retryOf,
  }) : assert(!continuation || retryOf == null);

  final AgentSession session;
  final String text;
  final bool continuation;
  final String? retryOf;
}

abstract interface class AgentConversationGateway {
  Future<AgentSession> startConversation(String personId);
  Future<AgentSession> resumeConversation(String personId);
  Future<AgentSession> loadConversation(String personId, String sessionId);
  Future<AgentSession> recoverConversation(AgentSession session);
}
