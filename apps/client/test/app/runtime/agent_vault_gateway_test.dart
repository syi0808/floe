import '../../support/app_host.dart';

import 'dart:io';

import 'package:floe_client/features/conversation/application/agent_conversation_gateway.dart';
import 'package:floe_client/app/runtime/agent_vault_gateway.dart';
import 'package:floe_client/features/experts/domain/agent_registry.dart';
import 'package:floe_client/features/actions/domain/agent_proposal.dart';
import 'package:floe_client/features/conversation/application/conversation_runtime_gateway.dart';
import 'package:floe_client/features/day/application/native_day_gateway.dart';
import 'package:floe_client/infrastructure/diagnostics/app_diagnostics.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'real Dart/C ABI status does not provision storage or access keys',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'floe-vault-boundary',
      );
      final path = '${directory.path}/day.db';
      final gateway = await TestAppHost.open(
        libraryPath: File('../../target/debug/libfloe_ffi.dylib').absolute.path,
        databasePath: path,
        deviceId: 'test-device',
      );
      addTearDown(() async {
        await gateway.close();
        await directory.delete(recursive: true);
      });
      expect(
        await gateway.runtime.vault.vaultStatus(localPersonId),
        AgentVaultState.missing,
      );
      expect(
        (gateway.runtime.vault as ConversationRuntimeProvider)
            .conversationRuntime,
        isNotNull,
      );
      expect(await Directory('$path.agent-vaults').exists(), isFalse);
      await expectLater(
        (gateway.runtime.vault as AgentConversationGateway).resumeConversation(
          localPersonId,
        ),
        throwsA(
          isA<AgentVaultException>().having(
            (error) => error.failure,
            'failure',
            'vault_unavailable',
          ),
        ),
      );
      expect(await Directory('$path.agent-vaults').exists(), isFalse);
      await expectLater(
        (gateway.runtime.vault as AgentProposalGateway).inspectProposal(
          personId: localPersonId,
          sessionId: '00000000-0000-4000-8000-000000000002',
          invocationId: '00000000-0000-4000-8000-000000000003',
        ),
        throwsA(
          isA<AgentVaultException>().having(
            (error) => error.failure,
            'failure',
            'vault_unavailable',
          ),
        ),
      );
      expect(await Directory('$path.agent-vaults').exists(), isFalse);
      await expectLater(
        (gateway.runtime.vault as AgentRegistryGateway).readRegistry(
          localPersonId,
        ),
        throwsA(
          isA<AgentVaultException>().having(
            (error) => error.failure,
            'failure',
            'vault_unavailable',
          ),
        ),
      );
      expect(await Directory('$path.agent-vaults').exists(), isFalse);
      await expectLater(
        gateway.runtime.vault.resumeConversation(localPersonId),
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
      final gateway = NativeAgentVaultGateway(
        transport.call,
        deviceId: 'test-device',
      );
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
      final gateway = NativeAgentVaultGateway(
        transport.call,
        deviceId: 'test-device',
      );
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
      deviceId: 'test-device',
    );
    await expectLater(gateway.vaultStatus('test'), throwsFormatException);
  });

  test('completed failure retains request identity and stage', () async {
    AppDiagnostics.clear();
    String? requestId;
    final gateway = NativeAgentVaultGateway((request) async {
      requestId = request['request_id']! as String;
      return {
        'request_id': requestId,
        'done': true,
        'events': <Object?>[],
        'next_sequence': 0,
        'state': 'ready',
        'session': null,
        'failure': {
          'schema_version': 1,
          'domain': 'capability',
          'category': 'transient',
          'reason_code': 'model_unavailable',
          'kind': 'model_unavailable',
          'stage': 'status',
          'safe_actions': <String>[],
          'affected_refs': <String>[],
          'incident_id': requestId,
          'retry_policy': 'never',
          'retryable': false,
          'recovery_action': 'none',
          'reload_required': false,
          'seal_session': false,
          'correlation_request_id': requestId,
        },
      };
    }, deviceId: 'test-device');

    await expectLater(
      gateway.vaultStatus('test'),
      throwsA(
        isA<AgentVaultException>()
            .having(
              (error) => error.requestId == requestId,
              'requestId matches transport request',
              isTrue,
            )
            .having((error) => error.stage, 'stage', 'status')
            .having((error) => error.retryable, 'retryable', isFalse),
      ),
    );
    final diagnostic = AppDiagnostics.records.single;
    expect(diagnostic.requestId, requestId);
    expect(diagnostic.operation, 'status');
    expect(diagnostic.failure, 'model_unavailable');
  });

  test('native failures require the versioned envelope', () async {
    final gateway = NativeAgentVaultGateway((request) async {
      final id = request['request_id']! as String;
      return {
        'request_id': id,
        'done': true,
        'events': <Object?>[],
        'next_sequence': 0,
        'state': 'ready',
        'session': null,
        'failure': 'capability_unavailable',
      };
    }, deviceId: 'test-device');
    await expectLater(gateway.vaultStatus('test'), throwsFormatException);
  });

  test('retryability is valid only for an explicit read retry', () async {
    Future<Map<String, dynamic>> response(Map<String, Object?> request) async {
      final id = request['request_id']! as String;
      return {
        'request_id': id,
        'done': true,
        'events': <Object?>[],
        'next_sequence': 0,
        'state': 'ready',
        'session': null,
        'failure': {
          'schema_version': 1,
          'domain': 'capability',
          'category': 'transient',
          'reason_code': 'capability_unavailable',
          'kind': 'capability_unavailable',
          'stage': 'status',
          'safe_actions': <String>[],
          'affected_refs': <String>[],
          'incident_id': id,
          'retry_policy': 'never',
          'retryable': true,
          'recovery_action': 'none',
          'reload_required': false,
          'seal_session': false,
          'correlation_request_id': id,
        },
      };
    }

    final gateway = NativeAgentVaultGateway(response, deviceId: 'test-device');
    await expectLater(gateway.vaultStatus('test'), throwsFormatException);
  });

  test(
    'failure stage and correlation cannot be rewritten by transport',
    () async {
      final gateway = NativeAgentVaultGateway((request) async {
        return {
          'request_id': request['request_id'],
          'done': true,
          'events': <Object?>[],
          'next_sequence': 0,
          'state': 'ready',
          'session': null,
          'failure': {
            'schema_version': 1,
            'domain': 'turn',
            'category': 'integrity',
            'reason_code': 'conflict',
            'kind': 'conflict',
            'stage': 'conversation_turn',
            'safe_actions': <String>[],
            'affected_refs': <String>[],
            'incident_id': request['request_id'],
            'retry_policy': 'never',
            'retryable': false,
            'recovery_action': 'refresh_session',
            'reload_required': false,
            'seal_session': false,
            'correlation_request_id': 'wrong-request',
          },
        };
      }, deviceId: 'test-device');
      await expectLater(gateway.vaultStatus('test'), throwsFormatException);
    },
  );
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
