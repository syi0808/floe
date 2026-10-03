import 'package:floe_client/app/runtime/owner_failure.dart';

enum AgentVaultState { missing, locked, ready, unavailable }

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
    this.reloadRequired,
    this.sealSession,
    this.ownerFailure,
  });

  factory AgentVaultException.fromAppWire(
    String failure, {
    OwnerFailure? ownerFailure,
    String? requestId,
    String? stage,
    Map<String, String> metadata = const {},
  }) => AgentVaultException(
    ownerFailure?.reason ?? failure,
    requestId: requestId,
    stage: stage,
    metadata: metadata,
    ownerFailure: ownerFailure,
    recoveryAction: ownerFailure?.recovery,
    domain: ownerFailure?.domain,
    category: ownerFailure?.category,
    reasonCode: ownerFailure?.reason ?? metadata['reason_code'],
    safeActions: ownerFailure?.safeActions.toList(growable: false) ?? const [],
    incidentId: ownerFailure?.incidentId,
    correlationRequestId: ownerFailure?.correlationId,
    reloadRequired: ownerFailure?.reloadRequired,
    sealSession: ownerFailure?.sealSession,
  );

  final String failure;
  final OwnerFailure? ownerFailure;
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

  /// Decided by the owner; never re-derived from [recoveryAction].
  final bool? reloadRequired;

  /// Decided by the owner; the client stops applying results when set.
  final bool? sealSession;

  bool get retryable => retryableOverride ?? false;

  @override
  String toString() => requestId == null
      ? 'AgentVaultException($failure)'
      : 'AgentVaultException($failure, requestId: $requestId)';
}

abstract interface class AgentVaultGateway {
  bool get hasPendingOperation;
  Future<AgentVaultState> resumePendingOperation(String personId);
  Future<AgentVaultState> vaultStatus(String personId);
  Future<AgentVaultState> createVault(String personId);
  Future<AgentVaultState> unlockVault(String personId);
  Future<void> lockVault(String personId);
}
