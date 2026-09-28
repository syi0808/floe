import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_squircle.dart';
import 'package:floe_client/app/floe_primitives.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/features/day/application/calendar_gateway.dart';
import 'package:floe_client/features/day/application/fake_day_gateway.dart';
import 'package:floe_client/features/day/domain/day_models.dart';
import 'package:floe_client/features/connections/domain/native_calendar_access.dart';
import 'package:floe_client/features/connections/domain/calendar_source_connection.dart';
import 'package:floe_client/features/connections/application/calendar_connection_view.dart';
import 'package:floe_client/features/connections/application/calendar_source_gateway.dart';
import 'package:floe_client/features/connections/presentation/connector_screen.dart';
import 'package:floe_client/features/conversation/application/agent_controller.dart';
import 'package:floe_client/infrastructure/native/apple_context_gateway.dart';
import 'package:floe_client/features/connections/application/local_server_client.dart';
import 'package:floe_client/l10n/app_localizations.dart';
import 'package:figma_squircle/figma_squircle.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../support/server_credentials.dart';
import '../../support/agent_registry.dart';

void main() {
  testWidgets('Apple connections expose status and exact connection detail', (
    tester,
  ) async {
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
                date: DateTime.utc(2026, 9, 4),
                now: DateTime.utc(2026, 9, 4),
                timezoneOffsetSeconds: 0,
              ),
              connection: null,
              onChanged: () async {},
              appleContext: _AppleConnections(),
              platform: TargetPlatform.iOS,
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(
      find.byKey(const ValueKey('connector-contacts.apple')),
      findsOneWidget,
    );
    expect(
      find.byKey(const ValueKey('connector-attention.apple')),
      findsOneWidget,
    );
    expect(
      find.byKey(const ValueKey('connector-feasibility.apple')),
      findsOneWidget,
    );
    expect(
      find.byKey(const ValueKey('connector-health.apple')),
      findsOneWidget,
    );

    await tester.tap(find.byKey(const ValueKey('connector-contacts.apple')));
    await tester.pumpAndSettle();
    expect(find.textContaining('Connection contacts.apple'), findsOneWidget);
    expect(find.textContaining('apple-test'), findsOneWidget);
    expect(find.text('System access'), findsOneWidget);
    expect(find.text('Allow access'), findsOneWidget);
    expect(find.text('Review selection'), findsNothing);
  });

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
    expect(find.textContaining('Bound to this device'), findsNothing);
    await tester.tap(find.text(strings.macosCalendar));
    await tester.pumpAndSettle();
    expect(find.text(strings.backToConnections), findsOneWidget);
    await tester.tap(find.text(strings.backToConnections));
    await tester.pumpAndSettle();
    expect(find.text(strings.availableServices), findsOneWidget);
    expect(find.text('Remote server connection'), findsNothing);
    expect(tester.takeException(), isNull);
  });

  testWidgets('macOS detail composes system, selected, and consumer access', (
    tester,
  ) async {
    final controller = AgentController(
      gateway: TestRegistryGateway(),
      personId: registryPerson,
    );
    addTearDown(controller.dispose);
    await controller.load();
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
                personId: registryPerson,
                date: DateTime.utc(2026, 9, 4),
                now: DateTime.utc(2026, 9, 4),
                timezoneOffsetSeconds: 0,
              ),
              connection: const CalendarConnectionView(
                connectionId: '00000000-0000-4000-8000-000000000010',
                deviceId: 'test-device',
                provider: 'event_kit',
                revision: 1,
                calendars: [ConnectedCalendar(id: 'home', name: 'Home')],
              ),
              onChanged: () async {},
              platform: TargetPlatform.macOS,
              initialDeviceCalendarDetail: true,
              agentController: controller,
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(
      find.byKey(const ValueKey('calendar-system-access')),
      findsOneWidget,
    );
    expect(find.text('Allowed'), findsOneWidget);
    expect(find.text('Calendars available to Floe'), findsOneWidget);
    expect(find.text('Data Floe can use'), findsNothing);
    expect(find.byKey(const ValueKey('calendar-access-setup')), findsNothing);
  });

  testWidgets('device detail manages one Use with Floe control', (
    tester,
  ) async {
    final access = _StubCalendarAccessGateway();
    final calendarIds = List.generate(11, (index) => 'calendar-$index');
    access.overview = NativeCalendarAccessOverview(
      personId: 'person',
      provider: 'event_kit',
      connectionId: 'connection',
      selectedResources: calendarIds,
      grantedResources: const [],
      sourceAuthority: access.overview.sourceAuthority,
      state: 'needs_review',
      reviewRequired: true,
    );
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
                personId: 'person',
                date: DateTime.utc(2026, 9, 4),
                now: DateTime.utc(2026, 9, 4),
                timezoneOffsetSeconds: 0,
              ),
              connection: CalendarConnectionView(
                connectionId: 'connection',
                deviceId: 'test-device',
                provider: 'event_kit',
                revision: 1,
                sourceAuthority: const CalendarSourceAuthority(
                  incarnation: '00000000-0000-4000-8000-000000000009',
                  epoch: 1,
                ),
                calendars: [
                  for (final calendarId in calendarIds)
                    ConnectedCalendar(id: calendarId, name: calendarId),
                ],
              ),
              onChanged: () async {},
              platform: TargetPlatform.macOS,
              initialDeviceCalendarDetail: true,
              nativeCalendarAccessGateway: access,
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(
      find.byKey(const ValueKey('device-calendar-observe')),
      findsOneWidget,
    );
    expect(find.text('Needs review'), findsOneWidget);
    expect(
      find.byKey(const ValueKey('device-calendar-use-with-floe')),
      findsOneWidget,
    );

    await tester.ensureVisible(
      find.byKey(const ValueKey('device-calendar-use-with-floe')),
    );
    await tester.tap(
      find.byKey(const ValueKey('device-calendar-use-with-floe')),
    );
    await tester.pumpAndSettle();
    expect(access.calls, ['inspect', 'preview', 'review']);
    expect(access.previewedCalendarIds, calendarIds);
    expect(access.reviewedCalendarIds, calendarIds);
    expect(access.reviewedFingerprint, 'f' * 64);
    expect(find.text('Active'), findsOneWidget);

    await tester.ensureVisible(
      find.byKey(const ValueKey('device-calendar-use-with-floe')),
    );
    await tester.tap(
      find.byKey(const ValueKey('device-calendar-use-with-floe')),
    );
    await tester.pumpAndSettle();
    expect(access.calls.last, 'pause');
    expect(access.pausedGrantId, isNotNull);
    expect(find.text('Paused'), findsOneWidget);
  });

  testWidgets(
    'calendar resource refresh preserves active Observe without review',
    (tester) async {
      final access = _StubCalendarAccessGateway();
      final initial = access.overview;
      access.overview = NativeCalendarAccessOverview(
        personId: initial.personId,
        provider: initial.provider,
        connectionId: initial.connectionId,
        selectedResources: const ['home'],
        grantedResources: const ['home'],
        sourceAuthority: initial.sourceAuthority,
        state: 'active',
        reviewRequired: false,
        grantId: 'grant',
        grantAuthority: const {'access_epoch': 1},
        consumerPolicy: const {'epoch': 1},
      );
      Widget screen(List<String> calendarIds, int revision) => MaterialApp(
        theme: FloeTheme.light,
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: Scaffold(
          body: SingleChildScrollView(
            child: ConnectorScreen(
              gateway: _DeviceCalendarGateway(),
              query: DayQuery(
                personId: 'person',
                date: DateTime.utc(2026, 9, 4),
                now: DateTime.utc(2026, 9, 4),
                timezoneOffsetSeconds: 0,
              ),
              connection: CalendarConnectionView(
                connectionId: 'connection',
                deviceId: 'test-device',
                provider: 'event_kit',
                revision: revision,
                sourceAuthority: CalendarSourceAuthority(
                  incarnation: '00000000-0000-4000-8000-000000000009',
                  epoch: revision,
                ),
                calendars: [
                  for (final calendarId in calendarIds)
                    ConnectedCalendar(id: calendarId, name: calendarId),
                ],
              ),
              onChanged: () async {},
              platform: TargetPlatform.macOS,
              initialDeviceCalendarDetail: true,
              nativeCalendarAccessGateway: access,
            ),
          ),
        ),
      );

      await tester.pumpWidget(screen(const ['home'], 1));
      await tester.pumpAndSettle();
      expect(access.calls, ['inspect']);
      access.overview = NativeCalendarAccessOverview(
        personId: initial.personId,
        provider: initial.provider,
        connectionId: initial.connectionId,
        selectedResources: const ['home', 'work'],
        grantedResources: const ['home', 'work'],
        sourceAuthority: const {
          'incarnation': '00000000-0000-4000-8000-000000000009',
          'epoch': 2,
        },
        state: 'active',
        reviewRequired: false,
        grantId: 'grant',
        grantAuthority: const {'access_epoch': 1},
        consumerPolicy: const {'epoch': 1},
      );
      await tester.pumpWidget(screen(const ['home', 'work'], 2));
      await tester.pumpAndSettle();
      expect(access.calls, ['inspect', 'inspect']);
      expect(find.text('Active'), findsOneWidget);
      expect(
        find.text('Floe uses the current calendars in this connection.'),
        findsOneWidget,
      );
      expect(find.textContaining('Using 1 of 2'), findsNothing);
    },
  );

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
    expect(find.text('Unavailable services'), findsOneWidget);
    expect(find.textContaining('Bound to this device'), findsNothing);
    expect(find.textContaining('Floe server ·'), findsNothing);
    expect(
      tester.getTopLeft(find.byKey(const Key('connector-github.issues'))).dy,
      lessThan(tester.getTopLeft(find.byKey(const Key('connector-gmail'))).dy),
    );
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
                  connection: CalendarConnectionView(
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
              connection: const CalendarConnectionView(
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
            calendarSourceGateway: gateway,
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
    expect(gateway.expectedRevision, isNull);
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
              calendarSourceGateway: gateway,
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
    expect(gateway.expectedRevision, isNull);
    expect(gateway.provider, 'microsoft_calendar');
    expect(gateway.boundCalendars.single.id, 'calendar@microsoft.test');
    expect(changed, 1);
  });

  testWidgets('catalog refresh cannot disconnect a remote source', (
    tester,
  ) async {
    final gateway = _RecordingCalendarGateway();
    final source = gateway._source(
      'calendar.google',
      '8a1d7fb0-435d-5d1e-aab4-53ed2894da61',
      const [
        CalendarSourceResource(
          handle: 'primary@example.test',
          label: 'Primary',
        ),
      ],
    );
    final date = DateTime.utc(2026, 9, 4);
    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: Scaffold(
          body: ConnectorScreen(
            gateway: gateway,
            calendarSourceGateway: gateway,
            query: DayQuery(
              personId: '00000000-0000-4000-8000-000000000001',
              date: date,
              now: date,
              timezoneOffsetSeconds: 0,
            ),
            connection: CalendarConnectionView.compose(source, null),
            remoteCalendarSources: [source],
            onChanged: () async {},
            serverClient: _CalendarCatalogClient(connectors: const []),
            platform: TargetPlatform.macOS,
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(gateway.disconnectCount, 0);
    expect(gateway.connectionId, isNull);
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
                calendarSourceGateway: gateway,
                query: DayQuery(
                  personId: '00000000-0000-4000-8000-000000000001',
                  date: date,
                  now: date,
                  timezoneOffsetSeconds: 0,
                ),
                connection: const CalendarConnectionView(
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
                connection: const CalendarConnectionView(
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

final class _RecordingCalendarGateway extends _DeviceCalendarGateway
    implements CalendarSourceGateway {
  String? connectionId;
  int? expectedRevision;
  String? deviceId;
  String? provider;
  List<CalendarChoice> boundCalendars = const [];
  int disconnectCount = 0;

  @override
  Future<CalendarSourceConnection> bindRemote(
    String personId, {
    required String connectorId,
    required String connectionId,
    required List<CalendarSourceResource> resources,
    CalendarSourceConnection? current,
  }) async {
    this.connectionId = connectionId;
    expectedRevision = current?.revision;
    deviceId = 'local-test-device';
    provider = connectorId == 'calendar.google'
        ? 'google_calendar'
        : 'microsoft_calendar';
    boundCalendars = [
      for (final resource in resources)
        CalendarChoice(resource.handle, resource.label, provider: provider!),
    ];
    return _source(connectorId, connectionId, resources);
  }

  @override
  Future<List<CalendarSourceConnection>> inspectRemote(String personId) async =>
      const [];

  @override
  Future<CalendarSourceConnection?> inspectNative(String personId) async =>
      null;

  @override
  Future<CalendarSourceConnection> establishNative(
    String personId, {
    required String resourceMode,
    required List<CalendarSourceResource> resources,
  }) async => _source('calendar.event_kit', 'native', resources);

  @override
  Future<CalendarSourceConnection> configureNative(
    String personId, {
    required CalendarSourceConnection current,
    required String resourceMode,
    required List<CalendarSourceResource> resources,
  }) async => current;

  @override
  Future<CalendarSourceConnection> reconcileNativeInventory(
    String personId, {
    required CalendarSourceConnection current,
    required List<CalendarSourceResource> resources,
  }) async => current;

  @override
  Future<CalendarSourceConnection> disconnectNative(
    String personId, {
    required CalendarSourceConnection current,
  }) async => current;

  @override
  Future<CalendarSourceConnection> disconnectRemote(
    String personId, {
    required CalendarSourceConnection current,
  }) async {
    disconnectCount++;
    return current;
  }

  CalendarSourceConnection _source(
    String connectorId,
    String connectionId,
    List<CalendarSourceResource> resources,
  ) => CalendarSourceConnection(
    connectorId: connectorId,
    connectionId: connectionId,
    executionOwnerId: 'local-test-device',
    state: 'ready',
    revision: 1,
    sourceAuthority: const CalendarSourceAuthority(
      incarnation: '00000000-0000-4000-8000-000000000009',
      epoch: 1,
    ),
    resourceMode: 'selected',
    resources: resources,
  );
}

final class _StubCalendarAccessGateway implements NativeCalendarAccessGateway {
  NativeCalendarAccessOverview overview = const NativeCalendarAccessOverview(
    personId: 'person',
    provider: 'event_kit',
    connectionId: 'connection',
    selectedResources: ['home'],
    grantedResources: [],
    sourceAuthority: {
      'incarnation': '00000000-0000-4000-8000-000000000009',
      'epoch': 1,
    },
    state: 'needs_review',
    reviewRequired: true,
  );
  final List<String> calls = [];
  String? reviewedFingerprint;
  List<String>? previewedCalendarIds;
  List<String>? reviewedCalendarIds;
  String? pausedGrantId;

  NativeCalendarAccessOverview _granted(String state) =>
      NativeCalendarAccessOverview(
        personId: overview.personId,
        provider: overview.provider,
        connectionId: overview.connectionId,
        selectedResources: overview.selectedResources,
        grantedResources: overview.selectedResources,
        sourceAuthority: overview.sourceAuthority,
        state: state,
        reviewRequired: false,
        grantId: 'grant',
        grantAuthority: const {'access_epoch': 1},
        consumerPolicy: const {'epoch': 1},
      );

  @override
  Future<NativeCalendarAccessOverview> inspectCalendarAccess(
    String personId,
  ) async {
    calls.add('inspect');
    return overview;
  }

  @override
  Future<NativeCalendarSubjectPreview> previewCalendarSubject({
    required String personId,
    required String provider,
    required String connectionId,
    required List<String> calendarIds,
    required String connectionScope,
    required int connectionRevision,
    required Map<String, Object?> sourceAuthority,
  }) async {
    calls.add('preview');
    previewedCalendarIds = calendarIds;
    return NativeCalendarSubjectPreview(
      provider: provider,
      deviceId: 'test-device',
      calendarIds: calendarIds,
      connectionScope: connectionScope,
      connectionId: connectionId,
      connectionRevision: connectionRevision,
      sourceAuthority: sourceAuthority,
      nativeSubjectFingerprint: 'f' * 64,
    );
  }

  @override
  Future<NativeCalendarAccessOverview> reviewCalendarAccess(
    String personId, {
    required String connectionId,
    required List<String> calendarIds,
    required Map<String, Object?> expectedSourceAuthority,
    required String expectedNativeSubjectFingerprint,
    required NativeCalendarAccessOverview reviewedOverview,
  }) async {
    calls.add('review');
    reviewedCalendarIds = calendarIds;
    reviewedFingerprint = expectedNativeSubjectFingerprint;
    overview = _granted('active');
    return overview;
  }

  @override
  Future<NativeCalendarAccessOverview> pauseCalendarAccess(
    String personId, {
    required NativeCalendarAccessOverview reviewedOverview,
  }) async {
    calls.add('pause');
    pausedGrantId = reviewedOverview.grantId;
    overview = _granted('paused');
    return overview;
  }

  @override
  Future<NativeCalendarAccessOverview> removeCalendarAccess(
    String personId, {
    required NativeCalendarAccessOverview reviewedOverview,
  }) async {
    calls.add('remove');
    overview = _granted('revoked');
    return overview;
  }
}

class _DeviceCalendarGateway
    implements CalendarGateway, CalendarSystemAccessGateway {
  @override
  Future<CalendarSystemAccess> inspectCalendarAccess() async =>
      CalendarSystemAccess.allowed;
  @override
  Future<List<CalendarChoice>> calendars() async => const [];

  @override
  Future<void> openCalendarSettings() async {}

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
        authKind: 'oauth_pkce',
        available: true,
        status: ServerConnectorStatus.available,
        requiredScopes: ['github.issues.read'],
        scopeFields: ['owner', 'repository'],
        capabilities: ServerConnectorCapabilities(
          connect: true,
          cancel: true,
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

final class _AppleConnections implements AppleContextApi {
  @override
  Future<List<Map<String, dynamic>>> connections() async => [
    _appleConnection('contacts.apple', 'apple_contacts', 'revoked'),
    _appleConnection('attention.apple', 'apple_screen_time', 'unsupported'),
    _appleConnection('feasibility.apple', 'apple_feasibility', 'pending'),
    _appleConnection('health.apple', 'apple_health', 'pending'),
  ];

  @override
  Future<bool> requestPermission(AppleContextSource source) async => false;

  @override
  Future<Map<String, dynamic>> readContacts({
    int limit = 64,
    List<String>? selectedHandles,
  }) async => {};

  @override
  Future<Map<String, dynamic>> readFeasibility(
    AppleFeasibilityQuery query,
  ) async => {};

  @override
  Future<Map<String, dynamic>> readWellbeing() async => {};

  @override
  Future<Map<String, dynamic>> screenTimeCapability() async => {};
}

Map<String, dynamic> _appleConnection(
  String id,
  String provider,
  String state,
) {
  final viewId = switch (provider) {
    'apple_contacts' => 'people.identity',
    'apple_feasibility' => 'schedule.feasibility',
    'apple_health' || 'apple_screen_time' => 'wellbeing.derived',
    _ => 'attention.coarse',
  };
  final capability = switch (provider) {
    'apple_contacts' => 'contacts.identity.read',
    'apple_feasibility' => 'schedule.feasibility.read',
    'apple_health' => 'health.derived.read',
    _ => 'attention.coarse.read',
  };
  final scopes = switch (provider) {
    'apple_contacts' => ['CNContactStore.contacts.read'],
    'apple_feasibility' => ['CLLocationManager.whenInUse'],
    'apple_health' => ['HKHealthStore.derived.read'],
    _ => ['FamilyControls.authorization'],
  };
  final descriptor = {
    'schema_version': 1,
    'id': id,
    'version': '1.0.0',
    'provider': provider,
    'execution': {'kind': 'device', 'device_id': 'apple-test'},
    'capabilities': [
      {
        'schema_version': 1,
        'id': capability,
        'version': '1.0.0',
        'authority': 'observe',
        'required_scopes': scopes,
        'output_view_id': viewId,
      },
    ],
    'views': [
      {
        'schema_version': 1,
        'id': viewId,
        'version': '1.0.0',
        'data_class': 'personal',
        'retention': 'ephemeral',
        'freshness_ttl_ms': 300000,
        'max_items': 64,
        'max_bytes': 32768,
        'provenance_required': true,
      },
    ],
  };
  final connection = <String, dynamic>{
    'schema_version': 1,
    'connector_id': id,
    'state': state,
    'granted_scopes': state == 'revoked' ? <String>[] : scopes,
    'observed_at_unix_ms': 2000,
  };
  if (state == 'revoked' || state == 'unsupported') {
    connection['last_failure'] = {
      'kind': state == 'unsupported' ? 'unsupported' : 'permission_denied',
      'observed_at_unix_ms': 2000,
    };
  }
  return {
    'descriptor': descriptor,
    'connection': connection,
    'views': <Object?>[],
  };
}
