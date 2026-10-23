import 'dart:async';
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/floe_app.dart';
import 'package:floe_client/app/floe_loading.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/app/runtime/runtime_controller.dart';
import 'package:floe_client/app/runtime/runtime_gateway.dart';
import 'package:floe_client/app/runtime/local_owner_gateways_scope.dart';
import 'package:floe_client/features/day/application/day_gateway.dart';
import 'package:floe_client/features/day/domain/day_models.dart';
import 'package:floe_client/features/day/presentation/calendar_context_rail.dart';
import 'package:floe_client/l10n/app_localizations.dart';
import 'package:floe_client/preview/product_fixture.dart';

DayQuery _query() {
  final now = DateTime(2026, 10, 14, 12);
  return DayQuery.local(personId: 'synthetic-v018', date: now, now: now);
}

DaySnapshot _snapshot(
  DayQuery query,
  DayCalendarCoverage? coverage, {
  List<DayItem> items = const [],
}) => DaySnapshot(
  personId: query.personId,
  date: query.date,
  generatedAt: query.now,
  timezoneOffsetSeconds: query.timezoneOffsetSeconds,
  items: items,
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
        sourceRef: 'synthetic-source',
        label: 'Synthetic calendar',
        state: DayCoverageState.current,
        lastRange: range,
        resources: [
          DayCalendarResourceCoverage(
            resourceRef: 'synthetic-calendar',
            label: 'Synthetic calendar',
            state: DayCoverageState.current,
            lastRange: range,
          ),
        ],
      ),
    ],
  );
}

DayCalendarCoverage _partialCoverage() => const DayCalendarCoverage(
  sources: [
    DayCalendarSourceCoverage(
      sourceRef: 'synthetic-source',
      label: 'Synthetic calendar',
      state: DayCoverageState.partial,
      resources: [],
    ),
  ],
);

EventItem _event(DayQuery query) => EventItem(
  id: 'synthetic-event',
  title: 'Synthetic event',
  revision: 1,
  createdAt: query.now,
  startsAt: query.startsAt.add(const Duration(hours: 10)),
  endsAt: query.startsAt.add(const Duration(hours: 11)),
  source: const CalendarDayItemSource(
    sourceRef: 'synthetic-source',
    calendarRef: 'synthetic-calendar',
    calendarLabel: 'Synthetic calendar',
  ),
);

Widget _app({
  required DayQuery query,
  required _SyntheticDayGateway gateway,
  required double textScale,
  RuntimeController? runtime,
}) => previewAppearance(
  FloeApp(
    personId: query.personId,
    query: query,
    gateway: gateway,
    ownerGateways: LocalOwnerGateways(runtime: runtime),
    builder: (context, child) => RepaintBoundary(
      key: const Key('full-day-screen'),
      child: MediaQuery(
        data: MediaQuery.of(context)
            .copyWith(textScaler: TextScaler.linear(textScale)),
        child: child!,
      ),
    ),
  ),
);

Future<void> _mount(
  WidgetTester tester, {
  required double width,
  double height = 1000,
  required double textScale,
  required DayQuery query,
  required _SyntheticDayGateway gateway,
  RuntimeController? runtime,
  FakeViewPadding padding = const FakeViewPadding(),
}) async {
  tester.view.physicalSize = Size(width, height);
  tester.view.devicePixelRatio = 1;
  tester.view.padding = padding;
  tester.view.viewPadding = padding;
  addTearDown(tester.view.resetPadding);
  addTearDown(tester.view.resetViewPadding);
  await tester.pumpWidget(
    _app(
      query: query,
      gateway: gateway,
      textScale: textScale,
      runtime: runtime,
    ),
  );
  await tester.pumpAndSettle();
}

Future<void> _loadProductionFonts() async {
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
}

