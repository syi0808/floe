import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/features/conversation/application/agent_request_id.dart';
import 'package:floe_client/features/connections/application/connections_gateway.dart';
import 'package:floe_client/features/connections/domain/connection_models.dart';

final class AppWireConnectionsGateway implements ConnectionsGateway {
  AppWireConnectionsGateway(this._transport);
  final AppWireTransport _transport;

  @override
  Future<GatewaySetup> prepareGatewaySetup({
    required String commandId,
    required String addressText,
  }) => _invoke(
    commandId,
    'connections.gateway.prepare_setup',
    {'address_text': addressText},
    'connections.gateway_setup',
    'setup',
    GatewaySetup.fromJson,
  );

  @override
  Future<ConnectionsOverview> overview() => _invoke(
    null,
    'connections.overview',
    {},
    'connections.overview',
    'overview',
    ConnectionsOverview.fromJson,
  );

  @override
  Future<GatewaySummary> getGateway({required GatewayRef gatewayRef}) =>
      _invoke(
        null,
        'connections.gateway.get',
        {'gateway_ref': gatewayRef.value},
        'connections.gateway',
        'gateway',
        GatewaySummary.fromJson,
      );

  @override
  Future<PairingSnapshot> startPairing({
    required String commandId,
    required GatewayTargetRef gatewayTargetRef,
  }) => _invoke(
    commandId,
    'connections.pairing.start',
    {'target_ref': gatewayTargetRef.value},
    'connections.pairing',
    'pairing',
    PairingSnapshot.fromJson,
  );

  @override
  Future<PairingSnapshot> confirmPairing({
    required String commandId,
    required ConnectionOperationRef operationRef,
    required int expectedRevision,
  }) => _invoke(
    commandId,
    'connections.pairing.confirm',
    {
      'operation_ref': operationRef.value,
      'expected_revision': expectedRevision,
    },
    'connections.pairing',
    'pairing',
    PairingSnapshot.fromJson,
  );

  @override
  Future<PairingSnapshot> observePairing({
    required ConnectionOperationRef operationRef,
  }) => _invoke(
    null,
    'connections.pairing.get',
    {'operation_ref': operationRef.value},
    'connections.pairing',
    'pairing',
    PairingSnapshot.fromJson,
  );

  @override
  Future<PairingSnapshot> cancelPairing({
    required String commandId,
    required ConnectionOperationRef operationRef,
    required int expectedRevision,
  }) => _invoke(
    commandId,
    'connections.pairing.cancel',
    {
      'operation_ref': operationRef.value,
      'expected_revision': expectedRevision,
    },
    'connections.pairing',
    'pairing',
    PairingSnapshot.fromJson,
  );

  @override
  Future<GatewaySummary> forgetGateway({
    required String commandId,
    required GatewayRef gatewayRef,
    required int expectedRevision,
  }) => _invoke(
    commandId,
    'connections.gateway.forget',
    {'gateway_ref': gatewayRef.value, 'expected_revision': expectedRevision},
    'connections.gateway',
    'gateway',
    GatewaySummary.fromJson,
  );

  @override
  Future<IntegrationReview> prepareIntegrationReview({
    required String commandId,
    required IntegrationRef integrationRef,
    required int expectedRevision,
  }) => _invoke(
    commandId,
    'connections.integration.prepare_review',
    {
      'integration_ref': integrationRef.value,
      'expected_revision': expectedRevision,
    },
    'connections.integration_review',
    'review',
    IntegrationReview.fromJson,
  );

  @override
  Future<IntegrationReview> inspectIntegrationReview({
    required IntegrationReviewRef reviewRef,
  }) => _invoke(
    null,
    'connections.integration.inspect_review',
    {'review_ref': reviewRef.toJson()},
    'connections.integration_review',
    'review',
    IntegrationReview.fromJson,
  );

  @override
  Future<ConnectionOperationSnapshot> startIntegration({
    required String commandId,
    required IntegrationRef integrationRef,
    required IntegrationReviewRef reviewedSelectionRef,
    required int expectedRevision,
  }) => _invoke(
    commandId,
    'connections.integration.start',
    {
      'integration_ref': integrationRef.value,
      'review_ref': reviewedSelectionRef.toJson(),
      'expected_revision': expectedRevision,
    },
    'connections.operation',
    'operation',
    ConnectionOperationSnapshot.fromJson,
  );

  @override
  Future<ConnectionOperationSnapshot> observeOperation({
    required ConnectionOperationRef operationRef,
  }) => _invoke(
    null,
    'connections.operation.get',
    {'operation_ref': operationRef.value},
    'connections.operation',
    'operation',
    ConnectionOperationSnapshot.fromJson,
  );

  @override
  Future<ConnectionOperationSnapshot> cancelOperation({
    required String commandId,
    required ConnectionOperationRef operationRef,
    required int expectedRevision,
  }) => _invoke(
    commandId,
    'connections.operation.cancel',
    {
      'operation_ref': operationRef.value,
      'expected_revision': expectedRevision,
    },
    'connections.operation',
    'operation',
    ConnectionOperationSnapshot.fromJson,
  );

  @override
  Future<SourceReview> prepareSourceReview({
    required String commandId,
    required SourceRef sourceRef,
    required int expectedRevision,
  }) => _invoke(
    commandId,
    'connections.source.prepare_review',
    {'source_ref': sourceRef.value, 'expected_revision': expectedRevision},
    'connections.source_review',
    'review',
    SourceReview.fromJson,
  );

