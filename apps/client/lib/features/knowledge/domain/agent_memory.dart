import 'dart:convert';

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

  factory AgentMemory.fromJson(Map<String, dynamic> json) {
    _exactKeys(
      json,
      const {
        'target_id',
        'revision',
        'statement',
        'memory_kind',
        'epistemic_status',
        'confidence_millis',
        'source_count',
        'origin',
        'created_at',
      },
      optional: const {'valid_from', 'valid_until'},
    );
    final memory = AgentMemory(
      targetId: _uuid(json['target_id'], 'memory.target_id'),
      revision: _positiveInteger(json['revision'], 'memory.revision'),
      statement: _statement(json['statement'], 'memory.statement'),
      memoryKind: _choice(json['memory_kind'], const {
        'fact',
        'observation',
        'inference',
        'preference',
        'commitment',
      }, 'memory.memory_kind'),
      epistemicStatus: _choice(json['epistemic_status'], const {
        'fact',
        'inference',
      }, 'memory.epistemic_status'),
      confidenceMillis: _boundedInteger(
        json['confidence_millis'],
        1000,
        'memory.confidence_millis',
      ),
      sourceCount: _boundedInteger(
        json['source_count'],
        1000000,
        'memory.source_count',
      ),
      origin: switch (json['origin']) {
        'user_provided' => AgentMemoryOrigin.userProvided,
        'learned' => AgentMemoryOrigin.learned,
        _ => throw const FormatException('Invalid memory origin.'),
      },
      createdAt: _timestamp(json['created_at'], 'memory.created_at'),
      validFrom: _optionalTimestamp(json, 'valid_from'),
      validUntil: _optionalTimestamp(json, 'valid_until'),
    );
    if (memory.validFrom != null &&
        memory.validUntil != null &&
        !memory.validFrom!.isBefore(memory.validUntil!)) {
      throw const FormatException('Invalid saved memory.');
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

  factory AgentMemoryOverview.fromJson(Map<String, dynamic> json) {
    _exactKeys(json, const {
      'schema_version',
      'person_id',
      'saved_count',
      'pending_count',
      'memories',
    });
    if (json['schema_version'] != 1) {
      throw const FormatException('Unsupported memory overview version.');
    }
    final rows = _objects(
      json['memories'],
      100,
      'overview.memories',
    ).map(AgentMemory.fromJson).toList(growable: false);
    final overview = AgentMemoryOverview(
      personId: _uuid(json['person_id'], 'overview.person_id'),
      savedCount: _boundedInteger(
        json['saved_count'],
        1000000000,
        'overview.saved_count',
      ),
      pendingCount: _boundedInteger(
        json['pending_count'],
        1000000000,
        'overview.pending_count',
      ),
      memories: List.unmodifiable(rows),
    );
    if (overview.savedCount < overview.memories.length ||
        overview.memories.map((memory) => memory.targetId).toSet().length !=
            overview.memories.length) {
      throw const FormatException('Invalid memory overview.');
    }
    return overview;
  }
}

abstract interface class AgentMemoryGateway {
  Future<AgentMemoryOverview> readMemory();
}

String _uuid(Object? value, String field) {
  if (value is! String ||
      value == '00000000-0000-0000-0000-000000000000' ||
      !RegExp(r'^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$')
          .hasMatch(value)) {
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

int _boundedInteger(Object? value, int maximum, String field) {
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

DateTime? _optionalTimestamp(Map<String, dynamic> json, String field) {
  if (!json.containsKey(field) || json[field] == null) return null;
  return _timestamp(json[field], 'memory.$field');
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

List<Map<String, dynamic>> _objects(Object? value, int maximum, String field) {
  if (value is! List || value.length > maximum) {
    throw FormatException('Invalid $field.');
  }
  return value
      .map((entry) {
        if (entry is! Map || entry.keys.any((key) => key is! String)) {
          throw FormatException('Invalid $field.');
        }
        return Map<String, dynamic>.from(entry);
      })
      .toList(growable: false);
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
