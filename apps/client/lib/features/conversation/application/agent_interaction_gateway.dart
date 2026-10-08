import 'package:floe_client/app/runtime/app_owner_exception.dart';
import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/features/conversation/infrastructure/app_wire_conversation_client.dart';
import 'package:floe_client/features/conversation/domain/agent_interaction.dart';

abstract interface class AgentInteractionGateway {
  Future<AgentInteractionSnapshot?> loadInteraction(String interactionId);

  Future<List<AgentInteractionSnapshot>> loadSessionInteractions(
    String sessionId,
  );

  Future<AgentInteractionResolveResult> decideInteraction(
    AgentInteractionSnapshot snapshot,
    AgentInteractionDecision decision,
  );

  Future<AgentInteractionRefreshResult> refreshInteraction(
    AgentInteractionSnapshot snapshot,
  );
}

final class NativeAgentInteractionGateway implements AgentInteractionGateway {
  NativeAgentInteractionGateway(this._client);

  final AppWireConversationClient _client;
  final Map<String, PreparedInteractionResolve> _pendingDecisions = {};
  final Map<String, PreparedInteractionRefresh> _pendingRefreshes = {};

  @override
  Future<AgentInteractionSnapshot?> loadInteraction(String interactionId) =>
      _client.getInteraction(interactionId);

  @override
  Future<List<AgentInteractionSnapshot>> loadSessionInteractions(
    String sessionId,
  ) => _client.listInteractions(sessionId);

  @override
  Future<AgentInteractionResolveResult> decideInteraction(
    AgentInteractionSnapshot snapshot,
    AgentInteractionDecision decision,
  ) async {
    final previous = _pendingDecisions[snapshot.id];
    if (previous != null &&
        (previous.sessionId != snapshot.sessionId ||
            previous.decision != decision)) {
      throw AppOwnerException('conflict', requestId: previous.commandId);
    }
    final command =
        previous ??
        _client.prepareInteractionResolve(
          interactionId: snapshot.id,
          sessionId: snapshot.sessionId,
          expectedRevision: snapshot.revision,
          decision: decision,
          targetDigest: snapshot.targetDigest,
        );
    final previouslySubmitted = previous != null;
    _pendingDecisions[snapshot.id] = command;
    try {
      final result = await _client.submitInteractionResolve(command);
      _pendingDecisions.remove(snapshot.id);
      return result;
    } on AppWireTransportException catch (error) {
      if (mayDiscardPendingCommand(
        error,
        previouslySubmitted: previouslySubmitted,
      )) {
        _pendingDecisions.remove(snapshot.id);
      }
      rethrow;
    }
  }

  @override
  Future<AgentInteractionRefreshResult> refreshInteraction(
    AgentInteractionSnapshot snapshot,
  ) async {
    final previous = _pendingRefreshes[snapshot.id];
    if (previous != null && previous.sessionId != snapshot.sessionId) {
      throw AppOwnerException('conflict', requestId: previous.commandId);
    }
    final command =
        previous ??
        _client.prepareInteractionRefresh(
          interactionId: snapshot.id,
          sessionId: snapshot.sessionId,
          expectedRevision: snapshot.revision,
        );
    final previouslySubmitted = previous != null;
    _pendingRefreshes[snapshot.id] = command;
    try {
      final result = await _client.submitInteractionRefresh(command);
      _pendingRefreshes.remove(snapshot.id);
      return result;
    } on AppWireTransportException catch (error) {
      if (mayDiscardPendingCommand(
        error,
        previouslySubmitted: previouslySubmitted,
      )) {
        _pendingRefreshes.remove(snapshot.id);
      }
      rethrow;
    }
  }
}
