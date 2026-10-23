import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/l10n/app_localizations.dart';
import 'package:floe_client/app/floe_app.dart';
import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/runtime/local_owner_gateways_scope.dart';
import 'package:floe_client/features/day/application/day_gateway.dart';
import 'package:floe_client/features/day/domain/day_models.dart';
import 'package:floe_client/features/day/presentation/day_appearance.dart';

final _query = DayQuery.local(
  personId: 'ui-v033-synthetic',
  date: DateTime(2026, 10, 23, 10),
  now: DateTime(2026, 10, 23, 10),
);

List<DayItem> _items() => [
  EventItem(
    id: 'studio-planning',
    title: 'Studio planning',
    revision: 1,
    createdAt: _query.now,
    startsAt: DateTime(2026, 10, 23),
    endsAt: DateTime(2026, 10, 24),
    isAllDay: true,
    source: const CalendarDayItemSource(
      sourceRef: 'synthetic-calendar-source',
      calendarRef: 'synthetic-calendar',
      calendarLabel: 'Work',
    ),
  ),
  EventItem(
    id: 'morning-focus',
    title: 'Morning focus',
    revision: 1,
    createdAt: _query.now,
    startsAt: DateTime(2026, 10, 23, 8, 30),
    endsAt: DateTime(2026, 10, 23, 9),
    source: const CalendarDayItemSource(
      sourceRef: 'synthetic-calendar-source',
      calendarRef: 'synthetic-calendar',
      calendarLabel: 'Work',
    ),
  ),
  EventItem(
    id: 'product-review',
    title: 'Product review',
    revision: 1,
    createdAt: _query.now,
    startsAt: DateTime(2026, 10, 23, 9, 30),
    endsAt: DateTime(2026, 10, 23, 10, 30),
    source: const CalendarDayItemSource(
      sourceRef: 'synthetic-calendar-source',
      calendarRef: 'synthetic-calendar',
      calendarLabel: 'Work',
    ),
  ),
  EventItem(
    id: 'partner-sync',
    title: 'Partner sync',
    revision: 1,
    createdAt: _query.now,
    startsAt: DateTime(2026, 10, 23, 13),
    endsAt: DateTime(2026, 10, 23, 14),
    source: const CalendarDayItemSource(
      sourceRef: 'synthetic-calendar-source',
      calendarRef: 'synthetic-calendar',
      calendarLabel: 'Work',
    ),
  ),
  EventItem(
    id: 'weekly-planning',
    title: 'Weekly planning',
    revision: 1,
    createdAt: _query.now,
    startsAt: DateTime(2026, 10, 23, 15),
    endsAt: DateTime(2026, 10, 23, 15, 45),
    source: const CalendarDayItemSource(
      sourceRef: 'synthetic-calendar-source',
      calendarRef: 'synthetic-calendar',
      calendarLabel: 'Work',
    ),
  ),
  TaskItem(
    id: 'launch-brief',
    title: 'Prepare the launch brief for partner review',
    revision: 1,
    createdAt: _query.now,
    deadline: null,
  ),
  TaskItem(
    id: 'revised-timeline',
    title: 'Send the revised timeline to Maya',
    revision: 1,
    createdAt: _query.now,
    deadline: null,
  ),
  TaskItem(
    id: 'design-room',
    title: 'Book a room for the design review',
    revision: 1,
    createdAt: _query.now,
    deadline: DateTime(2026, 10, 23, 10),
    completedAt: DateTime(2026, 10, 23, 9, 15),
  ),
  TaskItem(
    id: 'beta-feedback',
    title: 'Collect feedback from the beta team',
    revision: 1,
    createdAt: _query.now,
    deadline: DateTime(2026, 10, 23, 9),
  ),
  NoteItem(
    id: 'quick-thought',
    title: 'Quick thought\nCall Maya after the product review.',
    revision: 1,
    createdAt: DateTime(2026, 10, 23, 9, 48),
  ),
  NoteItem(
    id: 'launch-plan',
    title: 'Launch plan\nReview rollout timing before Friday’s review, confirm partner messaging, and leave room for a final accessibility pass before sharing the brief with the team.',
    revision: 1,
    createdAt: DateTime(2026, 10, 23, 9, 12),
  ),
  NoteItem(
    id: 'review-cues',
    title: 'Review cues\nKeep the decision list short and make the next owner clear.',
    revision: 1,
    createdAt: DateTime(2026, 10, 23, 8, 30),
  ),
  NoteItem(
    id: 'morning-plan',
    title: 'Morning plan\nProtect a little quiet time after the partner sync.',
    revision: 1,
    createdAt: DateTime(2026, 10, 23, 8),
  ),
];

