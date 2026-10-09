import 'dart:convert';

enum AssistantFeatureCommandKind { configure, sourcePrepareReview }

sealed class AssistantFeatureCommandResult {
  const AssistantFeatureCommandResult(this.kind);

  final AssistantFeatureCommandKind kind;
}

final class AssistantFeatureSnapshotCommandResult
    extends AssistantFeatureCommandResult {
  const AssistantFeatureSnapshotCommandResult(super.kind, this.snapshot);

  final AssistantFeatureSnapshot snapshot;
}

final class AssistantFeatureSourceReviewCommandResult
    extends AssistantFeatureCommandResult {
  const AssistantFeatureSourceReviewCommandResult(super.kind, this.review);

  final AssistantFeatureSourceReview review;
}

abstract interface class AssistantFeatureGateway {
  AssistantFeatureCommandKind? get pendingCommandKind;

  Future<AssistantFeatureSnapshot> readSnapshot();

  Future<AssistantFeatureSourceReview> prepareSourceReview({
    required String featureRef,
    required String sourceScopeRef,
    required String sourceRequirementRef,
    required int expectedBindingRevision,
  });

  Future<AssistantFeatureSourceReview> inspectSourceReview(
    AssistantFeatureSourceReviewRef reviewRef,
  );

  Future<AssistantFeatureSnapshot> configure({
    required String featureRef,
    required int expectedRevision,
    required bool enabled,
    required List<AssistantFeatureSourceSelection> sourceSelections,
  });

  Future<AssistantFeatureCommandResult> retryPendingCommand();
}

final class AssistantFeatureSnapshot {
  AssistantFeatureSnapshot.fromJson(Map<String, dynamic> json)
    : revision = _number(json['revision'], 'snapshot.revision'),
      features = List.unmodifiable(
        _entries(
          json['features'],
          128,
          'snapshot.features',
        ).map(AssistantFeature.fromJson),
      ) {
    _exactKeys(json, const {'revision', 'features'});
    if (features.map((feature) => feature.featureRef).toSet().length !=
        features.length) {
      throw const FormatException('Duplicate assistant feature.');
    }
  }

  final int revision;
  final List<AssistantFeature> features;
}

final class AssistantFeature {
  AssistantFeature.fromJson(Map<String, dynamic> json)
    : featureRef = _uuid(json['feature_ref'], 'feature_ref'),
      displayName = _boundedOwnerText(
        json['display_name'],
        128,
        'feature.display_name',
      ),
      description = _boundedOwnerText(
        json['description'],
        512,
        'feature.description',
      ),
      enabled = _boolean(json['enabled'], 'feature.enabled'),
      sourceGroups = List.unmodifiable(
        _entries(
          json['source_groups'],
          256,
          'feature.source_groups',
        ).map(AssistantFeatureSourceGroup.fromJson),
      ) {
    _exactKeys(json, const {
      'feature_ref',
      'display_name',
      'description',
      'enabled',
      'source_groups',
    });
    if (sourceGroups.map((group) => group.sourceScopeRef).toSet().length !=
        sourceGroups.length) {
      throw const FormatException('Duplicate assistant feature source group.');
    }
  }

  final String featureRef;
  final String displayName;
  final String description;
  final bool enabled;
  final List<AssistantFeatureSourceGroup> sourceGroups;
}

final class AssistantFeatureSourceGroup {
  AssistantFeatureSourceGroup.fromJson(Map<String, dynamic> json)
    : sourceScopeRef = _uuid(json['source_scope_ref'], 'source_scope_ref'),
      displayName = _boundedOwnerText(
        json['display_name'],
        128,
        'source_group.display_name',
      ),
      enabled = _boolean(json['enabled'], 'source_group.enabled'),
      bindingRevision = _positiveNumber(
        json['binding_revision'],
        'source_group.binding_revision',
      ),
      requirements = List.unmodifiable(
        _entries(
          json['requirements'],
          32,
          'source_group.requirements',
        ).map(AssistantFeatureSourceRequirement.fromJson),
      ) {
    _exactKeys(json, const {
      'source_scope_ref',
      'display_name',
      'enabled',
      'binding_revision',
      'requirements',
    });
    if (requirements.map((entry) => entry.requirementRef).toSet().length !=
        requirements.length) {
      throw const FormatException('Duplicate source requirement.');
    }
  }

  final String sourceScopeRef;
  final String displayName;
  final bool enabled;
  final int bindingRevision;
  final List<AssistantFeatureSourceRequirement> requirements;
}

