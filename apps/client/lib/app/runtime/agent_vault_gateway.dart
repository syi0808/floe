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

  /// Decided by the owner; never re-derived from [recoveryAction].
  final bool? reloadRequired;

  /// Decided by the owner; the client stops applying results when set.
  final bool? sealSession;

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

abstract interface class AgentVaultGateway {
  Future<AgentVaultState> vaultStatus(String personId);
  Future<AgentVaultState> createVault(String personId);
  Future<AgentVaultState> unlockVault(String personId);
  Future<void> lockVault(String personId);
}
