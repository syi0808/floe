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

final class AppWireConversationGateway implements AgentConversationGateway {
  AppWireConversationGateway(
    this._transport, {
    required AppWireConversationClient runtimeClient,
    required AppReadModel readModel,
    Future<void> Function()? beforeConversationStart,
  }) {
    _conversationRuntime = NativeConversationRuntimeGateway(
      client: runtimeClient,
      readModel: readModel,
      loadSession: loadConversation,
      beforeStartTurn: beforeConversationStart,
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
  _PendingSessionStart? _pending;
  bool _commandBusy = false;

  @override
  Future<AgentSession> startConversation(String personId) async {
    if (_commandBusy || _pending != null) {
      throw const AgentVaultException('conflict');
    }
    final pending = _PendingSessionStart(personId, newAgentRequestId());
    _pending = pending;
    return _submit(pending);
  }

  @override
  Future<AgentSession?> settlePendingSessionStart(String personId) async {
    final pending = _pending;
    if (pending == null) return null;
    if (pending.personId != personId) {
      throw const AgentVaultException('conflict');
    }
    return _submit(pending);
  }

  @override
  Future<AgentSession?> resumeConversation(String personId) =>
      _querySession(personId, {'kind': 'conversation.session.resume'});

  @override
  Future<AgentSession> loadConversation(String personId, String sessionId) =>
      _requiredQuerySession(personId, {
        'kind': 'conversation.session.get',
        'session_id': sessionId,
      });

  @override
  Future<AgentSession> loadEarlierConversation(
    String personId,
    String sessionId,
    String beforeMessageId,
  ) => _requiredQuerySession(personId, {
    'kind': 'conversation.session.get',
    'session_id': sessionId,
    'before_message_id': beforeMessageId,
  }, stage: 'conversation_history');

  Future<AgentSession> _requiredQuerySession(
    String personId,
    Map<String, Object?> payload, {
    String stage = 'conversation_session',
  }) async =>
      await _querySession(personId, payload, stage: stage) ??
      (throw const FormatException('Missing Conversation session result.'));

  Future<AgentSession?> _querySession(
    String personId,
    Map<String, Object?> payload, {
    String stage = 'conversation_session',
  }) async {
    // Reads neither own the command lock nor observe/mutate its retained slot.
    final requestId = newAgentRequestId();
    try {
      final result = await ownerQuery(_transport, requestId, payload);
      if (payload['kind'] == 'conversation.session.resume' &&
          result.length == 1 &&
          result['kind'] == 'conversation_session_absent') {
        return null;
      }
      return _decode(result, personId, payload['session_id'] as String?);
    } on NativeTransportException catch (error) {
      throw AgentVaultException.fromAppWire(
        error.metadata['agent_failure'] ?? error.code,
        requestId: requestId,
        stage: stage,
        metadata: error.metadata,
        ownerFailure: error.ownerFailure,
      );
    }
  }

  Future<AgentSession> _submit(_PendingSessionStart pending) async {
    if (_commandBusy) throw const AgentVaultException('conflict');
    _commandBusy = true;
    final wasSubmitted = pending.submitted;
    pending.submitted = true;
    try {
      final result = await ownerCommand(_transport, pending.commandId, const {
        'kind': 'conversation.session.start',
      });
      final session = _decode(result, pending.personId, null);
      if (identical(_pending, pending)) _pending = null;
      return session;
    } on NativeTransportException catch (error) {
      if (error.commandDisposition == NativeCommandDisposition.notApplied ||
          (!wasSubmitted &&
              error.commandDisposition ==
                  NativeCommandDisposition.notAdmitted)) {
        if (identical(_pending, pending)) _pending = null;
      }
      throw AgentVaultException.fromAppWire(
        error.metadata['agent_failure'] ?? error.code,
        requestId: pending.commandId,
        stage: 'conversation_session_start',
        metadata: error.metadata,
        ownerFailure: error.ownerFailure,
      );
    } finally {
      _commandBusy = false;
    }
  }

  AgentSession _decode(
    Map<String, dynamic> result,
    String personId,
    String? sessionId,
  ) {
    if (result.length != 2 ||
        result['kind'] != 'conversation_session' ||
        result['session'] is! Map) {
      throw const FormatException('Invalid Conversation session result.');
    }
    final session = AgentSession.fromJson(
      Map<String, Object?>.from(result['session'] as Map),
    );
    if (session.personId != personId ||
        sessionId != null && session.id != sessionId) {
      throw const FormatException('Conversation session mismatch.');
    }
    return session;
  }
}

final class _PendingSessionStart {
  _PendingSessionStart(this.personId, this.commandId);
  final String personId;
  final String commandId;
  bool submitted = false;
}
