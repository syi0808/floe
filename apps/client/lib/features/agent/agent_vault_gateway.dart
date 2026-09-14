import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';

import '../../infrastructure/diagnostics/app_diagnostics.dart';
import 'agent_calendar_experts.dart';
import 'agent_connections.dart';
import 'agent_conversation_gateway.dart';
import 'agent_fixture_gateway.dart';
import 'agent_memory_review.dart';
import 'agent_memory.dart';
import 'agent_personal_access.dart';
import 'agent_proposal.dart';
import 'agent_registry.dart';
import 'agent_request_id.dart';

enum AgentVaultState { missing, locked, ready, unavailable }

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
    required this.schemaVersion,
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

  final int schemaVersion;
  final String pairingId;
  final String status;
  final String personId;
  final String deviceId;
  final RemoteProducerIdentity? producer;
  final RemoteOwnerPublicKey? issuer;
  final String? issuerFingerprint;
  final String? clientId;
  final String? token;

  factory RemotePairingStatus.fromJson(Object? raw) {
    if (raw is! Map) throw const FormatException('Invalid pairing status');
    final value = Map<String, Object?>.from(raw);
    const fields = {
      'schema_version',
      'pairing_id',
      'status',
      'person_id',
      'device_id',
      'producer',
      'issuer',
      'issuer_fingerprint',
      'client_id',
      'token',
    };
    if (value.keys.any((key) => !fields.contains(key)) ||
        value['schema_version'] != 1 ||
        value['pairing_id'] is! String ||
        value['status'] is! String ||
        value['person_id'] is! String ||
        value['device_id'] is! String ||
        !{
          'pending',
          'local_confirmed',
          'approved',
          'rejected',
          'expired',
          'repair_required',
        }.contains(value['status'])) {
      throw const FormatException('Invalid pairing status');
    }
    final producer = value['producer'] == null
        ? null
        : RemoteProducerIdentity.fromJson(value['producer']);
    final issuer = value['issuer'] == null
        ? null
        : RemoteOwnerPublicKey.fromJson(value['issuer']);
    final issuerFingerprint = value['issuer_fingerprint'];
    final clientId = value['client_id'];
    final token = value['token'];
    if (issuerFingerprint != null && issuerFingerprint is! String ||
        clientId != null && clientId is! String ||
        token != null && token is! String) {
      throw const FormatException('Invalid pairing status');
    }
    final status = value['status'] as String;
    if (clientId != null && clientId != value['pairing_id'] ||
        status == 'approved' && token == null ||
        status != 'approved' && token != null) {
      throw const FormatException('Invalid pairing status');
    }
    return RemotePairingStatus(
      schemaVersion: 1,
      pairingId: value['pairing_id'] as String,
      status: status,
      personId: value['person_id'] as String,
      deviceId: value['device_id'] as String,
      producer: producer,
      issuer: issuer,
      issuerFingerprint: issuerFingerprint as String?,
      clientId: clientId as String?,
      token: token as String?,
    );
  }
}

abstract interface class RemotePairingGateway {
  Future<RemoteOwnerPublicKey> prepareRemotePairing({required String personId});

  Future<RemotePairingStatus> confirmRemotePairing({
    required String personId,
    required Map<String, Object?> route,
    required RemotePairingChallenge challenge,
    required String pollingProof,
  });

  Future<RemotePairingStatus> remotePairingStatus({
    required String personId,
    required Map<String, Object?> route,
    required String pairingId,
    required String pollingProof,
  });

  Future<RemotePairingStatus> finalizeRemotePairing({
    required String personId,
    required Map<String, Object?> route,
    required String pairingId,
    required String pollingProof,
    required RemotePairingChallenge challenge,
  });
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
    required this.recipient,
  });

  final String connectorId;
  final String connectionId;
  final String resource;
  final Map<String, Object?> sourceAuthority;
  final String providerIdentity;
  final String executionOwner;
  final RemoteProducerIdentity producer;
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

class AgentVaultException implements Exception {
  const AgentVaultException(
    this.failure, {
    this.requestId,
    this.stage,
    this.metadata = const {},
    this.recoveryAction,
    this.affectedRefs = const [],
    this.correlationRequestId,
    this.retryableOverride,
    this.domain,
    this.category,
    this.reasonCode,
    this.safeActions = const [],
    this.incidentId,
    this.retryPolicy,
  });

  final String failure;
  final String? requestId;
  final String? stage;
  final Map<String, String> metadata;
  final String? recoveryAction;
  final List<String> affectedRefs;
  final String? correlationRequestId;
  final bool? retryableOverride;
  final String? domain;
  final String? category;
  final String? reasonCode;
  final List<String> safeActions;
  final String? incidentId;
  final String? retryPolicy;

  bool get retryable =>
      retryableOverride ??
      const {
        'model_unavailable',
        'local_model_unavailable',
        'server_model_unavailable',
        'server_model_timeout',
        'quota_exceeded',
        'stalled',
        'deadline_exceeded',
        'interrupted',
        'transport_unavailable',
      }.contains(failure);

  @override
  String toString() => requestId == null
      ? 'AgentVaultException($failure)'
      : 'AgentVaultException($failure, requestId: $requestId)';
}

abstract interface class AgentVaultGateway
    implements AgentFixtureStreamingGateway {
  Future<AgentVaultState> vaultStatus(String personId);
  Future<AgentVaultState> createVault(String personId);
  Future<AgentVaultState> unlockVault(String personId);
  Future<void> lockVault(String personId);
}

