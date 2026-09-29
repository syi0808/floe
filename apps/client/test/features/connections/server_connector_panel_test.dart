import 'package:floe_client/features/connections/application/connector_authorization_gateway.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/l10n/app_localizations.dart';
import 'package:floe_client/features/connections/presentation/server_connector_panel.dart';
import 'package:floe_client/features/connections/application/connection_observe_gateway.dart';
import 'package:floe_client/features/connections/domain/connection_observe.dart';
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

  testWidgets('Use with Floe stays bound to the displayed connection', (
    tester,
  ) async {
    final actions = <Map<String, Object?>>[];
    final gateway = _ObserveGateway(actions, status: 'active');
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
          connectionObserveGateway: gateway,
          onBack: () {},
          onChanged: () async {},
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(
      find.byKey(const ValueKey('connection-use-with-floe')),
      findsOneWidget,
    );
    expect(actions.single['connection_id'], first.connectionId);
    expect(actions.single['kind'], 'inspect');

    await tester.tap(find.byKey(const ValueKey('connection-use-with-floe')));
    await tester.pumpAndSettle();
    expect(actions[1]['connection_id'], first.connectionId);
    expect(actions[1]['enabled'], false);

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
          connectionObserveGateway: gateway,
          onBack: () {},
          onChanged: () async {},
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(actions.last['connection_id'], second.connectionId);
    expect(actions.last['kind'], 'inspect');
  });

  testWidgets('Calendar scope edit never reviews active Observe', (
    tester,
  ) async {
    final client = _ConnectorClient(
      startResult: _attempt(ServerConnectorStatus.connected),
    );
    final operations = <Map<String, Object?>>[];
    final gateway = _ObserveGateway(operations, status: 'active');
    await tester.pumpWidget(
      _host(
        ServerConnectorPanel(
          authorization: _Authorization(),
          connector: _calendarConnector('00000000-0000-4000-8000-000000000011'),
          connection: _connection,
          client: client,
          connectionObserveGateway: gateway,
          onBack: () {},
          onChanged: () async {},
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(operations, hasLength(1));
    expect(operations.single['kind'], 'inspect');
    await tester.enterText(
      find.byKey(const Key('connector-scope-calendar_ids')),
      'opaque,id\nprimary\nopaque,id',
    );
    await tester.ensureVisible(find.text('Update scope'));
    await tester.tap(find.text('Update scope'));
    await tester.pumpAndSettle();
    expect(client.updatedScope, {
      'calendar_ids': ['opaque,id', 'primary'],
    });
    expect(operations, hasLength(1));
  });

  testWidgets('enabling reviews the bundle and echoes it back', (tester) async {
    final actions = <Map<String, Object?>>[];
    final gateway = _ObserveGateway(actions, status: 'paused');
    await tester.pumpWidget(
      _host(
        ServerConnectorPanel(
          authorization: _Authorization(),
          connector: _calendarConnector('00000000-0000-4000-8000-000000000011'),
          connection: _connection,
          client: LocalServerClient(
            store: MemoryServerCredentials(),
            deviceId: _connection.deviceId,
          ),
          connectionObserveGateway: gateway,
          onBack: () {},
          onChanged: () async {},
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(actions.single['kind'], 'inspect');

    await tester.tap(find.byKey(const ValueKey('connection-use-with-floe')));
    await tester.pumpAndSettle();
    expect(find.text('Allow Floe to read Google Calendar?'), findsOneWidget);
    expect(find.text('calendar.timeline'), findsOneWidget);

    await tester.tap(find.text('Allow'));
    await tester.pumpAndSettle();
    expect(actions.length, 3);
    expect(actions[1]['kind'], 'review');
    expect(actions[2]['kind'], 'set_enabled');
    expect(actions[2]['enabled'], true);
    final echoed = actions[2]['expected'] as ConnectionObserveReview;
    expect(echoed.members, ['calendar.timeline']);
  });

  testWidgets('dismissing the review enables nothing', (tester) async {
    final actions = <Map<String, Object?>>[];
    final gateway = _ObserveGateway(actions, status: 'paused');
    await tester.pumpWidget(
      _host(
        ServerConnectorPanel(
          authorization: _Authorization(),
          connector: _calendarConnector('00000000-0000-4000-8000-000000000011'),
          connection: _connection,
          client: LocalServerClient(
            store: MemoryServerCredentials(),
            deviceId: _connection.deviceId,
          ),
          connectionObserveGateway: gateway,
          onBack: () {},
          onChanged: () async {},
        ),
      ),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.byKey(const ValueKey('connection-use-with-floe')));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Cancel'));
    await tester.pumpAndSettle();
    expect(actions.where((action) => action['enabled'] == true), isEmpty);
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

ServerConnector _calendarConnector(String connectionId) => ServerConnector(
  id: 'calendar.google',
  name: 'Google Calendar',
  authKind: 'oauth_pkce',
  available: true,
  status: ServerConnectorStatus.connected,
  requiredScopes: const ['calendar.read'],
  scopeFields: const ['calendar_ids'],
  capabilities: _capabilities,
  scope: const {
    'calendar_ids': ['primary'],
  },
  connectionId: connectionId,
  connectionRevision: 1,
);

final class _ObserveGateway implements ConnectionObserveGateway {
  _ObserveGateway(this.actions, {required this.status});

  final List<Map<String, Object?>> actions;
  String status;

  @override
  Future<ConnectionObserveOverview> inspect({
    required String connectorId,
    required String connectionId,
  }) async {
    actions.add({
      'kind': 'inspect',
      'connector_id': connectorId,
      'connection_id': connectionId,
    });
    return _overview(connectorId, connectionId);
  }

  @override
  Future<ConnectionObserveReview> review({
    required String connectorId,
    required String connectionId,
  }) async {
    actions.add({
      'kind': 'review',
      'connector_id': connectorId,
      'connection_id': connectionId,
    });
    return ConnectionObserveReview.fromJson({
      'connector_id': connectorId,
      'connection_id': connectionId,
      'source_authority': {
        'incarnation': '00000000-0000-4000-8000-0000000000a1',
        'epoch': 3,
      },
      'connection_revision': 1,
      'native_subject': null,
      'producer_fingerprint': 'a' * 64,
      'members': [
        {
          'view_id': 'calendar.timeline',
          'resource': 'connection/$connectionId/view/calendar.timeline',
          'policy_digest': 'a' * 64,
          'expected_grant_id': null,
          'expected_grant_authority': null,
        },
      ],
    });
  }

  @override
  Future<ConnectionObserveOverview> setEnabled({
    required String connectorId,
    required String connectionId,
    required bool enabled,
    bool disconnecting = false,
    ConnectionObserveReview? expected,
  }) async {
    actions.add({
      'kind': 'set_enabled',
      'connector_id': connectorId,
      'connection_id': connectionId,
      'enabled': enabled,
      'disconnecting': disconnecting,
      'expected': expected,
    });
    status = enabled ? 'active' : 'paused';
    return _overview(connectorId, connectionId);
  }

  ConnectionObserveOverview _overview(
    String connectorId,
    String connectionId,
  ) => ConnectionObserveOverview.fromJson({
    'connector_id': connectorId,
    'connection_id': connectionId,
    'status': status,
    'enabled': status == 'active',
    'source_resources': ['primary'],
    'members': [
      {
        'view_id': 'calendar.timeline',
        'state': status == 'active' ? 'active' : 'paused',
        'review_required': false,
      },
    ],
  });
}

Widget _host(Widget child) => MaterialApp(
  theme: FloeTheme.light,
  localizationsDelegates: AppLocalizations.localizationsDelegates,
  supportedLocales: AppLocalizations.supportedLocales,
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
  Map<String, Object?>? updatedScope;
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

  @override
  Future<Map<String, Object?>> updateConnectorScope({
    required ServerConnection connection,
    required String connectorId,
    required String connectionId,
    required int connectionRevision,
    required Map<String, Object?> scope,
  }) async {
    updatedScope = scope;
    return {};
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
