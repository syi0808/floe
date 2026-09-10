abstract interface class AgentConnectionsGateway {
  Future<List<AgentConnection>> readConnections(String personId);
}

enum AgentConnectionState {
  pending,
  ready,
  degraded,
  unavailable,
  disconnected,
  revoked,
  unsupported,
}

final class AgentConnection {
  AgentConnection.fromJson(Map<String, dynamic> json)
    : descriptor = AgentConnectionDescriptor.fromJson(_map(json['descriptor'])),
      state = AgentConnectionState.values.byName(
        _string(_map(json['connection'])['state']),
      ),
      grantedScopes = _strings(
        _map(json['connection'])['granted_scopes'],
        maximum: 64,
      ),
      observedAt = _time(_map(json['connection'])['observed_at_unix_ms']),
      lastSuccessAt = _optionalTime(
        _map(json['connection'])['last_success_at_unix_ms'],
      ),
      failure = _failure(_map(json['connection'])['last_failure']),
      views = List.unmodifiable(
        _maps(json['views'], maximum: 128).map(AgentConnectionView.fromJson),
      ) {
    final connection = _map(json['connection']);
    if (connection['schema_version'] != 1 ||
        connection['connector_id'] != descriptor.id ||
        lastSuccessAt?.isAfter(observedAt) == true ||
        failure?.observedAt.isAfter(observedAt) == true ||
        grantedScopes.toSet().length != grantedScopes.length ||
        views.map((view) => view.sourceHandle).toSet().length != views.length ||
        views.any(
          (view) =>
              !descriptor.views.any((descriptor) => descriptor.id == view.id),
        ) ||
        state == AgentConnectionState.ready &&
            (lastSuccessAt == null || failure != null) ||
        state == AgentConnectionState.degraded && failure == null ||
        {
              AgentConnectionState.unavailable,
              AgentConnectionState.revoked,
              AgentConnectionState.unsupported,
            }.contains(state) &&
            failure == null) {
      throw const FormatException('Invalid connection snapshot');
    }
    for (final view in views) {
      final viewDescriptor = descriptor.views.singleWhere(
        (descriptor) => descriptor.id == view.id,
      );
      if (view.itemCount > viewDescriptor.maximumItems ||
          view.byteCount > viewDescriptor.maximumBytes ||
          view.expiresAt.difference(view.observedAt) >
              viewDescriptor.freshness ||
          viewDescriptor.provenanceRequired &&
              view.itemCount > view.provenanceCount) {
        throw const FormatException('View snapshot exceeds descriptor');
      }
    }
  }

  final AgentConnectionDescriptor descriptor;
  final AgentConnectionState state;
  final List<String> grantedScopes;
  final DateTime observedAt;
  final DateTime? lastSuccessAt;
  final AgentConnectionFailure? failure;
  final List<AgentConnectionView> views;

  bool get usable =>
      state == AgentConnectionState.ready ||
      state == AgentConnectionState.degraded;
}

final class AgentConnectionDescriptor {
  AgentConnectionDescriptor.fromJson(Map<String, dynamic> json)
    : id = _identifier(json['id']),
      version = _identifier(json['version']),
      provider = _identifier(json['provider']),
      execution = AgentExecutionLocation.fromJson(_map(json['execution'])),
      capabilities = List.unmodifiable(
        _maps(
          json['capabilities'],
          maximum: 128,
        ).map(AgentConnectorCapability.fromJson),
      ),
      views = List.unmodifiable(
        _maps(json['views'], maximum: 64).map(AgentViewDescriptor.fromJson),
      ) {
    if (json['schema_version'] != 1 ||
        capabilities.isEmpty ||
        capabilities.map((entry) => entry.id).toSet().length !=
            capabilities.length ||
        views.map((entry) => entry.id).toSet().length != views.length) {
      throw const FormatException('Invalid connection descriptor');
    }
  }

  final String id;
  final String version;
  final String provider;
  final AgentExecutionLocation execution;
  final List<AgentConnectorCapability> capabilities;
  final List<AgentViewDescriptor> views;
}

final class AgentExecutionLocation {
  AgentExecutionLocation.fromJson(Map<String, dynamic> json)
    : kind = _string(json['kind']),
      deviceId = json['device_id'] == null
          ? null
          : _identifier(json['device_id']) {
    if (kind != 'server' && kind != 'device' ||
        (kind == 'device') != (deviceId != null)) {
      throw const FormatException('Invalid connector execution location');
    }
  }

