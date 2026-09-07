import 'dart:io';

import 'package:floe_client/features/agent/agent_vault_gateway.dart';
import 'package:floe_client/features/day_canvas/application/ffi_day_gateway.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'real Dart/C ABI status does not provision storage or access keys',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'floe-vault-boundary',
      );
      final path = '${directory.path}/day.db';
      final gateway = await FfiDayGateway.open(
        libraryPath: File('../../target/debug/libfloe_ffi.dylib').absolute.path,
        databasePath: path,
      );
      addTearDown(() async {
        await gateway.close();
        await directory.delete(recursive: true);
      });
      expect(
        await gateway.secureAgent.vaultStatus(localPersonId),
        AgentVaultState.missing,
      );
      expect(await Directory('$path.agent-vaults').exists(), isFalse);
      await expectLater(
        gateway.secureAgent.resumeAgentFixture(localPersonId),
        throwsA(
          isA<AgentVaultException>().having(
            (error) => error.failure,
            'failure',
            'vault_unavailable',
          ),
        ),
      );
      expect(await Directory('$path.agent-vaults').exists(), isFalse);
    },
  );

  test(
    'lost create response is drained without provisioning a second key',
    () async {
      final transport = _Transport()..loseSubmit = true;
      final gateway = NativeAgentVaultGateway(transport.call);
      await expectLater(gateway.createVault('test'), throwsStateError);
      expect(transport.creates, 1);
      expect(await gateway.vaultStatus('test'), AgentVaultState.ready);
      expect(transport.creates, 1);
      expect(transport.pending, isNull);
    },
  );

  test(
    'lost release acknowledgement is resolved by read-only lookup',
    () async {
      final transport = _Transport()..loseRelease = true;
      final gateway = NativeAgentVaultGateway(transport.call);
      await expectLater(gateway.createVault('test'), throwsStateError);
      expect(await gateway.vaultStatus('test'), AgentVaultState.ready);
      expect(transport.creates, 1);
    },
  );

  test('request identity mismatch cannot be accepted as unlocked', () async {
    final transport = _Transport();
    final gateway = NativeAgentVaultGateway(
      (request) async => {
        ...await transport.call(request),
        'request_id': 'wrong-id',
      },
    );
    await expectLater(gateway.vaultStatus('test'), throwsFormatException);
  });
}

class _Transport {
  Map<String, dynamic>? pending;
  bool loseSubmit = false;
  bool loseRelease = false;
  int creates = 0;

  Future<Map<String, dynamic>> call(Map<String, Object?> request) async {
    final operation = request['operation'] as Map;
    if (operation['kind'] == 'submit') {
      if (pending != null) throw const AgentVaultException('conflict');
      final action = operation['action'] as Map;
      if (action['kind'] == 'create') creates++;
      pending = {
        'request_id': request['request_id'],
        'done': true,
        'events': <Object?>[],
        'next_sequence': 0,
        'state': creates == 0 ? 'missing' : 'ready',
        'session': null,
        'failure': null,
      };
      if (loseSubmit) {
        loseSubmit = false;
        throw StateError('lost synthetic response');
      }
    }
    if (pending == null) throw const AgentVaultException('not_found');
    final result = Map<String, dynamic>.from(pending!);
    if (operation['kind'] == 'release') {
      pending = null;
      if (loseRelease) {
        loseRelease = false;
        throw StateError('lost synthetic release');
      }
    }
    return result;
  }
}
