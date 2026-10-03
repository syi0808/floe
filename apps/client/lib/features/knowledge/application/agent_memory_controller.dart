import 'package:flutter/foundation.dart';

import 'package:floe_client/app/runtime/agent_vault_gateway.dart';
import 'package:floe_client/features/knowledge/application/memory_gateway.dart';
import 'package:floe_client/features/knowledge/domain/agent_memory.dart';
import 'package:floe_client/features/knowledge/domain/memory_review.dart';

final class AgentMemoryController extends ChangeNotifier {
  AgentMemoryController({
    required this.memoryGateway,
    required this.reviewGateway,
    required this.personId,
    required this.canOperate,
    required this.onFatalFailure,
  });

  final AgentMemoryGateway? memoryGateway;
  final AgentMemoryReviewGateway? reviewGateway;
  final String personId;
  final bool Function() canOperate;
  final void Function(AgentVaultException failure) onFatalFailure;

  int _operationGeneration = 0;
  bool _disposed = false;

  List<AgentMemoryCandidate>? candidates;
  String? reviewFailure;
  AgentMemoryOverview? overview;
  String? failure;
  AgentMemoryDecisionAcknowledgement? acknowledgement;
  bool busy = false;

  bool get hasMemory => memoryGateway != null;
  bool get hasReview => reviewGateway != null;
  bool get canRead => hasMemory && !busy && canOperate();
  bool get canReadReview => hasReview && !busy && canOperate();
  bool get canReview =>
      hasReview &&
      !busy &&
      canOperate() &&
      reviewGateway!.pendingCommandId == null;
  bool get canRetryDecision =>
      hasReview &&
      !busy &&
      canOperate() &&
      reviewGateway!.pendingCommandId != null;

  Future<void> load() async {
    final generation = _operationGeneration;
    if (!_isCurrent(generation) || !canRead) return;
    busy = true;
    failure = null;
    _notifyIfCurrent(generation);
    try {
      final loadedOverview = await _readOverview();
      if (!_isCurrent(generation)) return;
      overview = loadedOverview;
    } on Object catch (error) {
      if (!_isCurrent(generation)) return;
      overview = null;
      failure = _failure(error);
      _reportFatal(error, generation);
    } finally {
      if (_isCurrent(generation, requireCanOperate: false)) {
        busy = false;
        notifyListeners();
      }
    }
  }

  Future<void> loadReview() async {
    final generation = _operationGeneration;
    if (!_isCurrent(generation) || !canReadReview) return;
    busy = true;
    reviewFailure = null;
    _notifyIfCurrent(generation);
    try {
      final loadedReview = await _readReview();
      if (!_isCurrent(generation)) return;
      candidates = loadedReview.candidates;
    } on Object catch (error) {
      if (!_isCurrent(generation)) return;
      candidates = null;
      reviewFailure = _failure(error);
      _reportFatal(error, generation);
    } finally {
      if (_isCurrent(generation, requireCanOperate: false)) {
        busy = false;
        notifyListeners();
      }
    }
  }

  Future<void> decide(String candidateId, AgentMemoryDecision decision) async {
    final generation = _operationGeneration;
    if (!_isCurrent(generation) || !canReview) return;
    final candidate = candidates
        ?.where((entry) => entry.id == candidateId)
        .singleOrNull;
    if (candidate == null || !candidate.allowedActions.contains(decision)) {
      if (!_isCurrent(generation)) return;
      reviewFailure = 'conflict';
      _notifyIfCurrent(generation);
      return;
    }
    await _resolveDecision(generation, () => reviewGateway!.decideMemoryCandidate(
      personId: personId,
      candidateId: candidateId,
      decision: decision,
    ), expectedCandidateId: candidateId, expectedDecision: decision);
  }

