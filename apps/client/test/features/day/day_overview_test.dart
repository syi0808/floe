import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/features/day/domain/day_models.dart';
import 'package:floe_client/features/day/presentation/day_overview.dart';
import 'package:floe_client/l10n/app_localizations.dart';

final date = DateTime.utc(2026, 10, 22);
DayQuery query({DateTime? now, int offset = 0, int? endOffset}) => DayQuery(
  personId: 'test',
  date: date,
  now: now ?? date.add(const Duration(hours: 10)),
  timezoneOffsetSeconds: offset,
  endTimezoneOffsetSeconds: endOffset,
);
EventItem event(String id, int start, int end, {bool allDay = false}) =>
    EventItem(
      id: id,
      title: id,
      revision: 1,
      createdAt: date,
      startsAt: date.add(Duration(minutes: start)),
      endsAt: date.add(Duration(minutes: end)),
      isAllDay: allDay,
    );
DaySnapshot snapshot(
  DayQuery q,
  List<DayItem> items, {
  String? nowId,
  String? nextId,
}) {
  final timed =
      items
          .whereType<EventItem>()
          .where((e) => !e.isAllDay && e.endsAt.isAfter(e.startsAt))
          .toList()
        ..sort((a, b) {
          final start = a.startsAt.compareTo(b.startsAt);
          return start == 0 ? a.id.compareTo(b.id) : start;
        });
  final current = timed.where(
    (e) => !e.startsAt.isAfter(q.now) && e.endsAt.isAfter(q.now),
  );
  final future = timed.where((e) => e.startsAt.isAfter(q.now));
  return DaySnapshot(
    personId: q.personId,
    date: q.date,
    generatedAt: q.now,
    timezoneOffsetSeconds: q.timezoneOffsetSeconds,
    items: items,
    nowEventId: nowId ?? (current.isEmpty ? null : current.first.id),
    nextEventId: nextId ?? (future.isEmpty ? null : future.first.id),
  );
}

void main() {
  test('owner-selected IDs win over UI sort order', () {
    final q = query();
    final p = DayScheduleProjection(
      snapshot(q, [event('a', 580, 650), event('b', 590, 640)], nowId: 'b'),
      q,
    );
    expect(p.current.single.id, 'b');
    expect(p.remaining.single.id, 'a');
  });
  test(
    'all-day civil dates are not shifted through instant day boundaries',
    () {
      final q = query(offset: -43200);
      final p = DayScheduleProjection(
        snapshot(q, [event('civil-date', 0, 1440, allDay: true)]),
        q,
      );
      expect(p.allDay.single.id, 'civil-date');
      expect(p.current, isEmpty);
    },
  );
  test('retained snapshots never acquire live labels after a failed read', () {
    final old = query();
    final later = query(now: date.add(const Duration(hours: 12)));
    final p = DayScheduleProjection(
      snapshot(old, [event('old-current', 590, 640)]),
      later,
      observationCurrent: false,
    );
    expect(p.isToday, isFalse);
    expect(p.current, isEmpty);
    expect(p.next, isEmpty);
    expect(p.timed.single.id, 'old-current');
  });

  test('owner focus is retained and all overlapping or tied events remain in schedule', () {
    final q = query();
    final p = DayScheduleProjection(
      snapshot(q, [
        event('ended', 540, 600),
        event('now-b', 600, 650),
        event('now-a', 590, 630),
        event('next-b', 660, 700),
        event('next-a', 660, 720),
        event('later', 750, 800),
        event('all-day', 0, 1440, allDay: true),
        event('outside', 1440, 1450),
        event('invalid', 600, 600),
      ]),
      q,
    );
    expect(p.current.map((e) => e.id), ['now-a']);
    expect(p.next.map((e) => e.id), ['next-a']);
    expect(p.remaining.map((e) => e.id), ['now-b', 'next-b', 'later']);
    expect(p.earlier.map((e) => e.id), ['ended']);
    expect(p.allDay.map((e) => e.id), ['all-day']);
    expect(p.events.length, 7);
  });
  test('historical selected days do not claim Now or Next', () {
    final q = query(now: date.add(const Duration(days: 1)));
    final p = DayScheduleProjection(
      snapshot(q, [event('meeting', 600, 660)]),
      q,
    );
    expect(p.isToday, isFalse);
    expect(p.current, isEmpty);
    expect(p.next, isEmpty);
    expect(p.timed.single.id, 'meeting');
  });
  test('cross-midnight and independent day offsets retain late events', () {
    final q = query(
      offset: -14400,
      endOffset: -18000,
      now: date.add(const Duration(hours: 28)),
    );
    final p = DayScheduleProjection(
      snapshot(q, [
        event('overnight', 60, 300),
        event('late', 1680, 1720),
        event('after', 1740, 1800),
      ]),
      q,
    );
    expect(q.endsAt.difference(q.startsAt), const Duration(hours: 25));
    expect(p.events.map((e) => e.id), ['overnight', 'late']);
    expect(p.current.single.id, 'late');
  });
  for (final width in [320.0, 390.0]) {
    testWidgets(
      'agenda is readable at $width with double text and opens actual details',
      (tester) async {
        tester.view.physicalSize = Size(width, 900);
        tester.view.devicePixelRatio = 1;
        addTearDown(tester.view.resetPhysicalSize);
        addTearDown(tester.view.resetDevicePixelRatio);
        final q = query();
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
            home: Scaffold(
              body: SingleChildScrollView(
                child: DayOverview(
                  query: q,
                  snapshot: snapshot(q, [
                    event('Planning review', 590, 640),
                    event('Design meeting', 650, 700),
                    event('Earlier meeting', 480, 540),
                  ]),
                ),
              ),
            ),
          ),
        );
        await tester.pumpAndSettle();
        expect(find.text('Now'), findsOneWidget);
        expect(find.text('Next'), findsOneWidget);
        expect(find.text('Earlier meeting'), findsNothing);
        await tester.tap(find.text('Planning review'));
        await tester.pumpAndSettle();
        expect(find.text('Saved in Floe'), findsOneWidget);
        expect(tester.takeException(), isNull);
      },
    );
  }
  testWidgets('unknown empty coverage is not described as a free day', (
    tester,
  ) async {
    final q = query();
    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: Scaffold(
          body: DayOverview(query: q, snapshot: snapshot(q, [])),
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('No events loaded for this day.'), findsOneWidget);
    expect(find.text('No events scheduled for this day.'), findsNothing);
  });
}
