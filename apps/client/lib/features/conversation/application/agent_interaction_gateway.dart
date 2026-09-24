import 'package:floe_client/app/runtime/floe_client.dart';
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

  Future<AppCommandReceipt> resumeInteraction({
    required String sessionId,
    required String originRunId,
    required int expectedRevision,
  });
}

abstract interface class AgentInteractionProvider {
  AgentInteractionGateway? get interactionGateway;
}

final class NativeAgentInteractionGateway implements AgentInteractionGateway {
  NativeAgentInteractionGateway(this._client);

  final FloeClient _client;

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
    final command = _client.prepareInteractionResolve(
      interactionId: snapshot.id,
      sessionId: snapshot.sessionId,
      expectedRevision: snapshot.revision,
      decision: decision,
      targetDigest: snapshot.targetDigest,
    );
    // Decisions are idempotent under their stable command id: a lost
    // response resubmits the identical command, never a new decision.
    try {
      return await _client.submitInteractionResolve(command);
    } on Object {
      return _client.submitInteractionResolve(command);
    }
  }

  @override
  Future<AgentInteractionRefreshResult> refreshInteraction(
    AgentInteractionSnapshot snapshot,
  ) async {
    final command = _client.prepareInteractionRefresh(
      interactionId: snapshot.id,
      sessionId: snapshot.sessionId,
      expectedRevision: snapshot.revision,
    );
    try {
      return await _client.submitInteractionRefresh(command);
    } on Object {
      return _client.submitInteractionRefresh(command);
    }
  }

  @override
  Future<AppCommandReceipt> resumeInteraction({
    required String sessionId,
    required String originRunId,
    required int expectedRevision,
  }) async {
    final command = _client.prepareInteractionResume(
      sessionId: sessionId,
      originRunId: originRunId,
      expectedRevision: expectedRevision,
    );
    try {
      return await _client.submitInteractionResume(command);
    } on Object {
      final recovered = await _client.getCommand(command.commandId);
      if (recovered != null) return recovered;
      return _client.submitInteractionResume(command);
    }
  }
}
