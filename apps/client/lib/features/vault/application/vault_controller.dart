import 'package:flutter/foundation.dart';
import 'package:floe_client/app/runtime/agent_vault_gateway.dart';
import 'package:floe_client/infrastructure/diagnostics/app_diagnostics.dart';

/// App-lifetime observation of the single Rust Vault lifecycle. Features share
/// this controller; navigation and feature disposal never open or lock storage.
final class VaultController extends ChangeNotifier {
  VaultController({required this.gateway, required this.personId});
  final AgentVaultGateway gateway;
  final String personId;
  AgentVaultState? _state;
  AgentVaultException? _failure;
  bool _busy = false;
  AgentVaultState? get state => _state;
  AgentVaultException? get failure => _failure;
  bool get busy => _busy;
  bool _disposed = false;
  bool _closing = false;
  bool _desiredReady = true;
  int _failureRevision = 0;
  Future<void>? _opening;
  bool get ready =>
      !_disposed && !_closing && !busy && state == AgentVaultState.ready;
  bool get canRecover =>
      !_disposed &&
      !_closing &&
      !busy &&
      (state == AgentVaultState.locked ||
          gateway.hasPendingOperation ||
          failure?.safeActions.contains('reopen_vault') == true ||
          (failure?.retryPolicy != 'never' &&
              failure?.safeActions.contains('retry') == true));
  String? get reasonCode => _safeToken(failure?.reasonCode ?? failure?.failure);
  String? get incidentId => _safeToken(failure?.incidentId);

  /// Called once by AppRuntime after native callback services are available.
  /// Repeated observers join this same attempt, including its failure result.
  Future<void> open() => _opening ??= _run(_open);

  Future<AgentVaultState> _open() async {
    if (gateway.hasPendingOperation) {
      await gateway.resumePendingOperation(personId);
    }
    final current = await gateway.vaultStatus(personId);
    if (_closing || _disposed || !_desiredReady) return current;
    return switch (current) {
      AgentVaultState.missing => gateway.createVault(personId),
      AgentVaultState.locked => gateway.unlockVault(personId),
      _ => current,
    };
  }

  /// An uncertain observer rejoins the gateway's retained command identity.
  /// A new lifecycle attempt requires an explicit owner-projected safe action.
  Future<void> recover() async {
    if (!canRecover) return;
    if (failure?.safeActions.contains('reopen_vault') == true ||
        (failure == null && state == AgentVaultState.locked)) {
      _desiredReady = true;
    }
    await _run(_open);
  }

  Future<void> lock() async {
    if (_disposed || _closing || busy || !ready) return;
    _desiredReady = false;
    await _run(() async {
      await gateway.lockVault(personId);
      return gateway.vaultStatus(personId);
    });
  }

  /// A feature may report an owner-directed loss of the shared generation, but
  /// cannot repair, reopen or otherwise mutate the physical Vault itself.
  void reportFailure(AgentVaultException error) {
    final domain = error.ownerFailure?.domain ?? error.domain;
    if (_disposed ||
        _closing ||
        domain != 'vault' ||
        (error.reloadRequired != true && error.sealSession != true))
      return;
    _failureRevision++;
    _state = AgentVaultState.unavailable;
    _failure = error;
    notifyListeners();
  }

  Future<void> _run(Future<AgentVaultState> Function() operation) async {
    if (_disposed || _closing || busy) return;
    final failureRevision = _failureRevision;
    _busy = true;
    _failure = null;
    notifyListeners();
    try {
      final value = await operation();
      if (!_disposed && !_closing && failureRevision == _failureRevision) {
        _state = value;
      }
    } on Object catch (error, stackTrace) {
      final typed = error is AgentVaultException
          ? error
          : const AgentVaultException('storage_unavailable');
      if (!_disposed && !_closing && failureRevision == _failureRevision) {
        _failure = typed;
        _state = AgentVaultState.unavailable;
      }
      AppDiagnostics.error(
        component: 'vault',
        operation: typed.stage ?? 'open',
        error: StateError(
          'Local secure storage failed: ${_safeToken(typed.reasonCode ?? typed.failure) ?? 'storage_unavailable'}',
        ),
        stackTrace: stackTrace,
        failure: _safeToken(typed.failure),
        reasonCode: _safeToken(typed.reasonCode),
        failureDomain: _safeToken(typed.domain),
        failureCategory: _safeToken(typed.category),
        incidentId: _safeToken(typed.incidentId),
        requestId: _safeToken(typed.requestId),
        safeActions: typed.safeActions,
      );
    } finally {
      _busy = false;
      if (!_disposed && !_closing) notifyListeners();
    }
  }

  void closeAdmission() {
    if (_disposed || _closing) return;
    _closing = true;
    notifyListeners();
  }

  @override
  void dispose() {
    _disposed = true;
    super.dispose();
  }
}

String? _safeToken(String? value) =>
    value != null && RegExp(r'^[a-zA-Z0-9_.:-]{1,128}$').hasMatch(value)
    ? value
    : null;