  Future<void> retryPendingDecision() async {
    final generation = _operationGeneration;
    if (!_isCurrent(generation) || !canRetryDecision) return;
    final pendingCommandId = reviewGateway!.pendingCommandId;
    final pendingCandidateId = reviewGateway!.pendingCandidateId;
    final pendingDecision = reviewGateway!.pendingDecision;
    if (pendingCommandId == null ||
        pendingCandidateId == null ||
        pendingDecision == null) {
      if (!_isCurrent(generation)) return;
      reviewFailure = 'conflict';
      _notifyIfCurrent(generation);
      return;
    }
    await _resolveDecision(
      generation,
      () => reviewGateway!.retryPendingDecision(personId: personId),
      expectedCommandId: pendingCommandId,
      expectedCandidateId: pendingCandidateId,
      expectedDecision: pendingDecision,
    );
  }

  void clear() {
    _operationGeneration++;
    if (_disposed) return;
    candidates = null;
    reviewFailure = null;
    overview = null;
    failure = null;
    acknowledgement = null;
    busy = false;
    notifyListeners();
  }

  @override
  void dispose() {
    _operationGeneration++;
    _disposed = true;
    super.dispose();
  }

  Future<AgentMemoryOverview> _readOverview() async {
    final result = await memoryGateway!.readMemory();
    if (result.personId != personId) {
      throw const FormatException('Memory overview Person mismatch.');
    }
    return result;
  }

  Future<AgentMemoryReviewOverview> _readReview() async {
    final result = await reviewGateway!.readMemoryReview();
    if (result.personId != personId) {
      throw const FormatException('Memory review Person mismatch.');
    }
    return result;
  }

  Future<void> _resolveDecision(
    int generation,
    Future<AgentMemoryDecisionAcknowledgement> Function() submit, {
    String? expectedCommandId,
    required String expectedCandidateId,
    required AgentMemoryDecision expectedDecision,
  }) async {
    if (!_isCurrent(generation) || busy) return;
    busy = true;
    reviewFailure = null;
    acknowledgement = null;
    _notifyIfCurrent(generation);
    try {
      final confirmed = await submit();
      if (!_isCurrent(generation)) return;
      if (confirmed.candidateId != expectedCandidateId ||
          confirmed.decision != expectedDecision ||
          expectedCommandId != null &&
              confirmed.commandId != expectedCommandId) {
        throw const FormatException('Memory decision acknowledgement mismatch.');
      }
      acknowledgement = confirmed;
      await _reloadAfterDecision(generation);
    } on Object catch (error) {
      if (!_isCurrent(generation)) return;
      reviewFailure = _failure(error);
      _reportFatal(error, generation);
    } finally {
      if (_isCurrent(generation, requireCanOperate: false)) {
        busy = false;
        notifyListeners();
      }
    }
  }

  Future<void> _reloadAfterDecision(int generation) async {
    try {
      final loadedReview = await _readReview();
      if (!_isCurrent(generation)) return;
      candidates = loadedReview.candidates;
    } on Object catch (error) {
      if (!_isCurrent(generation)) return;
      candidates = null;
      reviewFailure = _failure(error);
      _reportFatal(error, generation);
      if (!_isCurrent(generation)) return;
    }
    try {
      final loadedOverview = await _readOverview();
      if (!_isCurrent(generation)) return;
      overview = loadedOverview;
    } on Object catch (error) {
      if (!_isCurrent(generation)) return;
      overview = null;
      failure = _failure(error);
      _reportFatal(error, generation);
    }
  }

  String _failure(Object error) =>
      error is AgentVaultException ? error.failure : 'storage_unavailable';

  bool _isCurrent(int generation, {bool requireCanOperate = true}) =>
      !_disposed &&
      generation == _operationGeneration &&
      (!requireCanOperate || canOperate());

  void _notifyIfCurrent(int generation) {
    if (_isCurrent(generation)) notifyListeners();
  }

  void _reportFatal(Object error, int generation) {
    if (error is AgentVaultException &&
        (error.reloadRequired == true || error.sealSession == true) &&
        _isCurrent(generation)) {
      onFatalFailure(error);
    }
  }
}
