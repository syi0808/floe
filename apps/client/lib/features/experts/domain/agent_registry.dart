import 'dart:convert';

enum AgentRegistryCommandKind {
  installationSetEnabled,
  bindingPrepareReview,
  bindingReplace,
}

sealed class AgentRegistryCommandResult {
  const AgentRegistryCommandResult(this.kind);

  final AgentRegistryCommandKind kind;
}

final class AgentDirectoryCommandResult extends AgentRegistryCommandResult {
  const AgentDirectoryCommandResult(super.kind, this.directory);

  final AgentDirectorySnapshot directory;
}

final class AgentBindingReviewCommandResult extends AgentRegistryCommandResult {
  const AgentBindingReviewCommandResult(super.kind, this.review);

  final AgentBindingReview review;
}

abstract interface class AgentRegistryGateway {
  AgentRegistryCommandKind? get pendingCommandKind;

  Future<AgentDirectorySnapshot> readDirectory();

  Future<AgentDirectorySnapshot> setInstallationEnabled({
    required String installationRef,
    required int expectedRevision,
    required bool enabled,
  });

  Future<AgentBindingInspection> inspectBinding({
    required String assignmentRef,
    required String requirementRef,
  });

  Future<AgentBindingReview> prepareBindingReview({
    required String assignmentRef,
    required String requirementRef,
    required int expectedBindingRevision,
  });

  Future<AgentBindingReview> inspectBindingReview(
    AgentBindingReviewRef reviewRef,
  );

  Future<AgentDirectorySnapshot> replaceBinding({
    required AgentBindingReview review,
    required List<String> candidateRefs,
  });

  Future<AgentRegistryCommandResult> retryPendingCommand();
}

final class AgentDirectorySnapshot {
  AgentDirectorySnapshot.fromJson(Map<String, dynamic> json)
    : revision = _number(json['revision'], 'directory.revision'),
      installations = List.unmodifiable(
        _entries(
          json['installations'],
          128,
          'directory.installations',
        ).map(AgentInstallation.fromJson),
      ),
      assignments = List.unmodifiable(
        _entries(
          json['assignments'],
          256,
          'directory.assignments',
        ).map(AgentAssignment.fromJson),
      ) {
    _exactKeys(json, const {'revision', 'installations', 'assignments'});
    if (installations.map((entry) => entry.installationRef).toSet().length !=
            installations.length ||
        assignments.map((entry) => entry.assignmentRef).toSet().length !=
            assignments.length) {
      throw const FormatException('Invalid Expert directory references.');
    }
    final installationRefs = installations
        .map((entry) => entry.installationRef)
        .toSet();
    if (assignments.any(
      (entry) => !installationRefs.contains(entry.installationRef),
    )) {
      throw const FormatException('Invalid Expert directory assignment.');
    }
  }

  final int revision;
  final List<AgentInstallation> installations;
  final List<AgentAssignment> assignments;
}

final class AgentInstallation {
  AgentInstallation.fromJson(Map<String, dynamic> json)
    : installationRef = _uuid(json['installation_ref'], 'installation_ref'),
      displayName = _boundedOwnerText(
        json['display_name'],
        128,
        'installation.display_name',
      ),
      version = _boundedOwnerText(json['version'], 64, 'installation.version'),
      enabled = _boolean(json['enabled'], 'installation.enabled') {
    _exactKeys(json, const {
      'installation_ref',
      'display_name',
      'version',
      'enabled',
    });
  }

  final String installationRef;
  final String displayName;
  final String version;
  final bool enabled;
}

final class AgentAssignment {
  AgentAssignment.fromJson(Map<String, dynamic> json)
    : assignmentRef = _uuid(json['assignment_ref'], 'assignment_ref'),
      installationRef = _uuid(json['installation_ref'], 'installation_ref'),
      displayName = _boundedOwnerText(
        json['display_name'],
        128,
        'assignment.display_name',
      ),
      enabled = _boolean(json['enabled'], 'assignment.enabled'),
      bindingRevision = _positiveNumber(
        json['binding_revision'],
        'assignment.binding_revision',
      ),
      requirements = List.unmodifiable(
        _entries(
          json['requirements'],
          32,
          'assignment.requirements',
        ).map(AgentSourceRequirement.fromJson),
      ) {
    _exactKeys(json, const {
      'assignment_ref',
      'installation_ref',
      'display_name',
      'enabled',
      'binding_revision',
      'requirements',
    });
    if (requirements.map((entry) => entry.requirementRef).toSet().length !=
        requirements.length) {
      throw const FormatException('Duplicate Expert requirement.');
    }
  }

  final String assignmentRef;
  final String installationRef;
  final String displayName;
  final bool enabled;
  final int bindingRevision;
  final List<AgentSourceRequirement> requirements;
}

final class AgentSourceRequirement {
  AgentSourceRequirement.fromJson(Map<String, dynamic> json)
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
      throw const FormatException('Invalid Expert requirement.');
    }
  }

  final String requirementRef;
  final String label;
  final int selectedCount;
  final int minimumSources;
}

