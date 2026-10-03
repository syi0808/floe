import 'package:floe_client/features/knowledge/domain/memory_review.dart';

abstract interface class AgentMemoryReviewGateway {
  Future<AgentMemoryReviewOverview> readMemoryReview(String personId);

  Future<AgentMemoryReviewOverview> decideMemoryCandidate({
    required String personId,
    required String candidateId,
    required AgentMemoryDecision decision,
  });
}
