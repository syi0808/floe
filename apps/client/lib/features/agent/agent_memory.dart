enum AgentMemoryOrigin { userProvided, learned }

final class AgentMemory {
  const AgentMemory({
    required this.targetId,
    required this.revision,
    required this.statement,
    required this.memoryKind,
    required this.epistemicStatus,
    required this.confidenceMillis,
    required this.sourceCount,
    required this.origin,
    required this.createdAt,
    this.validFrom,
    this.validUntil,
  });

  final String targetId;
  final int revision;
  final String statement;
  final String memoryKind;
  final String epistemicStatus;
  final int confidenceMillis;
  final int sourceCount;
  final AgentMemoryOrigin origin;
  final DateTime createdAt;
  final DateTime? validFrom;
  final DateTime? validUntil;

  String get category => switch (memoryKind) {
    'preference' => 'Preference',
    'fact' => 'Fact',
    'commitment' => 'Commitment',
    _ => 'Other',
  };

  factory AgentMemory.fromJson(Map<String, Object?> json) {
    final origin = switch (json['origin']) {
      'user_provided' => AgentMemoryOrigin.userProvided,
      'learned' => AgentMemoryOrigin.learned,
      _ => throw const FormatException('Invalid memory origin'),
    };
    final memory = AgentMemory(
      targetId: json['target_id'] as String,
      revision: json['revision'] as int,
      statement: json['statement'] as String,
      memoryKind: json['memory_kind'] as String,
      epistemicStatus: json['epistemic_status'] as String,
      confidenceMillis: json['confidence_millis'] as int,
      sourceCount: json['source_count'] as int,
      origin: origin,
      createdAt: DateTime.parse(json['created_at'] as String),
      validFrom: switch (json['valid_from']) {
        final String value => DateTime.parse(value),
        _ => null,
      },
      validUntil: switch (json['valid_until']) {
        final String value => DateTime.parse(value),
        _ => null,
      },
    );
    if (memory.targetId.isEmpty ||
        memory.revision < 1 ||
        memory.statement.trim().isEmpty ||
        !const {
          'fact',
          'observation',
          'inference',
          'preference',
          'commitment',
        }.contains(memory.memoryKind) ||
        !const {'fact', 'inference'}.contains(memory.epistemicStatus) ||
        memory.confidenceMillis < 0 ||
        memory.confidenceMillis > 1000 ||
        memory.sourceCount < 1 ||
        memory.validFrom != null &&
            memory.validUntil != null &&
            !memory.validFrom!.isBefore(memory.validUntil!)) {
      throw const FormatException('Invalid saved memory');
    }
    return memory;
  }
}

final class AgentMemoryOverview {
  const AgentMemoryOverview({
    required this.personId,
    required this.savedCount,
    required this.pendingCount,
    required this.memories,
  });

  final String personId;
  final int savedCount;
  final int pendingCount;
  final List<AgentMemory> memories;

  factory AgentMemoryOverview.fromJson(Map<String, Object?> json) {
    if (json['schema_version'] != 1) {
      throw const FormatException('Unsupported memory overview version');
    }
    final overview = AgentMemoryOverview(
      personId: json['person_id'] as String,
      savedCount: json['saved_count'] as int,
      pendingCount: json['pending_count'] as int,
      memories: List.unmodifiable(
        (json['memories'] as List<Object?>).map(
          (memory) =>
              AgentMemory.fromJson(Map<String, Object?>.from(memory! as Map)),
        ),
      ),
    );
    if (overview.personId.isEmpty ||
        overview.savedCount < overview.memories.length ||
        overview.pendingCount < 0 ||
        overview.memories.map((memory) => memory.targetId).toSet().length !=
            overview.memories.length) {
      throw const FormatException('Invalid memory overview');
    }
    return overview;
  }
}

abstract interface class AgentMemoryGateway {
  Future<AgentMemoryOverview> readMemory(String personId);
}