  @override
  Future<SourceReview> inspectSourceReview({
    required SourceReviewRef reviewRef,
  }) => _invoke(
    null,
    'connections.source.inspect_review',
    {'review_ref': reviewRef.toJson()},
    'connections.source_review',
    'review',
    SourceReview.fromJson,
  );

  @override
  Future<SourceSummary> configureSource({
    required String commandId,
    required SourceRef sourceRef,
    required SourceReviewRef reviewRef,
    required List<ResourceRef> selectedResourceRefs,
    required int expectedRevision,
  }) => _invoke(
    commandId,
    'connections.source.configure',
    {
      'source_ref': sourceRef.value,
      'review_ref': reviewRef.toJson(),
      'selected_resource_refs': selectedResourceRefs
          .map((ref) => ref.value)
          .toList(growable: false),
      'expected_revision': expectedRevision,
    },
    'connections.source',
    'source',
    SourceSummary.fromJson,
  );

  @override
  Future<ConnectionOperationSnapshot> disconnectSource({
    required String commandId,
    required SourceRef sourceRef,
    required int expectedRevision,
  }) => _invoke(
    commandId,
    'connections.disconnect',
    {'source_ref': sourceRef.value, 'expected_revision': expectedRevision},
    'connections.operation',
    'operation',
    ConnectionOperationSnapshot.fromJson,
  );

  @override
  Future<ObserveReview> prepareObserveReview({
    required String commandId,
    required SourceRef sourceRef,
    required int expectedRevision,
    required SourceProcessing requestedProcessing,
  }) => _invoke(
    commandId,
    'connections.observe.prepare_review',
    {
      'source_ref': sourceRef.value,
      'expected_revision': expectedRevision,
      'requested_processing': requestedProcessing.wire,
    },
    'connections.observe_review',
    'review',
    ObserveReview.fromJson,
  );

  @override
  Future<ObserveReview> inspectObserveReview({
    required ObserveReviewRef reviewRef,
  }) => _invoke(
    null,
    'connections.observe.inspect_review',
    {'review_ref': reviewRef.toJson()},
    'connections.observe_review',
    'review',
    ObserveReview.fromJson,
  );

  @override
  Future<LaunchAction> requestManagementLaunch({
    required String commandId,
    required GatewayRef gatewayRef,
    required int expectedRevision,
  }) => _invoke(
    commandId,
    'connections.gateway.management_launch',
    {'gateway_ref': gatewayRef.value, 'expected_revision': expectedRevision},
    'connections.launch',
    'launch_action',
    LaunchAction.fromJson,
  );

  @override
  Future<SourceSummary> setObserve({
    required String commandId,
    required SourceRef sourceRef,
    required bool enabled,
    ObserveReviewRef? reviewRef,
    required int expectedRevision,
  }) {
    if (enabled != (reviewRef != null))
      throw const FormatException('Invalid Observe decision.');
    return _invoke(
      commandId,
      'connections.observe.set',
      {
        'mutation': {
          'kind': enabled ? 'enable' : 'pause',
          'source_ref': sourceRef.value,
          'expected_revision': expectedRevision,
          if (reviewRef != null) 'review_ref': reviewRef.toJson(),
        },
      },
      'connections.source',
      'source',
      SourceSummary.fromJson,
    );
  }

  Future<T> _invoke<T>(
    String? commandId,
    String kind,
    Map<String, Object?> fields,
    String resultKind,
    String key,
    T Function(Object?) decode,
  ) async {
    if (commandId != null) GatewayRef(commandId);
    final request = <String, dynamic>{
      'schema_version': appWireProtocolVersion,
      'request_id': newAgentRequestId(),
      if (commandId != null) 'command_id': commandId,
      commandId == null ? 'query' : 'command': {'kind': kind, ...fields},
    };
    final value = commandId == null
        ? await _transport.queryV2(request)
        : await _transport.commandV2(request);
    final result = connectionObject(value, {'kind', key});
    if (result['kind'] != resultKind)
      throw const FormatException('Mismatched Connections reply.');
    final decoded = decode(result[key]);
    final input = fields['mutation'] is Map
        ? Map<String, Object?>.from(fields['mutation']! as Map)
        : fields;
    final expected = switch (decoded) {
      GatewaySummary() => ('gateway_ref', decoded.gatewayRef.value),
      PairingSnapshot() => ('operation_ref', decoded.operationRef.value),
      ConnectionOperationSnapshot() => (
        'operation_ref',
        decoded.operationRef.value,
      ),
      SourceSummary() => ('source_ref', decoded.sourceRef.value),
      SourceReview() => ('source_ref', decoded.sourceRef.value),
      ObserveReview() => ('source_ref', decoded.sourceRef.value),
      IntegrationReview() => ('integration_ref', decoded.integrationRef.value),
      _ => null,
    };
    if (expected != null &&
        input[expected.$1] != null &&
        input[expected.$1] != expected.$2) {
      throw const FormatException('Connections identity changed.');
    }
    final review = switch (decoded) {
      SourceReview() => decoded.reviewRef,
      ObserveReview() => decoded.reviewRef,
      IntegrationReview() => decoded.reviewRef,
      _ => null,
    };
    final expectedReview = input['review_ref'];
    if (review != null &&
        expectedReview is Map &&
        (review.id != expectedReview['id'] ||
            review.revision != expectedReview['revision'] ||
            review.digest != expectedReview['digest'])) {
      throw const FormatException(
        'Reviewed meaning changed during inspection.',
      );
    }
    if (decoded is ObserveReview &&
        input['requested_processing'] != null &&
        decoded.processingDisclosure.requested.wire !=
            input['requested_processing']) {
      throw const FormatException('Source processing choice changed.');
    }
    return decoded;
  }
}