final class NativeAgentVaultGateway
    implements
        AgentVaultGateway,
        AgentRegistryGateway,
        AgentProposalGateway,
        AgentConversationGateway,
        AgentCalendarExpertGateway,
        AgentConnectionsGateway,
        AgentPersonalAccessGateway,
        AgentMemoryGateway,
        AgentMemoryReviewGateway,
        RemotePairingGateway {
  NativeAgentVaultGateway(
    this.request, {
    required this.deviceId,
    this.resolveRemoteRoute,
  });

  final Future<Map<String, dynamic>> Function(Map<String, Object?>) request;
  final String deviceId;
  final Future<Map<String, Object?>?> Function()? resolveRemoteRoute;
  _VaultJob? _pending;
  AgentSession? _run;
  AgentConversationTurnRequest? _conversationRun;

  @override
  Future<RemoteOwnerPublicKey> prepareRemotePairing({
    required String personId,
  }) async {
    final result = await _perform(personId, {'kind': 'remote_pairing_prepare'});
    return RemoteOwnerPublicKey.fromJson(result['remote_owner']);
  }

  @override
  Future<RemotePairingStatus> confirmRemotePairing({
    required String personId,
    required Map<String, Object?> route,
    required RemotePairingChallenge challenge,
    required String pollingProof,
  }) async {
    final result = await _perform(personId, {
      'kind': 'remote_pairing_confirm',
      'route': route,
      'challenge': challenge.toJson(),
      'polling_proof': pollingProof,
    });
    return _pairingResult(result, personId);
  }

  @override
  Future<RemotePairingStatus> remotePairingStatus({
    required String personId,
    required Map<String, Object?> route,
    required String pairingId,
    required String pollingProof,
  }) async {
    final result = await _perform(personId, {
      'kind': 'remote_pairing_status',
      'route': route,
      'pairing_id': pairingId,
      'polling_proof': pollingProof,
    });
    return _pairingResult(result, personId);
  }

  @override
  Future<RemotePairingStatus> finalizeRemotePairing({
    required String personId,
    required Map<String, Object?> route,
    required String pairingId,
    required String pollingProof,
    required RemotePairingChallenge challenge,
  }) async {
    final result = await _perform(personId, {
      'kind': 'remote_pairing_finalize',
      'route': route,
      'pairing_id': pairingId,
      'polling_proof': pollingProof,
      'challenge': challenge.toJson(),
    });
    return _pairingResult(result, personId);
  }

  RemotePairingStatus _pairingResult(
    Map<String, dynamic> result,
    String personId,
  ) {
    final status = RemotePairingStatus.fromJson(result['remote_pairing']);
    if (result['state'] != 'ready' || status.personId != personId) {
      throw const FormatException('Pairing identity mismatch');
    }
    return status;
  }

  @override
  Future<List<AgentConnection>> readConnections(String personId) async {
    final result = await _perform(personId, {'kind': 'connections'});
    final raw = result['connections'];
    if (raw is! List || raw.length > 64) {
      throw const FormatException('Invalid connection overview');
    }
    return List.unmodifiable(
      raw.map(
        (entry) =>
            AgentConnection.fromJson(Map<String, dynamic>.from(entry as Map)),
      ),
    );
  }

  @override
  Future<AgentMemoryOverview> readMemory(String personId) async {
    final result = await _perform(personId, {'kind': 'memory'});
    final memory = AgentMemoryOverview.fromJson(
      Map<String, Object?>.from(result['memory'] as Map),
    );
    if (result['state'] != 'ready' || memory.personId != personId) {
      throw const FormatException('Memory overview scope mismatch');
    }
    return memory;
  }

  Future<RemoteProducerInspection> inspectRemoteProducer({
    required String personId,
    required Map<String, Object?> route,
  }) async {
    final result = await _perform(personId, {
      'kind': 'remote_authority_inspect_producer',
      'route': route,
    });
    final owner = result['remote_owner'];
    final ownerMap = owner is Map ? Map<String, Object?>.from(owner) : null;
    return RemoteProducerInspection(
      producer: RemoteProducerIdentity.fromJson(result['remote_producer']),
      ownerFingerprint: ownerMap?['fingerprint'] as String?,
    );
  }

  Future<RemoteEnrollmentStatus> reviewAndEnrollRemoteProducer({
    required String personId,
    required Map<String, Object?> route,
    required RemoteProducerIdentity producer,
  }) async {
    final result = await _perform(personId, {
      'kind': 'remote_authority_review_and_enroll',
      'route': route,
      'producer': producer.toJson(),
    });
    return RemoteEnrollmentStatus.fromJson(result['remote_enrollment']);
  }

  Future<RemoteEnrollmentStatus> remoteEnrollmentStatus({
    required String personId,
    required Map<String, Object?> route,
    required String enrollmentId,
  }) async {
    final result = await _perform(personId, {
      'kind': 'remote_authority_enrollment_status',
      'route': route,
      'enrollment_id': enrollmentId,
    });
    return RemoteEnrollmentStatus.fromJson(result['remote_enrollment']);
  }

  Future<RemoteCalendarGrantPreview> previewRemoteCalendarGrant({
    required String personId,
    required Map<String, Object?> route,
    required String connectorId,
    required String connectionId,
    required String resource,
  }) async {
    final result = await _perform(personId, {
      'kind': 'remote_calendar_grant_preview',
      'route': route,
      'connector_id': connectorId,
      'connection_id': connectionId,
      'resource': resource,
    });
    return RemoteCalendarGrantPreview.fromJson(
      result['remote_calendar_preview'],
    );
  }

  Future<RemoteCalendarGrantOverview> reviewRemoteCalendarGrant({
    required String personId,
    required Map<String, Object?> route,
    required String connectorId,
    required String connectionId,
    required String resource,
    required String expectedProducerFingerprint,
  }) async {
    final result = await _perform(personId, {
      'kind': 'remote_calendar_grant_review',
      'route': route,
      'connector_id': connectorId,
      'connection_id': connectionId,
      'resource': resource,
      'expected_producer_fingerprint': expectedProducerFingerprint,
    });
    return RemoteCalendarGrantOverview.fromJson(
      result['remote_calendar_grant'],
    );
  }

  Future<RemoteCalendarGrantOverview> pauseRemoteCalendarGrant({
    required String personId,
    required String grantId,
    required Map<String, Object?> expectedAuthority,
  }) async {
    final result = await _perform(personId, {
      'kind': 'remote_calendar_grant_pause',
      'grant_id': grantId,
      'expected_authority': expectedAuthority,
    });
    return RemoteCalendarGrantOverview.fromJson(
      result['remote_calendar_grant'],
    );
  }

  Future<RemoteViewGrantPreview> previewRemoteViewGrant({
    required String personId,
    required Map<String, Object?> route,
    required String viewId,
    required String connectorId,
    required String connectionId,
    required String resource,
    required String consumer,
  }) async {
    final result = await _perform(personId, {
      'kind': 'remote_view_grant_preview',
      'route': route,
      'view_id': viewId,
      'connector_id': connectorId,
      'connection_id': connectionId,
      'resource': resource,
      'consumer': consumer,
    });
    return RemoteViewGrantPreview.fromJson(result['remote_view_preview']);
  }

  Future<RemoteViewGrantOverview> reviewRemoteViewGrant({
    required String personId,
    required Map<String, Object?> route,
    required String viewId,
    required String connectorId,
    required String connectionId,
    required String resource,
    required String consumer,
    required String expectedProducerFingerprint,
    required Map<String, Object?> expectedSourceAuthority,
    required int expectedConnectionRevision,
    required String expectedProviderIdentity,
    required String expectedRecipient,
  }) async {
    final result = await _perform(personId, {
      'kind': 'remote_view_grant_review',
      'route': route,
      'view_id': viewId,
      'connector_id': connectorId,
      'connection_id': connectionId,
      'resource': resource,
      'consumer': consumer,
      'expected_producer_fingerprint': expectedProducerFingerprint,
      'expected_source_authority': expectedSourceAuthority,
      'expected_connection_revision': expectedConnectionRevision,
      'expected_provider_identity': expectedProviderIdentity,
      'expected_recipient': expectedRecipient,
    });
    return RemoteViewGrantOverview.fromJson(result['remote_view_grant']);
  }

  Future<RemoteViewGrantOverview> pauseRemoteViewGrant({
    required String personId,
    required String grantId,
    required Map<String, Object?> expectedAuthority,
  }) async {
    final result = await _perform(personId, {
      'kind': 'remote_view_grant_pause',
      'grant_id': grantId,
      'expected_authority': expectedAuthority,
    });
    return RemoteViewGrantOverview.fromJson(result['remote_view_grant']);
  }

  @override
  Future<AgentMemoryReviewOverview> readMemoryReview(String personId) =>
      _memoryReview(personId, null);

  @override
  Future<AgentMemoryReviewOverview> decideMemoryCandidate({
    required String personId,
    required String candidateId,
    required AgentMemoryDecision decision,
  }) => _memoryReview(personId, {
    'candidate_id': candidateId,
    'decision': decision.name,
  });

  Future<AgentMemoryReviewOverview> _memoryReview(
    String personId,
    Map<String, Object?>? decision,
  ) async {
    final result = await _perform(personId, {
      'kind': 'memory_review',
      'decision': decision,
    });
    final review = AgentMemoryReviewOverview.fromJson(
      Map<String, Object?>.from(result['memory_review'] as Map),
    );
    if (result['state'] != 'ready' || review.personId != personId) {
      throw const FormatException('Memory review scope mismatch');
    }
    return review;
  }

  @override
  Future<AgentSession> startConversation(String personId) =>
      _conversationSession(personId, {'kind': 'start'});

  @override
  Future<AgentSession> resumeConversation(String personId) =>
      _conversationSession(personId, {'kind': 'resume'});

  @override
  Future<AgentSession> loadConversation(String personId, String sessionId) =>
      _conversationSession(personId, {'kind': 'get', 'session_id': sessionId});

  @override
  Future<AgentSession> recoverConversation(AgentSession session) =>
      _conversationSession(session.personId, {
        'kind': 'recover',
        'session_id': session.id,
        'expected_revision': session.revision,
      });

  Future<AgentSession> _conversationSession(
    String personId,
    Map<String, Object?> operation,
  ) async {
    final result = await _perform(personId, {
      'kind': 'conversation_session',
      'operation': operation,
    });
    final session = AgentSession.fromJson(
      Map<String, Object?>.from(result['session'] as Map),
    );
    if (result['state'] != 'ready' ||
        session.personId != personId ||
        session.scope != null ||
        session.dataClasses.singleOrNull != 'personal') {
      throw const FormatException('Conversation session mismatch');
    }
    return session;
  }

  @override
  Future<AgentRunUpdate> beginConversationTurn(
    AgentConversationTurnRequest turn,
  ) async {
    if (_conversationRun != null &&
        !_sameConversationTurn(_conversationRun!, turn)) {
      throw const AgentVaultException('conflict');
    }
    Map<String, Object?>? remoteRoute;
    if (resolveRemoteRoute != null) {
      remoteRoute = await resolveRemoteRoute!();
    }
    if (_conversationRun != null &&
        !_sameConversationTurn(_conversationRun!, turn)) {
      throw const AgentVaultException('conflict');
    }
    if (_conversationRun == null) {
      if (_pending != null) await _drain();
      _pending = _VaultJob(
        turn.session.personId,
        newAgentRequestId(),
        'conversation_turn',
      );
      _run = turn.session;
      _conversationRun = turn;
    }
    final serialized = turn.toJson();
    serialized['device_id'] = deviceId;
    if (resolveRemoteRoute != null) serialized['remote_route'] = remoteRoute;
    return _conversationUpdate(
      turn,
      await _call(_pending!, {
        'kind': 'submit',
        'action': {'kind': 'conversation_turn', 'request': serialized},
      }),
    );
  }

  @override
  Future<AgentRunUpdate> pollConversationTurn(
    AgentConversationTurnRequest turn,
    int afterSequence,
  ) => _conversationCall(turn, {
    'kind': 'poll',
    'after_sequence': afterSequence,
  });

  @override
  Future<AgentRunUpdate> stopConversationTurn(
    AgentConversationTurnRequest turn,
  ) => _conversationCall(turn, {'kind': 'stop'});

  @override
  Future<AgentRunUpdate> releaseConversationTurn(
    AgentConversationTurnRequest turn,
  ) async {
    final result = await _conversationCall(turn, {'kind': 'release'});
    _pending = null;
    _run = null;
    _conversationRun = null;
    return result;
  }

  Future<AgentRunUpdate> _conversationCall(
    AgentConversationTurnRequest turn,
    Map<String, Object?> operation,
  ) async {
    if (_conversationRun == null ||
        !_sameConversationTurn(_conversationRun!, turn)) {
      throw const AgentVaultException('conflict');
    }
    return _conversationUpdate(turn, await _call(_pending!, operation));
  }

  AgentRunUpdate _conversationUpdate(
    AgentConversationTurnRequest turn,
    Map<String, dynamic> result,
  ) {
    final failure = _failureUpdateFields(result['failure'], _pending!);
    return AgentRunUpdate.fromJson({
      ...result,
      ...failure,
      'session_id': turn.session.id,
      'expected_revision': turn.session.revision,
    });
  }

  bool _sameConversationTurn(
    AgentConversationTurnRequest left,
    AgentConversationTurnRequest right,
  ) =>
      identical(left, right) ||
      left.session.personId == right.session.personId &&
          jsonEncode(left.toJson()) == jsonEncode(right.toJson());

  @override
  Future<AgentProposalInspection> inspectProposal({
    required String personId,
    required String sessionId,
    required String invocationId,
  }) async {
    final result = await _perform(personId, {
      'kind': 'inspect_proposal',
      'session_id': sessionId,
      'invocation_id': invocationId,
    });
    final inspection = AgentProposalInspection.fromJson(
      Map<String, dynamic>.from(result['proposal'] as Map),
    );
    if (result['state'] != 'ready' ||
        inspection.personId != personId ||
        inspection.sessionId != sessionId ||
        inspection.invocationId != invocationId) {
      throw const FormatException('Proposal inspection scope mismatch');
    }
    return inspection;
  }

  @override
  Future<AgentCalendarExperts> readCalendarExperts(String personId) async {
    final result = await _perform(personId, {
      'kind': 'calendar_experts',
      'setup': null,
    });
    return _calendarExperts(personId, result);
  }

  @override
  Future<CalendarSubjectPreview> previewCalendarSubject(
    CalendarSubjectPreviewRequest request,
  ) async {
    if (request.deviceId != deviceId) {
      throw const FormatException('Calendar preview device mismatch');
    }
    final result = await _perform(request.personId, {
      'kind': 'calendar_subject_preview',
      'request': request.toJson(),
    });
    if (result['state'] != 'ready' ||
        result['calendar_subject_preview'] is! Map) {
      throw const FormatException('Missing Calendar subject preview');
    }
    final preview = CalendarSubjectPreview.fromJson(
      result['calendar_subject_preview'],
    );
    final expectedCalendarIds = [...request.calendarIds]..sort();
    if (preview.provider != request.provider ||
        preview.deviceId != request.deviceId ||
        preview.connectionId != request.connectionId ||
        preview.connectionScope != request.connectionScope ||
        !listEquals(preview.calendarIds, expectedCalendarIds) ||
        preview.sourceAuthority != request.sourceAuthority) {
      throw const FormatException('Calendar preview identity mismatch');
    }
    return preview;
  }

  @override
  Future<AgentCalendarExperts> installCalendarExpert(
    AgentCalendarSetup setup,
  ) async {
    if (setup.deviceId != deviceId) {
      throw const FormatException('Calendar setup device mismatch');
    }
    final serialized = setup.toJson();
    final result = await _perform(setup.personId, {
      'kind': 'calendar_experts',
      'setup': serialized,
    });
    final overview = _calendarExperts(setup.personId, result);
    if (overview.receiptFor(setup) == null) {
      throw const FormatException('Missing Calendar setup receipt');
    }
    return overview;
  }

  @override
  Future<AgentCalendarExperts> configureCalendarAccess(
    AgentCalendarAccessRequest request,
  ) async {
    if (request.operation == AgentCalendarAccessOperation.setScope &&
        request.deviceId != deviceId) {
      throw const FormatException('Calendar scope device mismatch');
    }
    final serialized = request.toJson();
    final result = await _perform(request.personId, {
      'kind': 'calendar_access',
      'change': serialized,
    });
    return _calendarExperts(request.personId, result);
  }

  AgentCalendarExperts _calendarExperts(
    String personId,
    Map<String, dynamic> result,
  ) {
    final overview = AgentCalendarExperts.fromJson(
      Map<String, dynamic>.from(result['calendar_experts'] as Map),
    );
    if (overview.registry.personId != personId || result['state'] != 'ready') {
      throw const FormatException('Calendar Expert Person or vault mismatch');
    }
    return overview;
  }

  @override
  Future<PersonalAccessOverview> inspectPersonalAttention(
    String personId,
  ) async {
    return _personalAccess(personId, {'kind': 'inspect'});
  }

  @override
  Future<PersonalAccessOverview> reviewPersonalAttention(
    String personId, {
    required PersonalAccessOverview reviewedPreview,
    required List<String> consumers,
  }) async {
    final fingerprint = reviewedPreview.nativeSubjectFingerprint;
    if (fingerprint == null) {
      throw const FormatException('Attention preview unavailable');
    }
    if (reviewedPreview.personId != personId ||
        reviewedPreview.deviceId != deviceId ||
        consumers.isEmpty ||
        consumers.toSet().length != consumers.length) {
      throw const FormatException('Attention review scope changed');
    }
    return _personalAccess(personId, {
      'kind': 'review',
      'expected_native_subject_fingerprint': fingerprint,
      'consumers': List<String>.unmodifiable(consumers),
      'expected_grant_id': reviewedPreview.grantId,
      'expected_grant_authority': reviewedPreview.grantAuthority,
    });
  }

  @override
  Future<PersonalAccessOverview> setPersonalAttentionEnabled(
    String personId,
    bool enabled,
  ) async {
    return _personalAccess(personId, {
      'kind': 'set_enabled',
      'enabled': enabled,
    });
  }

  @override
  Future<PersonalAccessOverview> inspectPersonalFeasibility(
    String personId,
  ) async {
    return _personalAccess(personId, {
      'kind': 'inspect',
    }, connector: 'feasibility.apple');
  }

  @override
  Future<PersonalAccessOverview> reviewPersonalFeasibility(
    String personId, {
    required PersonalFeasibilityQuery query,
    required PersonalAccessOverview reviewedPreview,
    required List<String> consumers,
  }) async {
    final fingerprint = reviewedPreview.nativeSubjectFingerprint;
    if (fingerprint == null ||
        reviewedPreview.personId != personId ||
        reviewedPreview.deviceId != deviceId ||
        consumers.isEmpty ||
        consumers.toSet().length != consumers.length) {
      throw const FormatException('Feasibility review scope changed');
    }
    return _personalAccess(personId, {
      'kind': 'review',
      'expected_native_subject_fingerprint': fingerprint,
      'consumers': List<String>.unmodifiable(consumers),
      'expected_grant_id': reviewedPreview.grantId,
      'expected_grant_authority': reviewedPreview.grantAuthority,
      'feasibility_query': query.toJson(),
    }, connector: 'feasibility.apple');
  }

  @override
  Future<PersonalAccessOverview> setPersonalFeasibilityEnabled(
    String personId,
    bool enabled,
  ) async {
    return _personalAccess(personId, {
      'kind': 'set_enabled',
      'enabled': enabled,
    }, connector: 'feasibility.apple');
  }

  @override
  Future<PersonalAccessOverview> inspectPersonalWellbeing(
    String personId,
  ) async {
    return _personalAccess(personId, {
      'kind': 'inspect',
    }, connector: 'health.apple');
  }

  @override
  Future<PersonalAccessOverview> reviewPersonalWellbeing(
    String personId, {
    required PersonalAccessOverview reviewedPreview,
    required String nativeSubjectFingerprint,
  }) async {
    if (reviewedPreview.personId != personId ||
        reviewedPreview.deviceId != deviceId ||
        !RegExp(r'^[0-9a-f]{64}$').hasMatch(nativeSubjectFingerprint)) {
      throw const FormatException('Wellbeing review scope changed');
    }
    return _personalAccess(personId, {
      'kind': 'review',
      'expected_native_subject_fingerprint': nativeSubjectFingerprint,
      'consumers': const ['assistant'],
      'expected_grant_id': reviewedPreview.grantId,
      'expected_grant_authority': reviewedPreview.grantAuthority,
    }, connector: 'health.apple');
  }

  @override
  Future<PersonalAccessOverview> setPersonalWellbeingEnabled(
    String personId,
    bool enabled,
  ) async {
    return _personalAccess(personId, {
      'kind': 'set_enabled',
      'enabled': enabled,
    }, connector: 'health.apple');
  }

  @override
  Future<PersonalAccessOverview> inspectPersonalContacts(
    String personId,
    List<String> selectedHandles,
  ) async {
    final handles = _canonicalContactHandles(selectedHandles);
    return _personalContacts(personId, {
      'kind': 'inspect',
      'selected_handles': handles,
    });
  }

  @override
  Future<PersonalAccessOverview> reviewPersonalContacts(
    String personId, {
    required List<String> selectedHandles,
    required PersonalAccessOverview reviewedPreview,
    required List<String> consumers,
  }) async {
    final fingerprint = reviewedPreview.nativeSubjectFingerprint;
    if (fingerprint == null ||
        reviewedPreview.personId != personId ||
        reviewedPreview.deviceId != deviceId ||
        consumers.isEmpty ||
        consumers.toSet().length != consumers.length) {
      throw const FormatException('Contacts review scope changed');
    }
    return _personalContacts(personId, {
      'kind': 'review',
      'selected_handles': _canonicalContactHandles(selectedHandles),
      'expected_native_subject_fingerprint': fingerprint,
      'consumers': List<String>.unmodifiable(consumers),
      'expected_grant_id': reviewedPreview.grantId,
      'expected_grant_authority': reviewedPreview.grantAuthority,
    });
  }

  Future<PersonalAccessOverview> _personalAccess(
    String personId,
    Map<String, Object?> change, {
    String connector = 'attention.macos',
  }) async {
    final result = await _perform(personId, {
      'kind': 'personal_access',
      'change': {
        'connector': connector,
        'device_id': deviceId,
        'change': change,
      },
    });
    if (result['state'] != 'ready' || result['personal_access'] is! Map) {
      throw const FormatException('Missing personal access overview');
    }
    final overview = PersonalAccessOverview.fromJson(result['personal_access']);
    if (overview.personId != personId || overview.deviceId != deviceId) {
      throw const FormatException('Personal access scope mismatch');
    }
    return overview;
  }

  Future<PersonalAccessOverview> _personalContacts(
    String personId,
    Map<String, Object?> change,
  ) async {
    final result = await _perform(personId, {
      'kind': 'contacts_access',
      'change': {
        'connector': 'contacts.$platformContactsConnector',
        'device_id': deviceId,
        'change': change,
      },
    });
    if (result['state'] != 'ready' || result['personal_access'] is! Map) {
      throw const FormatException('Missing Contacts access overview');
    }
    final overview = PersonalAccessOverview.fromJson(result['personal_access']);
    if (overview.personId != personId || overview.deviceId != deviceId) {
      throw const FormatException('Contacts access scope mismatch');
    }
    return overview;
  }

  String get platformContactsConnector => Platform.isAndroid
      ? 'android'
      : Platform.isIOS
      ? 'apple'
      : 'unsupported';

  List<String> _canonicalContactHandles(List<String> handles) {
    final value = handles.toSet().toList()..sort();
    if (value.isEmpty ||
        value.length > 64 ||
        value.length != handles.length ||
        value.any(
          (handle) => handle.isEmpty || handle.contains(RegExp(r'\s')),
        )) {
      throw const FormatException('Invalid Contacts selection');
    }
    return List.unmodifiable(value);
  }

  @override
  Future<AgentRegistryView?> readRegistry(String personId) async {
    final result = await _perform(personId, {
      'kind': 'registry',
      'change': null,
    });
    final raw = result['registry'];
    if (raw == null) return null;
    final overview = AgentRegistryView.fromJson(
      Map<String, dynamic>.from(raw as Map),
    );
    if (overview.personId != personId) {
      throw const FormatException('Registry Person mismatch');
    }
    return overview;
  }

  @override
  Future<AgentRegistryView> configureRegistry(
    AgentRegistryView current, {
    required AgentRegistryTarget target,
    required String id,
    required bool enabled,
  }) async {
    final result = await _perform(current.personId, {
      'kind': 'registry',
      'change': {
        'instance_id': current.instanceId,
        'expected_revision': current.revision,
        'target': {'kind': target.wireName, 'id': id, 'enabled': enabled},
      },
    });
    final overview = AgentRegistryView.fromJson(
      Map<String, dynamic>.from(result['registry'] as Map),
    );
    if (overview.personId != current.personId ||
        overview.instanceId != current.instanceId ||
        overview.revision != current.revision + 1) {
      throw const FormatException('Registry configuration mismatch');
    }
    return overview;
  }

  @override
  Future<AgentVaultState> vaultStatus(String personId) =>
      _access(personId, 'status');
  @override
  Future<AgentVaultState> createVault(String personId) =>
      _access(personId, 'create');
  @override
  Future<AgentVaultState> unlockVault(String personId) =>
      _access(personId, 'unlock');
  @override
  Future<void> lockVault(String personId) async {
    await _access(personId, 'lock');
  }

  Future<AgentVaultState> _access(String personId, String kind) async {
    final result = await _perform(personId, {'kind': kind});
    return AgentVaultState.values.byName(result['state'] as String);
  }

  @override
  Future<AgentFixtureResult> startAgentFixture(String personId) =>
      _session(personId, {'kind': 'start'});
  @override
  Future<AgentFixtureResult> resumeAgentFixture(String personId) =>
      _session(personId, {'kind': 'resume'});
  @override
  Future<AgentFixtureResult> loadAgentFixture(
    String personId,
    String sessionId,
  ) => _session(personId, {'kind': 'get', 'session_id': sessionId});
  @override
  Future<AgentFixtureResult> recoverAgentFixture(AgentSession session) =>
      _session(session.personId, {
        'kind': 'recover',
        'session_id': session.id,
        'expected_revision': session.revision,
      });
  @override
  Future<AgentFixtureResult> runAgentFixture(
    AgentSession session,
    AgentFixturePrompt prompt,
  ) => _session(session.personId, _turn(session, prompt));

  Future<AgentFixtureResult> _session(
    String personId,
    Map<String, Object?> operation,
  ) async {
    final result = await _perform(personId, {
      'kind': 'session',
      'operation': operation,
    });
    final parsed = AgentFixtureResult.fromJson({
      'session': result['session'],
      'events': result['events'],
    });
    if (parsed.session.scope != null) {
      throw const FormatException('Calendar session is not a sample');
    }
    return parsed;
  }

  Future<Map<String, dynamic>> _perform(
    String personId,
    Map<String, Object?> action,
  ) async {
    if (_pending != null && _pending!.personId != personId) {
      throw const AgentVaultException('conflict');
    }
    if (_pending != null) await _drain();
    final job = _VaultJob(
      personId,
      newAgentRequestId(),
      action['kind']?.toString() ?? 'unknown',
    );
    _pending = job;
    final result = await _finish(
      await _call(job, {'kind': 'submit', 'action': action}),
    );
    await _release(job);
    if (result['failure'] != null) {
      throw _failureException(result['failure'], job);
    }
    return result;
  }

  Future<void> _drain() async {
    final job = _pending!;
    try {
      if (_run != null) await _call(job, {'kind': 'stop'});
      await _finish(await _call(job, {'kind': 'poll', 'after_sequence': 0}));
      await _release(job);
    } on AgentVaultException catch (error) {
      if (error.failure != 'not_found') rethrow;
      _pending = null;
      _run = null;
      _conversationRun = null;
    }
  }

  Future<Map<String, dynamic>> _finish(Map<String, dynamic> result) async {
    final elapsed = Stopwatch()..start();
    while (result['done'] != true) {
      if (elapsed.elapsed > const Duration(seconds: 35)) {
        throw const AgentVaultException('deadline_exceeded');
      }
      await Future<void>.delayed(const Duration(milliseconds: 80));
      result = await _call(_pending!, {'kind': 'poll', 'after_sequence': 0});
    }
    return result;
  }

  @override
  Future<AgentRunUpdate> beginAgentFixtureRun(
    AgentSession session,
    AgentFixturePrompt prompt,
  ) async {
    if (_run != null &&
        (_run!.id != session.id ||
            _run!.revision != session.revision ||
            _run!.personId != session.personId)) {
      throw const AgentVaultException('conflict');
    }
    if (_run == null) {
      if (_pending != null) await _drain();
      _pending = _VaultJob(
        session.personId,
        newAgentRequestId(),
        'fixture_turn',
      );
      _run = session;
    }
    return _update(
      session,
      await _call(_pending!, {
        'kind': 'submit',
        'action': {'kind': 'session', 'operation': _turn(session, prompt)},
      }),
    );
  }

  @override
  Future<AgentRunUpdate> pollAgentFixtureRun(
    AgentSession session,
    int afterSequence,
  ) => _runCall(session, {'kind': 'poll', 'after_sequence': afterSequence});
  @override
  Future<AgentRunUpdate> stopAgentFixtureRun(AgentSession session) =>
      _runCall(session, {'kind': 'stop'});
  @override
  Future<AgentRunUpdate> releaseAgentFixtureRun(AgentSession session) async {
    final result = await _runCall(session, {'kind': 'release'});
    _pending = null;
    _run = null;
    _conversationRun = null;
    return result;
  }

  Future<AgentRunUpdate> _runCall(
    AgentSession session,
    Map<String, Object?> operation,
  ) async {
    if (_run?.id != session.id ||
        _run?.revision != session.revision ||
        _run?.personId != session.personId) {
      throw const AgentVaultException('conflict');
    }
    return _update(session, await _call(_pending!, operation));
  }

  AgentRunUpdate _update(AgentSession session, Map<String, dynamic> result) {
    final failure = _failureUpdateFields(result['failure'], _pending!);
    return AgentRunUpdate.fromJson({
      ...result,
      ...failure,
      'session_id': session.id,
      'expected_revision': session.revision,
    });
  }

  Map<String, Object?> _failureUpdateFields(Object? raw, _VaultJob job) {
    if (raw == null) {
      return const {'failure': null, 'recovery_action': null};
    }
    final envelope = _failureEnvelope(raw, job);
    return {
      'failure': envelope.reasonCode,
      'recovery_action': envelope.recoveryAction,
      'failure_domain': envelope.domain,
      'failure_category': envelope.category,
      'failure_reason_code': envelope.reasonCode,
      'failure_safe_actions': envelope.safeActions,
      'failure_affected_refs': envelope.affectedRefs,
      'failure_incident_id': envelope.incidentId,
      'failure_retry_policy': envelope.retryPolicy,
    };
  }

  Future<Map<String, dynamic>> _call(
    _VaultJob job,
    Map<String, Object?> operation,
  ) async {
    late final Map<String, dynamic> result;
    try {
      result = await request({
        'schema_version': agentSchemaVersion,
        'person_id': job.personId,
        'request_id': job.id,
        'operation': operation,
      });
    } on Object catch (error, stackTrace) {
      final source = error is AgentVaultException ? error : null;
      final enriched = AgentVaultException(
        source?.failure ?? 'transport_unavailable',
        requestId: source?.requestId ?? job.id,
        stage: source?.stage ?? job.stage,
        metadata: source?.metadata ?? const {},
        recoveryAction: source?.recoveryAction,
        affectedRefs: source?.affectedRefs ?? const [],
        correlationRequestId: source?.correlationRequestId,
        retryableOverride: source?.retryableOverride,
        domain: source?.domain,
        category: source?.category,
        reasonCode: source?.reasonCode,
        safeActions: source?.safeActions ?? const [],
        incidentId: source?.incidentId,
        retryPolicy: source?.retryPolicy,
      );
      AppDiagnostics.error(
        component: 'agent_gateway',
        operation: enriched.stage!,
        error: error,
        stackTrace: stackTrace,
        failure: enriched.failure,
        failureDomain: enriched.domain,
        failureCategory: enriched.category,
        reasonCode: enriched.reasonCode,
        incidentId: enriched.incidentId,
        safeActions: enriched.safeActions,
        requestId: enriched.requestId,
        retryable: enriched.retryable,
      );
      Error.throwWithStackTrace(source == null ? error : enriched, stackTrace);
    }
    if (result['request_id'] != job.id ||
        result['done'] is! bool ||
        result['events'] is! List ||
        result['next_sequence'] is! int) {
      throw const FormatException('Invalid vault response');
    }
    _validateFailure(result['failure'], job);
    if (operation['kind'] != 'release' && result['done'] == true) {
      if (result['failure'] != null) {
        final completedError = _failureException(result['failure'], job);
        AppDiagnostics.error(
          component: 'agent_gateway',
          operation: job.stage,
          error: completedError,
          stackTrace: StackTrace.current,
          failure: completedError.failure,
          failureDomain: completedError.domain,
          failureCategory: completedError.category,
          reasonCode: completedError.reasonCode,
          incidentId: completedError.incidentId,
          safeActions: completedError.safeActions,
          requestId: job.id,
          retryable: completedError.retryable,
        );
      }
    }
    return result;
  }

  AgentVaultException _failureException(Object? raw, _VaultJob job) {
    final envelope = _failureEnvelope(raw, job);
    return AgentVaultException(
      envelope.reasonCode,
      requestId: envelope.correlationRequestId,
      stage: envelope.stage,
      recoveryAction: envelope.recoveryAction,
      affectedRefs: envelope.affectedRefs,
      correlationRequestId: envelope.correlationRequestId,
      retryableOverride: envelope.retryable,
      domain: envelope.domain,
      category: envelope.category,
      reasonCode: envelope.reasonCode,
      safeActions: envelope.safeActions,
      incidentId: envelope.incidentId,
      retryPolicy: envelope.retryPolicy,
    );
  }

  void _validateFailure(Object? raw, _VaultJob job) {
    if (raw != null) _failureEnvelope(raw, job);
  }

  _VaultFailureEnvelope _failureEnvelope(Object? raw, _VaultJob job) {
    if (raw is! Map) {
      throw const FormatException('Invalid vault failure envelope');
    }
    late final Map<String, Object?> value;
    try {
      value = Map<String, Object?>.from(raw);
    } on Object {
      throw const FormatException('Invalid vault failure envelope');
    }
    const fields = {
      'schema_version',
      'domain',
      'category',
      'reason_code',
      'kind',
      'stage',
      'safe_actions',
      'affected_refs',
      'incident_id',
      'retry_policy',
      'retryable',
      'recovery_action',
      'correlation_request_id',
    };
    if (value.length != fields.length ||
        !value.keys.toSet().containsAll(fields)) {
      throw const FormatException('Invalid vault failure envelope fields');
    }
    if (value['schema_version'] != agentSchemaVersion ||
        value['domain'] is! String ||
        value['category'] is! String ||
        value['reason_code'] is! String ||
        value['kind'] is! String ||
        value['stage'] != job.stage ||
        (value['stage'] as String).isEmpty ||
        (value['stage'] as String).length > 64 ||
        (value['kind'] as String).trim().isEmpty ||
        (value['kind'] as String).length > 128 ||
        (value['reason_code'] as String).trim().isEmpty ||
        (value['reason_code'] as String).length > 128 ||
        value['incident_id'] is! String ||
        (value['incident_id'] as String).isEmpty ||
        (value['incident_id'] as String).length > 128 ||
        value['retryable'] is! bool ||
        value['retry_policy'] is! String ||
        value['recovery_action'] is! String ||
        value['correlation_request_id'] != job.id) {
      throw const FormatException('Invalid vault failure envelope');
    }
    if (!const {
      'source',
      'capability',
      'turn',
      'session',
      'vault',
      'app',
    }.contains(value['domain'])) {
      throw const FormatException('Invalid vault failure domain');
    }
    if (!const {
      'user_configuration',
      'transient',
      'integrity',
      'security',
      'internal',
    }.contains(value['category'])) {
      throw const FormatException('Invalid vault failure category');
    }
    final refs = value['affected_refs'];
    if (refs is! List ||
        refs.length > 32 ||
        refs.any((ref) => ref is! String || ref.isEmpty || ref.length > 256)) {
      throw const FormatException('Invalid vault failure references');
    }
    final action = value['recovery_action']! as String;
    if (!const {
      'none',
      'retry_read',
      'refresh_session',
      'refresh_context',
      'review_source',
      'reopen_vault',
      'reconcile',
    }.contains(action)) {
      throw const FormatException('Invalid vault recovery action');
    }
    final safeActions = value['safe_actions'];
    if (safeActions is! List ||
        safeActions.length > 16 ||
        safeActions.any(
          (item) =>
              item is! String ||
              !const {
                'continue_without_source',
                'review_source',
                'retry',
                'refresh_session',
                'start_new_session',
                'reopen_vault',
                'reset_local_agent_state',
                'export_diagnostics',
              }.contains(item),
        ) ||
        safeActions.toSet().length != safeActions.length) {
      throw const FormatException('Invalid vault failure safe actions');
    }
    final retryPolicy = value['retry_policy']! as String;
    if (!const {'never', 'immediate', 'backoff'}.contains(retryPolicy)) {
      throw const FormatException('Invalid vault failure retry policy');
    }
    final retryable = value['retryable']! as bool;
    if (retryable != (action == 'retry_read') ||
        retryable != (retryPolicy != 'never') ||
        retryable != safeActions.contains('retry')) {
      throw const FormatException('Invalid vault recovery retry contract');
    }
    return _VaultFailureEnvelope(
      domain: value['domain']! as String,
      category: value['category']! as String,
      reasonCode: value['reason_code']! as String,
      kind: value['kind']! as String,
      stage: value['stage']! as String,
      safeActions: List.unmodifiable(safeActions.cast<String>()),
      affectedRefs: List.unmodifiable(refs.cast<String>()),
      retryable: retryable,
      incidentId: value['incident_id']! as String,
      retryPolicy: retryPolicy,
      recoveryAction: action,
      correlationRequestId: value['correlation_request_id']! as String,
    );
  }

  Future<void> _release(_VaultJob job) async {
    await _call(job, {'kind': 'release'});
    _pending = null;
    _run = null;
    _conversationRun = null;
  }

  Map<String, Object?> _turn(AgentSession session, AgentFixturePrompt prompt) =>
      {
        'kind': 'turn',
        'session_id': session.id,
        'expected_revision': session.revision,
        'prompt': prompt.wireName,
      };
}

final class _VaultFailureEnvelope {
  const _VaultFailureEnvelope({
    required this.domain,
    required this.category,
    required this.reasonCode,
    required this.kind,
    required this.stage,
    required this.safeActions,
    required this.affectedRefs,
    required this.retryable,
    required this.incidentId,
    required this.retryPolicy,
    required this.recoveryAction,
    required this.correlationRequestId,
  });

  final String domain;
  final String category;
  final String reasonCode;
  final String kind;
  final String stage;
  final List<String> safeActions;
  final List<String> affectedRefs;
  final bool retryable;
  final String incidentId;
  final String retryPolicy;
  final String recoveryAction;
  final String correlationRequestId;
}

final class _VaultJob {
  _VaultJob(this.personId, this.id, this.stage);
  final String personId;
  final String id;
  final String stage;
}
