import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:floe_client/app/runtime/native_transport.dart';
import 'package:floe_client/app/runtime/agent_vault_gateway.dart';
import 'package:floe_client/features/vault/application/vault_controller.dart';
import 'package:floe_client/features/connections/application/connections_gateway.dart';
import 'package:floe_client/features/connections/domain/connection_models.dart';
import 'package:floe_client/features/conversation/application/agent_request_id.dart';

/// Owns presentation and observation only. Rust owns every lifecycle transition.
final class ConnectionsController extends ChangeNotifier {
  ConnectionsController(this.gateway, {required this.vault}) {
    vault.addListener(_readinessChanged);
    _readinessChanged();
  }
  final VaultController vault;
  final ConnectionsGateway gateway;
  ConnectionsOverview? overview;
  String? pairingAddress;
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
  bool _wasReady = false;
  bool get storageOpening => vault.busy;
  String? get storageFailure => _safeToken(vault.reasonCode);
  String? get storageIncidentId => _safeToken(vault.incidentId);
  int _readinessGeneration = 0;
  int? _activeCommandGeneration;

  bool get ready => vault.ready;
  bool get busy => _commandBusy || !ready;
  bool get commandBusy => _commandBusy;
  bool get hasPendingPairingRequest =>
      _pendingCommands.containsKey('startPairing');
  bool get _acceptCommandResult =>
      !_disposed && ready && _activeCommandGeneration == _readinessGeneration;

  void _reportStorageFailure(Object error) {
    final owner = switch (error) {
      ConnectionsRequestFailure() => error.ownerFailure,
      NativeTransportException() => error.ownerFailure,
      _ => null,
    };
    if (owner != null) {
      vault.reportFailure(
        AgentVaultException.fromAppWire(owner.reason, ownerFailure: owner),
      );
    }
  }

  String get storageMessage {
    if (storageOpening) return 'Opening local secure storage…';
    if (storageFailure case final reason?) {
      return 'Local secure storage could not open ($reason).';
    }
    return switch (vault.state) {
      AgentVaultState.locked => 'Local secure storage is locked.',
      AgentVaultState.missing => 'Local secure storage is not ready yet.',
      AgentVaultState.unavailable => 'Local secure storage is unavailable.',
      _ => 'Waiting for local secure storage.',
    };
  }

