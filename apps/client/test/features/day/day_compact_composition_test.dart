import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter/semantics.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/floe_app.dart';
import 'package:floe_client/app/runtime/local_owner_gateways_scope.dart';
import 'package:floe_client/app/runtime/runtime_controller.dart';
import 'package:floe_client/app/runtime/runtime_gateway.dart';
import 'package:floe_client/features/day/application/day_gateway.dart';
import 'package:floe_client/features/day/domain/day_models.dart';
import 'package:floe_client/preview/product_fixture.dart';

final _today = DateTime(2026, 10, 23, 10);
const _personId = 'ui-v034-synthetic-day';

DayQuery _query() =>
    DayQuery.local(personId: _personId, date: _today, now: _today);

Future<void> _loadProductionFonts() async {
  final manifest = jsonDecode(
    await rootBundle.loadString('FontManifest.json'),
  ) as List<dynamic>;
  for (final value in manifest) {
    final entry = value as Map<String, dynamic>;
    final loader = FontLoader(entry['family'] as String);
    for (final font in entry['fonts'] as List<dynamic>) {
      loader.addFont(
        rootBundle.load((font as Map<String, dynamic>)['asset'] as String),
      );
    }
    await loader.load();
  }
}

Widget _app({
  required _ReadyDayGateway gateway,
  required RuntimeController runtime,
  required double textScale,
}) => previewAppearance(
  FloeApp(
    personId: _personId,
    gateway: gateway,
    query: _query(),
    clock: () => _today,
    ownerGateways: LocalOwnerGateways(runtime: runtime),
    builder: (context, child) => RepaintBoundary(
      key: const Key('ui-v034-day-capture'),
      child: MediaQuery(
        data: MediaQuery.of(context)
            .copyWith(textScaler: TextScaler.linear(textScale)),
        child: child!,
      ),
    ),
  ),
);

Future<RuntimeController> _runtime() async {
  final runtime = RuntimeController(
    gateway: _ReadyRuntimeGateway(),
    personId: _personId,
  );
  await runtime.open();
  return runtime;
}

Future<void> _mount(
  WidgetTester tester, {
  required double width,
  required double height,
  required double textScale,
  FakeViewPadding padding = const FakeViewPadding(),
}) async {
  tester.view.physicalSize = Size(width, height);
  tester.view.devicePixelRatio = 1;
  tester.view.padding = padding;
  tester.view.viewPadding = padding;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
  addTearDown(tester.view.resetPadding);
  addTearDown(tester.view.resetViewPadding);

  final runtime = await _runtime();
  addTearDown(runtime.dispose);
  await tester.pumpWidget(
    _app(gateway: _ReadyDayGateway(), runtime: runtime, textScale: textScale),
  );
  await tester.pumpAndSettle();
}

void _expectTargetAtLeast44(WidgetTester tester, Finder finder) {
  expect(finder.hitTestable(), findsOneWidget);
  final rect = tester.getRect(finder);
  expect(rect.width, greaterThanOrEqualTo(44));
  expect(rect.height, greaterThanOrEqualTo(44));
}

FocusNode _focusFor(WidgetTester tester, Finder focusElementFinder) {
  final element = tester.element(focusElementFinder);
  FocusNode? result;
  element.visitChildElements((child) {
    result = Focus.of(child, createDependency: false);
  });
  if (result == null) throw StateError('Focus widget has no child context.');
  return result!;
}

