import 'dart:async';
import 'dart:convert';

import 'package:floe_client/features/connections/application/local_server_client.dart';
import 'package:floe_client/features/connections/presentation/local_server_panel.dart';
import 'package:floe_client/app/runtime/agent_vault_gateway.dart';
import 'package:floe_client/app/local_identity.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../support/server_credentials.dart';

void main() {
  testWidgets(
    'stalled Keychain reads stop waiting without forgetting credentials',
    (tester) async {
      final pending = Completer<String?>();
      final calls = <String>[];
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        KeychainServerCredentialStore.channel,
        (call) {
          calls.add(call.method);
          return pending.future;
        },
      );
      addTearDown(() {
        pending.complete(null);
        tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
          KeychainServerCredentialStore.channel,
          null,
        );
      });
      final result = expectLater(
        KeychainServerCredentialStore().read(),
        throwsA(isA<TimeoutException>()),
      );
      await tester.pump(const Duration(seconds: 5));
      await result;
      expect(calls, ['read']);
    },
  );

  test('addresses stay literal loopback with no path or credentials', () {
    expect(
      LocalServerClient.normalizeAddress('http://localhost:9431/'),
      'http://127.0.0.1:9431',
    );
    for (final value in [
      'https://example.com',
      'http://192.168.1.1:8431',
      'http://127.0.0.1:8431/path',
      'http://user@127.0.0.1',
      'http://127.0.0.1?key=1',
      'http://127.0.0.1#secret',
      'http://127.0.0.1:0',
    ]) {
      expect(
        () => LocalServerClient.normalizeAddress(value),
        throwsA(isA<ServerConnectionException>()),
      );
    }
  });

  test(
    'connection credentials persist independently of provider keys',
    () async {
      final store = MemoryServerCredentials();
      final client = LocalServerClient(store: store);
      final saved = ServerConnection(
        address: 'http://127.0.0.1:9431',
        token: 'a' * 52,
        clientId: 'fixture-client',
        personId: '00000000-0000-4000-8000-000000000001',
        deviceId: 'local-client',
      );
      await client.save(saved);
      final restored = await LocalServerClient(store: store).connection();
      expect(restored?.toJson().keys, [
        'base_url',
        'token',
        'client_id',
        'person_id',
        'device_id',
        'allow_external',
        'external_recipients',
      ]);
      expect(restored?.allowExternal, isFalse);
      final consented = saved.withExternalConsent(
        true,
        recipients: ['Example AI'],
      );
      expect(consented.allowExternal, isTrue);
      expect(consented.coversExternalRecipient('Example AI'), isTrue);
      await client.save(consented);
      final restoredConsent = await client.connection();
      expect(restoredConsent!.externalRecipients, ['Example AI']);
      expect(
        restoredConsent.coversExternalRecipient('Different recipient'),
        isFalse,
      );
      expect(consented.toJson(), isNot(contains('model')));
      expect(consented.toJson(), isNot(contains('inference_class')));
      await store.delete();
      expect(await client.connection(), isNull);
    },
  );

  test('invalid saved credential fails closed', () async {
    final store = MemoryServerCredentials()
      ..value = jsonEncode({
        'base_url': 'http://127.0.0.1:8431',
        'token': 'a' * 52,
        'client_id': 'fixture',
        'allow_external': false,
        'external_recipients': <String>[],
      });
    await expectLater(
      LocalServerClient(store: store).connection(),
      throwsA(isA<ServerConnectionException>()),
    );
  });

  test('connection identity must match the active Person and device', () {
    final client = LocalServerClient(
      store: MemoryServerCredentials(),
      deviceId: 'current-device',
    );
    final connection = ServerConnection(
      address: 'http://127.0.0.1:8431',
      token: 'a' * 52,
      clientId: 'fixture',
      personId: '00000000-0000-4000-8000-000000000001',
      deviceId: 'other-device',
    );
    expect(
      () => client.save(connection),
      throwsA(
        isA<ServerConnectionException>().having(
          (error) => error.code,
          'code',
          'connection_identity_mismatch',
        ),
      ),
    );
  });

  testWidgets(
    'server panel exposes address entry and recovers from corrupt storage',
    (tester) async {
      final store = MemoryServerCredentials()..value = 'invalid';
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: SingleChildScrollView(
              child: LocalServerPanel(client: LocalServerClient(store: store)),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();
      expect(find.byKey(const Key('server-address')), findsOneWidget);
      expect(
        find.text('Saved connection is invalid. Forget it and pair again.'),
        findsOneWidget,
      );
      await tester.tap(find.text('Forget connection'));
      await tester.pumpAndSettle();
      expect(store.value, isNull);
      expect(find.text('Pair this device'), findsOneWidget);
      await tester.pumpWidget(const SizedBox());
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets('pairing cancellation sends the strict pairing identity', (
    tester,
  ) async {
    final client = _PairingClient();
    final gateway = _PairingGateway();
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SingleChildScrollView(
            child: LocalServerPanel(client: client, pairingGateway: gateway),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Pair this device'));
    await tester.pumpAndSettle();
    expect(find.text('Cancel pairing'), findsOneWidget);
    await tester.tap(find.text('Cancel pairing'));
    await tester.pump(const Duration(seconds: 2));
    await tester.pumpAndSettle();
    expect(client.cancelBody, {
      'schema_version': 1,
      'pairing_id': '00000000-0000-4000-8000-000000000002',
      'proof': 'polling-proof',
    });
  });
}

final class _PairingClient extends LocalServerClient {
  _PairingClient() : super(store: MemoryServerCredentials());

  Map<String, Object?>? cancelBody;

  @override
  Future<ServerPairingStart> startPairingStrict(
    String address, {
    required String issuerKeyId,
    required String issuerPublicKey,
  }) async => ServerPairingStart(
    pairingId: '00000000-0000-4000-8000-000000000002',
    proof: 'polling-proof',
    code: 'ABCD1234',
    expiresAt: DateTime.now().toUtc().add(const Duration(minutes: 5)),
    personId: personId,
    deviceId: deviceId,
    producer: _pairingProducer,
    issuer: _pairingIssuer,
    challengeId: '00000000-0000-4000-8000-000000000003',
    challengeB64Url: 'challenge',
    producerSignature: 'signature',
  );

  @override
  Future<Map<String, dynamic>> request(
    String address,
    String path, {
    Map<String, Object?>? body,
    String? token,
  }) async {
    if (path == '/pair/cancel') cancelBody = body;
    return <String, dynamic>{};
  }
}

final class _PairingGateway implements RemotePairingGateway {
  @override
  Future<RemoteOwnerPublicKey> prepareRemotePairing({
    required String personId,
  }) async => RemoteOwnerPublicKey.fromJson(_pairingIssuer);

  @override
  Future<RemotePairingStatus> confirmRemotePairing({
    required String personId,
    required Map<String, Object?> route,
    required RemotePairingChallenge challenge,
    required String pollingProof,
  }) async => _pairingStatus('local_confirmed');

  @override
  Future<RemotePairingStatus> remotePairingStatus({
    required String personId,
    required Map<String, Object?> route,
    required String pairingId,
    required String pollingProof,
  }) async => _pairingStatus('pending');

  @override
  Future<RemotePairingStatus> finalizeRemotePairing({
    required String personId,
    required Map<String, Object?> route,
    required String pairingId,
    required String pollingProof,
    required RemotePairingChallenge challenge,
  }) async => _pairingStatus('approved', token: 't' * 32);
}

RemotePairingStatus _pairingStatus(String status, {String? token}) =>
    RemotePairingStatus(
      schemaVersion: 1,
      pairingId: '00000000-0000-4000-8000-000000000002',
      status: status,
      personId: defaultLocalPersonId,
      deviceId: 'local-client',
      token: token,
    );

const _pairingIssuer = {
  'key_id': '00000000-0000-4000-8000-000000000004',
  'public_key': 'owner-public-key',
  'fingerprint': 'owner-fingerprint',
};

const _pairingProducer = {
  'schema_version': 1,
  'instance_id': '00000000-0000-4000-8000-000000000005',
  'execution_owner': '00000000-0000-4000-8000-000000000006',
  'audience': 'floe.server:fixture',
  'key_id': '00000000-0000-4000-8000-000000000007',
  'public_key': 'producer-public-key',
  'fingerprint': 'producer-fingerprint',
};