  void _readinessChanged() {
    if (_disposed) return;
    final wasReady = _wasReady;
    _wasReady = ready;
    if (wasReady != ready) {
      _readinessGeneration++;
      _loadGeneration++;
      _pairingObservation?.cancel();
      _operationObservation?.cancel();
      if (!ready) {
        overview = null;
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
  final Map<String, _PendingCommand> _pendingCommands = {};
  int get pendingCommandCount => _pendingCommands.length;
  List<({String commandId, String label})> get pendingRequests => [
    for (final value in _pendingCommands.values)
      (commandId: value.id, label: value.label),
  ];

  bool get hasUncertainCommand =>
      _pendingCommands.isNotEmpty && !_commandBusy && ready;

  Future<void> load() async {
    if (_disposed || !ready) return;
    final generation = ++_loadGeneration;
    try {
      final value = await gateway.overview();
      if (_disposed || !ready || generation != _loadGeneration) return;
      if (overview != null && value.revision < overview!.revision) return;
      overview = value;
      if (_pendingCommands.isEmpty) failure = null;
    } on Object catch (error) {
      if (!_disposed && ready && generation == _loadGeneration) {
        _reportStorageFailure(error);
        failure = _failureMessage(
          error,
          'Connection status could not be loaded.',
        );
      }
    }
    _notify();
  }

  Future<void> _command(
    String target,
    String label,
    Future<void> Function(String commandId) action,
  ) async {
    if (_disposed || busy) return;
    if (_pendingCommands.containsKey(target)) {
      failure =
          'The earlier $label request is unresolved. Recover that same request before changing it.';
      _notify();
      return;
    }
    final commandId = newAgentRequestId();
    final pending = _PendingCommand(commandId, label, () => action(commandId));
    _pendingCommands[target] = pending;
    await _runPending(target, pending);
  }

  /// Recovery retains each exact intent. Other targets and repair actions remain
  /// available; only the owner can decide whether their authority permits them.
  Future<void> retryPendingCommand([String? commandId]) async {
    if (_pendingCommands.isEmpty) return;
    final entry = commandId == null
        ? _pendingCommands.entries.first
        : _pendingCommands.entries
              .where((entry) => entry.value.id == commandId)
              .firstOrNull;
    if (entry != null) await _runPending(entry.key, entry.value);
  }

  Future<void> _runPending(String target, _PendingCommand pending) async {
    if (_disposed || _commandBusy || !ready) return;
    final wasUncertain = pending.wasUncertain;
    pending.wasUncertain = true;
    _commandBusy = true;
    final generation = _readinessGeneration;
    _activeCommandGeneration = generation;
    failure = null;
    _notify();
    try {
      await pending.action();
      if (_disposed || !ready || generation != _readinessGeneration) return;
      _pendingCommands.remove(target);
      await load();
    } on Object catch (error) {
      if (_disposed || !ready || generation != _readinessGeneration) return;
      if (error is ConnectionsCommandFailure &&
          error.commandId == pending.id &&
          (error.disposition == NativeCommandDisposition.notApplied ||
              (!wasUncertain &&
                  error.disposition == NativeCommandDisposition.notAdmitted))) {
        _pendingCommands.remove(target);
      }
      _reportStorageFailure(error);
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

  /// The owner retains preparation, Start and replay behind one user command.
  Future<void> pairGateway(String address) =>
      _command('startPairing', 'pairing', (id) async {
        // Display text belongs to the retained user intent, not authority.
        if (_acceptCommandResult) pairingAddress = address;
        final value = await gateway.startPairing(
          commandId: id,
          addressText: address,
        );
        if (_acceptCommandResult) {
          pairingAddress = address;
          _setPairing(value);
        }
      });

  Future<void> cancelPairing() async {
    final current = pairing;
    if (current == null) return;
    await _command(
      'cancelPairing:${current.operationRef.value}',
      'pairing cancellation',
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
      if (_disposed ||
          !ready ||
          generation != _readinessGeneration ||
          pairing?.operationRef != current.operationRef)
        return;
      // This snapshot identifies the operation, not a pending Start/Cancel
      // command. Only exact command replay may acknowledge that mutation.
      _setPairing(value);
      await load();
    } on Object catch (error) {
      if (_disposed || !ready || generation != _readinessGeneration) return;
      _reportStorageFailure(error);
      failure = _failureMessage(
        error,
        'Pairing status is unavailable. The operation is still retained.',
      );
      _notify();
    }
  }

  void _setPairing(PairingSnapshot value) {
    if (_disposed ||
        !ready ||
        (_activeCommandGeneration != null &&
            _activeCommandGeneration != _readinessGeneration))
      return;
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

  Future<void> forgetGateway(GatewaySummary value) => _command(
    'forgetGateway:${value.gatewayRef.value}',
    'Gateway removal',
    (id) async {
      await gateway.forgetGateway(
        commandId: id,
        gatewayRef: value.gatewayRef,
        expectedRevision: value.revision,
      );
    },
  );

  Future<LaunchAction?> prepareManagement(GatewaySummary value) async {
    LaunchAction? prepared;
    await _command(
      'management:${value.gatewayRef.value}',
      'Gateway management',
      (id) async {
        final launch = await gateway.requestManagementLaunch(
          commandId: id,
          gatewayRef: value.gatewayRef,
          expectedRevision: value.revision,
        );
        if (_acceptCommandResult) {
          launchAction = launch;
          prepared = launch;
        }
      },
    );
    return prepared;
  }

  Future<void> prepareIntegration(IntegrationSummary value) => _command(
    'prepareIntegration:${value.integrationRef.value}',
    'integration review',
    (id) async {
      final review = await gateway.prepareIntegrationReview(
        commandId: id,
        integrationRef: value.integrationRef,
        expectedRevision: value.revision,
      );
      if (_acceptCommandResult) {
        integrationReview = review;
        _integrationRevision = value.revision;
      }
    },
  );

  Future<void> startIntegration(IntegrationReview review) {
    final expectedRevision = _integrationRevision;
    if (expectedRevision == null) return Future<void>.value();
    return _command(
      'startIntegration:${review.integrationRef.value}',
      'integration connection',
      (id) async {
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
      },
    );
  }

  Future<void> prepareSource(SourceSummary value) => _command(
    'prepareSource:${value.sourceRef.value}',
    'source review',
    (id) async {
      final review = await gateway.prepareSourceReview(
        commandId: id,
        sourceRef: value.sourceRef,
        expectedRevision: value.revision,
      );
      if (_acceptCommandResult) sourceReview = review;
    },
  );

  Future<void> configureSource(
    SourceReview review,
    List<ResourceRef> selected,
  ) {
    final selection = List<ResourceRef>.unmodifiable(selected);
    return _command(
      'configureSource:${review.sourceRef.value}',
      'source configuration',
      (id) async {
        await gateway.configureSource(
          commandId: id,
          sourceRef: review.sourceRef,
          reviewRef: review.reviewRef,
          selectedResourceRefs: selection,
          expectedRevision: review.sourceRevision,
        );
        if (_acceptCommandResult) sourceReview = null;
      },
    );
  }

  Future<void> prepareObserve(
    SourceSummary value,
    SourceProcessing processing,
  ) => _command(
    'prepareObserve:${value.sourceRef.value}',
    'processing review',
    (id) async {
      final review = await gateway.prepareObserveReview(
        commandId: id,
        sourceRef: value.sourceRef,
        expectedRevision: value.revision,
        requestedProcessing: processing,
      );
      if (_acceptCommandResult) observeReview = review;
    },
  );

  Future<void> allowObserve(ObserveReview review) => _command(
    'allowObserve:${review.sourceRef.value}',
    'processing approval',
    (id) async {
      await gateway.setObserve(
        commandId: id,
        sourceRef: review.sourceRef,
        enabled: true,
        reviewRef: review.reviewRef,
        expectedRevision: review.sourceRevision,
      );
      if (_acceptCommandResult) observeReview = null;
    },
  );

  Future<void> pauseObserve(SourceSummary value) => _command(
    'pauseObserve:${value.sourceRef.value}',
    'observation pause',
    (id) async {
      await gateway.setObserve(
        commandId: id,
        sourceRef: value.sourceRef,
        enabled: false,
        expectedRevision: value.revision,
      );
    },
  );

  Future<void> disconnectSource(SourceSummary value) => _command(
    'disconnect:${value.sourceRef.value}',
    'source disconnection',
    (id) async {
      _setOperation(
        await gateway.disconnectSource(
          commandId: id,
          sourceRef: value.sourceRef,
          expectedRevision: value.revision,
        ),
      );
      if (_acceptCommandResult)
        operationLabel = value.displayLabels.join(' · ');
    },
  );

  Future<void> cancelOperation() async {
    final current = operation;
    if (current == null) return;
    await _command(
      'cancelOperation:${current.operationRef.value}',
      'operation cancellation',
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
      if (_disposed ||
          !ready ||
          generation != _readinessGeneration ||
          operation?.operationRef != current.operationRef)
        return;
      // Terminal operation state cannot stand in for the pending command's
      // identity; retain it until its own correlated replay is acknowledged.
      _setOperation(value);
      await load();
    } on Object catch (error) {
      if (_disposed || !ready || generation != _readinessGeneration) return;
      _reportStorageFailure(error);
      failure = _failureMessage(
        error,
        'Connection status is unavailable. Check it again.',
      );
      _notify();
    }
  }

  void _setOperation(ConnectionOperationSnapshot value) {
    if (_disposed ||
        !ready ||
        (_activeCommandGeneration != null &&
            _activeCommandGeneration != _readinessGeneration))
      return;
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
    vault.removeListener(_readinessChanged);
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
    NativeTransportException() =>
      _safeToken(error.ownerFailure?.reason) ??
          _safeToken(error.metadata['reason_code']) ??
          _safeToken(error.code),
    _ => null,
  };
  if (reason == null) return fallback;
  final message = switch (reason) {
    'vault_locked' => 'Local secure storage is locked.',
    'vault_unavailable' ||
    'storage_unavailable' => 'Local secure storage is unavailable ($reason).',
    'unsupported_version' => 'Local secure storage has an unsupported format.',
    _ => 'The connection request could not complete ($reason).',
  };
  if (error is ConnectionsRequestFailure) {
    return '$message\nError ID: ${error.errorId}\nRequest: ${error.requestId}';
  }
  return message;
}

final class _PendingCommand {
  _PendingCommand(this.id, this.label, this.action);
  final String id;
  final String label;
  final Future<void> Function() action;
  bool wasUncertain = false;
}
