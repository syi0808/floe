final class RemoteProducerIdentity {
  const RemoteProducerIdentity({
    required this.schemaVersion,
    required this.instanceId,
    required this.executionOwner,
    required this.audience,
    required this.keyId,
    required this.publicKey,
    required this.fingerprint,
  });

  final int schemaVersion;
  final String instanceId;
  final String executionOwner;
  final String audience;
  final String keyId;
  final String publicKey;
  final String fingerprint;

  factory RemoteProducerIdentity.fromJson(Object? raw) {
    if (raw is! Map) throw const FormatException('Invalid producer identity');
    final value = Map<String, Object?>.from(raw);
    const fields = {
      'schema_version',
      'instance_id',
      'execution_owner',
      'audience',
      'key_id',
      'public_key',
      'fingerprint',
    };
    if (value.keys.any((key) => !fields.contains(key))) {
      throw const FormatException('Invalid producer identity');
    }
    String text(String key) {
      final item = value[key];
      if (item is! String || item.isEmpty || item.length > 256) {
        throw const FormatException('Invalid producer identity');
      }
      return item;
    }

    final schema = value['schema_version'];
    if (schema != 1) throw const FormatException('Invalid producer identity');
    return RemoteProducerIdentity(
      schemaVersion: schema as int,
      instanceId: text('instance_id'),
      executionOwner: text('execution_owner'),
      audience: text('audience'),
      keyId: text('key_id'),
      publicKey: text('public_key'),
      fingerprint: text('fingerprint'),
    );
  }

  Map<String, Object> toJson() => {
    'schema_version': schemaVersion,
    'instance_id': instanceId,
    'execution_owner': executionOwner,
    'audience': audience,
    'key_id': keyId,
    'public_key': publicKey,
    'fingerprint': fingerprint,
  };
}

final class RemoteOwnerPublicKey {
  const RemoteOwnerPublicKey({
    required this.keyId,
    required this.publicKey,
    required this.fingerprint,
  });

  final String keyId;
  final String publicKey;
  final String fingerprint;

  factory RemoteOwnerPublicKey.fromJson(Object? raw) {
    if (raw is! Map) throw const FormatException('Invalid owner key');
    final value = Map<String, Object?>.from(raw);
    if (value.keys.any(
      (key) => !{'key_id', 'public_key', 'fingerprint'}.contains(key),
    )) {
      throw const FormatException('Invalid owner key');
    }
    String text(String key) {
      final item = value[key];
      if (item is! String || item.isEmpty || item.length > 256) {
        throw const FormatException('Invalid owner key');
      }
      return item;
    }

    return RemoteOwnerPublicKey(
      keyId: text('key_id'),
      publicKey: text('public_key'),
      fingerprint: text('fingerprint'),
    );
  }

  Map<String, Object> toJson() => {
    'key_id': keyId,
    'public_key': publicKey,
    'fingerprint': fingerprint,
  };
}

final class RemotePairingChallenge {
  const RemotePairingChallenge({
    required this.schemaVersion,
    required this.pairingId,
    required this.challengeId,
    required this.challengeB64Url,
    required this.producerSignature,
    required this.producer,
    required this.issuer,
    required this.expiresAtUnixMs,
  });

  final int schemaVersion;
  final String pairingId;
  final String challengeId;
  final String challengeB64Url;
  final String producerSignature;
  final RemoteProducerIdentity producer;
  final RemoteOwnerPublicKey issuer;
  final int expiresAtUnixMs;

  factory RemotePairingChallenge.fromJson(Object? raw) {
    if (raw is! Map) throw const FormatException('Invalid pairing challenge');
    final value = Map<String, Object?>.from(raw);
    const fields = {
      'schema_version',
      'pairing_id',
      'challenge_id',
      'challenge_b64url',
      'producer_signature',
      'producer',
      'issuer',
      'expires_at_unix_ms',
    };
    if (value.keys.any((key) => !fields.contains(key)) ||
        value['schema_version'] != 1 ||
        value['expires_at_unix_ms'] is! int) {
      throw const FormatException('Invalid pairing challenge');
    }
    String text(String key, {int maxLength = 256}) {
      final item = value[key];
      if (item is! String || item.isEmpty || item.length > maxLength) {
        throw const FormatException('Invalid pairing challenge');
      }
      return item;
    }

    final pairingId = text('pairing_id');
    return RemotePairingChallenge(
      schemaVersion: 1,
      pairingId: pairingId,
      challengeId: text('challenge_id'),
      challengeB64Url: text('challenge_b64url', maxLength: 8192),
      producerSignature: text('producer_signature'),
      producer: RemoteProducerIdentity.fromJson(value['producer']),
      issuer: RemoteOwnerPublicKey.fromJson(value['issuer']),
      expiresAtUnixMs: value['expires_at_unix_ms'] as int,
    );
  }

