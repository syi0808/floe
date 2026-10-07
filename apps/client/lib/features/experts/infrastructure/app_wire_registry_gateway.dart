import 'package:floe_client/app/runtime/app_owner_exception.dart';
import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/features/conversation/application/agent_request_id.dart';
import 'package:floe_client/features/experts/domain/agent_registry.dart';

final class AppWireRegistryGateway implements AgentRegistryGateway {
  AppWireRegistryGateway(this._transport);

  final AppWireTransport _transport;
  _PendingExpertCommand? _pendingCommand;

  @override
  AgentRegistryCommandKind? get pendingCommandKind => _pendingCommand?.kind;

  @override
  Future<AgentDirectorySnapshot> readDirectory() async =>
      _readDirectoryResult(await _query(const {'kind': 'experts.directory'}));

  @override
  Future<AgentDirectorySnapshot> setInstallationEnabled({
    required String installationRef,
    required int expectedRevision,
    required bool enabled,
  }) async {
    final intent = <String, Object?>{
      'kind': 'experts.installation.set_enabled',
      'installation_ref': installationRef,
      'expected_revision': expectedRevision,
      'enabled': enabled,
    };
    final directory = await _runCommand(
      AgentRegistryCommandKind.installationSetEnabled,
      intent,
      (result) {
        final decoded = _readDirectoryResult(result);
        if (!decoded.installations.any(
          (entry) =>
              entry.installationRef == installationRef &&
              entry.enabled == enabled,
        )) {
          throw const FormatException('Installation result scope mismatch.');
        }
        return decoded;
      },
    );
    return directory;
  }

  @override
  Future<AgentBindingInspection> inspectBinding({
    required String assignmentRef,
    required String requirementRef,
  }) async {
    final result = await _query({
      'kind': 'experts.binding.inspect',
      'assignment_ref': assignmentRef,
      'requirement_ref': requirementRef,
    });
    final binding = AgentBindingInspection.fromJson(
      _payload(result, 'experts.binding', 'binding'),
    );
    if (binding.assignmentRef != assignmentRef ||
        binding.requirementRef != requirementRef) {
      throw const FormatException('Expert binding scope mismatch.');
    }
    return binding;
  }

  @override
  Future<AgentBindingReview> prepareBindingReview({
    required String assignmentRef,
    required String requirementRef,
    required int expectedBindingRevision,
  }) async {
    final intent = <String, Object?>{
      'kind': 'experts.binding.prepare_review',
      'assignment_ref': assignmentRef,
      'requirement_ref': requirementRef,
      'expected_binding_revision': expectedBindingRevision,
    };
    return _runCommand(
      AgentRegistryCommandKind.bindingPrepareReview,
      intent,
      (result) => _readBindingReviewResult(
        result,
        assignmentRef: assignmentRef,
        requirementRef: requirementRef,
        bindingRevision: expectedBindingRevision,
      ),
    );
  }

  @override
  Future<AgentBindingReview> inspectBindingReview(
    AgentBindingReviewRef reviewRef,
  ) async {
    final result = await _query({
      'kind': 'experts.binding.inspect_review',
      'review_ref': reviewRef.toJson(),
    });
    final review = AgentBindingReview.fromJson(
      _payload(result, 'experts.binding_review', 'review'),
    );
    if (!review.reviewRef.matches(reviewRef)) {
      throw const FormatException('Expert review reference mismatch.');
    }
    return review;
  }

  @override
  Future<AgentDirectorySnapshot> replaceBinding({
    required AgentBindingReview review,
    required List<String> candidateRefs,
  }) async {
    _validateReplacement(review, candidateRefs);
    final intent = <String, Object?>{
      'kind': 'experts.binding.replace',
      'review_ref': Map<String, Object?>.unmodifiable(
        review.reviewRef.toJson(),
      ),
      'expected_binding_revision': review.bindingRevision,
      'candidate_refs': List<String>.unmodifiable(candidateRefs),
    };
    final directory = await _runCommand(
      AgentRegistryCommandKind.bindingReplace,
      intent,
      (result) {
        final decoded = _readDirectoryResult(result);
        if (!decoded.assignments.any(
          (entry) => entry.assignmentRef == review.assignmentRef,
        )) {
          throw const FormatException('Assignment missing from owner reply.');
        }
        return decoded;
      },
      expectedAssignmentRef: review.assignmentRef,
    );
    return directory;
  }