Widget _fixtureAppearance(Widget child) => DayAppearance(
  tones: const {
    'studio-planning': ItemTone.violet,
    'morning-focus': ItemTone.violet,
    'product-review': ItemTone.blue,
    'partner-sync': ItemTone.mint,
    'weekly-planning': ItemTone.amber,
    'launch-brief': ItemTone.violet,
    'revised-timeline': ItemTone.blue,
    'design-room': ItemTone.mint,
    'beta-feedback': ItemTone.amber,
  },
  notes: const {
    'quick-thought': NoteAppearance(
      excerpt: 'Call Maya after the product review.',
      timestamp: 'Today, 9:48 AM',
      tone: ItemTone.blue,
    ),
    'launch-plan': NoteAppearance(
      excerpt: 'Review rollout timing before Friday’s review, confirm partner messaging, and leave room for a final accessibility pass before sharing the brief with the team.',
      timestamp: 'Today, 9:12 AM',
    ),
    'review-cues': NoteAppearance(
      excerpt: 'Keep the decision list short and make the next owner clear.',
      timestamp: 'Today, 8:30 AM',
      tone: ItemTone.mint,
    ),
    'morning-plan': NoteAppearance(
      excerpt: 'Protect a little quiet time after the partner sync.',
      timestamp: 'Today, 8:00 AM',
      tone: ItemTone.amber,
    ),
  },
  tasks: const {
    'launch-brief': TaskAppearance(
      description: 'Prepare the launch brief for partner review. Confirm the timeline, key messages, and audience before sharing.',
      subtasks: [
        (title: 'Confirm the timeline', duration: '15m', done: true),
        (title: 'Review partner messaging', duration: '20m', done: false),
      ],
    ),
  },
  child: child,
);

