import 'package:floe_client/app/runtime/agent_vault_gateway.dart';
import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/app/runtime/native_transport.dart';
import 'package:floe_client/features/conversation/application/agent_request_id.dart';
import 'package:floe_client/features/knowledge/application/memory_gateway.dart';
import 'package:floe_client/features/knowledge/domain/agent_memory.dart';
import 'package:floe_client/features/knowledge/domain/memory_review.dart';

final class AppWireMemoryGateway
    implements AgentMemoryGateway, AgentMemoryReviewGateway {
  AppWireMemoryGateway(this._transport);

  final AppWireTransport _transport;
  _PendingMemoryDecision? _pending;

  @override
  String? get pendingCommandId => _pending?.commandId;

  @override
  AgentMemoryDecision? get pendingDecision => _pending?.decision;

  @override
  String? get pendingCandidateId => _pending?.candidateId;

  @override
  Future<AgentMemoryOverview> readMemory() async {
    final result = await _query(const {'kind': 'knowledge.memory.overview'});
    final overview = AgentMemoryOverview.fromJson(
      _payload(result, 'knowledge.memory.overview', 'overview'),
    );
    return overview;
  }

  @override
  Future<AgentMemoryReviewOverview> readMemoryReview() async {
    final result = await _query(const {'kind': 'knowledge.memory.review'});
    return AgentMemoryReviewOverview.fromJson(
      _payload(result, 'knowledge.memory.review', 'review'),
    );
  }

  @override
  Future<AgentMemoryDecisionAcknowledgement> decideMemoryCandidate({
    required String personId,
    required String candidateId,
    required AgentMemoryDecision decision,
  }) async {
    if (_pending != null) throw const AgentVaultException('conflict');
    final pending = _PendingMemoryDecision(
      commandId: newAgentRequestId(),
      personId: personId,
      candidateId: candidateId,
      decision: decision,
      payload: Map<String, Object?>.unmodifiable({
        'kind': 'knowledge.memory.decide',
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
      throw const AgentVaultException('conflict');
    }
    return _submit(pending);
  }

  Future<Map<String, dynamic>> _query(Map<String, Object?> query) async {
    final requestId = newAgentRequestId();
    try {
      return await _transport.queryV2({
        'schema_version': appWireProtocolVersion,
        'request_id': requestId,
        'query': query,
      });
    } on NativeTransportException catch (error) {
      throw _fromTransport(error, requestId, 'knowledge_query');
    }
  }

  Future<AgentMemoryDecisionAcknowledgement> _submit(
    _PendingMemoryDecision pending,
  ) async {
    final requestId = newAgentRequestId();
    try {
      final result = await _transport.commandV2({
        'schema_version': appWireProtocolVersion,
        'request_id': requestId,
        'command_id': pending.commandId,
        'command': pending.payload,
      });
      final acknowledgement = AgentMemoryDecisionAcknowledgement.fromJson(
        _payload(result, 'knowledge.memory.decision', 'acknowledgement'),
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
    } on NativeTransportException catch (error) {
      // Only these correlated owner rejections are proven precommit. A
      // transport/decode/storage failure retains the immutable retry request.
      if ((error.code == 'conflict' &&
              error.metadata['reason_code'] == 'conflict') ||
          (error.code == 'not_found' &&
              error.metadata['reason_code'] == 'not_found') ||
          (error.code == 'validation' &&
              error.metadata['reason_code'] == 'invalid_input') ||
          (error.code == 'unavailable' &&
              error.metadata['reason_code'] == 'stale_context')) {
        if (identical(_pending, pending)) _pending = null;
      }
      throw _fromTransport(error, pending.commandId, 'knowledge_decide');
    }
  }

  AgentVaultException _fromTransport(
    NativeTransportException error,
    String requestId,
    String stage,
  ) => AgentVaultException.fromAppWire(
    error.ownerFailure?.reason ?? error.metadata['reason_code'] ?? error.code,
    requestId: requestId,
    stage: stage,
    metadata: error.metadata,
    ownerFailure: error.ownerFailure,
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
  const _PendingMemoryDecision({
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
}
