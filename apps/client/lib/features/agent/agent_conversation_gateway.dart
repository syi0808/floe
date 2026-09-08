import 'agent_fixture_gateway.dart';

final class AgentConversationTurnRequest {
  const AgentConversationTurnRequest({
    required this.session,
    required this.text,
    this.continuation = false,
  });

  final AgentSession session;
  final String text;
  final bool continuation;

  Map<String, Object?> toJson() => {
    'session_id': session.id,
    'expected_revision': session.revision,
    'text': text,
    if (continuation) 'continuation': true,
  };
}

abstract interface class AgentConversationGateway {
  Future<AgentSession> startConversation(String personId);
  Future<AgentSession> resumeConversation(String personId);
  Future<AgentSession> loadConversation(String personId, String sessionId);
  Future<AgentSession> recoverConversation(AgentSession session);
  Future<AgentRunUpdate> beginConversationTurn(
    AgentConversationTurnRequest request,
  );
  Future<AgentRunUpdate> pollConversationTurn(
    AgentConversationTurnRequest request,
    int afterSequence,
  );
  Future<AgentRunUpdate> stopConversationTurn(
    AgentConversationTurnRequest request,
  );
  Future<AgentRunUpdate> releaseConversationTurn(
    AgentConversationTurnRequest request,
  );
}
