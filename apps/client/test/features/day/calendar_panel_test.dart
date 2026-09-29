import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/features/connections/application/calendar_connection_view.dart';
import 'package:floe_client/features/connections/application/calendar_source_gateway.dart';
import 'package:floe_client/features/connections/domain/source_connection.dart';
import 'package:floe_client/app/floe_selection.dart';
import 'package:floe_client/features/day/application/calendar_gateway.dart';
import 'package:floe_client/features/day/application/fake_day_gateway.dart';
import 'package:floe_client/features/day/domain/day_models.dart';
import 'package:floe_client/features/day/presentation/calendar_panel.dart';
import 'package:floe_client/l10n/app_localizations.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

class PanelCalendarGateway
    implements
        CalendarGateway,
        CalendarSystemAccessGateway,
        CalendarSourceGateway {
  List<CalendarChoice> selected = [];
  int syncCount = 0;
  int settingsCount = 0;
  CalendarSystemAccess systemAccess = CalendarSystemAccess.allowed;

  @override
  Future<CalendarSystemAccess> inspectCalendarAccess() async => systemAccess;

  @override
  Future<List<CalendarChoice>> calendars() async => const [
    CalendarChoice('home', 'Home'),
    CalendarChoice('work', 'Work'),
  ];

  @override
  Future<SourceConnection?> inspectNative(String personId) async =>
      null;

  @override
  Future<List<SourceConnection>> inspectRemote(String personId) async =>
      const [];

  @override
  Future<SourceConnection> establishNative(
    String personId, {
    required String resourceMode,
    required List<SourceResource> resources,
  }) async {
    selected = [
      for (final resource in resources)
        CalendarChoice(resource.handle, resource.label),
    ];
    return _source(resources);
  }

  @override
  Future<SourceConnection> configureNative(
    String personId, {
    required SourceConnection current,
    required String resourceMode,
    required List<SourceResource> resources,
  }) => establishNative(
    personId,
    resourceMode: resourceMode,
    resources: resources,
  );

  @override
  Future<SourceConnection> reconcileNativeInventory(
    String personId, {
    required SourceConnection current,
    required List<SourceResource> resources,
  }) async => current;

  @override
  Future<SourceConnection> disconnectNative(
    String personId, {
    required SourceConnection current,
  }) async => current;

  @override
  Future<SourceConnection> bindRemote(
    String personId, {
    required String connectorId,
    required String connectionId,
    required List<SourceResource> resources,
    SourceConnection? current,
  }) async => throw UnimplementedError();

  @override
  Future<SourceConnection> disconnectRemote(
    String personId, {
    required SourceConnection current,
  }) async => throw UnimplementedError();

  SourceConnection _source(List<SourceResource> resources) =>
      SourceConnection(
        connectorId: 'calendar.event_kit',
        connectionId: '00000000-0000-4000-8000-000000000010',
        executionOwnerId: 'test-device',
        state: 'ready',
        revision: 1,
        sourceAuthority: const SourceAuthority(
          incarnation: '00000000-0000-4000-8000-000000000009',
          epoch: 1,
        ),
        resourceMode: 'selected',
        resources: resources,
      );

  @override
  Future<DaySnapshot> syncCalendar(DayQuery query) {
    syncCount++;
    return FakeDayGateway().loadDay(query);
  }

  @override
  Future<void> openCalendarSettings() async {
    settingsCount++;
  }
}

