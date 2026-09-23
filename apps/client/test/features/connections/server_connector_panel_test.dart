import 'package:floe_client/features/connections/application/connector_authorization_gateway.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/app/floe_button.dart';
import 'package:floe_client/features/connections/presentation/server_connector_panel.dart';
import 'package:floe_client/features/connections/application/remote_access_gateway.dart';
import 'package:floe_client/features/connections/application/local_server_client.dart';
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
  personId: '00000000-0000-4000-8000-000000000001',
  deviceId: 'local-test-device',
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
          authorization: _Authorization(),
          connector: const ServerConnector(
            id: 'home_assistant.states',
            name: 'Home Assistant',
            authKind: 'secret',
            available: true,
            status: ServerConnectorStatus.available,
            requiredScopes: ['home.states.read'],
            scopeFields: ['base_url', 'entities'],
            capabilities: _capabilities,
            scope: {},
          ),
          connection: _connection,
          client: client,
          onBack: () {},
          onChanged: () async => changed++,
        ),
      ),
    );
    await tester.enterText(
      find.byKey(const Key('connector-scope-base_url')),
      'https://home.example.test',
    );
    await tester.enterText(
      find.byKey(const Key('connector-scope-entities')),
      'sensor.office, light.desk',
    );
    await tester.enterText(
      find.byKey(const Key('connector-secret')),
      'one-shot-secret',
    );
    await tester.tap(find.text('Connect securely'));
    await tester.pumpAndSettle();
    expect(client.receivedSecret, 'one-shot-secret');
    expect(client.receivedScope, {
      'base_url': 'https://home.example.test',
      'entities': ['sensor.office', 'light.desk'],
    });
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
            authorization: _Authorization(),
            connector: _oauthConnector(),
            connection: _connection,
            client: client,
            onBack: () {},
            onChanged: () async => changed++,
            authorizationLauncher: (uri) async {
              opened = uri;
              return true;
            },
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

  testWidgets('device OAuth displays the user code while polling', (
    tester,
  ) async {
    final pending = _attempt(
      ServerConnectorStatus.connecting,
      authorizationUrl: 'https://github.com/login/device',
      userCode: 'ABCD-EFGH',
    );
    await tester.pumpWidget(
      _host(
        ServerConnectorPanel(
          authorization: _Authorization(),
          connector: _oauthConnector(),
          connection: _connection,
          client: _ConnectorClient(startResult: pending, pollResult: pending),
          onBack: () {},
          onChanged: () async {},
          authorizationLauncher: (_) async => true,
        ),
      ),
    );

    await tester.tap(find.text('Continue to authorize'));
    await tester.pumpAndSettle();

    expect(
      find.text('Enter this code on the authorization page: ABCD-EFGH'),
      findsOneWidget,
    );
  });

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
          authorization: _Authorization(),
          connector: _oauthConnector(),
          connection: _connection,
          client: client,
          onBack: () {},
          onChanged: () async {},
          authorizationLauncher: (_) async => true,
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

  testWidgets('connection grants stay bound to the displayed connection', (
    tester,
  ) async {
    final actions = <Map<String, Object?>>[];
    final gateway = NativeRemoteAccessGateway((request) async {
      final operation = Map<String, Object?>.from(request['operation']! as Map);
      if (operation['kind'] == 'read_result') return _grantSuccess(request);
      final action = operation;
      actions.add(action);
      final kind = action['kind'];
      final payload = kind == 'calendar_grant_preview'
          ? {
              'calendar_preview': {
                'schema_version': 1,
                'person_id': _connection.personId,
                'connector_id': 'calendar.google',
                'connection_id': action['connection_id'],
                'resource': 'primary',
                'source_authority': <String, Object?>{},
                'provider_identity': 'google:fixture',
                'execution_owner': 'server-owner',
                'producer': _producer,
                'consumers': <String>['floe.builtin.schedule'],
                'purpose': 'everyday_assistance',
                'recipient': 'local_only',
              },
            }
          : {
              'calendar_grant': {
                'schema_version': 1,
                'person_id': _connection.personId,
                'grant_id': '00000000-0000-4000-8000-000000000010',
                'grant_authority': {'incarnation': 'fixture', 'epoch': 1},
                'connector_id': 'calendar.google',
                'connection_id': action['connection_id'],
                'resource': 'primary',
                'source_authority': <String, Object?>{},
                'execution_owner': 'server-owner',
                'state': 'active',
                'review_required': false,
                'consumers': <String>['floe.builtin.schedule'],
                'purpose': 'everyday_assistance',
                'recipient': 'local_only',
              },
            };
      return _grantSuccess(request, payload);
    });
    final first = _calendarConnector('00000000-0000-4000-8000-000000000011');
    await tester.pumpWidget(
      _host(
        ServerConnectorPanel(
          authorization: _Authorization(),
          connector: first,
          connection: _connection,
          client: LocalServerClient(
            store: MemoryServerCredentials(),
            deviceId: _connection.deviceId,
          ),
          remoteAccessGateway: gateway,
          onBack: () {},
          onChanged: () async {},
        ),
      ),
    );
    expect(
      find.byKey(const ValueKey('connection-calendar-preview')),
      findsOneWidget,
    );
    expect(
      tester
          .widget<FloeButton>(
            find.byKey(const ValueKey('connection-calendar-preview')),
          )
          .onPressed,
      isNotNull,
    );
    await tester.ensureVisible(
      find.byKey(const ValueKey('connection-calendar-preview')),
    );
    await tester.tap(find.byKey(const ValueKey('connection-calendar-preview')));
    await tester.pumpAndSettle();
    expect(actions.single['connection_id'], first.connectionId);
    await tester.tap(find.byKey(const ValueKey('connection-calendar-review')));
    await tester.pumpAndSettle();
    expect(actions[1]['connection_id'], first.connectionId);

    final second = _calendarConnector('00000000-0000-4000-8000-000000000012');
    await tester.pumpWidget(
      _host(
        ServerConnectorPanel(
          authorization: _Authorization(),
          connector: second,
          connection: _connection,
          client: LocalServerClient(
            store: MemoryServerCredentials(),
            deviceId: _connection.deviceId,
          ),
          remoteAccessGateway: gateway,
          onBack: () {},
          onChanged: () async {},
        ),
      ),
    );
    expect(
      find.byKey(const ValueKey('connection-calendar-preview')),
      findsOneWidget,
    );
    expect(
      find.byKey(const ValueKey('connection-calendar-review')),
      findsNothing,
    );
    expect(actions, hasLength(2));
  });

  testWidgets('non-calendar connector does not expose calendar grants', (
    tester,
  ) async {
    await tester.pumpWidget(
      _host(
        ServerConnectorPanel(
          authorization: _Authorization(),
          connector: const ServerConnector(
            id: 'github.repository',
            name: 'GitHub',
            authKind: 'oauth_pkce',
            available: true,
            status: ServerConnectorStatus.connected,
            requiredScopes: ['repo.read'],
            scopeFields: [],
            capabilities: _capabilities,
            scope: {},
            connectionId: '00000000-0000-4000-8000-000000000013',
            connectionRevision: 1,
          ),
          connection: _connection,
          client: LocalServerClient(
            store: MemoryServerCredentials(),
            deviceId: _connection.deviceId,
          ),
          onBack: () {},
          onChanged: () async {},
        ),
      ),
    );
    expect(
      find.byKey(const ValueKey('connection-calendar-preview')),
      findsNothing,
    );
    expect(find.byKey(const ValueKey('connection-view-preview')), findsNothing);
  });
}

