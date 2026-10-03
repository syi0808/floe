import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:floe_client/app/runtime/native_transport.dart';
import 'package:floe_client/app/runtime/agent_vault_gateway.dart';
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
  bool _commandBusy = false;
  AgentVaultState? _vaultState;
  bool storageOpening = false;
  String? storageFailure;
  String? storageIncidentId;
  Future<void> Function()? prepareStorage;
  int _readinessGeneration = 0;
  int? _activeCommandGeneration;

  bool get ready => _vaultState == AgentVaultState.ready && !storageOpening;
  bool get busy => _commandBusy || !ready;
  bool get _acceptCommandResult => !_disposed && ready &&
      _activeCommandGeneration == _readinessGeneration;

  String get storageMessage {
    if (storageOpening) return 'Opening local secure storage…';
    if (storageFailure case final reason?) {
      return 'Local secure storage could not open ($reason).';
    }
    return switch (_vaultState) {
      AgentVaultState.locked => 'Local secure storage is locked.',
      AgentVaultState.missing => 'Local secure storage is not ready yet.',
      AgentVaultState.unavailable => 'Local secure storage is unavailable.',
      _ => 'Waiting for local secure storage.',
    };
  }

  /// Mirrors the existing Vault lifecycle; Connections never opens it itself.
  void updateStorage({
    required AgentVaultState? state,
    required bool opening,
    String? failureReason,
    String? incidentId,
  }) {
    if (_disposed) return;
    final reason = _safeToken(failureReason);
    final incident = _safeToken(incidentId);
    if (_vaultState == state && storageOpening == opening &&
        storageFailure == reason && storageIncidentId == incident) return;
    final wasReady = ready;
    _vaultState = state;
    storageOpening = opening;
    storageFailure = reason;
    storageIncidentId = incident;
    if (wasReady != ready) {
      _readinessGeneration++;
      _loadGeneration++;
      _pairingObservation?.cancel();
      _operationObservation?.cancel();
      if (!ready) {
        overview = null;
        setup = null;
        sourceReview = null;
        observeReview = null;
        integrationReview = null;
        _integrationRevision = null;
        launchAction = null;
      }
    }
    _notify();
    if (!wasReady && ready) {
      unawaited(_refreshAfterReady(_readinessGeneration));
    }
  }

  Future<void> _refreshAfterReady(int generation) async {
    await load();
    if (_disposed || !ready || generation != _readinessGeneration) return;
    await observePairing();
    if (_disposed || !ready || generation != _readinessGeneration) return;
    await observeOperation();
  }
  bool _disposed = false;
  int _loadGeneration = 0;
  Timer? _pairingObservation;
  Timer? _operationObservation;
  Future<void> Function()? _pendingCommand;

  bool get hasUncertainCommand => _pendingCommand != null && !busy;

  Future<void> load() async {
    if (_disposed || !ready) return;
    final generation = ++_loadGeneration;
    try {
      final value = await gateway.overview();
      if (_disposed || !ready || generation != _loadGeneration) return;
      if (overview != null && value.revision < overview!.revision) return;
      overview = value;
      if (_pendingCommand == null) failure = null;
    } on Object catch (error) {
      if (!_disposed && ready && generation == _loadGeneration) {
        failure = _failureMessage(error, 'Connection status could not be loaded.');
      }
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
    _commandBusy = true;
    final generation = _readinessGeneration;
    _activeCommandGeneration = generation;
    failure = null;
    _notify();
    try {
      await pending();
      if (_disposed || !ready || generation != _readinessGeneration) return;
      _pendingCommand = null;
      await load();
    } on Object catch (error) {
      if (_disposed || !ready || generation != _readinessGeneration) return;
      final code = switch (error) {
        ConnectionsRequestFailure() => error.code,
        NativeTransportException() => error.code,
        _ => null,
      };
      // Preserve the existing terminal/uncertain rule. Readiness changes above
      // never release a retained command or create a replacement command ID.
      if (code != null && code != 'timeout' && code != 'ffi' &&
          code != 'invalid_response') _pendingCommand = null;
      failure = _failureMessage(
        error,
        'The result is not confirmed. Check status or recover the same request.',
      );
    } finally {
      _commandBusy = false;
      _activeCommandGeneration = null;
      _notify();
    }
  }

  Future<void> prepareGateway(String address) => _command((id) async {
    final value = await gateway.prepareGatewaySetup(
      commandId: id,
      addressText: address,
    );
    if (_acceptCommandResult) setup = value;
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
    if (current == null || _disposed || !ready) return;
    final generation = _readinessGeneration;
    try {
      final value = await gateway.observePairing(
        operationRef: current.operationRef,
      );
      if (_disposed || !ready || generation != _readinessGeneration ||
          pairing?.operationRef != current.operationRef) return;
      _setPairing(value);
      await load();
    } on Object catch (error) {
      if (_disposed || !ready || generation != _readinessGeneration) return;
      failure = _failureMessage(
        error, 'Pairing status is unavailable. The operation is still retained.',
      );
      _notify();
    }
  }

  void _setPairing(PairingSnapshot value) {
    if (_disposed || !ready || (_activeCommandGeneration != null &&
        _activeCommandGeneration != _readinessGeneration)) return;
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
    if (_acceptCommandResult) launchAction = launch;
  });

  Future<void> prepareIntegration(IntegrationSummary value) =>
      _command((id) async {
        final review = await gateway.prepareIntegrationReview(
          commandId: id,
          integrationRef: value.integrationRef,
          expectedRevision: value.revision,
        );
        if (_acceptCommandResult) {
          integrationReview = review;
          _integrationRevision = value.revision;
        }
      });

  Future<void> startIntegration(IntegrationReview review) {
    final expectedRevision = _integrationRevision;
    if (expectedRevision == null) return Future<void>.value();
    return _command((id) async {
        _setOperation(
          await gateway.startIntegration(
            commandId: id,
            integrationRef: review.integrationRef,
            reviewedSelectionRef: review.reviewRef,
            expectedRevision: expectedRevision,
          ),
        );
        if (_acceptCommandResult) {
          integrationReview = null;
          operationLabel = review.displayName;
        }
      });
  }

  Future<void> prepareSource(SourceSummary value) => _command((id) async {
    final review = await gateway.prepareSourceReview(
      commandId: id,
      sourceRef: value.sourceRef,
      expectedRevision: value.revision,
    );
    if (_acceptCommandResult) sourceReview = review;
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
    if (_acceptCommandResult) sourceReview = null;
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
    if (_acceptCommandResult) observeReview = review;
  });

  Future<void> allowObserve(ObserveReview review) => _command((id) async {
    await gateway.setObserve(
      commandId: id,
      sourceRef: review.sourceRef,
      enabled: true,
      reviewRef: review.reviewRef,
      expectedRevision: review.sourceRevision,
    );
    if (_acceptCommandResult) observeReview = null;
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
    if (_acceptCommandResult) operationLabel = value.displayLabels.join(' · ');
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
    if (current == null || _disposed || !ready) return;
    final generation = _readinessGeneration;
    try {
      final value = await gateway.observeOperation(
        operationRef: current.operationRef,
      );
      if (_disposed || !ready || generation != _readinessGeneration ||
          operation?.operationRef != current.operationRef) return;
      _setOperation(value);
      await load();
    } on Object catch (error) {
      if (_disposed || !ready || generation != _readinessGeneration) return;
      failure = _failureMessage(
        error, 'Connection status is unavailable. Check it again.',
      );
      _notify();
    }
  }

  void _setOperation(ConnectionOperationSnapshot value) {
    if (_disposed || !ready || (_activeCommandGeneration != null &&
        _activeCommandGeneration != _readinessGeneration)) return;
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
    prepareStorage = null;
    _readinessGeneration++;
    _loadGeneration++;
    _pairingObservation?.cancel();
    _operationObservation?.cancel();
    // Detaching a view neither cancels a Run nor cancels an owner operation.
    super.dispose();
  }
}

String? _safeToken(String? value) =>
    value != null && RegExp(r'^[a-zA-Z0-9_.:-]{1,128}$').hasMatch(value)
        ? value
        : null;

String _failureMessage(Object error, String fallback) {
  final reason = switch (error) {
    ConnectionsRequestFailure() => error.reason,
    NativeTransportException() => _safeToken(error.ownerFailure?.reason) ??
        _safeToken(error.metadata['reason_code']) ?? _safeToken(error.code),
    _ => null,
  };
  if (reason == null) return fallback;
  final message = switch (reason) {
    'vault_locked' => 'Local secure storage is locked.',
    'vault_unavailable' || 'storage_unavailable' =>
      'Local secure storage is unavailable ($reason).',
    'unsupported_version' => 'Local secure storage has an unsupported format.',
    _ => 'The connection request could not complete ($reason).',
  };
  if (error is ConnectionsRequestFailure) {
    return '$message\nError ID: ${error.errorId}\nRequest: ${error.requestId}';
  }
  return message;
}
