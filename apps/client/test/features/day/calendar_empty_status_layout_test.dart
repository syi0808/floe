import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/floe_loading.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/features/day/domain/day_models.dart';
import 'package:floe_client/features/day/presentation/calendar_agenda.dart';
import 'package:floe_client/l10n/app_localizations.dart';

DayQuery _query() {
  final now = DateTime(2026, 10, 14, 12);
  return DayQuery.local(personId: 'person', date: now, now: now);
}

DaySnapshot _emptySnapshot(
  DayQuery query, {
  DayCalendarCoverage? coverage,
  DateTime? generatedAt,
}) => DaySnapshot(
  personId: query.personId,
  date: query.date,
  generatedAt: generatedAt ?? query.now,
  timezoneOffsetSeconds: query.timezoneOffsetSeconds,
  items: const [],
  calendarCoverage: coverage,
);

DayCalendarCoverage _completeCoverage(DayQuery query) {
  final range = DayCalendarRange(
    startDate: query.date,
    endDateExclusive: DateTime(
      query.date.year,
      query.date.month,
      query.date.day + 1,
    ),
    timezoneOffsetSeconds: query.timezoneOffsetSeconds,
    endTimezoneOffsetSeconds: query.endTimezoneOffsetSeconds,
  );
  return DayCalendarCoverage(
    sources: [
      DayCalendarSourceCoverage(
        sourceRef: 'source',
        label: 'Work',
        state: DayCoverageState.current,
        lastRange: range,
        resources: [
          DayCalendarResourceCoverage(
            resourceRef: 'calendar',
            label: 'Work',
            state: DayCoverageState.current,
            lastRange: range,
          ),
        ],
      ),
    ],
  );
}

Widget _calendar({
  required DayQuery query,
  required DaySnapshot snapshot,
  required bool observationCurrent,
  required double textScale,
  bool loading = false,
  VoidCallback? onConnections,
  VoidCallback? onRefresh,
  ValueChanged<DateTime>? onCreateEvent,
}) => MaterialApp(
  theme: FloeTheme.light,
  locale: const Locale('en'),
  localizationsDelegates: AppLocalizations.localizationsDelegates,
  supportedLocales: AppLocalizations.supportedLocales,
  builder: (context, child) => MediaQuery(
    data: MediaQuery.of(context)
        .copyWith(textScaler: TextScaler.linear(textScale)),
    child: child!,
  ),
  home: Scaffold(
    body: CalendarAgenda(
      query: query,
      snapshot: snapshot,
      observationCurrent: observationCurrent,
      loading: loading,
      onConnections: onConnections ?? () {},
      onRefresh: onRefresh ?? () {},
      onCreateEvent: onCreateEvent,
    ),
  ),
);

