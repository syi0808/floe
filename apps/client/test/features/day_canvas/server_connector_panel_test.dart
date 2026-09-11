import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/features/day_canvas/presentation/server_connector_panel.dart';
import 'package:floe_client/features/server/local_server_client.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../support/server_credentials.dart';

const _capabilities = ServerConnectorCapabilities(
  connect: true,
  cancel: true,
  disconnect: true,
  scopeUpdate: true,
);

final _connection = ServerConnection(
  address: 'http://127.0.0.1:8431',
  token: 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
  clientId: 'fixture',
);

void main() {
  testWidgets('secret connector sends credential once with selected scope', (
    tester,
  ) async {
    final client = _ConnectorClient(
      startResult: _attempt(ServerConnectorStatus.connected),
    );
    var changed = 0;
    await tester.pumpWidget(
      _host(
        ServerConnectorPanel(
          connector: const ServerConnector(
            id: 'github.issues',
            name: 'GitHub Issues',
            authKind: 'secret',
            available: true,
            status: ServerConnectorStatus.available,
            requiredScopes: ['github.issues.read'],
            scopeFields: ['owner', 'repository'],
            capabilities: _capabilities,
            scope: {},
          ),
          connection: _connection,
          client: client,
          legacyUnscoped: false,
          onBack: () {},
          onChanged: () async => changed++,
        ),
      ),
    );
    await tester.enterText(
      find.byKey(const Key('connector-scope-owner')),
      'floe',
    );
    await tester.enterText(
      find.byKey(const Key('connector-scope-repository')),
      'client',
    );
    await tester.enterText(
      find.byKey(const Key('connector-secret')),
      'one-shot-secret',
    );
    await tester.tap(find.text('Connect securely'));
    await tester.pumpAndSettle();
    expect(client.receivedSecret, 'one-shot-secret');
    expect(client.receivedScope, {'owner': 'floe', 'repository': 'client'});
    expect(changed, 1);
    expect(find.byKey(const Key('connector-secret')), findsNothing);
    expect(find.text('one-shot-secret'), findsNothing);
  });

  testWidgets(
    'OAuth connector opens authorization URL and polls to connected',
    (tester) async {
      final client = _ConnectorClient(
        startResult: _attempt(
          ServerConnectorStatus.connecting,
          authorizationUrl: 'https://login.example.test/authorize?state=opaque',
        ),
        pollResult: _attempt(ServerConnectorStatus.connected),
      );
      Uri? opened;
      var changed = 0;
      await tester.pumpWidget(
        _host(
          ServerConnectorPanel(
            connector: _oauthConnector(),
            connection: _connection,
            client: client,
            legacyUnscoped: false,
            onBack: () {},
            onChanged: () async => changed++,
            authorizationLauncher: (uri) async {
              opened = uri;
              return true;
            },
            pollInterval: const Duration(milliseconds: 1),
          ),
        ),
      );
      await tester.tap(find.text('Continue to authorize'));
      await tester.pump(const Duration(milliseconds: 5));
      await tester.pumpAndSettle();
      expect(opened?.host, 'login.example.test');
      expect(client.polledAttempt, 'attempt');
      expect(find.text('Connected'), findsOneWidget);
      expect(changed, 1);
    },
  );

  testWidgets('pending OAuth attempt can be cancelled', (tester) async {
    final pending = _attempt(
      ServerConnectorStatus.connecting,
      authorizationUrl: 'https://login.example.test/authorize',
    );
    final client = _ConnectorClient(
      startResult: pending,
      pollResult: pending,
      cancelResult: _attempt(ServerConnectorStatus.available),
    );
    await tester.pumpWidget(
      _host(
        ServerConnectorPanel(
          connector: _oauthConnector(),
          connection: _connection,
          client: client,
          legacyUnscoped: false,
          onBack: () {},
          onChanged: () async {},
          authorizationLauncher: (_) async => true,
          pollInterval: const Duration(days: 1),
        ),
      ),
    );
    await tester.tap(find.text('Continue to authorize'));
    await tester.pumpAndSettle();
    expect(find.text('Cancel connection'), findsOneWidget);
    await tester.ensureVisible(find.text('Cancel connection'));
    await tester.tap(find.text('Cancel connection'));
    await tester.pumpAndSettle();
    expect(client.cancelledAttempt, 'attempt');
    expect(find.text('Available'), findsOneWidget);
  });

  testWidgets('legacy pairing disables mutation and explains re-pair', (
    tester,
  ) async {
    await tester.pumpWidget(
      _host(
        ServerConnectorPanel(
          connector: _oauthConnector(),
          connection: _connection,
          client: _ConnectorClient(
            startResult: _attempt(ServerConnectorStatus.connecting),
          ),
          legacyUnscoped: true,
          onBack: () {},
          onChanged: () async {},
        ),
      ),
    );
    expect(find.textContaining('pair this device again'), findsOneWidget);
    final button = tester.widget<FilledButton>(
      find.widgetWithText(FilledButton, 'Continue to authorize'),
    );
    expect(button.onPressed, isNull);
  });
}

Widget _host(Widget child) => MaterialApp(
  theme: FloeTheme.light,
  home: Scaffold(body: SingleChildScrollView(child: child)),
);

ServerConnector _oauthConnector() => const ServerConnector(
  id: 'microsoft.mail',
  name: 'Microsoft Mail',
  authKind: 'oauth_pkce',
  available: true,
  status: ServerConnectorStatus.available,
  requiredScopes: ['Mail.Read'],
  scopeFields: [],
  capabilities: _capabilities,
  scope: {},
);

ServerConnectorAttempt _attempt(
  ServerConnectorStatus status, {
  String? authorizationUrl,
}) => ServerConnectorAttempt(
  id: 'attempt',
  connectorId: 'microsoft.mail',
  connectionId: 'connection',
  status: status,
  createdAt: DateTime.utc(2026, 9, 11),
  authorizationUrl: authorizationUrl,
);

final class _ConnectorClient extends LocalServerClient {
  _ConnectorClient({
    required this.startResult,
    this.pollResult,
    this.cancelResult,
  }) : super(store: MemoryServerCredentials(), deviceId: 'local-test-device');

  final ServerConnectorAttempt startResult;
  final ServerConnectorAttempt? pollResult;
  final ServerConnectorAttempt? cancelResult;
  String? receivedSecret;
  Map<String, Object?>? receivedScope;
  String? polledAttempt;
  String? cancelledAttempt;

  @override
  Future<ServerConnectorAttempt> connectConnector({
    required ServerConnection connection,
    required String connectorId,
    required Map<String, Object?> scope,
    String? secret,
  }) async {
    receivedSecret = secret;
    receivedScope = scope;
    return startResult;
  }

  @override
  Future<ServerConnectorAttempt> connectorAttempt({
    required ServerConnection connection,
    required String connectorId,
    required String attemptId,
  }) async {
    polledAttempt = attemptId;
    return pollResult ?? startResult;
  }

  @override
  Future<ServerConnectorAttempt> cancelConnectorAttempt({
    required ServerConnection connection,
    required String connectorId,
    required String attemptId,
  }) async {
    cancelledAttempt = attemptId;
    return cancelResult ?? startResult;
  }
}