void main() {
  for (final width in [390.0, 1200.0]) {
    testWidgets('groups connected calendars without merging names at $width', (
      tester,
    ) async {
      await tester.binding.setSurfaceSize(Size(width, 1200));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      final date = DateTime.utc(2026, 9, 4);
      await tester.pumpWidget(
        MaterialApp(
          theme: FloeTheme.light,
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          supportedLocales: AppLocalizations.supportedLocales,
          home: Scaffold(
            body: SingleChildScrollView(
              child: CalendarPanel(
                gateway: PanelCalendarGateway(),
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
                  calendars: [
                    ConnectedCalendar(id: 'home', name: 'iCloud · Home'),
                    ConnectedCalendar(
                      id: 'work',
                      name: 'iCloud · Work, planning · 팀 일정',
                    ),
                    ConnectedCalendar(
                      id: 'birthdays',
                      name: 'long-account-address@example.com · Home',
                    ),
                    ConnectedCalendar(id: 'local', name: 'Local calendar'),
                  ],
                ),
                onChanged: () async {},
              ),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();
      expect(find.text('4 calendars'), findsOneWidget);
      expect(find.text('iCloud · 2'), findsOneWidget);
      expect(find.text('long-account-address@example.com · 1'), findsOneWidget);
      expect(find.text('Other calendars · 1'), findsOneWidget);
      expect(find.text('Home'), findsNWidgets(2));
      expect(find.text('Work, planning · 팀 일정'), findsOneWidget);
      expect(find.text('Local calendar'), findsOneWidget);
      expect(find.text('Old aggregate title'), findsNothing);
      expect(tester.takeException(), isNull);
    });
  }

  testWidgets(
    'preselects calendars, requires a selection, saves multiple, and cancels',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(1000, 1200));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      final gateway = PanelCalendarGateway();
      var changes = 0;
      final date = DateTime.utc(2026, 9, 4);
      await tester.pumpWidget(
        MaterialApp(
          theme: FloeTheme.light,
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          supportedLocales: AppLocalizations.supportedLocales,
          home: Scaffold(
            body: SingleChildScrollView(
              child: CalendarPanel(
                gateway: gateway,
                sourceGateway: gateway,
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
                  calendars: [ConnectedCalendar(id: 'home', name: 'Home')],
                ),
                onChanged: () async {
                  changes++;
                },
              ),
            ),
          ),
        ),
      );
      Future<void> openPicker() async {
        await tester.tap(find.text('Reconnect or change'));
        await tester.pump();
        await tester.pump(const Duration(seconds: 1));
        await tester.tap(find.text('Continue'));
        await tester.pump();
        await tester.pump(const Duration(seconds: 1));
      }

      await openPicker();
      final home = find.widgetWithText(FloeCheckboxTile, 'Home');
      final work = find.widgetWithText(FloeCheckboxTile, 'Work');
      expect(tester.widget<FloeCheckboxTile>(home).value, isTrue);
      await tester.tap(home);
      await tester.pump(const Duration(seconds: 1));
      await tester.tap(find.text('Continue'));
      await tester.pump(const Duration(seconds: 1));
      expect(find.byType(FloeCheckboxTile), findsNWidgets(2));
      expect(find.byType(FloeRadioTile<bool>), findsNWidgets(2));
      expect(gateway.selected, isEmpty);
      await tester.tap(home);
      await tester.tap(work);
      await tester.pump(const Duration(seconds: 1));
      await tester.tap(find.text('Continue'));
      await tester.pumpAndSettle();
      expect(gateway.selected.map((calendar) => calendar.id), ['home', 'work']);
      expect(gateway.syncCount, 1);
      expect(changes, 1);
      await openPicker();
      await tester.tap(work);
      await tester.tap(find.text('Cancel'));
      await tester.pumpAndSettle();
      expect(gateway.syncCount, 1);
      expect(changes, 1);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets('shows EventKit access state and recovery action', (
    tester,
  ) async {
    final gateway = PanelCalendarGateway()
      ..systemAccess = CalendarSystemAccess.denied;
    final date = DateTime.utc(2026, 9, 4);
    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: Scaffold(
          body: SingleChildScrollView(
            child: CalendarPanel(
              gateway: gateway,
              platform: TargetPlatform.macOS,
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
                calendars: [ConnectedCalendar(id: 'home', name: 'Home')],
              ),
              onChanged: () async {},
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
    expect(find.text('Needs attention'), findsOneWidget);
    await tester.tap(
      find.byKey(const ValueKey('calendar-system-access-recover')),
    );
    await tester.pumpAndSettle();
    expect(gateway.settingsCount, 1);
  });
}
