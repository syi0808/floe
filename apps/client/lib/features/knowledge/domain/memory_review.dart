import 'dart:convert';

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
    required this.allowedActions,
    this.validFrom,
    this.validUntil,
  });

  final String id;
  final String operation;
  final String statement;
  final String memoryKind;
  final String epistemicStatus;
  final int confidenceMillis;
  final int sourceCount;
  final DateTime createdAt;
  final DateTime? validFrom;
  final DateTime? validUntil;
  final List<AgentMemoryDecision> allowedActions;

  factory AgentMemoryCandidate.fromJson(Map<String, dynamic> json) {
    _exactKeys(json, const {
      'candidate_id',
      'operation',
      'statement',
      'memory_kind',
      'epistemic_status',
      'confidence_millis',
      'source_count',
      'created_at',
      'allowed_actions',
    }, optional: const {'valid_from', 'valid_until'});
    final candidate = AgentMemoryCandidate(
      id: _uuid(json['candidate_id'], 'candidate.candidate_id'),
      operation: _choice(
        json['operation'],
        const {'create', 'revise', 'retire'},
        'candidate.operation',
      ),
      statement: _statement(json['statement'], 'candidate.statement'),
      memoryKind: _choice(
        json['memory_kind'],
        const {'fact', 'observation', 'inference', 'preference', 'commitment'},
        'candidate.memory_kind',
      ),
      epistemicStatus: _choice(
        json['epistemic_status'],
        const {'fact', 'inference'},
        'candidate.epistemic_status',
      ),
      confidenceMillis: _integer(
        json['confidence_millis'],
        1000,
        'candidate.confidence_millis',
      ),
      sourceCount: _integer(
        json['source_count'],
        1000000,
        'candidate.source_count',
      ),
      createdAt: _timestamp(json['created_at'], 'candidate.created_at'),
      validFrom: _optionalTimestamp(json, 'valid_from'),
      validUntil: _optionalTimestamp(json, 'valid_until'),
      allowedActions: List.unmodifiable(
        _decisions(json['allowed_actions'], 'candidate.allowed_actions'),
      ),
    );
    if (candidate.validFrom != null &&
        candidate.validUntil != null &&
        !candidate.validFrom!.isBefore(candidate.validUntil!)) {
      throw const FormatException('Invalid memory candidate.');
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

  factory AgentMemoryReviewOverview.fromJson(Map<String, dynamic> json) {
    _exactKeys(json, const {'person_id', 'candidates'});
    final candidates = _objects(json['candidates'], 100, 'review.candidates')
        .map(AgentMemoryCandidate.fromJson)
        .toList(growable: false);
    if (candidates.map((candidate) => candidate.id).toSet().length !=
        candidates.length) {
      throw const FormatException('Duplicate memory candidate.');
    }
    return AgentMemoryReviewOverview(
      personId: _uuid(json['person_id'], 'review.person_id'),
      candidates: List.unmodifiable(candidates),
    );
  }
}

final class AgentMemoryDecisionAcknowledgement {
  const AgentMemoryDecisionAcknowledgement({
    required this.commandId,
    required this.candidateId,
    required this.decision,
    required this.committedAt,
    this.resultingTargetId,
    this.resultingRevision,
  });

  final String commandId;
  final String candidateId;
  final AgentMemoryDecision decision;
  final DateTime committedAt;
  final String? resultingTargetId;
  final int? resultingRevision;

  factory AgentMemoryDecisionAcknowledgement.fromJson(
    Map<String, dynamic> json,
  ) {
    _exactKeys(json, const {
      'command_id',
      'candidate_id',
      'decision',
      'committed_at',
      'resulting_target_id',
      'resulting_revision',
    });
    final targetValue = json['resulting_target_id'];
    final revisionValue = json['resulting_revision'];
    final acknowledgement = AgentMemoryDecisionAcknowledgement(
      commandId: _uuid(json['command_id'], 'acknowledgement.command_id'),
      candidateId: _uuid(json['candidate_id'], 'acknowledgement.candidate_id'),
      decision: _decision(json['decision'], 'acknowledgement.decision'),
      committedAt: _timestamp(
        json['committed_at'],
        'acknowledgement.committed_at',
      ),
      resultingTargetId: targetValue == null
          ? null
          : _uuid(targetValue, 'acknowledgement.resulting_target_id'),
      resultingRevision: revisionValue == null
          ? null
          : _positiveInteger(
              revisionValue,
              'acknowledgement.resulting_revision',
            ),
    );
    if (acknowledgement.committedAt.millisecondsSinceEpoch < 0 ||
        (targetValue == null) != (revisionValue == null)) {
      throw const FormatException('Invalid memory acknowledgement.');
    }
    return acknowledgement;
  }
}

String _uuid(Object? value, String field) {
  if (value is! String ||
      value == '00000000-0000-0000-0000-000000000000' ||
      !RegExp(
        r'^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$',
      ).hasMatch(value)) {
    throw FormatException('Invalid $field.');
  }
  return value;
}

String _statement(Object? value, String field) {
  if (value is! String ||
      value.trim().isEmpty ||
      utf8.encode(value).length > 2048 ||
      value.runes.any((rune) => rune < 32 && rune != 10 && rune != 9)) {
    throw FormatException('Invalid $field.');
  }
  return value;
}

String _choice(Object? value, Set<String> choices, String field) {
  if (value is! String || !choices.contains(value)) {
    throw FormatException('Invalid $field.');
  }
  return value;
}

int _integer(Object? value, int maximum, String field) {
  if (value is! int || value < 0 || value > maximum) {
    throw FormatException('Invalid $field.');
  }
  return value;
}

int _positiveInteger(Object? value, String field) {
  if (value is! int || value <= 0 || value > 0x7fffffffffffffff) {
    throw FormatException('Invalid $field.');
  }
  return value;
}

AgentMemoryDecision _decision(Object? value, String field) => switch (value) {
  'approve' => AgentMemoryDecision.approve,
  'reject' => AgentMemoryDecision.reject,
  _ => throw FormatException('Invalid $field.'),
};

List<AgentMemoryDecision> _decisions(Object? value, String field) {
  if (value is! List || value.length > 2) {
    throw FormatException('Invalid $field.');
  }
  final decisions = value.map((entry) => _decision(entry, field)).toList();
  if (decisions.toSet().length != decisions.length) {
    throw FormatException('Invalid $field.');
  }
  return decisions;
}

DateTime? _optionalTimestamp(Map<String, dynamic> json, String field) {
  if (!json.containsKey(field) || json[field] == null) return null;
  return _timestamp(json[field], 'candidate.$field');
}

DateTime _timestamp(Object? value, String field) {
  if (value is! String) {
    throw FormatException('Invalid $field.');
  }
  final match = RegExp(
    r'^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})(?:\.(\d{1,9}))?(Z|[+-]\d{2}:\d{2})$',
  ).firstMatch(value);
  if (match == null) throw FormatException('Invalid $field.');
  final year = int.parse(match.group(1)!);
  final month = int.parse(match.group(2)!);
  final day = int.parse(match.group(3)!);
  final hour = int.parse(match.group(4)!);
  final minute = int.parse(match.group(5)!);
  final second = int.parse(match.group(6)!);
  final local = DateTime.utc(year, month, day, hour, minute, second);
  if (month < 1 ||
      month > 12 ||
      day < 1 ||
      local.year != year ||
      local.month != month ||
      local.day != day ||
      hour > 23 ||
      minute > 59 ||
      second > 59) {
    throw FormatException('Invalid $field.');
  }
  final zone = match.group(8)!;
  if (zone != 'Z' &&
      (int.parse(zone.substring(1, 3)) > 23 ||
          int.parse(zone.substring(4, 6)) > 59)) {
    throw FormatException('Invalid $field.');
  }
  final parsed = DateTime.tryParse(value);
  if (parsed == null) throw FormatException('Invalid $field.');
  return parsed.toUtc();
}

List<Map<String, dynamic>> _objects(
  Object? value,
  int maximum,
  String field,
) {
  if (value is! List || value.length > maximum) {
    throw FormatException('Invalid $field.');
  }
  return value.map((entry) {
    if (entry is! Map || entry.keys.any((key) => key is! String)) {
      throw FormatException('Invalid $field.');
    }
    return Map<String, dynamic>.from(entry);
  }).toList(growable: false);
}

void _exactKeys(
  Map<String, dynamic> value,
  Set<String> required, {
  Set<String> optional = const {},
}) {
  final keys = value.keys.toSet();
  if (!keys.containsAll(required) ||
      keys.any((key) => !required.contains(key) && !optional.contains(key))) {
    throw const FormatException('Unexpected Knowledge wire fields.');
  }
}
