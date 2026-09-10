import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/features/day_canvas/domain/day_models.dart';
import 'package:floe_client/features/day_canvas/presentation/calendar_agenda.dart';
import 'package:floe_client/l10n/app_localizations.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('current time label replaces an overlapping hour label', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(900, 760);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    final date = DateTime(2026, 9, 8);
    final snapshot = DaySnapshot(
      personId: 'person',
      date: date,
      generatedAt: DateTime(2026, 9, 8, 10, 5),
      timezoneOffsetSeconds: date.timeZoneOffset.inSeconds,
      items: const [],
    );

    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: Scaffold(
          body: CalendarAgenda(snapshot: snapshot, onConnections: () {}),
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(
      find.byKey(const Key('calendar-current-time-label')),
      findsOneWidget,
    );
    expect(find.text('10:05'), findsOneWidget);
    expect(find.byKey(const Key('calendar-hour-label-10')), findsNothing);
    expect(find.byKey(const Key('calendar-hour-label-9')), findsOneWidget);
    expect(find.byKey(const Key('calendar-hour-label-11')), findsOneWidget);
    final status = find.byKey(const Key('empty-day-status'));
    expect(status, findsOneWidget);
    expect(
      find.ancestor(of: status, matching: find.byType(Positioned)),
      findsOneWidget,
    );
    expect(
      find.descendant(of: status, matching: find.byType(Material)),
      findsWidgets,
    );
  });

  testWidgets(
    'empty day keeps its grid active and supports double-click create',
    (tester) async {
      tester.view.physicalSize = const Size(900, 760);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final date = DateTime(2026, 9, 8);
      DateTime? requestedStart;
      final snapshot = DaySnapshot(
        personId: 'person',
        date: date,
        generatedAt: DateTime(2026, 9, 8, 10),
        timezoneOffsetSeconds: date.timeZoneOffset.inSeconds,
        calendar: CalendarConnection(
          id: 'calendar',
          name: 'Personal',
          provider: 'fixture',
          revision: 1,
          lastSuccessAt: DateTime(2026, 9, 8, 9),
        ),
        items: const [],
      );

      await tester.pumpWidget(
        MaterialApp(
          theme: FloeTheme.light,
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          supportedLocales: AppLocalizations.supportedLocales,
          home: Scaffold(
            body: SizedBox(
              height: 700,
              child: StatefulBuilder(
                builder: (context, setState) => CalendarAgenda(
                  snapshot: snapshot,
                  onConnections: () {},
                  draftStartsAt: requestedStart,
                  onCreateEvent: (value) =>
                      setState(() => requestedStart = value),
                ),
              ),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('A little breathing room.'), findsOneWidget);
      expect(
        find.text('No events on this day. Double-click a time to create one.'),
        findsOneWidget,
      );
      final banner = find.byKey(const Key('empty-day-banner'));
      final viewport = tester.getRect(find.byKey(const Key('calendar-scroll')));
      expect(
        find.ancestor(of: banner, matching: find.byType(Positioned)),
        findsOneWidget,
      );
      expect(viewport.contains(tester.getRect(banner).center), isTrue);
      expect(
        tester
            .widget<IgnorePointer>(
              find
                  .ancestor(of: banner, matching: find.byType(IgnorePointer))
                  .first,
            )
            .ignoring,
        isTrue,
      );
      expect(find.byKey(const Key('zoom-toolbar-divider')), findsOneWidget);
      final timeline = find.byKey(const Key('timeline-card'));
      final pointerGuard = tester.widget<IgnorePointer>(
        find
            .descendant(of: timeline, matching: find.byType(IgnorePointer))
            .first,
      );
      expect(pointerGuard.ignoring, isFalse);

      final point = viewport.center;
      await tester.tapAt(point);
      await tester.pump(const Duration(milliseconds: 50));
      await tester.tapAt(point);
      await tester.pump(const Duration(milliseconds: 350));

      expect(requestedStart, isNotNull);
      expect(requestedStart!.minute % 15, 0);
      expect(requestedStart!.year, date.year);
      expect(requestedStart!.month, date.month);
      expect(requestedStart!.day, date.day);
      expect(find.text('A little breathing room.'), findsNothing);
      expect(find.text('New event'), findsOneWidget);
      expect(tester.takeException(), isNull);
    },
  );
}