final class AssistantFeatureSourceRequirement {
  AssistantFeatureSourceRequirement.fromJson(Map<String, dynamic> json)
    : requirementRef = _identifier(json['requirement_ref'], 'requirement_ref'),
      label = _text(json['label'], 128, 'requirement.label'),
      selectedCount = _number(
        json['selected_count'],
        'requirement.selected_count',
      ),
      minimumSources = _number(
        json['minimum_sources'],
        'requirement.minimum_sources',
      ) {
    _exactKeys(json, const {
      'requirement_ref',
      'label',
      'selected_count',
      'minimum_sources',
    });
    if (label != requirementRef || selectedCount > 16 || minimumSources > 16) {
      throw const FormatException('Invalid source requirement.');
    }
  }

  final String requirementRef;
  final String label;
  final int selectedCount;
  final int minimumSources;
}

enum AssistantFeatureSourceAvailability { available, unavailable }

enum AssistantFeatureSourceReviewAction { replace, refresh }

final class AssistantFeatureSourceReviewRef {
  const AssistantFeatureSourceReviewRef({
    required this.id,
    required this.digest,
  });

  factory AssistantFeatureSourceReviewRef.fromJson(Map<String, dynamic> json) {
    _exactKeys(json, const {'id', 'digest'});
    final id = _uuid(json['id'], 'review_ref.id');
    final digest = _digest(json['digest'], 'review_ref.digest');
    if (digest == List.filled(64, '0').join()) {
      throw const FormatException('Invalid source review reference.');
    }
    return AssistantFeatureSourceReviewRef(id: id, digest: digest);
  }

  final String id;
  final String digest;

  bool matches(AssistantFeatureSourceReviewRef other) =>
      id == other.id && digest == other.digest;

  Map<String, Object?> toJson() => {'id': id, 'digest': digest};
}

final class AssistantFeatureSourceReview {
  AssistantFeatureSourceReview.fromJson(Map<String, dynamic> json)
    : reviewRef = AssistantFeatureSourceReviewRef.fromJson(
        _object(json['review_ref'], 'review_ref'),
      ),
      sourceScopeRef = _uuid(json['source_scope_ref'], 'source_scope_ref'),
      sourceRequirementRef = _identifier(
        json['source_requirement_ref'],
        'source_requirement_ref',
      ),
      bindingRevision = _positiveNumber(
        json['binding_revision'],
        'binding_revision',
      ),
      candidates = List.unmodifiable(
        _entries(
          json['candidates'],
          64,
          'candidates',
        ).map(AssistantFeatureSourceCandidate.fromJson),
      ),
      expiresAtUnixMs = _positiveNumber(
        json['expires_at_unix_ms'],
        'expires_at_unix_ms',
      ),
      allowedActions = List.unmodifiable(
        _enumList(
          json['allowed_actions'],
          2,
          'allowed_actions',
          AssistantFeatureSourceReviewAction.values,
        ),
      ) {
    _exactKeys(json, const {
      'review_ref',
      'source_scope_ref',
      'source_requirement_ref',
      'binding_revision',
      'candidates',
      'expires_at_unix_ms',
      'allowed_actions',
    });
    if (candidates.map((candidate) => candidate.candidateRef).toSet().length !=
            candidates.length ||
        candidates.where((candidate) => candidate.selected).length > 16 ||
        candidates.any(
          (candidate) =>
              candidate.availability ==
                  AssistantFeatureSourceAvailability.unavailable &&
              !candidate.selected,
        ) ||
        allowedActions.isEmpty ||
        allowedActions.toSet().length != allowedActions.length) {
      throw const FormatException('Invalid assistant feature source review.');
    }
  }

  final AssistantFeatureSourceReviewRef reviewRef;
  final String sourceScopeRef;
  final String sourceRequirementRef;
  final int bindingRevision;
  final List<AssistantFeatureSourceCandidate> candidates;
  final int expiresAtUnixMs;
  final List<AssistantFeatureSourceReviewAction> allowedActions;

  bool get expired => DateTime.now().millisecondsSinceEpoch >= expiresAtUnixMs;

  bool get canReplace =>
      allowedActions.contains(AssistantFeatureSourceReviewAction.replace) &&
      !expired;

  bool get canRefresh =>
      allowedActions.contains(AssistantFeatureSourceReviewAction.refresh);
}