  final String kind;
  final String? deviceId;
}

final class AgentConnectorCapability {
  AgentConnectorCapability.fromJson(Map<String, dynamic> json)
    : id = _identifier(json['id']),
      authority = _string(json['authority']),
      requiredScopes = _strings(json['required_scopes'], maximum: 64),
      outputViewId = json['output_view_id'] == null
          ? null
          : _identifier(json['output_view_id']) {
    if (json['schema_version'] != 1 ||
        _identifier(json['version']).isEmpty ||
        !{'observe', 'act', 'interact'}.contains(authority) ||
        (authority == 'observe') != (outputViewId != null) ||
        requiredScopes.toSet().length != requiredScopes.length) {
      throw const FormatException('Invalid connector capability');
    }
  }

  final String id;
  final String authority;
  final List<String> requiredScopes;
  final String? outputViewId;
}

final class AgentViewDescriptor {
  AgentViewDescriptor.fromJson(Map<String, dynamic> json)
    : id = _identifier(json['id']),
      retention = _string(json['retention']),
      freshness = Duration(milliseconds: _positive(json['freshness_ttl_ms'])),
      maximumItems = _positive(json['max_items']),
      maximumBytes = _positive(json['max_bytes']),
      provenanceRequired = _boolean(json['provenance_required']) {
    if (json['schema_version'] != 1 ||
        _identifier(json['version']).isEmpty ||
        _string(json['data_class']).isEmpty) {
      throw const FormatException('Invalid View descriptor');
    }
  }

  final String id;
  final String retention;
  final Duration freshness;
  final int maximumItems;
  final int maximumBytes;
  final bool provenanceRequired;
}

final class AgentConnectionView {
  AgentConnectionView.fromJson(Map<String, dynamic> json)
    : id = _identifier(json['view_id']),
      sourceHandle = _identifier(json['source_handle']),
      observedAt = _time(json['observed_at_unix_ms']),
      expiresAt = _time(json['expires_at_unix_ms']),
      itemCount = _nonnegative(json['item_count']),
      byteCount = _nonnegative(json['byte_count']),
      provenanceCount = _nonnegative(json['provenance_count']) {
    if (json['schema_version'] != 1 || !expiresAt.isAfter(observedAt)) {
      throw const FormatException('Invalid View snapshot');
    }
  }

  final String id;
  final String sourceHandle;
  final DateTime observedAt;
  final DateTime expiresAt;
  final int itemCount;
  final int byteCount;
  final int provenanceCount;
}

final class AgentConnectionFailure {
  const AgentConnectionFailure(this.kind, this.observedAt);
  final String kind;
  final DateTime observedAt;
}

AgentConnectionFailure? _failure(Object? value) {
  if (value == null) return null;
  final json = _map(value);
  return AgentConnectionFailure(
    _identifier(json['kind']),
    _time(json['observed_at_unix_ms']),
  );
}

Map<String, dynamic> _map(Object? value) {
  if (value is! Map) throw const FormatException('Expected object');
  return Map<String, dynamic>.from(value);
}

Iterable<Map<String, dynamic>> _maps(Object? value, {required int maximum}) {
  if (value is! List || value.length > maximum) {
    throw const FormatException('Invalid entry list');
  }
  return value.map(_map);
}

List<String> _strings(Object? value, {required int maximum}) {
  if (value is! List || value.length > maximum) {
    throw const FormatException('Invalid string list');
  }
  return List.unmodifiable(value.map(_identifier));
}

String _identifier(Object? value) {
  final result = _string(value);
  if (result.trim().isEmpty || result.length > 128) {
    throw const FormatException('Invalid identifier');
  }
  return result;
}

String _string(Object? value) {
  if (value is! String) throw const FormatException('Expected string');
  return value;
}

bool _boolean(Object? value) {
  if (value is! bool) throw const FormatException('Expected boolean');
  return value;
}

int _positive(Object? value) {
  final result = _nonnegative(value);
  if (result == 0) throw const FormatException('Expected positive integer');
  return result;
}

int _nonnegative(Object? value) {
  if (value is! int || value < 0) {
    throw const FormatException('Expected nonnegative integer');
  }
  return value;
}

DateTime _time(Object? value) =>
    DateTime.fromMillisecondsSinceEpoch(_nonnegative(value), isUtc: true);

DateTime? _optionalTime(Object? value) => value == null ? null : _time(value);
