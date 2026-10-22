import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/features/day/domain/day_models.dart';
import 'package:floe_client/features/day/presentation/calendar_agenda.dart';
import 'package:floe_client/features/day/presentation/calendar_event_details.dart';
import 'package:floe_client/l10n/app_localizations.dart';

Future<void> showTimeline(
  WidgetTester tester, {
  required DateTime now,
  double scale = 1,
  DayQuery? query,
}) async {
  final selected =
      query ??
      DayQuery(
        personId: 'fixture',
        date: DateTime.utc(2026, 10, 22),
        now: now,
        timezoneOffsetSeconds: 0,
        endTimezoneOffsetSeconds: 0,
      );
  final event = EventItem(
    id: 'short',
    title: 'A short review',
    revision: 1,
    createdAt: now,
    startsAt: now.subtract(const Duration(minutes: 15)),
    endsAt: now.add(const Duration(minutes: 15)),
  );
  await tester.pumpWidget(
    MaterialApp(
      theme: FloeTheme.light,
      localizationsDelegates: AppLocalizations.localizationsDelegates,
      supportedLocales: AppLocalizations.supportedLocales,
      builder: (context, child) => MediaQuery(
        data: MediaQuery.of(context)
            .copyWith(textScaler: TextScaler.linear(scale)),
        child: child!,
      ),
      home: Scaffold(
        body: CalendarAgenda(
          query: selected,
          snapshot: DaySnapshot(
            personId: selected.personId,
            date: selected.date,
            generatedAt: now,
            timezoneOffsetSeconds: selected.timezoneOffsetSeconds,
            items: [event],
          ),
          onConnections: () {},
          onRefresh: () {},
        ),
      ),
    ),
  );
  await tester.pumpAndSettle();
}

void main() {
  testWidgets(
    'time reference paints behind a short event and never intercepts its tap',
    (tester) async {
      await showTimeline(tester, now: DateTime.utc(2026, 10, 22, 9, 15));
      final marker = find.byKey(const Key('calendar-time-marker'));
      final stack = tester.widget<Stack>(
        find.ancestor(of: marker, matching: find.byType(Stack)).first,
      );
      final markerIndex = stack.children.indexWhere(
        (child) => child.key == const Key('calendar-time-marker'),
      );
      final eventIndex = stack.children.indexWhere(
        (child) => child.key == const ValueKey('calendar-event-short'),
      );
      expect(markerIndex, greaterThanOrEqualTo(0));
      expect(eventIndex, greaterThan(markerIndex));
      final ignores = tester.widget<IgnorePointer>(
        find.descendant(of: marker, matching: find.byType(IgnorePointer)).first,
      );
      expect(ignores.ignoring, isTrue);
      final card = find.byType(CalendarEventCard);
      await tester.ensureVisible(card);
      await tester.pumpAndSettle();
      final rect = tester.getRect(card);
      final label = tester.getRect(
        find.byKey(const Key('calendar-current-time-label')),
      );
      expect(label.center.dy, closeTo(rect.center.dy, 1));
      await tester.tapAt(Offset(rect.right - 60, rect.center.dy));
      // The existing calendar card also recognizes double-tap; allow its arena
      // to resolve the single tap before checking the detail handoff.
      await tester.pump(const Duration(milliseconds: 350));
      await tester.pumpAndSettle();
      expect(find.byType(CalendarEventDetails), findsOneWidget);
      expect(tester.takeException(), isNull);
    },
  );
  testWidgets(
    'large time labels do not overlap a neighboring hour after zoom',
    (tester) async {
      await showTimeline(
        tester,
        now: DateTime.utc(2026, 10, 22, 11, 52),
        scale: 2,
      );
      await tester.tap(find.byTooltip('Zoom in'));
      await tester.pumpAndSettle();
      expect(find.text('2×'), findsOneWidget);
      expect(
        find.byKey(const ValueKey('calendar-hour-label-12')),
        findsNothing,
      );
      expect(
        find.byKey(const ValueKey('calendar-hour-label-11')),
        findsOneWidget,
      );
      final gridTop = tester
          .getTopLeft(find.byKey(const Key('calendar-guides')))
          .dy;
      final markerCenter = tester
          .getRect(find.byKey(const Key('calendar-current-time-label')))
          .center
          .dy;
      expect(markerCenter, closeTo(gridTop + 16 + (11 * 60 + 52) * 2, 1));
      expect(tester.takeException(), isNull);
    },
  );
  testWidgets(
    'guide lines continue through the last hour of a 25-hour civil day',
    (tester) async {
      final now = DateTime.utc(2026, 11, 1, 12);
      await showTimeline(
        tester,
        now: now,
        query: DayQuery(
          personId: 'fixture',
          date: DateTime.utc(2026, 11, 1),
          now: now,
          timezoneOffsetSeconds: -14400,
          endTimezoneOffsetSeconds: -18000,
        ),
      );
      final guides = tester.widget<CustomPaint>(
        find.byKey(const Key('calendar-guides')),
      );
      expect(
        tester.getSize(find.byKey(const Key('calendar-guides'))).height,
        1532,
      );
      final recorder = ui.PictureRecorder();
      final canvas = Canvas(recorder);
      guides.painter!.paint(canvas, const Size(400, 1532));
      final picture = recorder.endRecording();
      final alphaAtFinalGuide = await tester.runAsync(() async {
        final image = await picture.toImage(400, 1532);
        final bytes = (await image.toByteData(
          format: ui.ImageByteFormat.rawRgba,
        ))!.buffer.asUint8List();
        // The final 25:00 boundary is at 16 + 1500, not the old 24-hour cutoff.
        final alpha =
            bytes[((1515 * 400 + 73) * 4) + 3] +
            bytes[((1516 * 400 + 73) * 4) + 3];
        image.dispose();
        return alpha;
      });
      picture.dispose();
      expect(alphaAtFinalGuide, greaterThan(0));
      expect(tester.takeException(), isNull);
    },
  );
}
