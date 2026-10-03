import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/app/runtime/owner_operation.dart';
import 'package:floe_client/app/runtime/agent_vault_gateway.dart';
import 'package:floe_client/app/runtime/native_transport.dart';
import 'package:floe_client/features/conversation/application/agent_request_id.dart';

final class AppWireVaultGateway implements AgentVaultGateway {
  AppWireVaultGateway(this._transport);

  final AppWireTransport _transport;
  final OwnerOperationObserver _operations = OwnerOperationObserver();

  @override
  bool get hasPendingOperation => _operations.hasPendingOperation;

  @override
  Future<AgentVaultState> vaultStatus(String personId) async {
    // Status is a pure query, not a queued lifecycle job. It has no receipt to
    // release and can never occupy the mutation observer's pending slot.
    final id = newAgentRequestId();
    try {
      final result = await ownerQuery(_transport, id, {'kind': 'vault.status'});
      if (result['kind'] != 'vault_operation' ||
          result['operation_id'] != id ||
          result['done'] != true ||
          result['failure'] != null) {
        throw const FormatException('Invalid Vault status result');
      }
      return AgentVaultState.values.byName(result['state'] as String);
    } on NativeTransportException catch (error) {
      if (error.ownerFailure == null &&
          (error.code == 'timeout' || error.code == 'ffi')) {
        throw AgentVaultException(
          error.code,
          requestId: id,
          stage: 'status',
          domain: 'transport',
          safeActions: const ['retry'],
          retryPolicy: 'explicit',
          retryableOverride: true,
        );
      }
      throw AgentVaultException.fromAppWire(
        error.metadata['reason_code'] ?? error.code,
        requestId: id,
        stage: 'status',
        metadata: error.metadata,
        ownerFailure: error.ownerFailure,
      );
    }
  }

  @override
  Future<AgentVaultState> resumePendingOperation(String personId) =>
      _operations.resume<AgentVaultState>(scope: personId);
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
    return _operations.observe(
      scope: personId,
      intent: ownerIntent(intent),
      stage: (intent['kind']! as String).split('.').last,
      resultKind: 'vault_operation',
      start: (operationId) => ownerCommand(_transport, operationId, intent),
      read: (operationId, release) =>
          ownerResult(_transport, 'vault.read_result', operationId, release),
      decode: decode,
    );
  }
}