void main() {
  setUpAll(_loadProductionFonts);

  testWidgets('standalone context rail keeps empty status by default', (
    tester,
  ) async {
    final query = _query();
    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: Scaffold(
          body: SingleChildScrollView(
            child: CalendarContextRail(
              snapshot: _snapshot(query, _completeCoverage(query)),
              query: query,
              observationCurrent: false,
              disabled: false,
              complete: (_, _) async {},
              onTasks: () {},
              onOpenTask: (_) {},
            ),
          ),
        ),
      ),
    );

    expect(find.text('Calendar status needs refresh'), findsOneWidget);
    expect(find.text('Calendar details'), findsNothing);
    expect(tester.takeException(), isNull);
  });

  for (final width in [390.0, 1200.0]) {
    for (final textScale in [1.0, 2.0]) {
      testWidgets(
        'full Day keeps neutral unknown status and toolbar access at $width, text $textScale',
        (tester) async {
          final query = _query();
          final gateway = _SyntheticDayGateway();
          addTearDown(tester.view.resetPhysicalSize);
          addTearDown(tester.view.resetDevicePixelRatio);
          await _mount(
            tester,
            width: width,
            textScale: textScale,
            query: query,
            gateway: gateway,
          );

          if (width <= 780) {
            // The default narrow Schedule retains a visible status in its
            // coverage row; Timeline puts the same status above its time grid.
            expect(find.text('Calendar status unavailable'), findsOneWidget);
            await tester.tap(find.text('Timeline'));
            await tester.pumpAndSettle();
            expect(find.byTooltip('Tasks and calendar status'), findsOneWidget);
            expect(find.byKey(const Key('day-status-warning')), findsNothing);
            expect(find.byKey(const Key('day-details-opener')), findsOneWidget);
          }

          final refresh = find.byTooltip('Refresh calendar');
          expect(refresh, findsOneWidget);
          expect(tester.getRect(refresh).bottom, lessThanOrEqualTo(1000));
          expect(find.byTooltip('More'), findsOneWidget);
          expect(find.text('Refresh calendar'), findsNothing);
          expect(
            find.text('Connect a calendar to create and collect events here.'),
            findsNothing,
          );
          expect(find.text('Calendar status unavailable'), findsOneWidget);

          final statusRect = tester.getRect(
            find.byKey(const Key('empty-day-status')),
          );
          final gridRect = tester.getRect(
            find.byKey(const Key('calendar-scroll')),
          );
          expect(statusRect.bottom, lessThanOrEqualTo(gridRect.top));
          expect(
            tester
                .getSemantics(find.byKey(const Key('empty-day-status')))
                .flagsCollection
                .isLiveRegion,
            isTrue,
          );

          if (width == 390 && textScale == 2) {
            await expectLater(
              find.byKey(const Key('full-day-screen')),
              matchesGoldenFile(
                'goldens/calendar_empty_full_day_unknown_390_2x.png',
              ),
            );
          }
          if (width == 1200 && textScale == 1) {
            await expectLater(
              find.byKey(const Key('full-day-screen')),
              matchesGoldenFile(
                'goldens/calendar_empty_full_day_unknown_1200.png',
              ),
            );
          }
          expect(tester.takeException(), isNull);
          await tester.pumpWidget(const SizedBox.shrink());
        },
      );
    }
  }

  testWidgets(
    'toolbar refresh owns full-Day recovery and the Connections action remains direct',
    (tester) async {
      final query = _query();
      final gateway = _SyntheticDayGateway(coverage: _completeCoverage(query));
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      await _mount(
        tester,
        width: 390,
        textScale: 2,
        query: query,
        gateway: gateway,
      );
      await tester.tap(find.text('Timeline'));
      await tester.pumpAndSettle();

      expect(find.text('A little breathing room.'), findsOneWidget);
      expect(find.byKey(const Key('empty-day-status')), findsNothing);
      expect(find.byTooltip('Refresh calendar'), findsOneWidget);

      // The quiet healthy disclosure opens the same full Day details view.
      final details = find.byTooltip('Calendars synced for this date');
      expect(details, findsOneWidget);
      await tester.tap(details);
      await tester.pumpAndSettle();
      expect(find.text('Tasks and calendar status'), findsOneWidget);
      await tester.tap(find.byKey(const Key('day-details-close')));
      await tester.pumpAndSettle();

      gateway.failNextRefresh = true;
      await tester.tap(find.byTooltip('Refresh calendar'));
      await tester.pumpAndSettle();
      expect(find.text('Calendar status needs refresh'), findsOneWidget);
      expect(find.text('A little breathing room.'), findsNothing);
      expect(find.text('Refresh calendar'), findsNothing);
      expect(find.text('Connections'), findsOneWidget);
      expect(find.byTooltip('Refresh calendar'), findsOneWidget);
      expect(find.byTooltip('Tasks and calendar status'), findsOneWidget);

      await tester.tap(find.text('Connections'));
      await tester.pumpAndSettle();
      expect(
        find.text('Connections are available in the native Floe app.'),
        findsOneWidget,
      );
      await tester.tap(find.byTooltip('Today'));
      await tester.pumpAndSettle();

      gateway.coverage = _completeCoverage(query);
      gateway.deferNextRefresh();
      await tester.tap(find.byTooltip('Refresh calendar'));
      await tester.pump(const Duration(milliseconds: 510));
      expect(find.byType(FloeSpinner).evaluate(), isNotEmpty);
      expect(find.byKey(const Key('empty-day-status')), findsNothing);
      expect(find.text('Calendar status needs refresh'), findsNothing);
      expect(find.byKey(const Key('day-status-warning')), findsOneWidget);
      expect(find.byTooltip('Tasks and calendar status'), findsNothing);
      expect(find.byTooltip('Calendar status needs refresh'), findsOneWidget);

      gateway.completeDeferredRefresh();
      await tester.pumpAndSettle();
      expect(find.text('A little breathing room.'), findsOneWidget);
      expect(find.byKey(const Key('empty-day-status')), findsNothing);
      expect(find.byKey(const Key('day-status-warning')), findsNothing);
      expect(find.byTooltip('Calendars synced for this date'), findsOneWidget);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox.shrink());
    },
  );

  testWidgets(
    'narrow draft-suppressed empty status moves to the toolbar warning once',
    (tester) async {
      final query = _query();
      final gateway = _SyntheticDayGateway(coverage: _partialCoverage());
      final runtime = RuntimeController(
        gateway: _ReadyRuntimeGateway(),
        personId: query.personId,
      );
      await runtime.open();
      addTearDown(runtime.dispose);
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      await _mount(
        tester,
        width: 390,
        textScale: 2,
        query: query,
        gateway: gateway,
        runtime: runtime,
      );
      await tester.tap(find.text('Timeline'));
      await tester.pumpAndSettle();

      expect(find.byKey(const Key('empty-day-status')), findsOneWidget);
      expect(find.byKey(const Key('day-status-warning')), findsNothing);
      expect(find.byKey(const Key('day-details-opener')), findsOneWidget);

      await tester.tap(find.byTooltip('Create event'));
      await tester.pumpAndSettle();
      expect(find.text('New event'), findsNWidgets(2));
      expect(find.byKey(const Key('empty-day-status')), findsNothing);
      expect(find.byKey(const Key('day-status-warning')), findsOneWidget);
      expect(find.byTooltip('Calendar status needs refresh'), findsOneWidget);
      expect(find.byKey(const Key('day-details-opener')), findsNothing);
      await tester.tap(find.byTooltip('Close'));
      await tester.pumpAndSettle();

      expect(find.byKey(const Key('empty-day-status')), findsOneWidget);
      expect(find.byKey(const Key('day-status-warning')), findsNothing);
      expect(find.byKey(const Key('day-details-opener')), findsOneWidget);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox.shrink());
    },
  );

  testWidgets(
    'wide Day keeps stale empty status in the rail while a draft is open',
    (tester) async {
      final query = _query();
      final gateway = _SyntheticDayGateway(coverage: _completeCoverage(query));
      final runtime = RuntimeController(
        gateway: _ReadyRuntimeGateway(),
        personId: query.personId,
      );
      await runtime.open();
      addTearDown(runtime.dispose);
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      await _mount(
        tester,
        width: 1200,
        textScale: 1,
        query: query,
        gateway: gateway,
        runtime: runtime,
      );
      expect(find.text('A little breathing room.'), findsOneWidget);
      expect(find.text('Calendar status needs refresh'), findsNothing);

      gateway.failNextRefresh = true;
      await tester.tap(find.byTooltip('Refresh calendar'));
      await tester.pumpAndSettle();
      expect(find.text('Calendar status needs refresh'), findsOneWidget);
      await tester.tap(find.text('Close'));
      await tester.pumpAndSettle();
      expect(find.text('Calendar status needs refresh'), findsOneWidget);

      await tester.tap(find.byTooltip('Create event'));
      await tester.pumpAndSettle();
      expect(find.text('New event'), findsNWidgets(2));
      expect(find.text('Calendar status needs refresh'), findsOneWidget);
      expect(find.text('Calendar details'), findsNothing);
      expect(find.byTooltip('Refresh calendar'), findsOneWidget);
      expect(tester.takeException(), isNull);

      await tester.tap(find.byTooltip('Close'));
      await tester.pumpAndSettle();
      expect(find.text('Calendar status needs refresh'), findsOneWidget);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox.shrink());
    },
  );

  testWidgets(
    '320px 200% Timeline keeps stale warning visible while the grid scrolls',
    (tester) async {
      final query = _query();
      final gateway = _SyntheticDayGateway(
        coverage: _completeCoverage(query),
        items: [_event(query)],
      );
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      await _mount(
        tester,
        width: 320,
        height: 560,
        textScale: 2,
        query: query,
        gateway: gateway,
        padding: const FakeViewPadding(top: 24, bottom: 34),
      );
      expect(tester.takeException(), isNull, reason: 'initial render');
      await tester.tap(find.text('Timeline'));
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull, reason: 'Timeline render');

      gateway.failNextRefresh = true;
      await tester.tap(find.byTooltip('Refresh calendar'));
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull, reason: 'failed refresh render');
      expect(find.text('Close'), findsOneWidget);
      const fullErrorMessage = 'Bad state: Day refresh failed: unavailable';
      final errorMessage = find.text(fullErrorMessage);
      expect(errorMessage, findsOneWidget);
      expect(find.bySemanticsLabel(fullErrorMessage), findsOneWidget);
      final fullMessageTooltip = find.byTooltip(fullErrorMessage);
      expect(fullMessageTooltip, findsOneWidget);
      final tooltip = tester.widget<Tooltip>(fullMessageTooltip);
      expect(tooltip.message, fullErrorMessage);
      expect(tooltip.excludeFromSemantics, isTrue);
      expect(find.text('Close').hitTestable(), findsOneWidget);
      await tester.longPress(errorMessage);
      await tester.pumpAndSettle();
      expect(find.text(fullErrorMessage), findsNWidgets(2));
      final tooltipMessageRect = tester.getRect(
        find.text(fullErrorMessage).last,
      );
      expect(tooltipMessageRect.left, greaterThanOrEqualTo(0));
      expect(tooltipMessageRect.top, greaterThanOrEqualTo(0));
      expect(tooltipMessageRect.right, lessThanOrEqualTo(320));
      expect(tooltipMessageRect.bottom, lessThanOrEqualTo(560));
      expect(tester.takeException(), isNull);
      // Tooltip content is in the app overlay, outside the Day screen boundary.
      await expectLater(
        find.byType(Overlay).first,
        matchesGoldenFile('goldens/calendar_compact_error_tooltip_320_2x.png'),
      );
      await tester.pump(const Duration(seconds: 3));
      await tester.pumpAndSettle();
      expect(find.text(fullErrorMessage), findsOneWidget);
      expect(find.bySemanticsLabel(fullErrorMessage), findsOneWidget);
      expect(find.text('Close').hitTestable(), findsOneWidget);
      await tester.tap(find.text('Close'));
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull, reason: 'dismissed error render');

      final warning = find.byKey(const Key('day-status-warning'));
      final statusTooltip = find.byTooltip('Calendar status needs refresh');
      final refresh = find.byTooltip('Refresh calendar');
      expect(warning, findsOneWidget);
      expect(statusTooltip, findsOneWidget);
      expect(warning.hitTestable(), findsOneWidget);
      expect(refresh.hitTestable(), findsOneWidget);
      expect(find.byKey(const Key('day-status-details')), findsNothing);
      expect(find.byKey(const Key('day-details-opener')), findsNothing);
      expect(tester.getSemantics(warning).flagsCollection.isLiveRegion, isTrue);
      expect(
        tester.getSemantics(warning).label,
        'Calendar status needs refresh',
      );
      final warningRect = tester.getRect(warning);
      expect(warningRect.width, greaterThanOrEqualTo(44));
      expect(warningRect.height, greaterThanOrEqualTo(44));
      final middle = tester.getRect(
        find.byKey(const Key('personal-day-middle-viewport')),
      );
      expect(warningRect.top, greaterThanOrEqualTo(middle.top));
      expect(warningRect.bottom, lessThanOrEqualTo(middle.bottom));

      await expectLater(
        find.byKey(const Key('full-day-screen')),
        matchesGoldenFile(
          'goldens/calendar_timeline_stale_events_widget_320_2x.png',
        ),
      );

      await tester.tap(warning);
      await tester.pumpAndSettle();
      expect(find.text('Tasks and calendar status'), findsOneWidget);
      await tester.tap(find.byKey(const Key('day-details-close')));
      await tester.pumpAndSettle();

      final timelineScroll = find
          .ancestor(
            of: find.byKey(const Key('timeline-card')),
            matching: find.byType(Scrollable),
          )
          .first;
      expect(timelineScroll.hitTestable(), findsOneWidget);
      await tester.drag(timelineScroll, const Offset(0, -180));
      await tester.pumpAndSettle();
      expect(find.text('Synthetic event').hitTestable(), findsOneWidget);
      expect(warning.hitTestable(), findsOneWidget);
      expect(tester.getRect(warning), warningRect);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox.shrink());
    },
  );

  testWidgets(
    'compact Timeline warning follows the calendar layout breakpoint',
    (tester) async {
      final query = _query();
      final gateway = _SyntheticDayGateway(
        coverage: _partialCoverage(),
        items: [_event(query)],
      );
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      await _mount(
        tester,
        width: 820,
        textScale: 1,
        query: query,
        gateway: gateway,
      );
      await tester.tap(find.text('Timeline'));
      await tester.pumpAndSettle();
      final warning = find.byKey(const Key('day-status-warning'));
      expect(warning.hitTestable(), findsOneWidget);
      expect(find.byKey(const Key('day-details-opener')), findsNothing);

      tester.view.physicalSize = const Size(950, 1000);
      await tester.pumpAndSettle();
      expect(warning.hitTestable(), findsOneWidget);
      expect(tester.takeException(), isNull);

      tester.view.physicalSize = const Size(1200, 1000);
      await tester.pumpAndSettle();
      expect(find.byKey(const Key('day-status-warning')), findsNothing);
      expect(find.byTooltip('Calendar status needs refresh'), findsNothing);
      expect(
        find.text('Some calendar data may be missing or out of date'),
        findsOneWidget,
      );
      expect(find.byKey(const Key('day-details-opener')), findsNothing);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox.shrink());
    },
  );

  for (final textScale in [1.0, 2.0]) {
    testWidgets(
      'incomplete event coverage keeps one visible Timeline details action at text $textScale',
      (tester) async {
        final query = _query();
        final gateway = _SyntheticDayGateway(
          coverage: _partialCoverage(),
          items: [_event(query)],
        );
        addTearDown(tester.view.resetPhysicalSize);
        addTearDown(tester.view.resetDevicePixelRatio);
        await _mount(
          tester,
          width: 390,
          textScale: textScale,
          query: query,
          gateway: gateway,
        );
        await tester.tap(find.text('Timeline'));
        await tester.pumpAndSettle();

        expect(find.text('Synthetic event'), findsOneWidget);
        expect(find.byTooltip('Calendar status needs refresh'), findsOneWidget);
        final statusAction = find.byKey(const Key('day-status-warning'));
        expect(statusAction, findsOneWidget);
        expect(statusAction.hitTestable(), findsOneWidget);
        expect(find.byTooltip('Tasks and calendar status'), findsNothing);
        expect(
          tester.getSemantics(statusAction).flagsCollection.isLiveRegion,
          isTrue,
        );
        expect(
          tester.getSemantics(statusAction).label,
          'Calendar status needs refresh',
        );
        final refresh = find.byTooltip('Refresh calendar');
        expect(refresh, findsOneWidget);
        expect(tester.getRect(refresh).bottom, lessThanOrEqualTo(1000));

        await tester.tap(statusAction);
        await tester.pumpAndSettle();
        expect(find.text('Tasks and calendar status'), findsOneWidget);
        await tester.tap(find.byKey(const Key('day-details-close')));
        await tester.pumpAndSettle();

        gateway.coverage = _completeCoverage(query);
        await tester.tap(find.byTooltip('Refresh calendar'));
        await tester.pumpAndSettle();
        expect(find.text('Synthetic event'), findsOneWidget);
        expect(find.byTooltip('Calendar status needs refresh'), findsNothing);
        expect(find.byKey(const Key('day-status-warning')), findsNothing);
        expect(
          find.byTooltip('Calendars synced for this date'),
          findsOneWidget,
        );
        expect(tester.takeException(), isNull);
        await tester.pumpWidget(const SizedBox.shrink());
      },
    );

    testWidgets(
      'retained events stay marked stale after refresh failure and dismissal at text $textScale',
      (tester) async {
        final query = _query();
        final gateway = _SyntheticDayGateway(
          coverage: _completeCoverage(query),
          items: [_event(query)],
        );
        addTearDown(tester.view.resetPhysicalSize);
        addTearDown(tester.view.resetDevicePixelRatio);
        await _mount(
          tester,
          width: 390,
          textScale: textScale,
          query: query,
          gateway: gateway,
        );
        await tester.tap(find.text('Timeline'));
        await tester.pumpAndSettle();

        expect(find.text('Synthetic event'), findsOneWidget);
        expect(
          find.byTooltip('Calendars synced for this date'),
          findsOneWidget,
        );
        gateway.failNextRefresh = true;
        await tester.tap(find.byTooltip('Refresh calendar'));
        await tester.pumpAndSettle();

        expect(find.text('Synthetic event'), findsOneWidget);
        expect(find.byTooltip('Calendar status needs refresh'), findsOneWidget);
        expect(find.byKey(const Key('day-status-warning')), findsOneWidget);
        expect(
          find.byTooltip('Bad state: Day refresh failed: unavailable'),
          findsNothing,
        );
        expect(find.byTooltip('Tasks and calendar status'), findsNothing);
        expect(find.text('Close'), findsOneWidget);
        await tester.tap(find.text('Close'));
        await tester.pumpAndSettle();

        // Dismissing the read error must not make the retained event snapshot
        // look current or remove its only visible status/details control.
        expect(find.text('Synthetic event'), findsOneWidget);
        expect(find.byTooltip('Calendar status needs refresh'), findsOneWidget);
        final statusAction = find.byKey(const Key('day-status-warning'));
        expect(statusAction, findsOneWidget);
        expect(statusAction.hitTestable(), findsOneWidget);
        expect(find.byTooltip('Tasks and calendar status'), findsNothing);
        final refresh = find.byTooltip('Refresh calendar');
        expect(refresh, findsOneWidget);
        expect(tester.getRect(refresh).bottom, lessThanOrEqualTo(1000));
        expect(statusAction.hitTestable(), findsOneWidget);

        final golden = textScale == 1
            ? 'goldens/calendar_timeline_stale_events_widget_390.png'
            : 'goldens/calendar_timeline_stale_events_widget_390_2x.png';
        await expectLater(
          find.byKey(const Key('full-day-screen')),
          matchesGoldenFile(golden),
        );

        gateway.coverage = _completeCoverage(query);
        await tester.tap(refresh);
        await tester.pumpAndSettle();
        expect(find.text('Synthetic event'), findsOneWidget);
        expect(find.byTooltip('Calendar status needs refresh'), findsNothing);
        expect(find.byKey(const Key('day-status-warning')), findsNothing);
        expect(
          find.byTooltip('Calendars synced for this date'),
          findsOneWidget,
        );
        expect(tester.takeException(), isNull);
        await tester.pumpWidget(const SizedBox.shrink());
      },
    );
  }
}

