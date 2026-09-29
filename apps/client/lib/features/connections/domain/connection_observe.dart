import 'dart:convert';

final class ConnectionObserveOverview {
  const ConnectionObserveOverview({
    required this.connectorId,
    required this.connectionId,
    required this.status,
    required this.enabled,
    required this.sourceResources,
    required this.members,
  });

  final String connectorId;
  final String connectionId;
  final String status;
  final bool enabled;
  final List<String> sourceResources;
  final List<ConnectionObserveMember> members;

  factory ConnectionObserveOverview.fromJson(Object? raw) {
    if (raw is! Map) throw const FormatException('Invalid Observe overview');
    final value = Map<String, Object?>.from(raw);
    const fields = {
      'connector_id',
      'connection_id',
      'status',
      'enabled',
      'source_resources',
      'members',
    };
    if (value.keys.any((key) => !fields.contains(key)) ||
        value['connector_id'] is! String ||
        value['connection_id'] is! String ||
        value['enabled'] is! bool ||
        value['source_resources'] is! List ||
        value['members'] is! List) {
      throw const FormatException('Invalid Observe overview');
    }
    final status = value['status'];
    if (status is! String ||
        !const {
          'active',
          'paused',
          'needs_review',
          'needs_system_access',
          'reconnect_required',
          'unavailable',
        }.contains(status)) {
      throw const FormatException('Invalid Observe status');
    }
    return ConnectionObserveOverview(
      connectorId: value['connector_id']! as String,
      connectionId: value['connection_id']! as String,
      status: status,
      enabled: value['enabled']! as bool,
      sourceResources: List<String>.unmodifiable(
        (value['source_resources']! as List).cast<String>(),
      ),
      members: List<ConnectionObserveMember>.unmodifiable(
        (value['members']! as List).map(ConnectionObserveMember.fromJson),
      ),
    );
  }
}

final class ConnectionObserveMember {
  const ConnectionObserveMember({
    required this.viewId,
    required this.state,
    required this.reviewRequired,
  });

  final String viewId;
  final String state;
  final bool reviewRequired;

  factory ConnectionObserveMember.fromJson(Object? raw) {
    if (raw is! Map) throw const FormatException('Invalid Observe member');
    final value = Map<String, Object?>.from(raw);
    if (value.length != 3 ||
        value['view_id'] is! String ||
        value['review_required'] is! bool ||
        !const {'active', 'paused', 'revoked'}.contains(value['state'])) {
      throw const FormatException('Invalid Observe member');
    }
    return ConnectionObserveMember(
      viewId: value['view_id']! as String,
      state: value['state']! as String,
      reviewRequired: value['review_required']! as bool,
    );
  }
}

final class ConnectionObserveReview {
  ConnectionObserveReview._(
    this.connectorId,
    this.connectionId,
    this.members,
    this._json,
  );

  final String connectorId;
  final String connectionId;
  final List<String> members;
  final Map<String, Object?> _json;

  factory ConnectionObserveReview.fromJson(Object? raw) {
    if (raw is! Map) throw const FormatException('Invalid Observe review');
    final value = Map<String, Object?>.from(raw);
    const fields = {
      'connector_id',
      'connection_id',
      'source_authority',
      'connection_revision',
      'native_subject',
      'producer_fingerprint',
      'members',
    };
    if (value.keys.any((key) => !fields.contains(key)) ||
        value['connector_id'] is! String ||
        value['connection_id'] is! String ||
        value['source_authority'] is! Map ||
        value['members'] is! List) {
      throw const FormatException('Invalid Observe review');
    }
    final members = value['members']! as List;
    if (members.isEmpty || members.length > 8) {
      throw const FormatException('Invalid Observe review');
    }
    final viewIds = <String>[];
    for (final rawMember in members) {
      if (rawMember is! Map) {
        throw const FormatException('Invalid Observe review');
      }
      final member = Map<String, Object?>.from(rawMember);
      const memberFields = {
        'view_id',
        'policy_digest',
        'resource',
        'expected_grant_id',
        'expected_grant_authority',
      };
      if (member.keys.any((key) => !memberFields.contains(key)) ||
          member['view_id'] is! String ||
          member['policy_digest'] is! String ||
          member['resource'] is! String) {
        throw const FormatException('Invalid Observe review');
      }
      viewIds.add(member['view_id']! as String);
    }
    for (var index = 1; index < viewIds.length; index++) {
      if (viewIds[index - 1].compareTo(viewIds[index]) >= 0) {
        throw const FormatException('Invalid Observe review');
      }
    }
    return ConnectionObserveReview._(
      value['connector_id']! as String,
      value['connection_id']! as String,
      List<String>.unmodifiable(viewIds),
      Map<String, Object?>.unmodifiable(
        jsonDecode(jsonEncode(value)) as Map<String, dynamic>,
      ),
    );
  }

  Map<String, Object?> toJson() =>
      Map<String, Object?>.from(jsonDecode(jsonEncode(_json)) as Map);
}
