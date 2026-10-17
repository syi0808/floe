import 'package:floe_client/features/conversation/application/conversation_runtime_gateway.dart';
import 'package:floe_client/features/conversation/application/agent_interaction_gateway.dart';
import 'package:floe_client/features/conversation/domain/agent_session.dart';

final class AgentContinuationRequest {
  const AgentContinuationRequest({required this.session});
  final AgentSession session;
}

abstract interface class AgentConversationGateway {
  ConversationRuntimeGateway get conversationRuntime;
  AgentInteractionGateway get interactionGateway;

  /// A fresh user intent. It cannot overwrite an unresolved Start.
  Future<AgentSession> startConversation(String personId);

  /// Complete the retained Start with its exact immutable ID, or null if absent.
  /// This is explicitly a command operation, never part of a Session query.
  Future<AgentSession?> settlePendingSessionStart(String personId);
  Future<AgentSession?> resumeConversation(String personId);
  Future<AgentSession> loadConversation(String personId, String sessionId);
  Future<AgentSession> loadEarlierConversation(
    String personId,
    String sessionId,
    String beforeMessageId,
  );
}
