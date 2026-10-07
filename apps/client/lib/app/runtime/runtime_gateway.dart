import 'package:floe_client/app/runtime/owner_failure.dart';

enum RuntimeReadinessState { unknown, ready, preparationRequired, unavailable }

final class RuntimeReadinessSnapshot {
  const RuntimeReadinessSnapshot({required this.state, this.failure});

  final RuntimeReadinessState state;
  final OwnerFailure? failure;
}

final class RuntimePreparationResult {
  const RuntimePreparationResult({
    required this.operationId,
    required this.done,
    this.failure,
  });

  final String operationId;
  final bool done;
  final String? failure;
}

abstract interface class RuntimeGateway {
  Future<RuntimeReadinessSnapshot> readiness(String requestId);
  Future<RuntimePreparationResult> prepare(String operationId);
  Future<RuntimePreparationResult> getPreparation(String operationId);
  Future<RuntimePreparationResult> acknowledge(String operationId);
}