Future<void> _loadBundledFonts() async {
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

Widget _app({double scale = 1, bool reduceMotion = false}) =>
    _fixtureAppearance(
      FloeApp(
        personId: _query.personId,
        gateway: _CurrentDayGateway(),
        query: _query,
        clock: () => _query.now,
        ownerGateways: const LocalOwnerGateways(),
        builder: (context, child) => MediaQuery(
          data: MediaQuery.of(context).copyWith(
            textScaler: TextScaler.linear(scale),
            disableAnimations: reduceMotion,
          ),
          child: RepaintBoundary(
            key: const Key('v033-screen-capture'),
            child: child!,
          ),
        ),
      ),
    );

void _setViewport(WidgetTester tester, double width, double height) {
  tester.view.physicalSize = Size(width, height);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
}

Future<void> _open(WidgetTester tester, String destination) async {
  await tester.tap(find.byTooltip(destination));
  await tester.pumpAndSettle();
}

void main() {
  setUpAll(_loadBundledFonts);

  for (final (width, height, suffix) in [
    (1280.0, 900.0, '1280'),
    (390.0, 844.0, '390'),
  ]) {
    testWidgets('core screens render coherently at $suffix px', (tester) async {
      _setViewport(tester, width, height);
      await tester.pumpWidget(_app());
      await tester.pumpAndSettle();

      expect(find.textContaining('Studio planning'), findsOneWidget);
      expect(find.text('Calendar status unavailable'), findsNothing);
      if (width <= 780) {
        final dock = find.byKey(const Key('compact-dock-slot'));
        expect(tester.getRect(dock).bottom, lessThanOrEqualTo(height));
        for (final label in ['Today', 'Tasks', 'Notes', 'Floe']) {
          expect(
            find.descendant(of: dock, matching: find.text(label)),
            findsOneWidget,
          );
        }
      }
      await _open(tester, 'Tasks');
      expect(
        find.text('Prepare the launch brief for partner review'),
        findsOneWidget,
      );
      final taskTitle = tester.widget<Text>(
        find.byKey(const Key('tasks-page-title')),
      );
      expect(taskTitle.style, FloeType.pageTitle);
      expect(
        tester.widget<Text>(find.byKey(const Key('tasks-page-summary'))).style,
        FloeType.bodySmall,
      );
      final tasksTitleTop = tester
          .getRect(find.byKey(const Key('tasks-page-title')))
          .top;
      final taskSummaryRect = tester.getRect(
        find.byKey(const Key('tasks-page-summary')),
      );
      final firstTaskRect = tester.getRect(
        find.text('Prepare the launch brief for partner review'),
      );
      expect(firstTaskRect.top - taskSummaryRect.bottom, lessThan(48));
      final taskCollection = find.byKey(const Key('tasks-collection'));
      final collectionRect = tester.getRect(taskCollection);
      expect(collectionRect.width, closeTo(width > 1000 ? 840 : width - 24, 1));
      expect(find.text('Task · Time not set'), findsNothing);
      expect(find.textContaining('Overdue · Due '), findsOneWidget);
      final copy = AppLocalizations.of(tester.element(taskCollection));
      expect(find.text(copy.deleteTask), findsNothing);
      expect(
        find.byTooltip(
          copy.deleteItemLabel('Prepare the launch brief for partner review'),
        ),
        findsNothing,
      );
      final taskSemantics = tester.ensureSemantics();
      try {
        expect(
          find.bySemanticsLabel(
            'Prepare the launch brief for partner review. Time not set',
          ),
          findsOneWidget,
        );
        expect(
          find.bySemanticsLabel(RegExp(r'Overdue · Due .*')),
          findsOneWidget,
        );
      } finally {
        taskSemantics.dispose();
      }
      await _open(tester, 'Notes');
      expect(find.byKey(const Key('notes-page-title')), findsOneWidget);
      expect(find.byKey(const Key('notes-count')), findsOneWidget);
      expect(find.text('4'), findsOneWidget);
      expect(find.text('Quick thought'), findsOneWidget);
      expect(find.text('Launch plan'), findsOneWidget);
      final notesTitleRect = tester.getRect(
        find.byKey(const Key('notes-page-title')),
      );
      expect(
        notesTitleRect.top,
        closeTo(tasksTitleTop, 1),
        reason: 'collection headings share a top rhythm at $suffix px',
      );
      expect(
        tester.widget<Text>(find.byKey(const Key('notes-page-title'))).style,
        FloeType.pageTitle,
      );
      final semantics = tester.ensureSemantics();
      try {
        expect(find.bySemanticsLabel('Notes · 4'), findsOneWidget);
      } finally {
        semantics.dispose();
      }
      await expectLater(
        find.byKey(const Key('v033-screen-capture')),
        matchesGoldenFile('goldens/v033-notes-$suffix.png'),
      );
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox.shrink());
    });
  }

  testWidgets('large narrow titles, long content, return and reduced motion', (
    tester,
  ) async {
    _setViewport(tester, 320, 560);
    await tester.pumpWidget(_app(scale: 2, reduceMotion: true));
    await tester.pump(const Duration(milliseconds: 550));
    await tester.pumpAndSettle();
    await _open(tester, 'Tasks');
    expect(find.text('Tasks'), findsNWidgets(2));
    await tester.tap(find.text('Prepare the launch brief for partner review'));
    await tester.pumpAndSettle();
    expect(find.text('Confirm the timeline'), findsOneWidget);
    await tester.tap(find.text('Back'));
    await tester.pumpAndSettle();
    expect(
      find.text('Prepare the launch brief for partner review'),
      findsOneWidget,
    );

    await _open(tester, 'Notes');
    expect(find.byKey(const Key('notes-page-title')), findsOneWidget);
    expect(find.byKey(const Key('notes-count')), findsOneWidget);
    expect(find.text('Launch plan'), findsOneWidget);
    await tester.ensureVisible(find.text('Launch plan'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Launch plan'));
    await tester.pumpAndSettle();
    expect(find.byKey(const Key('note-original-content')), findsOneWidget);
    await tester.tap(find.byTooltip('Close'));
    await tester.pumpAndSettle();
    expect(find.byKey(const Key('notes-page-title')), findsOneWidget);
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox.shrink());
  });
}

final class _CurrentDayGateway implements DayGateway {
  @override
  Future<DaySnapshot> loadDay(DayQuery query) async {
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
    final items = _items();
    return DaySnapshot(
      personId: query.personId,
      date: query.date,
      generatedAt: query.now,
      timezoneOffsetSeconds: query.timezoneOffsetSeconds,
      items: items,
      nowEventId: 'product-review',
      nextEventId: 'partner-sync',
      calendarCoverage: DayCalendarCoverage(
        sources: [
          DayCalendarSourceCoverage(
            sourceRef: 'synthetic-calendar-source',
            label: 'Work',
            state: DayCoverageState.current,
            lastRange: range,
            resources: [
              DayCalendarResourceCoverage(
                resourceRef: 'synthetic-calendar',
                label: 'Work',
                state: DayCoverageState.current,
                lastRange: range,
              ),
            ],
          ),
        ],
      ),
    );
  }

  @override
  dynamic noSuchMethod(Invocation invocation) => throw StateError(
    'Unexpected synthetic Day gateway call: ${invocation.memberName}',
  );
}
