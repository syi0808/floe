import 'package:floe_client/features/conversation/application/conversation_runtime_gateway.dart';
import 'package:floe_client/features/conversation/application/agent_interaction_gateway.dart';
import 'package:floe_client/features/conversation/domain/agent_session.dart';

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
  ConversationRuntimeGateway get conversationRuntime;
  AgentInteractionGateway get interactionGateway;
  Future<AgentSession> startConversation(String personId);
  Future<AgentSession?> resumeConversation(String personId);
  Future<AgentSession> loadConversation(String personId, String sessionId);
  Future<AgentSession> loadEarlierConversation(
    String personId,
    String sessionId,
    String beforeMessageId,
  );
  Future<AgentSession> recoverConversation(AgentSession session);
}