final class _SyntheticDayGateway implements DayGateway, DayRefreshGateway {
  _SyntheticDayGateway({this.coverage, this.items = const []});

  DayCalendarCoverage? coverage;
  List<DayItem> items;
  bool failNextRefresh = false;
  Completer<DayRefreshSnapshot>? _nextRefresh;
  Completer<DayRefreshSnapshot>? _activeRefresh;
  DayQuery? _activeQuery;
  int _refreshCount = 0;

  @override
  Future<DaySnapshot> loadDay(DayQuery query) async =>
      _snapshot(query, coverage, items: items);

  @override
  Future<List<ManualCalendarDestination>>
  loadExternalCalendarDestinations() async => const [];

  @override
  Future<DayRefreshSnapshot> refreshDay({
    required String commandId,
    required DayQuery query,
  }) async {
    final sequence = ++_refreshCount;
    final operationRef = 'synthetic-refresh-$sequence';
    if (failNextRefresh) {
      failNextRefresh = false;
      return FailedDayRefresh(
        operationRef: operationRef,
        revision: 1,
        failure: DayRefreshFailure.unavailable,
      );
    }
    final deferred = _nextRefresh;
    if (deferred != null) {
      _nextRefresh = null;
      _activeRefresh = deferred;
      _activeQuery = query;
      return deferred.future;
    }
    return CompletedDayRefresh(
      operationRef: operationRef,
      revision: 1,
      day: _snapshot(query, coverage, items: items),
    );
  }

