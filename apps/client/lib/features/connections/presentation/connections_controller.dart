import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:floe_client/app/runtime/native_transport.dart';
import 'package:floe_client/features/connections/application/connections_gateway.dart';
import 'package:floe_client/features/connections/domain/connection_models.dart';
import 'package:floe_client/features/conversation/application/agent_request_id.dart';

/// Owns presentation and observation only. Rust owns every lifecycle transition.
final class ConnectionsController extends ChangeNotifier {
  ConnectionsController(this.gateway);
  final ConnectionsGateway gateway;
  ConnectionsOverview? overview;
  GatewaySetup? setup;
  PairingSnapshot? pairing;
  ConnectionOperationSnapshot? operation;
  String? operationLabel;
  SourceReview? sourceReview;
  ObserveReview? observeReview;
  IntegrationReview? integrationReview;
  int? _integrationRevision;
  LaunchAction? launchAction;
  String? failure;
  bool busy = false;
  bool _disposed = false;
  int _loadGeneration = 0;
  Timer? _pairingObservation;
  Timer? _operationObservation;
  Future<void> Function()? _pendingCommand;

  bool get hasUncertainCommand => _pendingCommand != null && !busy;

  Future<void> load() async {
    final generation = ++_loadGeneration;
    try {
      final value = await gateway.overview();
      if (_disposed || generation != _loadGeneration) return;
      if (overview != null && value.revision < overview!.revision) return;
      overview = value;
      failure = null;
    } on Object {
      if (!_disposed && generation == _loadGeneration)
        failure = 'Connection status could not be loaded.';
    }
    _notify();
  }

  Future<void> _command(Future<void> Function(String commandId) action) async {
    if (_disposed || busy || _pendingCommand != null) return;
    final commandId = newAgentRequestId();
    _pendingCommand = () => action(commandId);
    await retryPendingCommand();
  }

  /// Replays the identical owner-idempotent command after an uncertain response.
  /// A new decision never borrows the pending command's durable identity.
  Future<void> retryPendingCommand() async {
    final pending = _pendingCommand;
    if (_disposed || busy || pending == null) return;
    busy = true;
    failure = null;
    _notify();
    try {
      await pending();
      _pendingCommand = null;
      await load();
    } on NativeTransportException catch (error) {
      if (error.code != 'timeout' && error.code != 'ffi')
        _pendingCommand = null;
      failure = _pendingCommand == null
          ? 'The owner could not apply this request. Refresh its current state.'
          : 'The result is not confirmed. Check status or recover the same request.';
    } on Object {
      failure = 'The result is not confirmed. Check status or recover the same request.';
    } finally {
      busy = false;
      _notify();
    }
  }

  Future<void> prepareGateway(String address) => _command((id) async {
    final value = await gateway.prepareGatewaySetup(
      commandId: id,
      addressText: address,
    );
    if (!_disposed) setup = value;
  });

  Future<void> startPairing() async {
    final target = setup;
    if (target == null) return;
    await _command(
      (id) async => _setPairing(
        await gateway.startPairing(
          commandId: id,
          gatewayTargetRef: target.targetRef,
        ),
      ),
    );
  }

  Future<void> confirmPairing() async {
    final current = pairing;
    if (current == null) return;
    await _command(
      (id) async => _setPairing(
        await gateway.confirmPairing(
          commandId: id,
          operationRef: current.operationRef,
          expectedRevision: current.revision,
        ),
      ),
    );
  }

  Future<void> cancelPairing() async {
    final current = pairing;
    if (current == null) return;
    await _command(
      (id) async => _setPairing(
        await gateway.cancelPairing(
          commandId: id,
          operationRef: current.operationRef,
          expectedRevision: current.revision,
        ),
      ),
    );
  }

  Future<void> observePairing() async {
    final current = pairing;
    if (current == null || _disposed) return;
    try {
      final value = await gateway.observePairing(
        operationRef: current.operationRef,
      );
      if (pairing?.operationRef != current.operationRef) return;
      _setPairing(value);
      await load();
    } on Object {
      failure =
          'Pairing status is unavailable. The operation is still retained.';
      _notify();
    }
  }

  void _setPairing(PairingSnapshot value) {
    if (_disposed) return;
    final previous = pairing;
    if (previous?.operationRef == value.operationRef &&
        value.revision < previous!.revision)
      return;
    pairing = value;
    _pairingObservation?.cancel();
    final delay = value.nextObservationAfterMs;
    if (delay != null)
      _pairingObservation = Timer(
        Duration(milliseconds: delay),
        () => unawaited(observePairing()),
      );
    _notify();
  }

  Future<void> forgetGateway(GatewaySummary value) => _command((id) async {
    await gateway.forgetGateway(
      commandId: id,
      gatewayRef: value.gatewayRef,
      expectedRevision: value.revision,
    );
  });