enum AgentCandidateAvailability { available, unavailable }

enum AgentBindingReviewAction { replace, refresh }

final class AgentBindingReviewRef {
  const AgentBindingReviewRef({required this.id, required this.digest});

  factory AgentBindingReviewRef.fromJson(Map<String, dynamic> json) {
    _exactKeys(json, const {'id', 'digest'});
    final id = _uuid(json['id'], 'review_ref.id');
    final digest = _digest(json['digest'], 'review_ref.digest');
    if (digest == List.filled(64, '0').join()) {
      throw const FormatException('Invalid Expert review reference.');
    }
    return AgentBindingReviewRef(id: id, digest: digest);
  }

  final String id;
  final String digest;

  bool matches(AgentBindingReviewRef other) =>
      id == other.id && digest == other.digest;

  Map<String, Object?> toJson() => {'id': id, 'digest': digest};
}

final class AgentBindingReview {
  AgentBindingReview.fromJson(Map<String, dynamic> json)
    : reviewRef = AgentBindingReviewRef.fromJson(
        _object(json['review_ref'], 'review_ref'),
      ),
      assignmentRef = _uuid(json['assignment_ref'], 'assignment_ref'),
      requirementRef = _identifier(json['requirement_ref'], 'requirement_ref'),
      bindingRevision = _positiveNumber(
        json['binding_revision'],
        'binding_revision',
      ),
      candidates = List.unmodifiable(
        _entries(
          json['candidate_refs_and_labels'],
          64,
          'candidate_refs_and_labels',
        ).map(AgentBindingCandidate.fromJson),
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
          AgentBindingReviewAction.values,
        ),
      ) {
    _exactKeys(json, const {
      'review_ref',
      'assignment_ref',
      'requirement_ref',
      'binding_revision',
      'candidate_refs_and_labels',
      'expires_at_unix_ms',
      'allowed_actions',
    });
    if (candidates.map((candidate) => candidate.candidateRef).toSet().length !=
            candidates.length ||
        candidates.where((candidate) => candidate.selected).length > 16 ||
        candidates.any(
          (candidate) =>
              candidate.availability ==
                  AgentCandidateAvailability.unavailable &&
              !candidate.selected,
        ) ||
        allowedActions.isEmpty ||
        allowedActions.toSet().length != allowedActions.length) {
      throw const FormatException('Invalid Expert binding review.');
    }
  }

  final AgentBindingReviewRef reviewRef;
  final String assignmentRef;
  final String requirementRef;
  final int bindingRevision;
  final List<AgentBindingCandidate> candidates;
  final int expiresAtUnixMs;
  final List<AgentBindingReviewAction> allowedActions;

  bool get expired => DateTime.now().millisecondsSinceEpoch >= expiresAtUnixMs;

  bool get canReplace =>
      allowedActions.contains(AgentBindingReviewAction.replace) && !expired;

  bool get canRefresh =>
      allowedActions.contains(AgentBindingReviewAction.refresh);
}

final class AgentBindingCandidate {
  AgentBindingCandidate.fromJson(Map<String, dynamic> json)
    : candidateRef = _uuid(json['candidate_ref'], 'candidate_ref'),
      label = _candidateLabel(json['label']),
      availability = switch (json['availability']) {
        'available' => AgentCandidateAvailability.available,
        'unavailable' => AgentCandidateAvailability.unavailable,
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
  final AgentCandidateAvailability availability;
  final bool selected;
}

final class AgentBindingInspection {
  AgentBindingInspection.fromJson(Map<String, dynamic> json)
    : assignmentRef = _uuid(json['assignment_ref'], 'assignment_ref'),
      requirementRef = _identifier(json['requirement_ref'], 'requirement_ref'),
      bindingRevision = _positiveNumber(
        json['binding_revision'],
        'binding_revision',
      ),
      candidates = List.unmodifiable(
        _entries(
          json['candidates'],
          64,
          'binding.candidates',
        ).map(AgentBindingInspectionCandidate.fromJson),
      ) {
    _exactKeys(json, const {
      'assignment_ref',
      'requirement_ref',
      'binding_revision',
      'candidates',
    });
  }

  final String assignmentRef;
  final String requirementRef;
  final int bindingRevision;
  final List<AgentBindingInspectionCandidate> candidates;
}

final class AgentBindingInspectionCandidate {
  AgentBindingInspectionCandidate.fromJson(Map<String, dynamic> json)
    : label = _candidateLabel(json['label']),
      availability = switch (json['availability']) {
        'available' => AgentCandidateAvailability.available,
        'unavailable' => AgentCandidateAvailability.unavailable,
        _ => throw const FormatException('Invalid candidate availability.'),
      },
      selected = _boolean(json['selected'], 'candidate.selected') {
    _exactKeys(json, const {'label', 'availability', 'selected'});
  }

  final String label;
  final AgentCandidateAvailability availability;
  final bool selected;
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