  Map<String, Object> toJson() => {
    'schema_version': schemaVersion,
    'pairing_id': pairingId,
    'challenge_id': challengeId,
    'challenge_b64url': challengeB64Url,
    'producer_signature': producerSignature,
    'producer': producer.toJson(),
    'issuer': issuer.toJson(),
    'expires_at_unix_ms': expiresAtUnixMs,
  };
}

final class RemotePairingStatus {
  const RemotePairingStatus({
    required this.operationId,
    required this.pairingId,
    required this.status,
    required this.personId,
    required this.deviceId,
    this.producer,
    this.issuer,
    this.issuerFingerprint,
    this.clientId,
    this.token,
  });

  final String operationId;
  final String pairingId;
  final String status;
  final String personId;
  final String deviceId;
  final RemoteProducerIdentity? producer;
  final RemoteOwnerPublicKey? issuer;
  final String? issuerFingerprint;
  final String? clientId;
  final String? token;

  factory RemotePairingStatus.fromJson(Object? raw, String operationId) {
    if (raw is! Map) throw const FormatException('Invalid pairing report');
    final value = Map<String, Object?>.from(raw);
    const fields = {
      'pairing_id',
      'person_id',
      'device_id',
      'producer',
      'issuer',
      'issuer_fingerprint',
      'outcome',
    };
    if (value.keys.any((key) => !fields.contains(key))) {
      throw const FormatException('Invalid pairing report');
    }
    String text(Map value, String key, {int maximum = 256}) {
      final item = value[key];
      if (item is! String || item.isEmpty || item.length > maximum) {
        throw const FormatException('Invalid pairing report');
      }
      return item;
    }

    final pairingId = text(value, 'pairing_id');
    final personId = text(value, 'person_id');
    final deviceId = text(value, 'device_id');
    final outcome = value['outcome'];
    if (outcome is! Map) throw const FormatException('Invalid pairing outcome');
    final status = text(outcome, 'status');
    if (!const {
          'pending',
          'local_confirmed',
          'approved',
          'rejected',
          'expired',
          'repair_required',
        }.contains(status) ||
        outcome.keys.any(
          (key) =>
              !(status == 'approved'
                      ? {'status', 'client_id', 'token'}
                      : {'status'})
                  .contains(key),
        )) {
      throw const FormatException('Invalid pairing outcome');
    }
    final clientId = status == 'approved' ? text(outcome, 'client_id') : null;
    final token = status == 'approved'
        ? text(outcome, 'token', maximum: 8192)
        : null;
    if (clientId != null && clientId != pairingId) {
      throw const FormatException('Pairing identity mismatch');
    }
    final fingerprint = value['issuer_fingerprint'];
    if (fingerprint != null &&
        (fingerprint is! String ||
            fingerprint.isEmpty ||
            fingerprint.length > 256)) {
      throw const FormatException('Invalid pairing issuer');
    }
    return RemotePairingStatus(
      operationId: operationId,
      pairingId: pairingId,
      status: status,
      personId: personId,
      deviceId: deviceId,
      producer: value['producer'] == null
          ? null
          : RemoteProducerIdentity.fromJson(value['producer']),
      issuer: value['issuer'] == null
          ? null
          : RemoteOwnerPublicKey.fromJson(value['issuer']),
      issuerFingerprint: fingerprint as String?,
      clientId: clientId,
      token: token,
    );
  }

  @override
  String toString() => 'RemotePairingStatus($status, credential: [REDACTED])';
}

final class RemoteEnrollmentStatus {
  const RemoteEnrollmentStatus({
    required this.enrollmentId,
    required this.keyId,
    required this.fingerprint,
    required this.localConfirmed,
    required this.adminApproved,
    required this.active,
  });

