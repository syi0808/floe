import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_squircle.dart';
import 'package:floe_client/app/floe_primitives.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/features/day_canvas/domain/day_models.dart';
import 'package:floe_client/features/day_canvas/presentation/connector_screen.dart';
import 'package:floe_client/features/server/local_server_client.dart';
import 'package:floe_client/l10n/app_localizations.dart';
import 'package:figma_squircle/figma_squircle.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../support/server_credentials.dart';

void main() {
  testWidgets('service has only an icon surface and still opens details', (
    tester,
  ) async {
    final date = DateTime.utc(2026, 9, 4);
    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: Scaffold(
          body: SingleChildScrollView(
            child: ConnectorScreen(
              gateway: null,
              query: DayQuery(
                personId: 'test',
                date: date,
                now: date,
                timezoneOffsetSeconds: 0,
              ),
              connection: null,
              onChanged: () async {},
              deviceId: 'local-test-device',
              platform: TargetPlatform.macOS,
            ),
          ),
        ),
      ),
    );
    final strings = AppLocalizations.of(
      tester.element(find.byType(ConnectorScreen)),
    );
    expect(find.byType(FloePressable), findsOneWidget);
    final iconSurface = tester.widget<FloeSquircle>(
      find.byWidgetPredicate(
        (widget) => widget is FloeSquircle && widget.child is Icon,
      ),
    );
    expect(iconSurface.child, isA<Icon>());
    expect(iconSurface.borderWidth, 0);
    final serviceMaterial = tester.widget<Material>(
      find
          .ancestor(of: find.byType(InkWell), matching: find.byType(Material))
          .first,
    );
    expect(serviceMaterial.color, FloePalette.neutral0);
    expect(serviceMaterial.clipBehavior, Clip.antiAlias);
    final serviceShape = serviceMaterial.shape! as SmoothRectangleBorder;
    expect(serviceShape.side.color, FloePalette.neutral200);
    expect(serviceShape.side.width, 1);
    expect(find.text(strings.macosCalendar), findsNothing);
    expect(find.text('Apple Calendar'), findsOneWidget);
    expect(find.textContaining('local-test-device'), findsOneWidget);
    await tester.tap(find.text('Apple Calendar'));
    await tester.pumpAndSettle();
    expect(find.text(strings.backToConnections), findsOneWidget);
    await tester.tap(find.text(strings.backToConnections));
    await tester.pumpAndSettle();
    expect(find.text(strings.availableServices), findsOneWidget);
    expect(find.text('Remote server connection'), findsNothing);
    expect(tester.takeException(), isNull);
  });

  testWidgets('device-native and disconnected server catalog are composed', (
    tester,
  ) async {
    final date = DateTime.utc(2026, 9, 4);
    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: Scaffold(
          body: SingleChildScrollView(
            child: ConnectorScreen(
              gateway: null,
              query: DayQuery(
                personId: 'test',
                date: date,
                now: date,
                timezoneOffsetSeconds: 0,
              ),
              connection: null,
              onChanged: () async {},
              serverClient: _CatalogClient(),
              deviceId: 'local-test-device',
              platform: TargetPlatform.macOS,
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.byKey(const Key('connector-calendar-apple')), findsOneWidget);
    expect(find.byKey(const Key('connector-github.issues')), findsOneWidget);
    expect(find.byKey(const Key('connector-gmail')), findsOneWidget);
    expect(find.text('GitHub Issues'), findsOneWidget);
    expect(find.text('Gmail'), findsOneWidget);
    expect(find.text('Unavailable'), findsNWidgets(2));
    expect(find.text('macOS Calendar'), findsNothing);
  });
}

final class _CatalogClient extends LocalServerClient {
  _CatalogClient()
    : super(store: MemoryServerCredentials(), deviceId: 'local-test-device');

  @override
  Future<ServerConnection?> connection() async => ServerConnection(
    address: 'http://127.0.0.1:8431',
    token: 'a' * 32,
    clientId: 'fixture',
    personId: '00000000-0000-4000-8000-000000000001',
    deviceId: 'local-test-device',
  );

  @override
  Future<ServerConnectorCatalog> connectorCatalog(
    ServerConnection connection,
  ) async => const ServerConnectorCatalog(
    personId: '00000000-0000-4000-8000-000000000001',
    deviceId: 'local-test-device',
    connectors: [
      ServerConnector(
        id: 'github.issues',
        name: 'GitHub Issues',
        authKind: 'secret',
        available: true,
        status: ServerConnectorStatus.available,
        requiredScopes: ['github.issues.read'],
        scopeFields: ['owner', 'repository'],
        capabilities: ServerConnectorCapabilities(
          connect: true,
          cancel: false,
          disconnect: true,
          scopeUpdate: true,
        ),
        scope: {},
      ),
      ServerConnector(
        id: 'gmail',
        name: 'Gmail',
        authKind: 'oauth_pkce',
        available: false,
        status: ServerConnectorStatus.unavailable,
        requiredScopes: ['gmail.readonly'],
        scopeFields: [],
        capabilities: ServerConnectorCapabilities(
          connect: true,
          cancel: true,
          disconnect: true,
          scopeUpdate: false,
        ),
        scope: {},
      ),
    ],
  );
}
