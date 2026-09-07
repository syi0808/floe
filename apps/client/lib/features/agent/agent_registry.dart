enum AgentRegistryTarget {
  installation,
  assignment,
  calendarView;

  String get wireName => this == calendarView ? 'calendar_view' : name;
}

abstract interface class AgentRegistryGateway {
  Future<AgentRegistryView?> readRegistry(String personId);
  Future<AgentRegistryView> configureRegistry(
    AgentRegistryView current, {
    required AgentRegistryTarget target,
    required String id,
    required bool enabled,
  });
}

final class AgentRegistryView {
  AgentRegistryView.fromJson(Map<String, dynamic> json)
    : personId = _identifier(json['person_id']),
      instanceId = _identifier(json['instance_id']),
      revision = _number(json['revision']),
      installations = List.unmodifiable(
        _entries(json['installations'], 128).map(AgentInstallation.fromJson),
      ),
      assignments = List.unmodifiable(
        _entries(json['assignments'], 256).map(AgentAssignment.fromJson),
      ) {
    if (json['schema_version'] != 1 ||
        installations.map((entry) => entry.id).toSet().length !=
            installations.length ||
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
  final List<AgentAssignment> assignments;
}

final class AgentInstallation {
  AgentInstallation.fromJson(Map<String, dynamic> json)
    : id = _identifier(json['id']),
      enabled = _boolean(json['enabled']),
      packageId = _text((json['package'] as Map)['id']),
      version = _text((json['package'] as Map)['version']),
      kind = _text((json['package'] as Map)['kind']) {
    if (kind != 'tool' && kind != 'expert') {
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
      grantedToolCount = _number(json['granted_tool_count']),
      grantedViewCount = _number(json['granted_view_count']),
      stateRevision = _number(json['state_revision']),
      completedInvocations = _number(json['completed_invocations']) {
    if (grantedToolCount > 1 || grantedViewCount > 4) {
      throw const FormatException('Invalid grant counts');
    }
  }
  final String id;
  final String installationId;
  final bool enabled;
  final int grantedToolCount;
  final int grantedViewCount;
  final int stateRevision;
  final int completedInvocations;
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

String _text(Object? value) {
  if (value is! String || value.isEmpty || value.length > 128) {
    throw const FormatException('Invalid registry label');
  }
  return value;
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