final class AssistantFeatureSourceCandidate {
  AssistantFeatureSourceCandidate.fromJson(Map<String, dynamic> json)
    : candidateRef = _uuid(json['candidate_ref'], 'candidate_ref'),
      label = _candidateLabel(json['label']),
      availability = switch (json['availability']) {
        'available' => AssistantFeatureSourceAvailability.available,
        'unavailable' => AssistantFeatureSourceAvailability.unavailable,
        _ => throw const FormatException('Invalid candidate availability.'),
      },
      selected = _boolean(json['selected'], 'candidate.selected') {
    _exactKeys(json, const {
      'candidate_ref',
      'label',
      'availability',
      'selected',
    });
  }

  final String candidateRef;
  final String label;
  final AssistantFeatureSourceAvailability availability;
  final bool selected;
}

final class AssistantFeatureSourceSelection {
  AssistantFeatureSourceSelection({
    required this.review,
    required List<String> candidateRefs,
  }) : candidateRefs = List.unmodifiable(candidateRefs) {
    if (candidateRefs.length > 16 ||
        candidateRefs.toSet().length != candidateRefs.length) {
      throw const FormatException('Invalid source selection.');
    }
    for (final reference in candidateRefs) {
      final candidate = review.candidates
          .where((entry) => entry.candidateRef == reference)
          .singleOrNull;
      if (candidate == null ||
          candidate.availability ==
                  AssistantFeatureSourceAvailability.unavailable &&
              !candidate.selected) {
        throw const FormatException('Candidate is outside source review.');
      }
    }
  }

  final AssistantFeatureSourceReview review;
  final List<String> candidateRefs;

  Map<String, Object?> toJson() => {
    'source_scope_ref': review.sourceScopeRef,
    'source_requirement_ref': review.sourceRequirementRef,
    'review_ref': review.reviewRef.toJson(),
    'expected_binding_revision': review.bindingRevision,
    'candidate_refs': candidateRefs,
  };
}

String _candidateLabel(Object? value) {
  final label = _text(value, 256, 'candidate.label');
  if (label.trim().isEmpty) {
    throw const FormatException('Invalid candidate label.');
  }
  return label;
}

String _identifier(Object? value, String field) {
  if (value is! String ||
      value.isEmpty ||
      utf8.encode(value).length > 128 ||
      !RegExp(r'^[A-Za-z0-9._-]+$').hasMatch(value)) {
    throw FormatException('Invalid $field.');
  }
  return value;
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

String _digest(Object? value, String field) {
  if (value is! String || !RegExp(r'^[0-9a-f]{64}$').hasMatch(value)) {
    throw FormatException('Invalid $field.');
  }
  return value;
}

String _text(Object? value, int maximumBytes, String field) {
  if (value is! String || utf8.encode(value).length > maximumBytes) {
    throw FormatException('Invalid $field.');
  }
  return value;
}

String _boundedOwnerText(Object? value, int maximumBytes, String field) {
  final text = _text(value, maximumBytes, field);
  if (text.trim().isEmpty ||
      text.runes.any((rune) => rune < 32 && rune != 10)) {
    throw FormatException('Invalid $field.');
  }
  return text;
}

int _number(Object? value, String field) {
  if (value is! int || value < 0 || value > 0x7fffffffffffffff) {
    throw FormatException('Invalid $field.');
  }
  return value;
}

int _positiveNumber(Object? value, String field) {
  final number = _number(value, field);
  if (number == 0) throw FormatException('Invalid $field.');
  return number;
}

bool _boolean(Object? value, String field) {
  if (value is! bool) throw FormatException('Invalid $field.');
  return value;
}

Map<String, dynamic> _object(Object? value, String field) {
  if (value is! Map || value.keys.any((key) => key is! String)) {
    throw FormatException('Invalid $field.');
  }
  return Map<String, dynamic>.from(value);
}

List<Map<String, dynamic>> _entries(Object? value, int maximum, String field) {
  if (value is! List || value.length > maximum) {
    throw FormatException('Invalid $field.');
  }
  return value.map((entry) => _object(entry, field)).toList(growable: false);
}

void _exactKeys(Map<String, dynamic> value, Set<String> fields) {
  if (value.length != fields.length ||
      !value.keys.toSet().containsAll(fields)) {
    throw const FormatException('Unexpected wire fields.');
  }
}

List<T> _enumList<T extends Enum>(
  Object? value,
  int maximum,
  String field,
  List<T> allowed,
) {
  if (value is! List || value.length > maximum) {
    throw FormatException('Invalid $field.');
  }
  return value
      .map((entry) {
        if (entry is! String) throw FormatException('Invalid $field.');
        for (final option in allowed) {
          if (option.name == entry) return option;
        }
        throw FormatException('Invalid $field.');
      })
      .toList(growable: false);
}
