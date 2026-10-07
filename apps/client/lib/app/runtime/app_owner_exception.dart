import 'package:floe_client/app/runtime/owner_failure.dart';
import 'package:floe_client/app/runtime/app_wire_transport.dart';

/// An owner or AppWire operation failed. This is transport/domain evidence;
/// only an owner projection explicitly in the Vault domain updates shared
/// Runtime readiness.
class AppOwnerException implements Exception {
  const AppOwnerException(
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
    this.commandOutcome,
  });

  factory AppOwnerException.fromAppWire(
    String failure, {
    OwnerFailure? ownerFailure,
    String? requestId,
    String? stage,
    Map<String, String> metadata = const {},
    CommandOutcome? commandOutcome,
  }) => AppOwnerException(
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
    commandOutcome: commandOutcome,
  );

  final String failure;
  final OwnerFailure? ownerFailure;
  final CommandOutcome? commandOutcome;
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
      ? 'AppOwnerException($failure)'
      : 'AppOwnerException($failure, requestId: $requestId)';
}