  Future<void> prepareManagement(GatewaySummary value) => _command((id) async {
    final launch = await gateway.requestManagementLaunch(
      commandId: id,
      gatewayRef: value.gatewayRef,
      expectedRevision: value.revision,
    );
    if (!_disposed) launchAction = launch;
  });

  Future<void> prepareIntegration(IntegrationSummary value) =>
      _command((id) async {
        final review = await gateway.prepareIntegrationReview(
          commandId: id,
          integrationRef: value.integrationRef,
          expectedRevision: value.revision,
        );
        if (!_disposed) {
          integrationReview = review;
          _integrationRevision = value.revision;
        }
      });

  Future<void> startIntegration(IntegrationReview review) =>
      _command((id) async {
        _setOperation(
          await gateway.startIntegration(
            commandId: id,
            integrationRef: review.integrationRef,
            reviewedSelectionRef: review.reviewRef,
            expectedRevision:
                _integrationRevision ??
                (throw StateError('Missing reviewed integration revision.')),
          ),
        );
        if (!_disposed) {
          integrationReview = null;
          operationLabel = review.displayName;
        }
      });

  Future<void> prepareSource(SourceSummary value) => _command((id) async {
    final review = await gateway.prepareSourceReview(
      commandId: id,
      sourceRef: value.sourceRef,
      expectedRevision: value.revision,
    );
    if (!_disposed) sourceReview = review;
  });

  Future<void> configureSource(
    SourceReview review,
    List<ResourceRef> selected,
  ) => _command((id) async {
    await gateway.configureSource(
      commandId: id,
      sourceRef: review.sourceRef,
      reviewRef: review.reviewRef,
      selectedResourceRefs: selected,
      expectedRevision: review.sourceRevision,
    );
    if (!_disposed) sourceReview = null;
  });

  Future<void> prepareObserve(
    SourceSummary value,
    SourceProcessing processing,
  ) => _command((id) async {
    final review = await gateway.prepareObserveReview(
      commandId: id,
      sourceRef: value.sourceRef,
      expectedRevision: value.revision,
      requestedProcessing: processing,
    );
    if (!_disposed) observeReview = review;
  });

  Future<void> allowObserve(ObserveReview review) => _command((id) async {
    await gateway.setObserve(
      commandId: id,
      sourceRef: review.sourceRef,
      enabled: true,
      reviewRef: review.reviewRef,
      expectedRevision: review.sourceRevision,
    );
    if (!_disposed) observeReview = null;
  });

  Future<void> pauseObserve(SourceSummary value) => _command((id) async {
    await gateway.setObserve(
      commandId: id,
      sourceRef: value.sourceRef,
      enabled: false,
      expectedRevision: value.revision,
    );
  });

  Future<void> disconnectSource(SourceSummary value) => _command((id) async {
    _setOperation(
      await gateway.disconnectSource(
        commandId: id,
        sourceRef: value.sourceRef,
        expectedRevision: value.revision,
      ),
    );
    if (!_disposed) operationLabel = value.displayLabels.join(' · ');
  });

  Future<void> cancelOperation() async {
    final current = operation;
    if (current == null) return;
    await _command(
      (id) async => _setOperation(
        await gateway.cancelOperation(
          commandId: id,
          operationRef: current.operationRef,
          expectedRevision: current.revision,
        ),
      ),
    );
  }

  Future<void> observeOperation() async {
    final current = operation;
    if (current == null || _disposed) return;
    try {
      final value = await gateway.observeOperation(
        operationRef: current.operationRef,
      );
      if (operation?.operationRef != current.operationRef) return;
      _setOperation(value);
      await load();
    } on Object {
      failure = 'Connection operation status is unavailable. Check it again.';
      _notify();
    }
  }

  void _setOperation(ConnectionOperationSnapshot value) {
    if (_disposed) return;
    final previous = operation;
    if (previous?.operationRef == value.operationRef &&
        value.revision < previous!.revision)
      return;
    operation = value;
    launchAction = value.launchAction;
    _operationObservation?.cancel();
    final delay = value.nextObservationAfterMs;
    if (delay != null)
      _operationObservation = Timer(
        Duration(milliseconds: delay),
        () => unawaited(observeOperation()),
      );
    _notify();
  }

  void dismissReview() {
    sourceReview = null;
    observeReview = null;
    integrationReview = null;
    _notify();
  }

  void _notify() {
    if (!_disposed) notifyListeners();
  }

  @override
  void dispose() {
    _disposed = true;
    _loadGeneration++;
    _pairingObservation?.cancel();
    _operationObservation?.cancel();
    // Detaching a view neither cancels a Run nor cancels an owner operation.
    super.dispose();
  }
}