void main() {
  setUpAll(_loadProductionFonts);

  testWidgets('390px Day brings events forward and keeps complete actions', (
    tester,
  ) async {
    await _mount(tester, width: 390, height: 844, textScale: 1);

    // A completed destination transition gives the golden a fully painted dock.
    await tester.tap(find.byTooltip('Tasks'));
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Today'));
    await tester.pumpAndSettle();

    expect(find.text('Schedule'), findsOneWidget);
    expect(find.text('Timeline'), findsOneWidget);
    expect(find.text('Now'), findsOneWidget);
    expect(find.text('Next'), findsOneWidget);
    expect(find.text('Also today'), findsOneWidget);
    expect(find.text('Studio stand-up'), findsOneWidget);
    expect(find.text('Work'), findsWidgets);
    expect(find.byTooltip('Create event'), findsOneWidget);
    expect(find.byTooltip('Refresh calendar'), findsOneWidget);
    expect(find.byKey(const Key('day-go-today')), findsNothing);
    final healthy = find.byKey(const Key('day-calendar-healthy-details'));
    expect(healthy, findsOneWidget);
    _expectTargetAtLeast44(tester, healthy);

    final semantics = tester.ensureSemantics();
    try {
      expect(
        find.bySemanticsLabel('Calendars synced for this date'),
        findsOneWidget,
      );
      expect(
        tester
            .getSemantics(healthy)
            .getSemanticsData()
            .hasAction(SemanticsAction.tap),
        isTrue,
      );
    } finally {
      semantics.dispose();
    }

    for (final label in ['Today', 'Tasks', 'Notes', 'Floe']) {
      expect(
        find.descendant(
          of: find.byKey(const Key('compact-dock-slot')),
          matching: find.text(label),
        ),
        findsOneWidget,
      );
    }

    final firstEventTop = tester.getRect(find.text('Studio stand-up')).top;
    // v033 source review measured the same title at y=369px. This bound
    // protects the intended compact composition while the golden records it.
    expect(firstEventTop, lessThan(300));
    expect(tester.takeException(), isNull);
    await expectLater(
      find.byKey(const Key('ui-v034-day-capture')),
      matchesGoldenFile('goldens/day_compact_390.png'),
    );

    // Healthy status remains a route to the same source/resource evidence.
    final statusButton = find.descendant(
      of: healthy,
      matching: find.byType(TextButton),
    );
    final focusElements = find.descendant(
      of: statusButton,
      matching: find.byType(Focus),
    );
    expect(focusElements, findsWidgets);
    final focusNode = _focusFor(tester, focusElements.last);
    for (var index = 0; index < 24 && !focusNode.hasPrimaryFocus; index++) {
      await tester.sendKeyEvent(LogicalKeyboardKey.tab);
      await tester.pump();
    }
    expect(focusNode.hasPrimaryFocus, isTrue);
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pumpAndSettle();
    expect(find.byKey(const Key('day-details-close')), findsOneWidget);
    await tester.tap(find.text('Calendars synced for this date'));
    await tester.pumpAndSettle();
    await tester.ensureVisible(find.text('Source · Floe calendars'));
    await tester.pumpAndSettle();
    expect(find.text('Source · Floe calendars'), findsOneWidget);
    expect(find.text('Resource · Work'), findsOneWidget);
    expect(find.text('Resource · Personal'), findsOneWidget);
    await tester.tap(find.byKey(const Key('day-details-close')));
    await tester.pumpAndSettle();

    // Available event targets remain exposed without submitting an action.
    await tester.tap(find.text('Studio stand-up'));
    await tester.pumpAndSettle();
    expect(find.text('Edit event'), findsOneWidget);
    expect(find.text('Delete'), findsOneWidget);
    await tester.tap(find.byTooltip('Close'));
    await tester.pumpAndSettle();
    await tester.pumpWidget(const SizedBox.shrink());
  });

  testWidgets('320px at 200% wraps safely and retains full date and Today', (
    tester,
  ) async {
    await _mount(
      tester,
      width: 320,
      height: 560,
      textScale: 2,
      padding: const FakeViewPadding(top: 20, bottom: 16),
    );

    expect(find.byKey(const Key('day-compact-toolbar')), findsOneWidget);
    expect(find.byTooltip('Create event'), findsOneWidget);
    expect(find.byTooltip('Refresh calendar'), findsOneWidget);
    _expectTargetAtLeast44(tester, find.byTooltip('Previous day'));
    _expectTargetAtLeast44(tester, find.byTooltip('Next day'));
    _expectTargetAtLeast44(tester, find.byTooltip('Create event'));
    _expectTargetAtLeast44(tester, find.byTooltip('Refresh calendar'));
    expect(tester.takeException(), isNull);

    await tester.tap(find.byTooltip('Previous day'));
    await tester.pumpAndSettle();
    final todayButton = find.byKey(const Key('day-go-today'));
    expect(todayButton, findsOneWidget);
    _expectTargetAtLeast44(tester, todayButton);
    expect(find.text('Thu, Oct 22'), findsNothing);
    final fullDateSemantics = tester.ensureSemantics();
    try {
      expect(find.bySemanticsLabel('Thu, Oct 22'), findsOneWidget);
    } finally {
      fullDateSemantics.dispose();
    }
    expect(tester.takeException(), isNull);
    await expectLater(
      find.byKey(const Key('ui-v034-day-capture')),
      matchesGoldenFile('goldens/day_compact_320_200.png'),
    );

    await tester.tap(todayButton);
    await tester.pumpAndSettle();
    expect(find.text('Studio stand-up'), findsOneWidget);
    expect(find.byKey(const Key('day-go-today')), findsNothing);
    final healthy = find.byKey(const Key('day-calendar-healthy-details'));
    expect(healthy.hitTestable(), findsOneWidget);
    _expectTargetAtLeast44(tester, healthy);
    await expectLater(
      find.byKey(const Key('ui-v034-day-capture')),
      matchesGoldenFile('goldens/day_compact_320_200_today.png'),
    );
    await tester.tap(healthy);
    await tester.pumpAndSettle();
    expect(find.byKey(const Key('day-details-close')), findsOneWidget);
    await tester.tap(find.byKey(const Key('day-details-close')));
    await tester.pumpAndSettle();
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox.shrink());
  });

  testWidgets('incomplete coverage keeps its prominent status, never healthy', (
    tester,
  ) async {
    for (final state in [
      DayCoverageState.stale,
      DayCoverageState.partial,
      DayCoverageState.unavailable,
    ]) {
      final query = _query();
      final gateway = _ReadyDayGateway(
        coverage: DayCalendarCoverage(
          sources: [
            DayCalendarSourceCoverage(
              sourceRef: 'synthetic-source',
              label: 'Floe calendars',
              state: state,
              resources: const [],
            ),
          ],
        ),
      );
      final runtime = await _runtime();
      tester.view.physicalSize = const Size(390, 844);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      addTearDown(runtime.dispose);
      await tester.pumpWidget(
        previewAppearance(
          FloeApp(
            personId: _personId,
            gateway: gateway,
            query: query,
            clock: () => _today,
            ownerGateways: LocalOwnerGateways(runtime: runtime),
          ),
        ),
      );
      await tester.pumpAndSettle();
      expect(
        find.byKey(const Key('day-calendar-healthy-details')),
        findsNothing,
      );
      expect(
        find.text('Some calendar data may be missing or out of date'),
        findsOneWidget,
      );
      await tester.tap(find.text('Timeline'));
      await tester.pumpAndSettle();
      expect(find.byKey(const Key('day-status-warning')), findsOneWidget);
      expect(find.byTooltip('Calendar status needs refresh'), findsOneWidget);
      expect(
        find.byKey(const Key('day-calendar-healthy-details')),
        findsNothing,
      );
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox.shrink());
    }
  });

  testWidgets('320px large-text Timeline keeps stale status actionable', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(320, 560);
    tester.view.devicePixelRatio = 1;
    tester.view.padding = const FakeViewPadding(top: 20, bottom: 16);
    tester.view.viewPadding = const FakeViewPadding(top: 20, bottom: 16);
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    addTearDown(tester.view.resetPadding);
    addTearDown(tester.view.resetViewPadding);
    final query = _query();
    final gateway = _ReadyDayGateway(
      coverage: DayCalendarCoverage(
        sources: [
          DayCalendarSourceCoverage(
            sourceRef: 'synthetic-source',
            label: 'Floe calendars',
            state: DayCoverageState.stale,
            resources: const [],
          ),
        ],
      ),
    );
    final runtime = await _runtime();
    addTearDown(runtime.dispose);
    await tester.pumpWidget(
      previewAppearance(
        FloeApp(
          personId: _personId,
          gateway: gateway,
          query: query,
          clock: () => _today,
          ownerGateways: LocalOwnerGateways(runtime: runtime),
          builder: (context, child) => MediaQuery(
            data: MediaQuery.of(context)
                .copyWith(textScaler: const TextScaler.linear(2)),
            child: child!,
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(
      find.text('Some calendar data may be missing or out of date'),
      findsOneWidget,
    );
    await tester.ensureVisible(find.text('Timeline'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Timeline'));
    await tester.pumpAndSettle();
    final warning = find.byKey(const Key('day-status-warning'));
    _expectTargetAtLeast44(tester, warning);
    expect(
      tester.getSemantics(warning).getSemanticsData().label,
      'Calendar status needs refresh',
    );
    expect(find.byKey(const Key('day-calendar-healthy-details')), findsNothing);
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox.shrink());
  });
}

final class _ReadyDayGateway implements DayGateway, DayRefreshGateway {
  _ReadyDayGateway({this.coverage});

  final DayCalendarCoverage? coverage;
  int refreshCount = 0;

  DaySnapshot _snapshot(DayQuery query) {
    final date = query.date;
    DateTime at(int hour, int minute) =>
        DateTime(date.year, date.month, date.day, hour, minute);
    final range = DayCalendarRange(
      startDate: query.date,
      endDateExclusive: DateTime(date.year, date.month, date.day + 1),
      timezoneOffsetSeconds: query.timezoneOffsetSeconds,
      endTimezoneOffsetSeconds: query.endTimezoneOffsetSeconds,
    );
    final current = EventItem(
      id: 'synthetic-current-event',
      title: 'Studio stand-up',
      revision: 2,
      createdAt: query.now,
      startsAt: at(9, 30),
      endsAt: at(10, 30),
      source: const CalendarDayItemSource(
        sourceRef: 'synthetic-source',
        calendarRef: 'synthetic-work-calendar',
        calendarLabel: 'Work',
      ),
      actionTarget: const DayEventTarget(
        eventId: 'synthetic-current-event',
        expectedRevision: 2,
      ),
    );
    final next = EventItem(
      id: 'synthetic-next-event',
      title: 'Launch brief review',
      revision: 1,
      createdAt: query.now,
      startsAt: at(10, 30),
      endsAt: at(11, 0),
      source: const CalendarDayItemSource(
        sourceRef: 'synthetic-source',
        calendarRef: 'synthetic-work-calendar',
        calendarLabel: 'Work',
      ),
    );
    final later = EventItem(
      id: 'synthetic-later-event',
      title: 'Planning block',
      revision: 1,
      createdAt: query.now,
      startsAt: at(13, 30),
      endsAt: at(14, 30),
      source: const CalendarDayItemSource(
        sourceRef: 'synthetic-source',
        calendarRef: 'synthetic-work-calendar',
        calendarLabel: 'Work',
      ),
    );
    final complete = DayCalendarCoverage(
      sources: [
        DayCalendarSourceCoverage(
          sourceRef: 'synthetic-source',
          label: 'Floe calendars',
          state: DayCoverageState.current,
          lastRange: range,
          resources: [
            DayCalendarResourceCoverage(
              resourceRef: 'synthetic-work-calendar',
              label: 'Work',
              state: DayCoverageState.current,
              lastRange: range,
            ),
            DayCalendarResourceCoverage(
              resourceRef: 'synthetic-personal-calendar',
              label: 'Personal',
              state: DayCoverageState.current,
              lastRange: range,
            ),
          ],
        ),
      ],
    );
    return DaySnapshot(
      personId: query.personId,
      date: query.date,
      generatedAt: query.now,
      timezoneOffsetSeconds: query.timezoneOffsetSeconds,
      items: [current, next, later],
      nowEventId: 'synthetic-current-event',
      nextEventId: 'synthetic-next-event',
      calendarCoverage: coverage ?? complete,
    );
  }

  @override
  Future<DaySnapshot> loadDay(DayQuery query) async => _snapshot(query);

  @override
  Future<DayRefreshSnapshot> refreshDay({
    required String commandId,
    required DayQuery query,
  }) async {
    refreshCount++;
    return CompletedDayRefresh(
      operationRef: 'synthetic-day-refresh-$refreshCount',
      revision: 1,
      day: _snapshot(query),
    );
  }

  @override
  Future<DayRefreshSnapshot> observeDayRefresh(String operationRef) async =>
      throw StateError('Synthetic refresh should already be terminal.');

  @override
  Future<List<ManualCalendarDestination>>
  loadExternalCalendarDestinations() async => const [];

  @override
  dynamic noSuchMethod(Invocation invocation) => throw StateError(
    'Unexpected synthetic Day command: ${invocation.memberName}',
  );
}

final class _ReadyRuntimeGateway implements RuntimeGateway {
  @override
  Future<RuntimeReadinessSnapshot> readiness(String requestId) async =>
      const RuntimeReadinessSnapshot(state: RuntimeReadinessState.ready);

  @override
  Future<RuntimePreparationResult> prepare(String operationId) async =>
      throw StateError('The ready synthetic Runtime cannot prepare.');

  @override
  Future<RuntimePreparationResult> getPreparation(String operationId) async =>
      throw StateError('The ready synthetic Runtime has no preparation.');

  @override
  Future<RuntimePreparationResult> acknowledge(String operationId) async =>
      throw StateError(
        'The ready synthetic Runtime has nothing to acknowledge.',
      );
}