  final String enrollmentId;
  final String keyId;
  final String fingerprint;
  final bool localConfirmed;
  final bool adminApproved;
  final bool active;

  factory RemoteEnrollmentStatus.fromJson(Object? raw) {
    if (raw is! Map) throw const FormatException('Invalid enrollment status');
    final value = Map<String, Object?>.from(raw);
    const fields = {
      'enrollment_id',
      'key_id',
      'fingerprint',
      'local_confirmed',
      'admin_approved',
      'active',
    };
    if (value.keys.any((key) => !fields.contains(key))) {
      throw const FormatException('Invalid enrollment status');
    }
    final enrollmentId = value['enrollment_id'];
    final keyId = value['key_id'];
    final fingerprint = value['fingerprint'];
    if (enrollmentId is! String ||
        enrollmentId.isEmpty ||
        keyId is! String ||
        keyId.isEmpty ||
        fingerprint is! String ||
        fingerprint.isEmpty ||
        value['local_confirmed'] is! bool ||
        value['admin_approved'] is! bool ||
        value['active'] is! bool) {
      throw const FormatException('Invalid enrollment status');
    }
    return RemoteEnrollmentStatus(
      enrollmentId: enrollmentId,
      keyId: keyId,
      fingerprint: fingerprint,
      localConfirmed: value['local_confirmed'] as bool,
      adminApproved: value['admin_approved'] as bool,
      active: value['active'] as bool,
    );
  }
}

final class RemoteProducerInspection {
  const RemoteProducerInspection({
    required this.producer,
    this.ownerFingerprint,
  });

  final RemoteProducerIdentity producer;
  final String? ownerFingerprint;
}

final class ObserveAuthority {
  const ObserveAuthority({required this.incarnation, required this.epoch});

  final String incarnation;
  final int epoch;

  factory ObserveAuthority.fromJson(Object? raw, String field) {
    if (raw is! Map) throw const FormatException('Invalid observe bundle');
    final value = Map<String, Object?>.from(raw);
    if (value.keys.any((key) => !{'incarnation', 'epoch'}.contains(key))) {
      throw const FormatException('Invalid observe bundle');
    }
    return ObserveAuthority(
      incarnation: _uuid(value['incarnation'], field),
      epoch: _epoch(value['epoch'], field),
    );
  }

  Map<String, Object> toJson() => {'incarnation': incarnation, 'epoch': epoch};
}

final class ObserveGrantAuthority {
  const ObserveGrantAuthority({
    required this.incarnation,
    required this.accessEpoch,
  });

  final String incarnation;
  final int accessEpoch;

  factory ObserveGrantAuthority.fromJson(Object? raw, String field) {
    if (raw is! Map) throw const FormatException('Invalid observe bundle');
    final value = Map<String, Object?>.from(raw);
    if (value.keys.any(
      (key) => !{'incarnation', 'access_epoch'}.contains(key),
    )) {
      throw const FormatException('Invalid observe bundle');
    }
    return ObserveGrantAuthority(
      incarnation: _uuid(value['incarnation'], field),
      accessEpoch: _epoch(value['access_epoch'], field),
    );
  }

  Map<String, Object> toJson() => {
    'incarnation': incarnation,
    'access_epoch': accessEpoch,
  };
}

String _uuid(Object? raw, String field) {
  if (raw is! String || !_uuidPattern.hasMatch(raw)) {
    throw FormatException('Invalid observe bundle: $field');
  }
  return raw;
}

int _epoch(Object? raw, String field) {
  if (raw is! int || raw <= 0) {
    throw FormatException('Invalid observe bundle: $field');
  }
  return raw;
}

final _uuidPattern = RegExp(
  r'^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$',
);

final class ConnectionObserveMember {
  const ConnectionObserveMember({
    required this.viewId,
    required this.policyFingerprint,
    required this.resource,
    required this.producerFingerprint,
    required this.sourceAuthority,
    this.connectionRevision,
    required this.providerIdentity,
    required this.recipient,
    this.expectedGrantId,
    this.expectedGrantAuthority,
    this.expectedPolicy,
  });

