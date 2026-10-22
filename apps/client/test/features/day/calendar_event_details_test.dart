import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:intl/intl.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/features/day/domain/day_models.dart';
import 'package:floe_client/features/day/presentation/calendar_event_details.dart';
import 'package:floe_client/l10n/app_localizations.dart';

EventItem event(
  DateTime start,
  DateTime end, {
  bool allDay = false,
  bool writable = false,
}) => EventItem(
  id: 'event',
  title: 'Planning review',
  revision: 1,
  createdAt: start,
  startsAt: start,
  endsAt: end,
  isAllDay: allDay,
  timezone: 'Source/Zone',
  actionTarget: writable
      ? const DayEventTarget(eventId: 'verified-target', expectedRevision: 9)
      : null,
);
DaySnapshot snapshot(EventItem item) => DaySnapshot(
  personId: 'fixture',
  date: DateTime(2030, 1, 1),
  generatedAt: item.startsAt,
  timezoneOffsetSeconds: DateTime(2030, 1, 1).timeZoneOffset.inSeconds,
  items: [item],
);
Widget app(Widget home) => MaterialApp(
  theme: FloeTheme.light,
  localizationsDelegates: AppLocalizations.localizationsDelegates,
  supportedLocales: AppLocalizations.supportedLocales,
  builder: (context, child) => MediaQuery(
    data: MediaQuery.of(context).copyWith(alwaysUse24HourFormat: true),
    child: child!,
  ),
  home: home,
);

void main() {
  testWidgets(
    'detail retains the schedule offset when it differs from the device',
    (tester) async {
      final item = event(
        DateTime.utc(2026, 10, 22, 23, 30),
        DateTime.utc(2026, 10, 23, 1),
      );
      final selected = DaySnapshot(
        personId: 'fixture',
        date: DateTime.utc(2026, 10, 22),
        generatedAt: item.startsAt,
        timezoneOffsetSeconds: 36000,
        items: [item],
      );
      await tester.pumpWidget(
        app(CalendarEventDetails(event: item, snapshot: selected)),
      );
      await tester.pumpAndSettle();
      expect(find.text('Fri, Oct 23, 2026'), findsOneWidget);
      expect(find.text('09:30 – 11:00'), findsOneWidget);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets('a repeated local hour distinguishes DST endpoint zones', (
    tester,
  ) async {
    final item = event(
      DateTime.utc(2026, 11, 1, 5, 30),
      DateTime.utc(2026, 11, 1, 6, 30),
    );
    await tester.pumpWidget(
      app(CalendarEventDetails(event: item, snapshot: snapshot(item))),
    );
    await tester.pumpAndSettle();
    final first = item.startsAt.toLocal();
    final last = item.endsAt.toLocal();
    if (first.timeZoneOffset != last.timeZoneOffset) {
      expect(find.textContaining(first.timeZoneName), findsOneWidget);
      expect(find.textContaining(last.timeZoneName), findsOneWidget);
      expect(find.text('01:30 – 01:30'), findsNothing);
    }
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    'overnight details use real endpoint dates, never the selected snapshot date',
    (tester) async {
      final item = event(
        DateTime(2026, 10, 22, 23, 30).toUtc(),
        DateTime(2026, 10, 23, 1).toUtc(),
      );
      await tester.pumpWidget(
        app(CalendarEventDetails(event: item, snapshot: snapshot(item))),
      );
      await tester.pumpAndSettle();
      expect(
        find.text(
          '${DateFormat.yMMMEd('en').format(item.startsAt.toLocal())} · 23:30',
        ),
        findsOneWidget,
      );
      expect(
        find.text(
          '${DateFormat.yMMMEd('en').format(item.endsAt.toLocal())} · 01:00',
        ),
        findsOneWidget,
      );
      expect(find.textContaining('2030'), findsNothing);
      expect(find.text('Original timezone: Source/Zone'), findsNothing);
      await tester.tap(find.text('Calendar details'));
      await tester.pumpAndSettle();
      expect(find.text('Original timezone: Source/Zone'), findsOneWidget);
      expect(tester.takeException(), isNull);
    },
  );
  testWidgets(
    'all-day range displays inclusive civil dates without converting or mutating them',
    (tester) async {
      final item = event(
        DateTime.utc(2026, 10, 22),
        DateTime.utc(2026, 10, 24),
        allDay: true,
      );
      await tester.pumpWidget(
        app(CalendarEventDetails(event: item, snapshot: snapshot(item))),
      );
      await tester.pumpAndSettle();
      expect(find.text('All day'), findsOneWidget);
      expect(
        find.text('Thu, Oct 22, 2026 – Fri, Oct 23, 2026'),
        findsOneWidget,
      );
      expect(find.textContaining('exclusive'), findsNothing);
      expect(item.endsAt, DateTime.utc(2026, 10, 24));
      expect(find.text('Local time'), findsNothing);
    },
  );
  testWidgets(
    'a targetless event never offers edit or delete, even with callbacks requested',
    (tester) async {
      final item = event(
        DateTime.utc(2026, 10, 22, 10),
        DateTime.utc(2026, 10, 22, 11),
      );
      await tester.pumpWidget(
        app(
          CalendarEventDetails(
            event: item,
            snapshot: snapshot(item),
            canEdit: true,
            canDelete: true,
          ),
        ),
      );
      await tester.pumpAndSettle();
      expect(find.text('Edit event'), findsNothing);
      expect(find.text('Delete'), findsNothing);
    },
  );
  testWidgets(
    'closing is inert, while Edit dispatches only the selected callback after closing',
    (tester) async {
      var edits = 0;
      var deletes = 0;
      final item = event(
        DateTime.utc(2026, 10, 22, 10),
        DateTime.utc(2026, 10, 22, 11),
        writable: true,
      );
      await tester.pumpWidget(
        app(
          Builder(
            builder: (context) => Scaffold(
              body: TextButton(
                onPressed: () => openCalendarEvent(
                  context,
                  item,
                  snapshot(item),
                  onEdit: () => edits++,
                  onDelete: () => deletes++,
                ),
                child: const Text('Open'),
              ),
            ),
          ),
        ),
      );
      await tester.tap(find.text('Open'));
      await tester.pumpAndSettle();
      await tester.tap(find.byTooltip('Close'));
      await tester.pumpAndSettle();
      expect(edits, 0);
      expect(deletes, 0);
      await tester.tap(find.text('Open'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Edit event'));
      await tester.pumpAndSettle();
      expect(edits, 1);
      expect(deletes, 0);
      expect(find.text('Open'), findsOneWidget);
    },
  );
  for (final width in [320.0, 390.0]) {
    testWidgets('long event detail fits $width with 2x text', (tester) async {
      tester.view.physicalSize = Size(width, 900);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final item = event(
        DateTime(2026, 10, 22, 23, 30).toUtc(),
        DateTime(2026, 10, 23, 1).toUtc(),
        writable: true,
      );
      await tester.pumpWidget(
        MaterialApp(
          theme: FloeTheme.light,
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          supportedLocales: AppLocalizations.supportedLocales,
          builder: (context, child) => MediaQuery(
            data: MediaQuery.of(context)
                .copyWith(textScaler: const TextScaler.linear(2)),
            child: child!,
          ),
          home: CalendarEventDetails(
            event: item,
            snapshot: snapshot(item),
            canEdit: true,
            canDelete: true,
          ),
        ),
      );
      await tester.pumpAndSettle();
      await tester.ensureVisible(find.text('Edit event'));
      expect(tester.takeException(), isNull);
    });
  }
}
