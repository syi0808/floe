import 'dart:convert';

final class AgentExpertInsight {
  const AgentExpertInsight(this.kind, this.title, this.start, this.end);

  final String kind;
  final String? title;
  final DateTime? start;
  final DateTime? end;

  static AgentExpertInsight parse(Object? value) {
    final json = value! as Map<String, dynamic>;
    final kind = json['kind']! as String;
    if (kind == 'no_focus_window') {
      _keys(json, {'kind'});
      return AgentExpertInsight(kind, null, null, null);
    }
    _keys(json, {
      'kind',
      'starts_at_unix_ms',
      'ends_at_unix_ms',
      if (kind == 'commitment') ...{'evidence_handle', 'untrusted_title'},
    });
    if (kind != 'commitment' && kind != 'focus_window') {
      throw const FormatException('Unknown Expert insight.');
    }
    final start = DateTime.fromMillisecondsSinceEpoch(
      json['starts_at_unix_ms']! as int,
      isUtc: true,
    );
    final end = DateTime.fromMillisecondsSinceEpoch(
      json['ends_at_unix_ms']! as int,
      isUtc: true,
    );
    if (start.millisecondsSinceEpoch < 0 ||
        !end.isAfter(start) ||
        end.difference(start) > const Duration(days: 1)) {
      throw const FormatException('Invalid Expert interval.');
    }
    String? title;
    if (kind == 'commitment') {
      _text(json['evidence_handle'], 128);
      title = _text(json['untrusted_title'], 256, allowEmpty: true);
    }
    return AgentExpertInsight(kind, title, start, end);
  }
}

final class AgentExpertResult {
  const AgentExpertResult(
    this.expert,
    this.version,
    this.source,
    this.insights,
  );

  final String expert;
  final String version;
  final String source;
  final List<AgentExpertInsight> insights;

  static AgentExpertResult? tryParse(
    String? output, {
    required String callId,
    required String personId,
  }) {
    if (output == null || output.length > 16384) return null;
    try {
      if (utf8.encode(output).length > 16384) return null;
      final json = jsonDecode(output) as Map<String, dynamic>;
      _keys(json, {
        'schema_version',
        'invocation_id',
        'instance_id',
        'person_id',
        'assignment_id',
        'package',
        'view_handle',
        'source_handle',
        'data_class',
        'expires_at_unix_ms',
        'insights',
        'action_proposals',
        'state_revision',
        'view_calls',
      });
      if (json['schema_version'] != 1 ||
          json['invocation_id'] != callId ||
          json['person_id'] != personId ||
          json['data_class'] != 'synthetic' ||
          json['view_calls'] != 1 ||
          (json['state_revision']! as int) < 1 ||
          (json['expires_at_unix_ms']! as num) < 0 ||
          (json['action_proposals']! as List).isNotEmpty) {
        return null;
      }
      for (final key in ['instance_id', 'assignment_id', 'view_handle']) {
        _text(json[key], 128);
      }
      final package = json['package']! as Map<String, dynamic>;
      _keys(package, {'kind', 'id', 'version'});
      if (package['kind'] != 'expert') return null;
      final insights = json['insights']! as List;
      if (insights.isEmpty || insights.length > 8) return null;
      return AgentExpertResult(
        _text(package['id'], 128),
        _text(package['version'], 128),
        _text(json['source_handle'], 128),
        List.unmodifiable(insights.map(AgentExpertInsight.parse)),
      );
    } on FormatException {
      return null;
    } on TypeError {
      return null;
    } on ArgumentError {
      return null;
    }
  }
}

void _keys(Map<String, dynamic> json, Set<String> expected) {
  if (json.length != expected.length || !expected.containsAll(json.keys)) {
    throw const FormatException('Invalid Expert fields.');
  }
}

String _text(Object? value, int maxBytes, {bool allowEmpty = false}) {
  final text = value! as String;
  if ((!allowEmpty && text.isEmpty) || utf8.encode(text).length > maxBytes) {
    throw const FormatException('Invalid Expert text.');
  }
  return text;
}