  final String viewId;
  final String policyFingerprint;
  final String resource;
  final String producerFingerprint;
  final ObserveAuthority sourceAuthority;
  final int? connectionRevision;
  final String providerIdentity;
  final String recipient;
  final String? expectedGrantId;
  final ObserveGrantAuthority? expectedGrantAuthority;
  final ObserveAuthority? expectedPolicy;

  factory ConnectionObserveMember.fromJson(Object? raw) {
    if (raw is! Map) throw const FormatException('Invalid observe member');
    final value = Map<String, Object?>.from(raw);
    const fields = {
      'view_id',
      'policy_fingerprint',
      'resource',
      'producer_fingerprint',
      'source_authority',
      'connection_revision',
      'provider_identity',
      'recipient',
      'expected_grant_id',
      'expected_grant_authority',
      'expected_policy',
    };
    if (value.keys.any((key) => !fields.contains(key))) {
      throw const FormatException('Invalid observe member');
    }
    String text(String key) {
      final item = value[key];
      if (item is! String || item.isEmpty || item.length > 256) {
        throw const FormatException('Invalid observe member');
      }
      return item;
    }

    final revision = value['connection_revision'];
    if (revision != null && (revision is! int || revision <= 0)) {
      throw const FormatException('Invalid observe member');
    }
    final grantId = value['expected_grant_id'];
    final grantAuthority = value['expected_grant_authority'];
    final policy = value['expected_policy'];
    final coherent =
        (grantId == null && grantAuthority == null && policy == null) ||
        (grantId != null && grantAuthority != null && policy != null);
    if (!coherent) throw const FormatException('Invalid observe member');
    final policyFingerprint = text('policy_fingerprint');
    if (!RegExp(r'^[0-9a-f]{64}$').hasMatch(policyFingerprint)) {
      throw const FormatException('Invalid observe member');
    }
    return ConnectionObserveMember(
      viewId: text('view_id'),
      policyFingerprint: policyFingerprint,
      resource: text('resource'),
      producerFingerprint: text('producer_fingerprint'),
      sourceAuthority: ObserveAuthority.fromJson(
        value['source_authority'],
        'source_authority',
      ),
      connectionRevision: revision as int?,
      providerIdentity: text('provider_identity'),
      recipient: text('recipient'),
      expectedGrantId: grantId == null
          ? null
          : _uuid(grantId, 'expected_grant_id'),
      expectedGrantAuthority: grantAuthority == null
          ? null
          : ObserveGrantAuthority.fromJson(
              grantAuthority,
              'expected_grant_authority',
            ),
      expectedPolicy: policy == null
          ? null
          : ObserveAuthority.fromJson(policy, 'expected_policy'),
    );
  }

  Map<String, Object?> toJson() => {
    'view_id': viewId,
    'policy_fingerprint': policyFingerprint,
    'resource': resource,
    'producer_fingerprint': producerFingerprint,
    'source_authority': sourceAuthority.toJson(),
    'connection_revision': connectionRevision,
    'provider_identity': providerIdentity,
    'recipient': recipient,
    'expected_grant_id': expectedGrantId,
    'expected_grant_authority': expectedGrantAuthority?.toJson(),
    'expected_policy': expectedPolicy?.toJson(),
  };
}

final class ConnectionObserveBundle {
  const ConnectionObserveBundle({required this.members});

  final List<ConnectionObserveMember> members;

  factory ConnectionObserveBundle.fromJson(Object? raw) {
    if (raw is! Map) throw const FormatException('Invalid observe bundle');
    final value = Map<String, Object?>.from(raw);
    if (value.keys.any((key) => key != 'members')) {
      throw const FormatException('Invalid observe bundle');
    }
    final members = value['members'];
    if (members is! List || members.isEmpty || members.length > 8) {
      throw const FormatException('Invalid observe bundle');
    }
    final parsed = members.map(ConnectionObserveMember.fromJson).toList();
    for (var index = 1; index < parsed.length; index++) {
      if (parsed[index - 1].viewId.compareTo(parsed[index].viewId) >= 0) {
        throw const FormatException('Invalid observe bundle');
      }
    }
    return ConnectionObserveBundle(members: List.unmodifiable(parsed));
  }

  Map<String, Object> toJson() => {
    'members': members.map((member) => member.toJson()).toList(),
  };
}
