import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/app/runtime/owner_operation.dart';
import 'package:floe_client/app/runtime/agent_vault_gateway.dart';

final class AppWireVaultGateway implements AgentVaultGateway {
  AppWireVaultGateway(this._transport);

  final AppWireTransport _transport;
  final OwnerOperationObserver _operations = OwnerOperationObserver();

  @override
  bool get hasPendingOperation => _operations.hasPendingOperation;

  @override
  Future<AgentVaultState> vaultStatus(String personId) =>
      _access(personId, 'status');
  @override
  Future<AgentVaultState> createVault(String personId) =>
      _access(personId, 'create');
  @override
  Future<AgentVaultState> unlockVault(String personId) =>
      _access(personId, 'unlock');
  @override
  Future<void> lockVault(String personId) async {
    await _access(personId, 'lock');
  }

  Future<AgentVaultState> _access(String personId, String kind) async {
    return _observe(
      personId,
      {'kind': 'vault.$kind'},
      decode: (result) {
        return AgentVaultState.values.byName(result['state'] as String);
      },
    );
  }

  Future<T> _observe<T>(
    String personId,
    Map<String, Object?> intent, {
    required T Function(Map<String, dynamic>) decode,
  }) {
    final command = const <String>{
      'vault.create',
      'vault.unlock',
      'vault.lock',
    }.contains(intent['kind']);
    return _operations.observe(
      scope: personId,
      intent: ownerIntent(intent),
      stage: (intent['kind']! as String).split('.').last,
      resultKind: 'vault_operation',
      start: (operationId) => command
          ? ownerCommand(_transport, operationId, intent)
          : ownerQuery(_transport, operationId, intent),
      read: (operationId, release) =>
          ownerResult(_transport, 'vault.read_result', operationId, release),
      decode: decode,
    );
  }
}
