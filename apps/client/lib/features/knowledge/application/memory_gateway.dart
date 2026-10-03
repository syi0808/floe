import 'package:floe_client/features/knowledge/domain/memory_review.dart';

abstract interface class AgentMemoryReviewGateway {
  String? get pendingCommandId;

  AgentMemoryDecision? get pendingDecision;

  String? get pendingCandidateId;

  Future<AgentMemoryReviewOverview> readMemoryReview();

  Future<AgentMemoryDecisionAcknowledgement> decideMemoryCandidate({
    required String personId,
    required String candidateId,
    required AgentMemoryDecision decision,
  });

  Future<AgentMemoryDecisionAcknowledgement> retryPendingDecision({
    required String personId,
  });
}
