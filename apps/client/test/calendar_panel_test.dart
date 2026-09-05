import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/app/floe_selection.dart';
import 'package:floe_client/features/day_canvas/application/calendar_gateway.dart';
import 'package:floe_client/features/day_canvas/application/fake_day_gateway.dart';
import 'package:floe_client/features/day_canvas/domain/day_models.dart';
import 'package:floe_client/features/day_canvas/presentation/calendar_panel.dart';
import 'package:floe_client/l10n/app_localizations.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

class PanelCalendarGateway implements CalendarGateway {
  List<CalendarChoice> selected = [];
  int syncCount = 0;

  @override
  Future<DaySnapshot> disconnectCalendar(DayQuery query) =>
      FakeDayGateway().loadDay(query);

  @override
  Future<List<CalendarChoice>> calendars() async => const [
    CalendarChoice('home', 'Home'),
    CalendarChoice('work', 'Work'),
  ];

  @override
  Future<DaySnapshot> selectCalendar(CalendarChoice calendar, DayQuery query) =>
      selectCalendars([calendar], query);

  @override
  Future<DaySnapshot> selectCalendars(
    List<CalendarChoice> calendars,
    DayQuery query, {
    bool includeAll = false,
  }) {
    selected = calendars;
    return FakeDayGateway().loadDay(query);
  }

  @override
  Future<DaySnapshot> syncCalendar(DayQuery query) {
    syncCount++;
    return FakeDayGateway().loadDay(query);
  }

  @override
  Future<void> openCalendarSettings() async {}
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
                connection: const CalendarConnection(
                  id: 'home',
                  name: 'Old aggregate title',
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
                    ConnectedCalendar(id: 'legacy', name: 'Local calendar'),
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
                query: DayQuery(
                  personId: 'test',
                  date: date,
                  now: date,
                  timezoneOffsetSeconds: 0,
                ),
                connection: const CalendarConnection(
                  id: 'home',
                  name: 'Home',
                  provider: 'event_kit',
                  revision: 1,
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
}