void main() {
  setUpAll(() async {
    // Widget tests default to placeholder fonts. Load every family from the
    // production manifest, including Material and package icon fonts.
    final manifest = jsonDecode(
      await rootBundle.loadString('FontManifest.json'),
    ) as List<dynamic>;
    for (final value in manifest) {
      final entry = value as Map<String, dynamic>;
      final fonts = FontLoader(entry['family'] as String);
      for (final font in entry['fonts'] as List<dynamic>) {
        fonts.addFont(
          rootBundle.load((font as Map<String, dynamic>)['asset'] as String),
        );
      }
      await fonts.load();
    }
  });

  for (final width in [390.0, 1200.0]) {
    for (final scale in [1.0, 2.0]) {
      for (final coverage in <DayCalendarCoverage?>[
        null,
        const DayCalendarCoverage(sources: []),
      ]) {
        testWidgets(
          'unknown coverage stays neutral and recovery actions fit at $width, text $scale',
          (tester) async {
            tester.view.physicalSize = Size(width, 1000);
            tester.view.devicePixelRatio = 1;
            addTearDown(tester.view.resetPhysicalSize);
            addTearDown(tester.view.resetDevicePixelRatio);
            final query = _query();
            var refreshes = 0;
            var connections = 0;
            await tester.pumpWidget(
              _calendar(
                query: query,
                snapshot: _emptySnapshot(query, coverage: coverage),
                observationCurrent: true,
                textScale: scale,
                onConnections: () => connections++,
                onRefresh: () => refreshes++,
              ),
            );
            await tester.pumpAndSettle();
            await tester.ensureVisible(
              find.byKey(const Key('empty-day-status')),
            );
            await tester.pumpAndSettle();
            final statusRect = tester.getRect(
              find.byKey(const Key('empty-day-status')),
            );
            final gridRect = tester.getRect(
              find.byKey(const Key('calendar-scroll')),
            );

            expect(find.text('Calendar status unavailable'), findsOneWidget);
            expect(statusRect.bottom, lessThanOrEqualTo(gridRect.top));
            expect(
              tester
                  .getSemantics(find.byKey(const Key('empty-day-status')))
                  .flagsCollection
                  .isLiveRegion,
              isTrue,
            );
            expect(
              find.text(
                'Connect a calendar to create and collect events here.',
              ),
              findsNothing,
            );
            expect(find.text('Refresh calendar'), findsOneWidget);
            expect(find.text('Connections'), findsOneWidget);
            await tester.tap(find.text('Refresh calendar'));
            await tester.tap(find.text('Connections'));
            expect(refreshes, 1);
            expect(connections, 1);
            expect(tester.takeException(), isNull);
            await tester.pumpWidget(const SizedBox.shrink());
          },
        );
      }

      testWidgets(
        'complete empty status requires a current observation and recovers at $width, text $scale',
        (tester) async {
          tester.view.physicalSize = Size(width, 1000);
          tester.view.devicePixelRatio = 1;
          addTearDown(tester.view.resetPhysicalSize);
          addTearDown(tester.view.resetDevicePixelRatio);
          final query = _query();
          final coverage = _completeCoverage(query);
          final retained = _emptySnapshot(query, coverage: coverage);
          var refreshes = 0;
          var connections = 0;
          final createdEvents = <DateTime>[];

          await tester.pumpWidget(
            _calendar(
              query: query,
              snapshot: retained,
              observationCurrent: true,
              textScale: scale,
              onConnections: () => connections++,
              onRefresh: () => refreshes++,
              onCreateEvent: createdEvents.add,
            ),
          );
          await tester.pumpAndSettle();
          expect(find.text('A little breathing room.'), findsOneWidget);
          expect(find.byKey(const Key('empty-day-status')), findsNothing);
          final bannerRect = tester.getRect(
            find.byKey(const Key('empty-day-banner')),
          );
          final gridRect = tester.getRect(
            find.byKey(const Key('calendar-scroll')),
          );
          expect(bannerRect.bottom, lessThanOrEqualTo(gridRect.top));
          final openGridPoint = Offset(gridRect.left + 100, gridRect.top + 32);
          await tester.tapAt(openGridPoint);
          await tester.pump(const Duration(milliseconds: 50));
          await tester.tapAt(openGridPoint);
          await tester.pump();
          expect(createdEvents, hasLength(1));

          // A failed read keeps its previous complete empty snapshot, but the
          // UI must not continue presenting that old observation as current.
          await tester.pumpWidget(
            _calendar(
              query: query,
              snapshot: retained,
              observationCurrent: false,
              textScale: scale,
              onConnections: () => connections++,
              onRefresh: () => refreshes++,
            ),
          );
          await tester.pumpAndSettle();
          expect(find.text('Calendar status needs refresh'), findsOneWidget);
          expect(find.text('A little breathing room.'), findsNothing);
          await tester.ensureVisible(find.byKey(const Key('empty-day-status')));
          await tester.tap(find.text('Refresh calendar'));
          await tester.tap(find.text('Connections'));
          expect(refreshes, 1);
          expect(connections, 1);

          final recovered = _emptySnapshot(
            query,
            coverage: coverage,
            generatedAt: query.now.add(const Duration(minutes: 1)),
          );
          await tester.pumpWidget(
            _calendar(
              query: query,
              snapshot: recovered,
              observationCurrent: true,
              textScale: scale,
            ),
          );
          await tester.pumpAndSettle();
          expect(find.text('A little breathing room.'), findsOneWidget);
          expect(find.byKey(const Key('empty-day-status')), findsNothing);
          expect(tester.takeException(), isNull);
          await tester.pumpWidget(const SizedBox.shrink());
        },
      );
    }
  }

  testWidgets('stale narrow status stays outside grid at 200% text', (
    tester,
  ) async {
    final query = _query();
    tester.view.physicalSize = const Size(390, 1000);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);

    await tester.pumpWidget(
      _calendar(
        query: query,
        snapshot: _emptySnapshot(query, coverage: _completeCoverage(query)),
        observationCurrent: false,
        textScale: 2,
      ),
    );
    await tester.pumpAndSettle();
    final statusRect = tester.getRect(
      find.byKey(const Key('empty-day-status')),
    );
    final gridRect = tester.getRect(find.byKey(const Key('calendar-scroll')));
    expect(statusRect.bottom, lessThanOrEqualTo(gridRect.top));
    await expectLater(
      find.byKey(const Key('timeline-card')),
      matchesGoldenFile('goldens/calendar_empty_stale_390_2x.png'),
    );
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox.shrink());
  });

  testWidgets(
    'loading hides stale empty status until the failed read settles',
    (tester) async {
      final query = _query();
      final retained = _emptySnapshot(
        query,
        coverage: _completeCoverage(query),
      );
      tester.view.physicalSize = const Size(390, 1000);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);

      await tester.pumpWidget(
        _calendar(
          query: query,
          snapshot: retained,
          observationCurrent: false,
          loading: true,
          textScale: 2,
        ),
      );
      expect(find.byType(FloeSpinner), findsOneWidget);
      expect(find.byKey(const Key('empty-day-status')), findsNothing);

      // A failed latest read leaves the cached complete snapshot in place and
      // settles into the stale recovery state.
      await tester.pumpWidget(
        _calendar(
          query: query,
          snapshot: retained,
          observationCurrent: false,
          loading: false,
          textScale: 2,
        ),
      );
      await tester.pumpAndSettle();
      expect(find.text('Calendar status needs refresh'), findsOneWidget);
      expect(find.byType(FloeSpinner), findsNothing);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox.shrink());
    },
  );
}
