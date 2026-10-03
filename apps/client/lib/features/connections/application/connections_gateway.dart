import 'package:floe_client/features/connections/domain/connection_models.dart';

abstract interface class ConnectionsGateway {
  Future<GatewaySetup> prepareGatewaySetup({
    required String commandId,
    required String addressText,
  });
  Future<ConnectionsOverview> overview();
  Future<GatewaySummary> getGateway({required GatewayRef gatewayRef});
  Future<PairingSnapshot> startPairing({
    required String commandId,
    required GatewayTargetRef gatewayTargetRef,
  });
  Future<PairingSnapshot> confirmPairing({
    required String commandId,
    required ConnectionOperationRef operationRef,
    required int expectedRevision,
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
  Future<SourceSummary> configureSource({
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
