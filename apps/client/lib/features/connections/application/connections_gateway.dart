import 'package:floe_client/features/connections/domain/connection_models.dart';
import 'package:floe_client/app/runtime/owner_failure.dart';
import 'package:floe_client/app/runtime/app_wire_transport.dart';

abstract interface class ConnectionsGateway {
  Future<ConnectionsOverview> overview();
  Future<GatewaySummary> getGateway({required GatewayRef gatewayRef});
  Future<PairingSnapshot> startPairing({
    required String commandId,
    required String addressText,
  });
  Future<PairingSnapshot> observePairing({
    required ConnectionOperationRef operationRef,
  });
  Future<PairingSnapshot> cancelPairing({
    required String commandId,
    required ConnectionOperationRef operationRef,
    required int expectedRevision,
  });
  Future<GatewaySummary> forgetGateway({
    required String commandId,
    required GatewayRef gatewayRef,
    required int expectedRevision,
  });
  Future<IntegrationReview> prepareIntegrationReview({
    required String commandId,
    required IntegrationRef integrationRef,
    required int expectedRevision,
  });
  Future<IntegrationReview> inspectIntegrationReview({
    required IntegrationReviewRef reviewRef,
  });
  Future<ConnectionOperationSnapshot> startIntegration({
    required String commandId,
    required IntegrationRef integrationRef,
    required IntegrationReviewRef reviewedSelectionRef,
    required int expectedRevision,
  });
  Future<ConnectionOperationSnapshot> observeOperation({
    required ConnectionOperationRef operationRef,
  });
  Future<ConnectionOperationSnapshot> cancelOperation({
    required String commandId,
    required ConnectionOperationRef operationRef,
    required int expectedRevision,
  });
  Future<SourceReview> prepareSourceReview({
    required String commandId,
    required SourceRef sourceRef,
    required int expectedRevision,
  });
  Future<SourceReview> inspectSourceReview({
    required SourceReviewRef reviewRef,
  });
  Future<SourceConfigurationResult> configureSource({
    required String commandId,
    required SourceRef sourceRef,
    required SourceReviewRef reviewRef,
    required List<ResourceRef> selectedResourceRefs,
    required int expectedRevision,
  });
  Future<ConnectionOperationSnapshot> disconnectSource({
    required String commandId,
    required SourceRef sourceRef,
    required int expectedRevision,
  });
  Future<ObserveReview> prepareObserveReview({
    required String commandId,
    required SourceRef sourceRef,
    required int expectedRevision,
    required SourceProcessing requestedProcessing,
  });
  Future<ObserveReview> inspectObserveReview({
    required ObserveReviewRef reviewRef,
  });
  Future<LaunchAction> requestManagementLaunch({
    required String commandId,
    required GatewayRef gatewayRef,
    required int expectedRevision,
  });
  Future<SourceSummary> setObserve({
    required String commandId,
    required SourceRef sourceRef,
    required bool enabled,
    ObserveReviewRef? reviewRef,
    required int expectedRevision,
  });
}

/// Correlated safe failure data; no request payload or private transport data.
sealed class ConnectionsRequestFailure implements Exception {
  const ConnectionsRequestFailure({
    required this.code,
    required this.reason,
    required this.requestId,
    required this.errorId,
    required this.ownerFailure,
  });

  final String code;
  final String reason;
  final String requestId;
  final String errorId;
  final OwnerFailure? ownerFailure;

  @override
  String toString() => 'Connections request failed ($reason; $errorId).';
}

final class ConnectionsCommandFailure extends ConnectionsRequestFailure {
  const ConnectionsCommandFailure({
    required super.code,
    required super.reason,
    required super.requestId,
    required super.errorId,
    required super.ownerFailure,
    required this.commandId,
    required this.disposition,
  });
  final String commandId;
  final CommandOutcome disposition;
}

final class ConnectionsReadFailure extends ConnectionsRequestFailure {
  const ConnectionsReadFailure({
    required super.code,
    required super.reason,
    required super.requestId,
    required super.errorId,
    required super.ownerFailure,
  });
}
