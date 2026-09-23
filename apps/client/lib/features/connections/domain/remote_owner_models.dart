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

final class RemoteCalendarGrantPreview {
  const RemoteCalendarGrantPreview({
    required this.connectorId,
    required this.connectionId,
    required this.resource,
    required this.sourceAuthority,
    required this.providerIdentity,
    required this.executionOwner,
    required this.producer,
    required this.consumers,
    required this.recipient,
  });

  final String connectorId;
  final String connectionId;
  final String resource;
  final Map<String, Object?> sourceAuthority;
  final String providerIdentity;
  final String executionOwner;
  final RemoteProducerIdentity producer;
  final List<String> consumers;
  final String recipient;

  factory RemoteCalendarGrantPreview.fromJson(Object? raw) {
    if (raw is! Map) {
      throw const FormatException('Invalid calendar grant preview');
    }
    final value = Map<String, Object?>.from(raw);
    if (value['schema_version'] != 1 ||
        value['connector_id'] is! String ||
        value['connection_id'] is! String ||
        value['resource'] is! String ||
        value['provider_identity'] is! String ||
        value['execution_owner'] is! String ||
        value['consumers'] is! List ||
        value['recipient'] is! String ||
        value['source_authority'] is! Map) {
      throw const FormatException('Invalid calendar grant preview');
    }
    return RemoteCalendarGrantPreview(
      connectorId: value['connector_id'] as String,
      connectionId: value['connection_id'] as String,
      resource: value['resource'] as String,
      sourceAuthority: Map<String, Object?>.from(
        value['source_authority'] as Map,
      ),
      providerIdentity: value['provider_identity'] as String,
      executionOwner: value['execution_owner'] as String,
      producer: RemoteProducerIdentity.fromJson(value['producer']),
      consumers: List<String>.from(value['consumers'] as List),
      recipient: value['recipient'] as String,
    );
  }
}

final class RemoteCalendarGrantOverview {
  const RemoteCalendarGrantOverview({
    required this.grantId,
    required this.grantAuthority,
    required this.state,
    required this.connectorId,
    this.connectionId,
    required this.resource,
    required this.recipient,
  });

  final String grantId;
  final Map<String, Object?> grantAuthority;
  final String state;
  final String connectorId;
  final String? connectionId;
  final String resource;
  final String recipient;

  factory RemoteCalendarGrantOverview.fromJson(Object? raw) {
    if (raw is! Map) {
      throw const FormatException('Invalid calendar grant overview');
    }
    final value = Map<String, Object?>.from(raw);
    if (value['schema_version'] != 1 ||
        value['grant_id'] is! String ||
        value['grant_authority'] is! Map ||
        value['state'] is! String ||
        value['connector_id'] is! String ||
        (value['connection_id'] != null && value['connection_id'] is! String) ||
        value['resource'] is! String ||
        value['recipient'] is! String) {
      throw const FormatException('Invalid calendar grant overview');
    }
    return RemoteCalendarGrantOverview(
      grantId: value['grant_id'] as String,
      grantAuthority: Map<String, Object?>.from(
        value['grant_authority'] as Map,
      ),
      state: value['state'] as String,
      connectorId: value['connector_id'] as String,
      connectionId: value['connection_id'] as String?,
      resource: value['resource'] as String,
      recipient: value['recipient'] as String,
    );
  }
}

final class RemoteViewGrantPreview {
  const RemoteViewGrantPreview({
    required this.viewId,
    required this.connectorId,
    required this.connectionId,
    required this.connectionRevision,
    required this.resource,
    required this.sourceAuthority,
    required this.providerIdentity,
    required this.executionOwner,
    required this.producer,
    required this.consumer,
    required this.purpose,
    required this.recipient,
  });

  final String viewId;
  final String connectorId;
  final String connectionId;
  final int connectionRevision;
  final String resource;
  final Map<String, Object?> sourceAuthority;
  final String providerIdentity;
  final String executionOwner;
  final RemoteProducerIdentity producer;
  final String consumer;
  final String purpose;
  final String recipient;

