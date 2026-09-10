import 'package:flutter/foundation.dart';

import '../agent_memory.dart';
import '../agent_memory_review.dart';
import '../agent_vault_gateway.dart';

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
  final void Function(String failure) onFatalFailure;

  List<AgentMemoryCandidate>? candidates;
  String? reviewFailure;
  AgentMemoryOverview? overview;
  String? failure;
  bool busy = false;

  bool get hasMemory => memoryGateway != null;
  bool get hasReview => reviewGateway != null;
  bool get canRead => hasMemory && !busy && canOperate();
  bool get canReview => hasReview && !busy && canOperate();

  Future<void> load() async {
    if (!canRead) return;
    busy = true;
    failure = null;
    notifyListeners();
    try {
      final result = await memoryGateway!.readMemory(personId);
      if (!canOperate()) return;
      if (result.personId != personId) {
        throw const FormatException('Memory overview Person mismatch');
      }
      overview = result;
    } on Object catch (error) {
      if (!canOperate()) return;
      overview = null;
      failure = _failure(error);
      _reportFatal(failure!);
    } finally {
      busy = false;
      notifyListeners();
    }
  }

  Future<void> loadReview() => _review();

  Future<void> decide(String candidateId, AgentMemoryDecision decision) async {
    await _review(candidateId: candidateId, decision: decision);
    if (reviewFailure == null) await load();
  }

  void clear() {
    candidates = null;
    reviewFailure = null;
    overview = null;
    failure = null;
    notifyListeners();
  }

  Future<void> _review({
    String? candidateId,
    AgentMemoryDecision? decision,
  }) async {
    if (!canReview || (candidateId == null) != (decision == null)) return;
    busy = true;
    reviewFailure = null;
    notifyListeners();
    try {
      final result = candidateId == null
          ? await reviewGateway!.readMemoryReview(personId)
          : await reviewGateway!.decideMemoryCandidate(
              personId: personId,
              candidateId: candidateId,
              decision: decision!,
            );
      if (!canOperate()) return;
      if (result.personId != personId) {
        throw const FormatException('Memory review Person mismatch');
      }
      candidates = result.candidates;
    } on Object catch (error) {
      if (!canOperate()) return;
      reviewFailure = _failure(error);
      _reportFatal(reviewFailure!);
    } finally {
      busy = false;
      notifyListeners();
    }
  }

  String _failure(Object error) =>
      error is AgentVaultException ? error.failure : 'storage_unavailable';

  void _reportFatal(String reason) {
    if (reason == 'vault_unavailable' || reason == 'interrupted') {
      onFatalFailure(reason);
    }
  }
}