  void deferNextRefresh() {
    _nextRefresh = Completer<DayRefreshSnapshot>();
  }

  void completeDeferredRefresh() {
    final refresh = _activeRefresh!;
    final query = _activeQuery!;
    refresh.complete(
      CompletedDayRefresh(
        operationRef: 'synthetic-refresh-$_refreshCount',
        revision: 1,
        day: _snapshot(query, coverage, items: items),
      ),
    );
    _activeRefresh = null;
    _activeQuery = null;
  }

  @override
  Future<DayRefreshSnapshot> observeDayRefresh(String operationRef) =>
      throw StateError('Terminal synthetic refresh should not be observed.');

  @override
  dynamic noSuchMethod(Invocation invocation) =>
      throw StateError('Unexpected gateway call: ${invocation.memberName}');
}

final class _ReadyRuntimeGateway implements RuntimeGateway {
  @override
  Future<RuntimeReadinessSnapshot> readiness(String requestId) async =>
      const RuntimeReadinessSnapshot(state: RuntimeReadinessState.ready);

  @override
  Future<RuntimePreparationResult> prepare(String operationId) =>
      throw StateError('Ready runtime should not prepare.');

  @override
  Future<RuntimePreparationResult> getPreparation(String operationId) =>
      throw StateError('Ready runtime should not have preparation work.');

  @override
  Future<RuntimePreparationResult> acknowledge(String operationId) =>
      throw StateError('Ready runtime should not acknowledge preparation.');
}
