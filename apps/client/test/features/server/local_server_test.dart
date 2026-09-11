import 'dart:convert';

import 'package:floe_client/features/server/local_server_client.dart';
import 'package:floe_client/features/server/local_server_panel.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../support/server_credentials.dart';

void main() {
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
      );
      await client.save(saved);
      final restored = await LocalServerClient(store: store).connection();
      expect(restored?.toJson().keys, [
        'base_url',
        'token',
        'client_id',
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
        'base_url': 'https://evil.example',
        'token': 'a' * 52,
        'client_id': 'fixture',
      });
    await expectLater(
      LocalServerClient(store: store).connection(),
      throwsA(isA<ServerConnectionException>()),
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
}