  factory RemoteViewGrantPreview.fromJson(Object? raw) {
    if (raw is! Map) throw const FormatException('Invalid remote view preview');
    final value = Map<String, Object?>.from(raw);
    if (value['schema_version'] != 1 ||
        value['view_id'] is! String ||
        value['connector_id'] is! String ||
        value['connection_id'] is! String ||
        value['connection_revision'] is! int ||
        (value['connection_revision'] as int) <= 0 ||
        value['resource'] is! String ||
        value['source_authority'] is! Map ||
        value['provider_identity'] is! String ||
        value['execution_owner'] is! String ||
        value['consumer'] is! String ||
        value['purpose'] is! String ||
        value['recipient'] is! String) {
      throw const FormatException('Invalid remote view preview');
    }
    return RemoteViewGrantPreview(
      viewId: value['view_id'] as String,
      connectorId: value['connector_id'] as String,
      connectionId: value['connection_id'] as String,
      connectionRevision: value['connection_revision'] as int,
      resource: value['resource'] as String,
      sourceAuthority: Map<String, Object?>.from(
        value['source_authority'] as Map,
      ),
      providerIdentity: value['provider_identity'] as String,
      executionOwner: value['execution_owner'] as String,
      producer: RemoteProducerIdentity.fromJson(value['producer']),
      consumer: value['consumer'] as String,
      purpose: value['purpose'] as String,
      recipient: value['recipient'] as String,
    );
  }
}

final class RemoteViewGrantOverview {
  const RemoteViewGrantOverview({
    required this.grantId,
    required this.grantAuthority,
    required this.viewId,
    required this.connectorId,
    required this.connectionId,
    required this.connectionRevision,
    required this.resource,
    required this.sourceAuthority,
    required this.executionOwner,
    required this.state,
    required this.reviewRequired,
    required this.consumer,
    required this.purpose,
    required this.recipient,
  });

  final String grantId;
  final Map<String, Object?> grantAuthority;
  final String viewId;
  final String connectorId;
  final String connectionId;
  final int? connectionRevision;
  final String resource;
  final Map<String, Object?> sourceAuthority;
  final String executionOwner;
  final String state;
  final bool reviewRequired;
  final String consumer;
  final String purpose;
  final String recipient;

  factory RemoteViewGrantOverview.fromJson(Object? raw) {
    if (raw is! Map) throw const FormatException('Invalid remote view grant');
    final value = Map<String, Object?>.from(raw);
    if (value['schema_version'] != 1 ||
        value['grant_id'] is! String ||
        value['grant_authority'] is! Map ||
        value['view_id'] is! String ||
        value['connector_id'] is! String ||
        value['connection_id'] is! String ||
        value['connection_revision'] != null &&
            (value['connection_revision'] is! int ||
                (value['connection_revision'] as int) <= 0) ||
        value['resource'] is! String ||
        value['source_authority'] is! Map ||
        value['execution_owner'] is! String ||
        value['state'] is! String ||
        value['review_required'] is! bool ||
        value['consumer'] is! String ||
        value['purpose'] is! String ||
        value['recipient'] is! String) {
      throw const FormatException('Invalid remote view grant');
    }
    return RemoteViewGrantOverview(
      grantId: value['grant_id'] as String,
      grantAuthority: Map<String, Object?>.from(
        value['grant_authority'] as Map,
      ),
      viewId: value['view_id'] as String,
      connectorId: value['connector_id'] as String,
      connectionId: value['connection_id'] as String,
      connectionRevision: value['connection_revision'] as int?,
      resource: value['resource'] as String,
      sourceAuthority: Map<String, Object?>.from(
        value['source_authority'] as Map,
      ),
      executionOwner: value['execution_owner'] as String,
      state: value['state'] as String,
      reviewRequired: value['review_required'] as bool,
      consumer: value['consumer'] as String,
      purpose: value['purpose'] as String,
      recipient: value['recipient'] as String,
    );
  }
}
