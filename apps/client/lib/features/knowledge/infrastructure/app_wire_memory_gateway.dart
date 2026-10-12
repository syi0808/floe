import 'package:floe_client/app/runtime/app_owner_exception.dart';
import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/features/conversation/application/agent_request_id.dart';
import 'package:floe_client/features/knowledge/application/memory_gateway.dart';
import 'package:floe_client/features/knowledge/domain/agent_memory.dart';
import 'package:floe_client/features/knowledge/domain/memory_review.dart';

final class AppWireMemoryGateway
    implements
        AgentMemoryGateway,
        AgentMemoryReviewGateway,
        AgentMemoryForgetGateway {
  AppWireMemoryGateway(this._transport);

  final AppWireTransport _transport;
  _PendingMemoryDecision? _pending;
  _PendingMemoryForget? _pendingForget;
  @override
  String? get pendingForgetCommandId => _pendingForget?.commandId;
  @override
  String? get pendingForgetTargetId => _pendingForget?.targetId;
  @override
  int? get pendingForgetRevision => _pendingForget?.revision;

  @override
  String? get pendingCommandId => _pending?.commandId;

  @override
  AgentMemoryDecision? get pendingDecision => _pending?.decision;

  @override
  String? get pendingCandidateId => _pending?.candidateId;

  @override
  Future<AgentMemoryOverview> readMemory() async {
    final result = await _query(const {'kind': 'memory.overview'});
    final overview = AgentMemoryOverview.fromJson(
      _payload(result, 'memory.overview', 'overview'),
    );
    return overview;
  }

  @override
  Future<AgentMemoryReviewOverview> readMemoryReview() async {
    final result = await _query(const {'kind': 'memory.review'});
    return AgentMemoryReviewOverview.fromJson(
      _payload(result, 'memory.review', 'review'),
    );
  }

  @override
  Future<AgentMemoryDecisionAcknowledgement> decideMemoryCandidate({
    required String personId,
    required String candidateId,
    required AgentMemoryDecision decision,
  }) async {
    if (_pending != null || _pendingForget != null) {
      throw const AppOwnerException('conflict');
    }
    final pending = _PendingMemoryDecision(
      commandId: newAgentRequestId(),
      personId: personId,
      candidateId: candidateId,
      decision: decision,
      payload: Map<String, Object?>.unmodifiable({
        'kind': 'memory.decide',
        'candidate_id': candidateId,
        'decision': decision.name,
      }),
    );
    _pending = pending;
    return _submit(pending);
  }

  @override
  Future<AgentMemoryDecisionAcknowledgement> retryPendingDecision({
    required String personId,
  }) {
    final pending = _pending;
    if (pending == null || pending.personId != personId) {
      throw const AppOwnerException('conflict');
    }
    return _submit(pending);
  }

  @override
  Future<AgentMemoryForgetAcknowledgement> forgetMemory({
    required String personId,
    required String targetId,
    required int expectedRevision,
  }) {
    if (_pending != null || _pendingForget != null) {
      throw const AppOwnerException('conflict');
    }
    final pending = _PendingMemoryForget(
      commandId: newAgentRequestId(),
      personId: personId,
      targetId: targetId,
      revision: expectedRevision,
    );
    _pendingForget = pending;
    return _submitForget(pending);
  }

  @override
  Future<AgentMemoryForgetAcknowledgement> retryPendingForget({
    required String personId,
  }) {
    final pending = _pendingForget;
    if (pending == null || pending.personId != personId) {
      throw const AppOwnerException('conflict');
    }
    return _submitForget(pending);
  }

  Future<AgentMemoryForgetAcknowledgement> _submitForget(
    _PendingMemoryForget pending,
  ) async {
    final previouslySubmitted = pending.submitted;
    pending.submitted = true;
    try {
      final result = await _transport.commandV2({
        'schema_version': appWireProtocolVersion,
        'request_id': newAgentRequestId(),
        'command_id': pending.commandId,
        'command': {
          'kind': 'memory.forget',
          'target_id': pending.targetId,
          'expected_revision': pending.revision,
        },
      });
      final acknowledgement = AgentMemoryForgetAcknowledgement.fromJson(
        _payload(result, 'memory.forgotten', 'acknowledgement'),
      );
      if (acknowledgement.commandId != pending.commandId ||
          acknowledgement.targetId != pending.targetId ||
          acknowledgement.forgottenRevision != pending.revision) {
        throw const FormatException('Memory forget acknowledgement mismatch.');
      }
      if (identical(_pendingForget, pending)) _pendingForget = null;
      return acknowledgement;
    } on AppWireTransportException catch (error) {
      if (mayDiscardPendingCommand(
        error,
        previouslySubmitted: previouslySubmitted,
      )) {
        if (identical(_pendingForget, pending)) _pendingForget = null;
      }
      throw _fromTransport(error, pending.commandId, 'knowledge_forget');
    }
  }

  Future<Map<String, dynamic>> _query(Map<String, Object?> query) async {
    final requestId = newAgentRequestId();
    try {
      return await _transport.queryV2({
        'schema_version': appWireProtocolVersion,
        'request_id': requestId,
        'query': query,
      });
    } on AppWireTransportException catch (error) {
      throw _fromTransport(error, requestId, 'knowledge_query');
    }
  }

  Future<AgentMemoryDecisionAcknowledgement> _submit(
    _PendingMemoryDecision pending,
  ) async {
    final previouslySubmitted = pending.submitted;
    pending.submitted = true;
    final requestId = newAgentRequestId();
    try {
      final result = await _transport.commandV2({
        'schema_version': appWireProtocolVersion,
        'request_id': requestId,
        'command_id': pending.commandId,
        'command': pending.payload,
      });
      final acknowledgement = AgentMemoryDecisionAcknowledgement.fromJson(
        _payload(result, 'memory.decision', 'acknowledgement'),
      );
      if (acknowledgement.commandId != pending.commandId ||
          acknowledgement.candidateId != pending.candidateId ||
          acknowledgement.decision != pending.decision) {
        throw const FormatException(
          'Memory decision acknowledgement mismatch.',
        );
      }
      if (identical(_pending, pending)) _pending = null;
      return acknowledgement;
    } on AppWireTransportException catch (error) {
      if (mayDiscardPendingCommand(
        error,
        previouslySubmitted: previouslySubmitted,
      )) {
        if (identical(_pending, pending)) _pending = null;
      }
      throw _fromTransport(error, pending.commandId, 'knowledge_decide');
    }
  }

  AppOwnerException _fromTransport(
    AppWireTransportException error,
    String requestId,
    String stage,
  ) => AppOwnerException.fromAppWire(
    error.ownerFailure?.reason ?? error.metadata['reason_code'] ?? error.code,
    requestId: requestId,
    stage: stage,
    metadata: error.metadata,
    ownerFailure: error.ownerFailure,
    commandOutcome: error.commandOutcome,
  );

  static Map<String, dynamic> _payload(
    Map<String, dynamic> result,
    String kind,
    String field,
  ) {
    if (result.length != 2 ||
        result['kind'] != kind ||
        !result.containsKey(field)) {
      throw const FormatException('Unexpected Knowledge owner result.');
    }
    final value = result[field];
    if (value is! Map || value.keys.any((key) => key is! String)) {
      throw const FormatException('Malformed Knowledge owner result.');
    }
    return Map<String, dynamic>.from(value);
  }
}

final class _PendingMemoryDecision {
  _PendingMemoryDecision({
    required this.commandId,
    required this.personId,
    required this.candidateId,
    required this.decision,
    required this.payload,
  });

  final String commandId;
  final String personId;
  final String candidateId;
  final AgentMemoryDecision decision;
  final Map<String, Object?> payload;
  bool submitted = false;
}

final class _PendingMemoryForget {
  _PendingMemoryForget({
    required this.commandId,
    required this.personId,
    required this.targetId,
    required this.revision,
  });
  final String commandId;
  final String personId;
  final String targetId;
  final int revision;
  bool submitted = false;
}
