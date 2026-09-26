enum AgentRegistryTarget {
  installation,
  assignment;

  String get wireName => name;
}

abstract interface class AgentRegistryGateway {
  Future<AgentRegistryView?> readRegistry(String personId);
  Future<AgentRegistryView> configureRegistry(
    AgentRegistryView current, {
    required AgentRegistryTarget target,
    required String id,
    required bool enabled,
  });
  Future<AgentCandidateCatalog> readCandidates(
    String personId, {
    required String assignmentId,
    required String requirementKey,
  });
  Future<AgentCandidateCatalog> replaceSelection(
    String personId, {
    required AgentInstallation installation,
    required AgentExpertDefinition definition,
    required AgentAssignment assignment,
    required AgentSourceRequirement requirement,
    required List<String> candidateIds,
  });
}

final class AgentCandidateCatalog {
  AgentCandidateCatalog.fromJson(Map<String, dynamic> json)
    : assignmentId = _identifier(json['assignment_id']),
      requirementKey = _text(json['requirement_key']),
      bindingRevision = _number(json['binding_revision']),
      candidates = List.unmodifiable(
        _entries(json['candidates'], 32).map(AgentSourceCandidate.fromJson),
      ) {
    if (bindingRevision == 0 ||
        candidates.map((candidate) => candidate.id).toSet().length !=
            candidates.length) {
      throw const FormatException('Invalid Expert candidate catalog');
    }
  }

  final String assignmentId;
  final String requirementKey;
  final int bindingRevision;
  final List<AgentSourceCandidate> candidates;
}

final class AgentSourceCandidate {
  AgentSourceCandidate.fromJson(Map<String, dynamic> json)
    : id = _text(json['candidate_id'], maximum: 64),
      title = _text(json['title']),
      detail = _text(json['detail']),
      availability = _text(json['availability']),
      selected = _boolean(json['selected']) {
    if (!RegExp(r'^[0-9a-f]{64}$').hasMatch(id) ||
        !const {'available', 'unavailable'}.contains(availability)) {
      throw const FormatException('Invalid Expert source candidate');
    }
  }

  final String id;
  final String title;
  final String detail;
  final String availability;
  final bool selected;
}

final class AgentRegistryView {
  AgentRegistryView.fromJson(Map<String, dynamic> json)
    : personId = _identifier(json['person_id']),
      instanceId = _identifier(json['instance_id']),
      revision = _number(json['revision']),
      installations = List.unmodifiable(
        _entries(json['installations'], 128).map(AgentInstallation.fromJson),
      ),
      definitions = List.unmodifiable(
        _entries(json['definitions'], 128).map(AgentExpertDefinition.fromJson),
      ),
      assignments = List.unmodifiable(
        _entries(json['assignments'], 256).map(AgentAssignment.fromJson),
      ) {
    if (json['schema_version'] != 3 ||
        installations.map((entry) => entry.id).toSet().length !=
            installations.length ||
        definitions.map((entry) => entry.packageId).toSet().length !=
            definitions.length ||
        installations.any(
          (entry) => !definitions.any(
            (definition) =>
                definition.packageId == entry.packageId &&
                definition.version == entry.version,
          ),
        ) ||
        assignments.map((entry) => entry.id).toSet().length !=
            assignments.length ||
        assignments.any(
          (entry) => !installations.any(
            (installation) => installation.id == entry.installationId,
          ),
        )) {
      throw const FormatException('Invalid registry overview');
    }
  }

  final String personId;
  final String instanceId;
  final int revision;
  final List<AgentInstallation> installations;
  final List<AgentExpertDefinition> definitions;
  final List<AgentAssignment> assignments;
}

final class AgentExpertDefinition {
  AgentExpertDefinition.fromJson(Map<String, dynamic> json)
    : packageId = _text((json['package'] as Map)['id']),
      version = _text((json['package'] as Map)['version']),
      definitionRevision = _number(json['definition_revision']),
      name = _text(json['name']),
      description = _text(json['description'], maximum: 512),
      domainTags = List.unmodifiable(_labels(json['domain_tags'], 8, 64)),
      skills = List.unmodifiable(_labels(json['skills'], 8, 256));

  final String packageId;
  final String version;
  final int definitionRevision;
  final String name;
  final String description;
  final List<String> domainTags;
  final List<String> skills;
}

final class AgentInstallation {
  AgentInstallation.fromJson(Map<String, dynamic> json)
    : id = _identifier(json['id']),
      enabled = _boolean(json['enabled']),
      packageId = _text((json['package'] as Map)['id']),
      version = _text((json['package'] as Map)['version']),
      kind = _text((json['package'] as Map)['kind']) {
    if (kind != 'expert') {
      throw const FormatException('Invalid package kind');
    }
  }
  final String id;
  final String packageId;
  final String version;
  final String kind;
  final bool enabled;
}

final class AgentAssignment {
  AgentAssignment.fromJson(Map<String, dynamic> json)
    : id = _identifier(json['id']),
      installationId = _identifier(json['installation_id']),
      enabled = _boolean(json['enabled']),
      stateRevision = _number(json['state_revision']),
      completedInvocations = _number(json['completed_invocations']),
      bindingRevision = _number(json['binding_revision']),
      requirements = List.unmodifiable(
        _entries(json['requirements'], 32).map(AgentSourceRequirement.fromJson),
      );
  final String id;
  final String installationId;
  final bool enabled;
  final int stateRevision;
  final int completedInvocations;
  final int bindingRevision;
  final List<AgentSourceRequirement> requirements;
}

final class AgentSourceRequirement {
  AgentSourceRequirement.fromJson(Map<String, dynamic> json)
    : key = _text(json['key']),
      capability = _text(json['capability']),
      contractVersion = _number(json['contract_version']),
      minimumSources = _number(json['minimum_sources']),
      maximumSources = _number(json['maximum_sources']),
      selectedCount = _number(json['selected_count']) {
    if (contractVersion == 0 ||
        minimumSources > maximumSources ||
        selectedCount > maximumSources) {
      throw const FormatException('Invalid source requirement');
    }
  }

  final String key;
  final String capability;
  final int contractVersion;
  final int minimumSources;
  final int maximumSources;
  final int selectedCount;
}

String _identifier(Object? value) {
  if (value is! String ||
      !RegExp(
        r'^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$',
      ).hasMatch(value)) {
    throw const FormatException('Invalid registry identifier');
  }
  return value;
}

String _text(Object? value, {int maximum = 128}) {
  if (value is! String || value.isEmpty || value.length > maximum) {
    throw const FormatException('Invalid registry label');
  }
  return value;
}

Iterable<String> _labels(Object? value, int maximum, int length) {
  if (value is! List ||
      value.length > maximum ||
      value.any(
        (entry) => entry is! String || entry.isEmpty || entry.length > length,
      )) {
    throw const FormatException('Invalid registry labels');
  }
  return value.cast<String>();
}

int _number(Object? value) {
  if (value is! int || value < 0) {
    throw const FormatException('Invalid registry counter');
  }
  return value;
}

bool _boolean(Object? value) {
  if (value is! bool) throw const FormatException('Invalid registry flag');
  return value;
}

Iterable<Map<String, dynamic>> _entries(Object? value, int maximum) {
  if (value is! List || value.length > maximum) {
    throw const FormatException('Invalid registry entries');
  }
  return value.map((entry) => Map<String, dynamic>.from(entry as Map));
}
