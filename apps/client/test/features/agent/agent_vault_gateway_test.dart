import 'dart:io';

import 'package:floe_client/features/agent/agent_conversation_gateway.dart';
import 'package:floe_client/features/agent/agent_fixture_gateway.dart';
import 'package:floe_client/features/agent/agent_vault_gateway.dart';
import 'package:floe_client/features/agent/agent_registry.dart';
import 'package:floe_client/features/agent/agent_proposal.dart';
import 'package:floe_client/features/day_canvas/application/ffi_day_gateway.dart';
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
      final gateway = await FfiDayGateway.open(
        libraryPath: File('../../target/debug/libfloe_ffi.dylib').absolute.path,
        databasePath: path,
        deviceId: 'test-device',
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
        (gateway.secureAgent as AgentConversationGateway).resumeConversation(
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
        (gateway.secureAgent as AgentProposalGateway).inspectProposal(
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
        (gateway.secureAgent as AgentRegistryGateway).readRegistry(
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

  test(
    'configured route failure does not submit a native conversation turn',
    () async {
      var nativeCalls = 0;
      var failRoute = true;
      final jobIds = <Object?>[];
      final operations = <Object?>[];
      final gateway = NativeAgentVaultGateway(
        (request) async {
          nativeCalls++;
          jobIds.add(request['request_id']);
          operations.add((request['operation'] as Map)['kind']);
          throw const AgentVaultException('transport_unavailable');
        },
        deviceId: 'test-device',
        resolveRemoteRoute: () async {
          if (!failRoute) return null;
          throw const AgentVaultException(
            'server_model_unavailable',
            stage: 'remote_route',
          );
        },
      );
      final session = AgentSession.fromJson({
        'schema_version': 1,
        'id': 'session',
        'person_id': 'person',
        'scope': null,
        'revision': 1,
        'active_turn': null,
        'last_outcome': null,
        'continuation': null,
        'data_classes': ['personal'],
        'messages': <Object?>[],
      });
      final request = AgentConversationTurnRequest(
        session: session,
        text: 'hello',
      );

      await expectLater(
        gateway.beginConversationTurn(request),
        throwsA(
          isA<AgentVaultException>().having(
            (error) => error.failure,
            'failure',
            'server_model_unavailable',
          ),
        ),
      );
      expect(nativeCalls, 0);
      failRoute = false;
      await expectLater(
        gateway.beginConversationTurn(request),
        throwsA(
          isA<AgentVaultException>().having(
            (error) => error.failure,
            'failure',
            'transport_unavailable',
          ),
        ),
      );
      failRoute = true;
      await expectLater(
        gateway.beginConversationTurn(request),
        throwsA(
          isA<AgentVaultException>().having(
            (error) => error.failure,
            'failure',
            'server_model_unavailable',
          ),
        ),
      );
      await expectLater(
        gateway.pollConversationTurn(request, 0),
        throwsA(
          isA<AgentVaultException>().having(
            (error) => error.failure,
            'failure',
            'transport_unavailable',
          ),
        ),
      );
      expect(operations, ['submit', 'poll']);
      expect(jobIds[0], jobIds[1]);
    },
  );

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
            'correlation_request_id': 'wrong-request',
          },
        };
      }, deviceId: 'test-device');
      await expectLater(gateway.vaultStatus('test'), throwsFormatException);
    },
  );

  test(
    'pairing lifecycle uses strict proof actions without connector grants',
    () async {
      const personId = '00000000-0000-4000-8000-000000000001';
      const pairingId = '00000000-0000-4000-8000-000000000002';
      const owner = {
        'key_id': '00000000-0000-4000-8000-000000000003',
        'public_key': 'owner-public-key',
        'fingerprint': 'owner-fingerprint',
      };
      final producer = RemoteProducerIdentity(
        schemaVersion: 1,
        instanceId: '00000000-0000-4000-8000-000000000004',
        executionOwner: '00000000-0000-4000-8000-000000000005',
        audience: 'floe.server:00000000-0000-4000-8000-000000000004',
        keyId: '00000000-0000-4000-8000-000000000006',
        publicKey: 'producer-public-key',
        fingerprint: 'producer-fingerprint',
      );
      final challenge = RemotePairingChallenge(
        schemaVersion: 1,
        pairingId: pairingId,
        challengeId: '00000000-0000-4000-8000-000000000007',
        challengeB64Url: 'challenge',
        producerSignature: 'producer-signature',
        producer: producer,
        issuer: RemoteOwnerPublicKey.fromJson(owner),
        expiresAtUnixMs: 4102444800000,
      );
      final route = <String, Object?>{
        'base_url': 'http://127.0.0.1:8431',
        'bearer_token': '',
        'purpose': 'everyday_assistance',
        'external': false,
        'allow_external': false,
        'pairing': {
          'client_id': pairingId,
          'person_id': personId,
          'device_id': 'test-device',
        },
        'calendar_connections': <Object?>[],
      };
      final calls = <String>[];
      final gateway = NativeAgentVaultGateway((request) async {
        final operation = Map<String, Object?>.from(
          request['operation']! as Map,
        );
        if (operation['kind'] == 'release') return _pairingSuccess(request);
        final action = Map<String, Object?>.from(operation['action']! as Map);
        final kind = action['kind']! as String;
        calls.add(kind);
        final payload = switch (kind) {
          'remote_pairing_prepare' => {'remote_owner': owner},
          'remote_pairing_confirm' => {
            'remote_pairing': _pairingStatus(
              personId,
              pairingId,
              'local_confirmed',
            ),
          },
          'remote_pairing_status' => {
            'remote_pairing': _pairingStatus(
              personId,
              pairingId,
              'approved',
              token: 't' * 32,
            ),
          },
          'remote_pairing_finalize' => {
            'remote_pairing': _pairingStatus(
              personId,
              pairingId,
              'approved',
              token: 't' * 32,
            ),
          },
          _ => throw StateError('unexpected action $kind'),
        };
        return _pairingSuccess(request, payload);
      }, deviceId: 'test-device');

      expect(
        (await gateway.prepareRemotePairing(personId: personId)).fingerprint,
        'owner-fingerprint',
      );
      expect(
        (await gateway.confirmRemotePairing(
          personId: personId,
          route: route,
          challenge: challenge,
          pollingProof: 'polling-proof',
        )).status,
        'local_confirmed',
      );
      expect(
        (await gateway.remotePairingStatus(
          personId: personId,
          route: route,
          pairingId: pairingId,
          pollingProof: 'polling-proof',
        )).token,
        't' * 32,
      );
      expect(
        (await gateway.finalizeRemotePairing(
          personId: personId,
          route: route,
          pairingId: pairingId,
          pollingProof: 'polling-proof',
          challenge: challenge,
        )).status,
        'approved',
      );
      expect(calls, [
        'remote_pairing_prepare',
        'remote_pairing_confirm',
        'remote_pairing_status',
        'remote_pairing_finalize',
      ]);
      expect(calls.any((kind) => kind.contains('grant')), isFalse);
    },
  );

  test('pairing status accepts explicit rejection and expiry only', () {
    Map<String, Object?> status(String value) => {
      'schema_version': 1,
      'pairing_id': '00000000-0000-4000-8000-000000000002',
      'status': value,
      'person_id': '00000000-0000-4000-8000-000000000001',
      'device_id': 'test-device',
    };
    expect(RemotePairingStatus.fromJson(status('rejected')).status, 'rejected');
    expect(RemotePairingStatus.fromJson(status('expired')).status, 'expired');
    expect(
      () => RemotePairingStatus.fromJson(status('unknown')),
      throwsFormatException,
    );
    expect(
      () => RemotePairingStatus.fromJson({
        ...status('approved'),
        'token': 'token',
        'client_id': 'wrong-client',
      }),
      throwsFormatException,
    );
  });
}

Map<String, dynamic> _pairingSuccess(
  Map<String, dynamic> request, [
  Map<String, Object?> payload = const {},
]) => {
  'request_id': request['request_id'],
  'done': true,
  'events': <Object?>[],
  'next_sequence': 0,
  'state': 'ready',
  ...payload,
};

Map<String, Object?> _pairingStatus(
  String personId,
  String pairingId,
  String status, {
  String? token,
}) => {
  'schema_version': 1,
  'pairing_id': pairingId,
  'status': status,
  'person_id': personId,
  'device_id': 'test-device',
  'token': ?token,
};

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
