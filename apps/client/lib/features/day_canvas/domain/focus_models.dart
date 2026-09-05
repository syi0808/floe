class FocusPreferenceValue {
  const FocusPreferenceValue({
    required this.startMinute,
    required this.endMinute,
    required this.durationMinutes,
  });

  final int startMinute;
  final int endMinute;
  final int durationMinutes;

  Map<String, dynamic> toJson() => {
    'start_minute': startMinute,
    'end_minute': endMinute,
    'duration_minutes': durationMinutes,
  };

  factory FocusPreferenceValue.fromJson(Map<String, dynamic> json) =>
      FocusPreferenceValue(
        startMinute: json['start_minute'] as int,
        endMinute: json['end_minute'] as int,
        durationMinutes: json['duration_minutes'] as int,
      );
}

class FocusPreference {
  const FocusPreference({
    required this.personId,
    required this.revision,
    required this.source,
    required this.updatedAt,
    required this.value,
  });

  final String personId;
  final int revision;
  final String source;
  final DateTime updatedAt;
  final FocusPreferenceValue? value;

  factory FocusPreference.fromJson(Map<String, dynamic> json) =>
      FocusPreference(
        personId: json['person_id'] as String,
        revision: json['revision'] as int,
        source: json['source'] as String,
        updatedAt: DateTime.parse(json['updated_at'] as String),
        value: json['value'] == null
            ? null
            : FocusPreferenceValue.fromJson(
                Map<String, dynamic>.from(json['value'] as Map),
              ),
      );
}

class FocusEvidence {
  const FocusEvidence({required this.id, required this.label});
  final String id;
  final String label;
}

class FocusProposal {
  const FocusProposal({
    required this.id,
    required this.personId,
    required this.startsAt,
    required this.endsAt,
    required this.timezoneOffsetSeconds,
    required this.reason,
    required this.evidence,
    required this.inferenceClass,
    required this.calendarWarning,
  });

  final String id;
  final String personId;
  final DateTime startsAt;
  final DateTime endsAt;
  final int timezoneOffsetSeconds;
  final String reason;
  final List<FocusEvidence> evidence;
  final String inferenceClass;
  final bool calendarWarning;

  factory FocusProposal.fromJson(Map<String, dynamic> json) {
    final slot = json['slot'] as Map;
    return FocusProposal(
      id: json['id'] as String,
      personId: json['person_id'] as String,
      startsAt: DateTime.parse(slot['starts_at'] as String),
      endsAt: DateTime.parse(slot['ends_at'] as String),
      timezoneOffsetSeconds: json['timezone_offset_seconds'] as int,
      reason: json['reason'] as String,
      evidence: (json['evidence'] as List)
          .map(
            (source) => FocusEvidence(
              id: source['id'] as String,
              label: source['label'] as String,
            ),
          )
          .toList(growable: false),
      inferenceClass: json['inference_class'] as String,
      calendarWarning: json['calendar_warning'] as bool,
    );
  }
}
