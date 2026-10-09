import 'package:floe_client/app/runtime/app_owner_exception.dart';
import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/features/conversation/application/agent_request_id.dart';
import 'package:floe_client/features/conversation/assistant_features/domain/assistant_feature.dart';

final class AppWireAssistantFeatureGateway implements AssistantFeatureGateway {
  AppWireAssistantFeatureGateway(this._transport);

  final AppWireTransport _transport;
  _PendingAssistantFeatureCommand? _pendingCommand;

  @override
  AssistantFeatureCommandKind? get pendingCommandKind => _pendingCommand?.kind;

  @override
  Future<AssistantFeatureSnapshot> readSnapshot() async => _readSnapshotResult(
    await _query(const {'kind': 'conversation.assistant_features.snapshot'}),
  );

  @override
  Future<AssistantFeatureSourceReview> prepareSourceReview({
    required String featureRef,
    required String sourceScopeRef,
    required String sourceRequirementRef,
    required int expectedBindingRevision,
  }) async {
    final intent = <String, Object?>{
      'kind': 'conversation.assistant_features.source.prepare_review',
      'feature_ref': featureRef,
      'source_scope_ref': sourceScopeRef,
      'source_requirement_ref': sourceRequirementRef,
      'expected_binding_revision': expectedBindingRevision,
    };
    return _runCommand(
      AssistantFeatureCommandKind.sourcePrepareReview,
      intent,
      (result) => _readSourceReviewResult(
        result,
        sourceScopeRef: sourceScopeRef,
        sourceRequirementRef: sourceRequirementRef,
        bindingRevision: expectedBindingRevision,
      ),
    );
  }

  @override
  Future<AssistantFeatureSourceReview> inspectSourceReview(
    AssistantFeatureSourceReviewRef reviewRef,
  ) async {
    final result = await _query({
      'kind': 'conversation.assistant_features.source.inspect_review',
      'review_ref': reviewRef.toJson(),
    });
    final review = AssistantFeatureSourceReview.fromJson(
      _payload(
        result,
        'conversation.assistant_features.source_review',
        'review',
      ),
    );
    if (!review.reviewRef.matches(reviewRef)) {
      throw const FormatException('Source review reference mismatch.');
    }
    return review;
  }

  @override
  Future<AssistantFeatureSnapshot> configure({
    required String featureRef,
    required int expectedRevision,
    required bool enabled,
    required List<AssistantFeatureSourceSelection> sourceSelections,
  }) async {
    final intent = <String, Object?>{
      'kind': 'conversation.assistant_features.configure',
      'feature_ref': featureRef,
      'expected_revision': expectedRevision,
      'enabled': enabled,
      'source_selections': List<Map<String, Object?>>.unmodifiable(
        sourceSelections.map((selection) => selection.toJson()),
      ),
    };
    final snapshot = await _runCommand(
      AssistantFeatureCommandKind.configure,
      intent,
      (result) => _readSnapshotResult(result),
    );
    final updated = snapshot.features
        .where((feature) => feature.featureRef == featureRef)
        .singleOrNull;
    if (updated == null || updated.enabled != enabled) {
      throw const FormatException('Assistant feature result scope mismatch.');
    }
    return snapshot;
  }

  @override
  Future<AssistantFeatureCommandResult> retryPendingCommand() async {
    final pending = _pendingCommand;
    if (pending == null) throw const AppOwnerException('conflict');
    return switch (pending.kind) {
      AssistantFeatureCommandKind.configure =>
        AssistantFeatureSnapshotCommandResult(
          pending.kind,
          await _runPending(pending, _readSnapshotResult),
        ),
      AssistantFeatureCommandKind.sourcePrepareReview =>
        AssistantFeatureSourceReviewCommandResult(
          pending.kind,
          await _runPending(
            pending,
            (result) => _readSourceReviewResult(
              result,
              sourceScopeRef: pending.intent['source_scope_ref']! as String,
              sourceRequirementRef:
                  pending.intent['source_requirement_ref']! as String,
              bindingRevision:
                  pending.intent['expected_binding_revision']! as int,
            ),
          ),
        ),
    };
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
      throw _fromTransport(error, requestId, 'assistant_features_query');
    }
  }

  Future<T> _runCommand<T>(
    AssistantFeatureCommandKind kind,
    Map<String, Object?> intent,
    T Function(Map<String, dynamic>) decode,
  ) async {
    if (_pendingCommand != null) {
      throw const AppOwnerException('conflict');
    }
    final pending = _PendingAssistantFeatureCommand(
      kind: kind,
      commandId: newAgentRequestId(),
      intent: Map<String, Object?>.unmodifiable(intent),
    );
    _pendingCommand = pending;
    return _runPending(pending, decode);
  }

  Future<T> _runPending<T>(
    _PendingAssistantFeatureCommand pending,
    T Function(Map<String, dynamic>) decode,
  ) async {
    final previouslySubmitted = pending.submitted;
    pending.submitted = true;
    final requestId = newAgentRequestId();
    try {
      final result = await _transport.commandV2({
        'schema_version': appWireProtocolVersion,
        'request_id': requestId,
        'command_id': pending.commandId,
        'command': pending.intent,
      });
      final decoded = decode(result);
      if (identical(_pendingCommand, pending)) _pendingCommand = null;
      return decoded;
    } on AppWireTransportException catch (error) {
      if (mayDiscardPendingCommand(
        error,
        previouslySubmitted: previouslySubmitted,
      )) {
        if (identical(_pendingCommand, pending)) _pendingCommand = null;
      }
      throw _fromTransport(
        error,
        pending.commandId,
        'assistant_features_command',
      );
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

  AssistantFeatureSnapshot _readSnapshotResult(Map<String, dynamic> result) =>
      AssistantFeatureSnapshot.fromJson(
        _payload(
          result,
          'conversation.assistant_features.snapshot',
          'snapshot',
        ),
      );

  AssistantFeatureSourceReview _readSourceReviewResult(
    Map<String, dynamic> result, {
    required String sourceScopeRef,
    required String sourceRequirementRef,
    required int bindingRevision,
  }) {
    final review = AssistantFeatureSourceReview.fromJson(
      _payload(
        result,
        'conversation.assistant_features.source_review',
        'review',
      ),
    );
    if (review.sourceScopeRef != sourceScopeRef ||
        review.sourceRequirementRef != sourceRequirementRef ||
        review.bindingRevision != bindingRevision) {
      throw const FormatException(
        'Assistant feature source review scope mismatch.',
      );
    }
    return review;
  }

  static Map<String, dynamic> _payload(
    Map<String, dynamic> result,
    String kind,
    String field,
  ) {
    if (result.length != 2 ||
        result['kind'] != kind ||
        !result.containsKey(field)) {
      throw const FormatException('Unexpected assistant feature owner result.');
    }
    final value = result[field];
    if (value is! Map || value.keys.any((key) => key is! String)) {
      throw const FormatException('Malformed assistant feature owner result.');
    }
    return Map<String, dynamic>.from(value);
  }
}

final class _PendingAssistantFeatureCommand {
  _PendingAssistantFeatureCommand({
    required this.kind,
    required this.commandId,
    required this.intent,
  });

  final AssistantFeatureCommandKind kind;
  final String commandId;
  final Map<String, Object?> intent;
  bool submitted = false;
}
