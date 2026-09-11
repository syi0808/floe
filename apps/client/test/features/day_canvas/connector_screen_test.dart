import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_squircle.dart';
import 'package:floe_client/app/floe_primitives.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/features/day_canvas/application/calendar_gateway.dart';
import 'package:floe_client/features/day_canvas/application/fake_day_gateway.dart';
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
    expect(find.text(strings.macosCalendar), findsOneWidget);
    expect(find.text(strings.appleCalendar), findsNothing);
    expect(find.textContaining('local-test-device'), findsOneWidget);
    await tester.tap(find.text(strings.macosCalendar));
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
    expect(find.text('macOS Calendar'), findsOneWidget);
  });

  for (final testCase
      in <
        ({
          TargetPlatform platform,
          String provider,
          String cardKey,
          String Function(AppLocalizations) name,
          String Function(AppLocalizations) description,
        })
      >[
        (
          platform: TargetPlatform.iOS,
          provider: 'event_kit',
          cardKey: 'connector-calendar-apple',
          name: (strings) => strings.appleCalendar,
          description: (strings) => strings.calendarsAlreadyOnThisIphoneOrIpad,
        ),
        (
          platform: TargetPlatform.macOS,
          provider: 'event_kit',
          cardKey: 'connector-calendar-apple',
          name: (strings) => strings.macosCalendar,
          description: (strings) => strings.calendarsAlreadyOnThisMac,
        ),
        (
          platform: TargetPlatform.android,
          provider: 'android',
          cardKey: 'connector-calendar-android',
          name: (strings) => strings.androidCalendar,
          description: (strings) =>
              strings.selectedCalendarsOnThisAndroidDevice,
        ),
      ]) {
    testWidgets(
      '${testCase.platform.name} exposes its connected device calendar',
      (tester) async {
        final date = DateTime.utc(2026, 9, 4);
        await tester.pumpWidget(
          MaterialApp(
            theme: FloeTheme.light,
            localizationsDelegates: AppLocalizations.localizationsDelegates,
            supportedLocales: AppLocalizations.supportedLocales,
            home: Scaffold(
              body: SingleChildScrollView(
                child: ConnectorScreen(
                  gateway: _DeviceCalendarGateway(),
                  query: DayQuery(
                    personId: 'test',
                    date: date,
                    now: date,
                    timezoneOffsetSeconds: 0,
                  ),
                  connection: CalendarConnection(
                    connectionId: '00000000-0000-4000-8000-000000000010',
                    deviceId: 'test-device',
                    provider: testCase.provider,
                    revision: 1,
                    calendars: const [
                      ConnectedCalendar(id: 'calendar', name: 'Calendar'),
                    ],
                  ),
                  onChanged: () async {},
                  deviceId: 'local-test-device',
                  platform: testCase.platform,
                ),
              ),
            ),
          ),
        );
        await tester.pumpAndSettle();
        final strings = AppLocalizations.of(
          tester.element(find.byType(ConnectorScreen)),
        );
        final name = testCase.name(strings);
        final description = testCase.description(strings);
        expect(find.byKey(Key(testCase.cardKey)), findsOneWidget);
        expect(find.text(name), findsOneWidget);
        expect(find.text(description), findsOneWidget);
        expect(find.text(strings.connectedServicesCount(1)), findsOneWidget);
        expect(find.text('Connected'), findsOneWidget);

        await tester.tap(find.text(name));
        await tester.pumpAndSettle();
        expect(find.text(name), findsNWidgets(2));
        expect(find.text(description), findsOneWidget);
        expect(find.text(strings.backToConnections), findsOneWidget);
        expect(tester.takeException(), isNull);
      },
    );
  }

  testWidgets('does not count a connection from another device provider', (
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
              gateway: _DeviceCalendarGateway(),
              query: DayQuery(
                personId: 'test',
                date: date,
                now: date,
                timezoneOffsetSeconds: 0,
              ),
              connection: const CalendarConnection(
                connectionId: '00000000-0000-4000-8000-000000000010',
                deviceId: 'test-device',
                provider: 'event_kit',
                revision: 1,
                calendars: [],
              ),
              onChanged: () async {},
              platform: TargetPlatform.android,
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    final strings = AppLocalizations.of(
      tester.element(find.byType(ConnectorScreen)),
    );
    expect(find.byKey(const Key('connector-calendar-android')), findsOneWidget);
    expect(find.text(strings.availableServices), findsOneWidget);
    expect(find.text('Available'), findsOneWidget);
    expect(find.textContaining('connected service'), findsNothing);
  });

  testWidgets('binds the exact server Calendar selector into Rust state', (
    tester,
  ) async {
    final gateway = _RecordingCalendarGateway();
    var changed = 0;
    final date = DateTime.utc(2026, 9, 4);
    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: Scaffold(
          body: ConnectorScreen(
            gateway: gateway,
            query: DayQuery(
              personId: '00000000-0000-4000-8000-000000000001',
              date: date,
              now: date,
              timezoneOffsetSeconds: 0,
            ),
            connection: null,
            onChanged: () async => changed++,
            serverClient: _CalendarCatalogClient(),
            platform: TargetPlatform.macOS,
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(gateway.connectionId, '8a1d7fb0-435d-5d1e-aab4-53ed2894da61');
    expect(gateway.connectionRevision, 7);
    expect(gateway.deviceId, 'local-test-device');
    expect(gateway.provider, 'google_calendar');
    expect(gateway.boundCalendars.single.id, 'primary@example.test');
    expect(changed, 1);
  });

  testWidgets('requires an explicit choice when two server calendars connect', (
    tester,
  ) async {
    final gateway = _RecordingCalendarGateway();
    var changed = 0;
    final date = DateTime.utc(2026, 9, 4);
    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: Scaffold(
          body: SingleChildScrollView(
            child: ConnectorScreen(
              gateway: gateway,
              query: DayQuery(
                personId: '00000000-0000-4000-8000-000000000001',
                date: date,
                now: date,
                timezoneOffsetSeconds: 0,
              ),
              connection: null,
              onChanged: () async => changed++,
              serverClient: _CalendarCatalogClient(
                connectors: const [
                  _googleCalendarConnector,
                  _microsoftCalendarConnector,
                ],
              ),
              platform: TargetPlatform.macOS,
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(gateway.connectionId, isNull);
    expect(changed, 0);
    expect(
      find.text(
        'Choose which connected calendar Floe and Schedule should use.',
      ),
      findsOneWidget,
    );
    expect(find.text('Use for Floe & Schedule'), findsNWidgets(2));

    await tester.tap(
      find.descendant(
        of: find.byKey(const Key('calendar-provider-calendar.microsoft')),
        matching: find.text('Use for Floe & Schedule'),
      ),
    );
    await tester.pumpAndSettle();

    expect(gateway.connectionId, _microsoftCalendarConnector.connectionId);
    expect(gateway.connectionRevision, 11);
    expect(gateway.provider, 'microsoft_calendar');
    expect(gateway.boundCalendars.single.id, 'calendar@microsoft.test');
    expect(changed, 1);
  });

  testWidgets(
    'keeps device calendar active until server is explicitly chosen',
    (tester) async {
      final gateway = _RecordingCalendarGateway();
      var changed = 0;
      final date = DateTime.utc(2026, 9, 4);
      await tester.pumpWidget(
        MaterialApp(
          theme: FloeTheme.light,
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          supportedLocales: AppLocalizations.supportedLocales,
          home: Scaffold(
            body: SingleChildScrollView(
              child: ConnectorScreen(
                gateway: gateway,
                query: DayQuery(
                  personId: '00000000-0000-4000-8000-000000000001',
                  date: date,
                  now: date,
                  timezoneOffsetSeconds: 0,
                ),
                connection: const CalendarConnection(
                  connectionId: '00000000-0000-4000-8000-000000000020',
                  deviceId: 'local-test-device',
                  provider: 'event_kit',
                  revision: 4,
                  calendars: [
                    ConnectedCalendar(id: 'device-calendar', name: 'Personal'),
                  ],
                ),
                onChanged: () async => changed++,
                serverClient: _CalendarCatalogClient(),
                platform: TargetPlatform.macOS,
              ),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(gateway.connectionId, isNull);
      expect(changed, 0);
      expect(
        find.text(
          'macOS Calendar supplies calendar context to Floe and Schedule.',
        ),
        findsOneWidget,
      );
      expect(
        find.descendant(
          of: find.byKey(const Key('calendar-provider-device')),
          matching: find.text('Active'),
        ),
        findsOneWidget,
      );

      await tester.tap(
        find.descendant(
          of: find.byKey(const Key('calendar-provider-calendar.google')),
          matching: find.text('Use for Floe & Schedule'),
        ),
      );
      await tester.pumpAndSettle();

      expect(gateway.provider, 'google_calendar');
      expect(changed, 1);
    },
  );

  testWidgets(
    'offers device calendar selection while a server calendar is active',
    (tester) async {
      final date = DateTime.utc(2026, 9, 4);
      await tester.pumpWidget(
        MaterialApp(
          theme: FloeTheme.light,
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          supportedLocales: AppLocalizations.supportedLocales,
          home: Scaffold(
            body: SingleChildScrollView(
              child: ConnectorScreen(
                gateway: _DeviceCalendarGateway(),
                query: DayQuery(
                  personId: '00000000-0000-4000-8000-000000000001',
                  date: date,
                  now: date,
                  timezoneOffsetSeconds: 0,
                ),
                connection: const CalendarConnection(
                  connectionId: '8a1d7fb0-435d-5d1e-aab4-53ed2894da61',
                  deviceId: 'local-test-device',
                  provider: 'google_calendar',
                  revision: 7,
                  calendars: [
                    ConnectedCalendar(
                      id: 'primary@example.test',
                      name: 'Google Calendar',
                    ),
                  ],
                ),
                onChanged: () async {},
                serverClient: _CalendarCatalogClient(),
                platform: TargetPlatform.macOS,
              ),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(
        find.text(
          'Google Calendar supplies calendar context to Floe and Schedule.',
        ),
        findsOneWidget,
      );
      await tester.tap(
        find.descendant(
          of: find.byKey(const Key('calendar-provider-device')),
          matching: find.text('Choose calendars'),
        ),
      );
      await tester.pumpAndSettle();
      expect(find.text('Back to connections'), findsOneWidget);
      expect(find.text('Connect'), findsOneWidget);
    },
  );
}

final class _RecordingCalendarGateway extends _DeviceCalendarGateway {
  String? connectionId;
  int? connectionRevision;
  String? deviceId;
  String? provider;
  List<CalendarChoice> boundCalendars = const [];

  @override
  Future<DaySnapshot> bindCalendarConnection({
    required String connectionId,
    required int connectionRevision,
    required String deviceId,
    required String provider,
    required List<CalendarChoice> calendars,
    required DayQuery query,
  }) {
    this.connectionId = connectionId;
    this.connectionRevision = connectionRevision;
    this.deviceId = deviceId;
    this.provider = provider;
    boundCalendars = calendars;
    return FakeDayGateway().loadDay(query);
  }
}

class _DeviceCalendarGateway implements CalendarGateway {
  @override
  Future<DaySnapshot> bindCalendarConnection({
    required String connectionId,
    required int connectionRevision,
    required String deviceId,
    required String provider,
    required List<CalendarChoice> calendars,
    required DayQuery query,
  }) => FakeDayGateway().loadDay(query);

  @override
  Future<List<CalendarChoice>> calendars() async => const [];

  @override
  Future<DaySnapshot> disconnectCalendar(DayQuery query) =>
      FakeDayGateway().loadDay(query);

  @override
  Future<void> openCalendarSettings() async {}

  @override
  Future<DaySnapshot> selectCalendar(CalendarChoice calendar, DayQuery query) =>
      FakeDayGateway().loadDay(query);

  @override
  Future<DaySnapshot> selectCalendars(
    List<CalendarChoice> calendars,
    DayQuery query, {
    bool includeAll = false,
  }) => FakeDayGateway().loadDay(query);

  @override
  Future<DaySnapshot> syncCalendar(DayQuery query) =>
      FakeDayGateway().loadDay(query);
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

final class _CalendarCatalogClient extends LocalServerClient {
  _CalendarCatalogClient({this.connectors = const [_googleCalendarConnector]})
    : super(store: MemoryServerCredentials(), deviceId: 'local-test-device');

  final List<ServerConnector> connectors;

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
  ) async => ServerConnectorCatalog(
    personId: '00000000-0000-4000-8000-000000000001',
    deviceId: 'local-test-device',
    connectors: connectors,
  );
}

const _googleCalendarConnector = ServerConnector(
  id: 'calendar.google',
  name: 'Google Calendar',
  authKind: 'oauth_pkce',
  available: true,
  status: ServerConnectorStatus.connected,
  requiredScopes: ['calendar.readonly'],
  scopeFields: ['calendar_id'],
  capabilities: ServerConnectorCapabilities(
    connect: true,
    cancel: true,
    disconnect: true,
    scopeUpdate: true,
  ),
  scope: {'calendar_id': 'primary@example.test'},
  connectionId: '8a1d7fb0-435d-5d1e-aab4-53ed2894da61',
  connectionRevision: 7,
);

const _microsoftCalendarConnector = ServerConnector(
  id: 'calendar.microsoft',
  name: 'Microsoft Calendar',
  authKind: 'oauth_pkce',
  available: true,
  status: ServerConnectorStatus.connected,
  requiredScopes: ['calendar.readonly'],
  scopeFields: ['calendar_id'],
  capabilities: ServerConnectorCapabilities(
    connect: true,
    cancel: true,
    disconnect: true,
    scopeUpdate: true,
  ),
  scope: {'calendar_id': 'calendar@microsoft.test'},
  connectionId: '3d2e7a71-194b-4b47-84cc-b58c5ce17772',
  connectionRevision: 11,
);