  @override
  Future<AgentRegistryCommandResult> retryPendingCommand() async {
    final pending = _pendingCommand;
    if (pending == null) throw const AppOwnerException('conflict');
    return switch (pending.kind) {
      AgentRegistryCommandKind.installationSetEnabled ||
      AgentRegistryCommandKind.bindingReplace => AgentDirectoryCommandResult(
        pending.kind,
        await _runPending(pending, (result) {
          final directory = _readDirectoryResult(result);
          if (pending.kind == AgentRegistryCommandKind.installationSetEnabled &&
              !directory.installations.any(
                (entry) =>
                    entry.installationRef ==
                        pending.intent['installation_ref'] &&
                    entry.enabled == pending.intent['enabled'],
              )) {
            throw const FormatException(
              'Installation missing from owner reply.',
            );
          }
          if (pending.kind == AgentRegistryCommandKind.bindingReplace &&
              !directory.assignments.any(
                (entry) => entry.assignmentRef == pending.expectedAssignmentRef,
              )) {
            throw const FormatException('Assignment missing from owner reply.');
          }
          return directory;
        }),
      ),
      AgentRegistryCommandKind.bindingPrepareReview =>
        AgentBindingReviewCommandResult(
          pending.kind,
          await _runPending(
            pending,
            (result) => _readBindingReviewResult(
              result,
              assignmentRef: pending.intent['assignment_ref']! as String,
              requirementRef: pending.intent['requirement_ref']! as String,
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
      throw _fromTransport(error, requestId, 'experts_query');
    }
  }

  Future<T> _runCommand<T>(
    AgentRegistryCommandKind kind,
    Map<String, Object?> intent,
    T Function(Map<String, dynamic>) decode, {
    String? expectedAssignmentRef,
  }) async {
    final current = _pendingCommand;
    if (current != null) {
      throw const AppOwnerException('conflict');
    }
    final pending = _PendingExpertCommand(
      kind: kind,
      commandId: newAgentRequestId(),
      intent: Map<String, Object?>.unmodifiable(intent),
      expectedAssignmentRef: expectedAssignmentRef,
    );
    _pendingCommand = pending;
    return _runPending(pending, decode);
  }

  Future<T> _runPending<T>(
    _PendingExpertCommand pending,
    T Function(Map<String, dynamic>) decode,
  ) async {
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
        if (identical(_pendingCommand, pending)) _pendingCommand = null;
      }
      // The transport error may follow a committed owner command whose
      // acknowledgement was lost. Keep the exact command ID and payload for
      // an explicit retry; never turn uncertainty into a fresh command.
      throw _fromTransport(error, pending.commandId, 'experts_command');
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
  );

  AgentDirectorySnapshot _readDirectoryResult(Map<String, dynamic> result) =>
      AgentDirectorySnapshot.fromJson(
        _payload(result, 'experts.directory', 'directory'),
      );

  AgentBindingReview _readBindingReviewResult(
    Map<String, dynamic> result, {
    required String assignmentRef,
    required String requirementRef,
    required int bindingRevision,
  }) {
    final review = AgentBindingReview.fromJson(
      _payload(result, 'experts.binding_review', 'review'),
    );
    if (review.assignmentRef != assignmentRef ||
        review.requirementRef != requirementRef ||
        review.bindingRevision != bindingRevision) {
      throw const FormatException('Expert binding review scope mismatch.');
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
      throw const FormatException('Unexpected Experts owner result.');
    }
    final value = result[field];
    if (value is! Map || value.keys.any((key) => key is! String)) {
      throw const FormatException('Malformed Experts owner result.');
    }
    return Map<String, dynamic>.from(value);
  }

  static void _validateReplacement(
    AgentBindingReview review,
    List<String> candidateRefs,
  ) {
    if (!review.canReplace ||
        candidateRefs.length > 16 ||
        candidateRefs.toSet().length != candidateRefs.length) {
      throw const AppOwnerException('conflict');
    }
    for (final reference in candidateRefs) {
      final candidate = review.candidates
          .where((entry) => entry.candidateRef == reference)
          .singleOrNull;
      if (candidate == null ||
          candidate.availability == AgentCandidateAvailability.unavailable &&
              !candidate.selected) {
        throw const FormatException(
          'Candidate is outside the prepared review.',
        );
      }
    }
  }
}

final class _PendingExpertCommand {
  const _PendingExpertCommand({
    required this.kind,
    required this.commandId,
    required this.intent,
    this.expectedAssignmentRef,
  });

  final AgentRegistryCommandKind kind;
  final String commandId;
  final Map<String, Object?> intent;
  final String? expectedAssignmentRef;
}