const _producer = {
  'schema_version': 1,
  'instance_id': '00000000-0000-4000-8000-000000000020',
  'execution_owner': '00000000-0000-4000-8000-000000000021',
  'audience': 'floe.server:fixture',
  'key_id': '00000000-0000-4000-8000-000000000022',
  'public_key': 'producer',
  'fingerprint': 'fingerprint',
};

ServerConnector _calendarConnector(String connectionId) => ServerConnector(
  id: 'calendar.google',
  name: 'Google Calendar',
  authKind: 'oauth_pkce',
  available: true,
  status: ServerConnectorStatus.connected,
  requiredScopes: const ['calendar.read'],
  scopeFields: const ['calendar_id'],
  capabilities: _capabilities,
  scope: const {'calendar_id': 'primary'},
  connectionId: connectionId,
  connectionRevision: 1,
);

Map<String, dynamic> _grantSuccess(
  Map<String, dynamic> request, [
  Map<String, Object?> payload = const {},
]) => {
  'operation_id':
      (request['operation'] as Map)['operation_id'] ?? request['request_id'],
  'done': true,
  ...payload,
};

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
  String? userCode,
}) => ServerConnectorAttempt(
  id: 'attempt',
  connectorId: 'microsoft.mail',
  connectionId: 'connection',
  status: status,
  createdAt: DateTime.utc(2026, 9, 11),
  authorizationUrl: authorizationUrl,
  userCode: userCode,
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

final class _Authorization implements ConnectorAuthorizationGateway {
  @override
  Future<AuthorizationDirective> startAuthorization({
    required ServerConnection connection,
    required String connectorId,
    required ServerConnectorAttempt attempt,
  }) async => attempt.status == ServerConnectorStatus.connected
      ? const AuthorizationSettled(state: 'connected')
      : OpenAuthorizationPage(
          authorizationUrl: attempt.authorizationUrl!,
          pollAfter: const Duration(milliseconds: 1),
        );

  @override
  Future<AuthorizationDirective> observeAuthorization({
    required ServerConnection connection,
    required String connectorId,
    required ServerConnectorAttempt attempt,
  }) async => attempt.status == ServerConnectorStatus.connected
      ? const AuthorizationSettled(state: 'connected')
      : const ObserveAgain(pollAfter: Duration(days: 1));

  @override
  Future<void> cancelAuthorization({
    required ServerConnection connection,
    required String connectorId,
  }) async {}
}
