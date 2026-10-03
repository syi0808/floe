import 'package:flutter/foundation.dart';
import 'package:floe_client/app/runtime/agent_vault_gateway.dart';

/// Vault presentation/lifecycle is separate from Conversation and Run control.
final class VaultController extends ChangeNotifier {
  VaultController({required this.gateway, required this.personId});
  final AgentVaultGateway gateway;
  final String personId;
  AgentVaultState? state;
  bool busy = false;
  bool _disposed = false;

  Future<AgentVaultState> inspect() =>
      _run(() => gateway.vaultStatus(personId));
  Future<AgentVaultState> create() => _run(() => gateway.createVault(personId));
  Future<AgentVaultState> unlock() => _run(() => gateway.unlockVault(personId));

  Future<AgentVaultState> open() async {
    final current = await inspect();
    return switch (current) {
      AgentVaultState.missing => create(),
      AgentVaultState.locked => unlock(),
      _ => current,
    };
  }

  Future<void> lock() async {
    await _run(() async {
      await gateway.lockVault(personId);
      return gateway.vaultStatus(personId);
    });
  }

  Future<AgentVaultState> _run(
    Future<AgentVaultState> Function() operation,
  ) async {
    if (_disposed || busy) throw StateError('Vault request is unavailable.');
    busy = true;
    notifyListeners();
    try {
      final value = await operation();
      if (!_disposed) state = value;
      return value;
    } finally {
      busy = false;
      if (!_disposed) notifyListeners();
    }
  }

  @override
  void dispose() {
    _disposed = true;
    super.dispose();
  }
}
