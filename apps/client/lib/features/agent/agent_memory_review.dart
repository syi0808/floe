enum AgentMemoryDecision { approve, reject }

final class AgentMemoryCandidate {
  const AgentMemoryCandidate({
    required this.id,
    required this.operation,
    required this.statement,
    required this.memoryKind,
    required this.epistemicStatus,
    required this.confidenceMillis,
    required this.sourceCount,
    required this.createdAt,
  });

  final String id;
  final String operation;
  final String statement;
  final String memoryKind;
  final String epistemicStatus;
  final int confidenceMillis;
  final int sourceCount;
  final DateTime createdAt;

  factory AgentMemoryCandidate.fromJson(Map<String, Object?> json) {
    final payload = Map<String, Object?>.from(json['payload'] as Map);
    if (json['state'] != 'pending' || payload['kind'] != 'memory') {
      throw const FormatException('Unsupported memory candidate');
    }
    final value = Map<String, Object?>.from(payload['value'] as Map);
    final sources = (json['source_refs'] as List<Object?>?) ?? const [];
    final candidate = AgentMemoryCandidate(
      id: json['id'] as String,
      operation: json['operation'] as String,
      statement: value['statement'] as String,
      memoryKind: value['kind'] as String,
      epistemicStatus: value['epistemic_status'] as String,
      confidenceMillis: value['confidence_millis'] as int,
      sourceCount: sources.length,
      createdAt: DateTime.parse(json['created_at'] as String),
    );
    if (candidate.id.isEmpty ||
        candidate.statement.trim().isEmpty ||
        candidate.confidenceMillis < 0 ||
        candidate.confidenceMillis > 1000 ||
        !const {'create', 'revise', 'retire'}.contains(candidate.operation)) {
      throw const FormatException('Invalid memory candidate');
    }
    return candidate;
  }
}

final class AgentMemoryReviewOverview {
  const AgentMemoryReviewOverview({
    required this.personId,
    required this.candidates,
  });

  final String personId;
  final List<AgentMemoryCandidate> candidates;

  factory AgentMemoryReviewOverview.fromJson(Map<String, Object?> json) {
    if (json['schema_version'] != 1) {
      throw const FormatException('Unsupported memory review version');
    }
    return AgentMemoryReviewOverview(
      personId: json['person_id'] as String,
      candidates: List.unmodifiable(
        (json['candidates'] as List<Object?>).map(
          (candidate) => AgentMemoryCandidate.fromJson(
            Map<String, Object?>.from(candidate! as Map),
          ),
        ),
      ),
    );
  }
}

abstract interface class AgentMemoryReviewGateway {
  Future<AgentMemoryReviewOverview> readMemoryReview(String personId);

  Future<AgentMemoryReviewOverview> decideMemoryCandidate({
    required String personId,
    required String candidateId,
    required AgentMemoryDecision decision,
  });
}
