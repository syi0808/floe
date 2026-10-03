import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/app/runtime/owner_operation.dart';
import 'package:floe_client/app/runtime/agent_vault_gateway.dart';
import 'package:floe_client/app/runtime/native_transport.dart';
import 'package:floe_client/features/conversation/application/agent_request_id.dart';
import 'package:floe_client/features/conversation/infrastructure/app_wire_conversation_client.dart';
import 'package:floe_client/app/runtime/app_read_model.dart';
import 'package:floe_client/features/conversation/application/conversation_runtime_gateway.dart';
import 'package:floe_client/features/conversation/application/agent_conversation_gateway.dart';
import 'package:floe_client/features/conversation/application/agent_interaction_gateway.dart';
import 'package:floe_client/features/conversation/domain/agent_session.dart';

final class AppWireConversationGateway
    implements AgentConversationGateway {
  AppWireConversationGateway(
    this._transport, {
    required AppWireConversationClient runtimeClient,
    required AppReadModel readModel,
    Future<void> Function()? beforeConversationStart,
  }) {
    _conversationRuntime = NativeConversationRuntimeGateway(
      client: runtimeClient, readModel: readModel,
      loadSession: loadConversation, beforeStartTurn: beforeConversationStart,
    );
    _interactionGateway = NativeAgentInteractionGateway(runtimeClient);
  }
  late final ConversationRuntimeGateway _conversationRuntime;
  @override
  ConversationRuntimeGateway get conversationRuntime => _conversationRuntime;
  late final AgentInteractionGateway _interactionGateway;
  @override
  AgentInteractionGateway get interactionGateway => _interactionGateway;

  final AppWireTransport _transport;
  _PendingSessionCommand? _pending;
  bool _busy = false;

  @override
  Future<AgentSession> startConversation(String personId) =>
      _session(personId, {'kind': 'conversation.session.start'}, command: true);
  @override
  Future<AgentSession> resumeConversation(String personId) =>
      _session(personId, {'kind': 'conversation.session.resume'});
  @override
  Future<AgentSession> loadConversation(String personId, String sessionId) =>
      _session(personId, {'kind': 'conversation.session.get', 'session_id': sessionId});
  @override
  Future<AgentSession> recoverConversation(AgentSession session) =>
      _session(session.personId, {'kind': 'conversation.session.recover',
        'session_id': session.id, 'expected_revision': session.revision}, command: true);

  Future<AgentSession> _session(String personId, Map<String, Object?> payload, {bool command = false}) async {
    if (_busy) throw const AgentVaultException('conflict');
    _busy = true;
    try {
      final intent = ownerIntent(payload);
      if (_pending case final pending?) {
        if (pending.personId != personId) throw const AgentVaultException('conflict');
        final recovered = await _submit(pending);
        if (pending.intent == intent) return recovered;
      }
      if (command) {
        final pending = _PendingSessionCommand(personId, newAgentRequestId(), intent, Map.unmodifiable(payload));
        _pending = pending;
        return await _submit(pending);
      }
      return _decode(await ownerQuery(_transport, newAgentRequestId(), payload), personId, payload['session_id'] as String?);
    } on NativeTransportException catch (error) {
      throw AgentVaultException.fromAppWire(error.metadata['agent_failure'] ?? error.code,
        requestId: _pending?.commandId, stage: 'conversation_session', metadata: error.metadata, ownerFailure: error.ownerFailure);
    } finally { _busy = false; }
  }

  Future<AgentSession> _submit(_PendingSessionCommand pending) async {
    // A lost response keeps the exact command and identity for owner replay.
    try {
      final result = await ownerCommand(_transport, pending.commandId, pending.payload);
      final session = _decode(result, pending.personId, pending.payload['session_id'] as String?);
      _pending = null;
      return session;
    } on NativeTransportException catch (error) {
      if (error.code != 'timeout' && error.code != 'ffi') _pending = null;
      rethrow;
    }
  }

  AgentSession _decode(Map<String, dynamic> result, String personId, String? sessionId) {
    if (result.length != 2 || result['kind'] != 'conversation_session' || result['session'] is! Map) {
      throw const FormatException('Invalid Conversation session result.');
    }
    final session = AgentSession.fromJson(Map<String, Object?>.from(result['session'] as Map));
    if (session.personId != personId || sessionId != null && session.id != sessionId) {
      throw const FormatException('Conversation session mismatch.');
    }
    return session;
  }
}

final class _PendingSessionCommand {
  const _PendingSessionCommand(this.personId, this.commandId, this.intent, this.payload);
  final String personId;
  final String commandId;
  final String intent;
  final Map<String, Object?> payload;
}
