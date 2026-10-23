import 'dart:convert';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/floe_app.dart';
import 'package:floe_client/app/floe_button.dart';
import 'package:floe_client/app/runtime/local_owner_gateways_scope.dart';
import 'package:floe_client/app/runtime/runtime_controller.dart';
import 'package:floe_client/app/runtime/runtime_gateway.dart';
import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/features/actions/application/calendar_action_gateway.dart';
import 'package:floe_client/features/actions/domain/calendar_action.dart';
import 'package:floe_client/features/day/application/fake_day_gateway.dart';
import 'package:floe_client/features/day/domain/day_models.dart';
import 'package:floe_client/preview/product_fixture.dart';
import 'package:intl/intl.dart';

const _personId = 'responsive-host-fixture';
final _date = DateTime(2026, 9, 4, 14, 28);
final _query = DayQuery.local(personId: _personId, date: _date, now: _date);

final _notes = [
  NoteItem(
    id: 'launch-plan',
    title: 'Launch plan\nReview rollout timing before Friday’s review.',
    revision: 1,
    createdAt: DateTime(2026, 9, 4, 14, 10),
  ),
  NoteItem(
    id: 'design-review',
    title: 'Design review\nReview the narrow layout before the next demo.',
    revision: 1,
    createdAt: DateTime(2026, 9, 4, 12, 35),
  ),
  NoteItem(
    id: 'reading-list',
    title: 'Reading list\nTwo articles to revisit after the launch plan is settled.',
    revision: 1,
    createdAt: DateTime(2026, 9, 4, 9, 20),
  ),
];

List<NoteItem> _manyNotes() => [
  for (var index = 1; index <= 16; index++)
    NoteItem(
      id: 'note-${index.toString().padLeft(2, '0')}',
      title:
          'Note ${index.toString().padLeft(2, '0')}\nSynthetic note body $index remains readable after resizing.',
      revision: 1,
      createdAt: DateTime(2026, 9, 4, 16 - (index % 12), index),
    ),
];

List<TaskItem> _manyTasks() => [
  for (var index = 1; index <= 16; index++)
    TaskItem(
      id: 'task-${index.toString().padLeft(2, '0')}',
      title: 'Synthetic task ${index.toString().padLeft(2, '0')}',
      revision: 1,
      createdAt: DateTime(2026, 9, 4, 8, index),
    ),
];

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

Widget _app({
  required FakeDayGateway gateway,
  LocalOwnerGateways owners = const LocalOwnerGateways(),
  DayQuery? query,
  double textScale = 1,
  Key captureKey = const Key('responsive-host-capture'),
}) => previewAppearance(
  FloeApp(
    personId: _personId,
    gateway: gateway,
    query: query ?? _query,
    ownerGateways: owners,
    builder: (context, child) => MediaQuery(
      data: MediaQuery.of(context)
          .copyWith(textScaler: TextScaler.linear(textScale)),
      child: RepaintBoundary(key: captureKey, child: child!),
    ),
  ),
);

void _expectCompactDockLabelsFit(WidgetTester tester) {
  final dock = find.byKey(const Key('compact-navigation-dock'));
  for (final label in ['Today', 'Tasks', 'Notes', 'Floe']) {
    final text = find.descendant(of: dock, matching: find.text(label));
    expect(text, findsOneWidget);
    final paragraph = tester.renderObject<RenderParagraph>(text);
    expect(
      paragraph.size.height,
      lessThanOrEqualTo(paragraph.preferredLineHeight + 1),
      reason: '$label should stay on one line at large text',
    );
    final context = tester.element(text);
    final style = DefaultTextStyle.of(context).style.merge(FloeType.caption);
    final naturalText = TextPainter(
      text: TextSpan(text: label, style: style),
      textDirection: Directionality.of(context),
      textScaler: MediaQuery.textScalerOf(context),
      locale: Localizations.localeOf(context),
      maxLines: 1,
    )..layout();
    final button = find
        .ancestor(of: text, matching: find.byType(TextButton))
        .first;
    final target = tester.getRect(button);
    final padding = tester
        .widget<TextButton>(button)
        .style!
        .padding!
        .resolve(const <WidgetState>{})!;
    expect(
      naturalText.width,
      lessThanOrEqualTo(target.width - padding.horizontal),
      reason: '$label should fit its actual text constraints',
    );
    expect(target.width, greaterThanOrEqualTo(44));
    expect(target.height, greaterThanOrEqualTo(44));
    naturalText.dispose();
  }
}

bool _hasKeyAncestor(BuildContext? context, Key key) {
  var found = false;
  context?.visitAncestorElements((element) {
    if (element.widget.key == key) {
      found = true;
      return false;
    }
    return true;
  });
  return found;
}

void _setViewport(
  WidgetTester tester, {
  required double width,
  required double height,
  double textScale = 1,
  FakeViewPadding padding = const FakeViewPadding(),
}) {
  tester.view.physicalSize = Size(width, height);
  tester.view.devicePixelRatio = 1;
  tester.view.padding = padding;
  tester.view.viewPadding = padding;
  tester.platformDispatcher.textScaleFactorTestValue = textScale;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
  addTearDown(tester.view.resetPadding);
  addTearDown(tester.view.resetViewPadding);
  addTearDown(tester.platformDispatcher.clearTextScaleFactorTestValue);
}

void main() {
  setUpAll(_loadBundledFonts);

  for (final scale in [1.0, 2.0]) {
    testWidgets('keyboard focus stays in the compact viewport at $scale×', (
      tester,
    ) async {
      _setViewport(tester, width: 390, height: 844, textScale: scale);
      await tester.pumpWidget(
        _app(
          gateway: FakeDayGateway(initialItems: _notes),
          textScale: scale,
        ),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.byTooltip('Notes'));
      await tester.pumpAndSettle();

      final host = find.byKey(const Key('personal-day-middle-viewport'));
      final header = find.byKey(const Key('compact-navigation-header'));
      final dock = find.byKey(const Key('compact-navigation-dock'));
      expect(header, findsOneWidget);
      expect(dock, findsOneWidget);

      final scrollView = find.byKey(
        const PageStorageKey<String>('workspace-notes-list'),
      );
      final scroll = tester.state<ScrollableState>(
        find
            .descendant(of: scrollView, matching: find.byType(Scrollable))
            .first,
      );
      final finalCard = find.byKey(const ValueKey('note-preview-reading-list'));
      var focused = false;
      for (var tab = 0; tab < 24 && !focused; tab++) {
        await tester.sendKeyEvent(LogicalKeyboardKey.tab);
        await tester.pumpAndSettle();
        focused = _hasKeyAncestor(
          FocusManager.instance.primaryFocus?.context,
          const ValueKey('note-preview-reading-list'),
        );
      }
      expect(focused, isTrue, reason: 'Tab should focus the final note card');

      final hostRect = tester.getRect(host);
      final headerRect = tester.getRect(header);
      final dockRect = tester.getRect(dock);
      final cardRect = tester.getRect(finalCard);
      final titleRect = tester.getRect(find.text('Reading list'));
      final excerptRect = tester.getRect(
        find.text('Two articles to revisit after the launch plan is settled.'),
      );
      final timestampRect = tester.getRect(find.text('9:20 AM'));
      expect(headerRect.bottom, lessThanOrEqualTo(hostRect.top));
      expect(hostRect.bottom, lessThanOrEqualTo(dockRect.top));
      for (final rect in [cardRect, titleRect, excerptRect, timestampRect]) {
        expect(rect.top, greaterThanOrEqualTo(hostRect.top), reason: '$rect');
        expect(
          rect.bottom,
          lessThanOrEqualTo(hostRect.bottom),
          reason: '$rect',
        );
      }
      expect(scroll.position.pixels, greaterThan(0));

      if (scale == 2) {
        await expectLater(
          find.byKey(const Key('responsive-host-capture')),
          matchesGoldenFile(
            'goldens/personal-day-final-note-focused-390-2x.png',
          ),
        );
      }

      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.pumpAndSettle();
      expect(find.byKey(const Key('note-original-content')), findsOneWidget);
      expect(find.textContaining('Reading list'), findsOneWidget);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox.shrink());
    });
  }

  testWidgets('short compact host keeps safe areas and Day controls visible', (
    tester,
  ) async {
    _setViewport(
      tester,
      width: 320,
      height: 560,
      textScale: 2,
      padding: const FakeViewPadding(top: 24, bottom: 34),
    );
    await tester.pumpWidget(
      _app(
        gateway: FakeDayGateway(
          initialItems: [
            EventItem(
              id: 'synthetic-morning-event',
              title: 'Synthetic morning event',
              revision: 1,
              createdAt: _date,
              startsAt: DateTime(2026, 9, 4, 9),
              endsAt: DateTime(2026, 9, 4, 10),
            ),
          ],
        ),
        textScale: 2,
      ),
    );
    await tester.pumpAndSettle();

    final timelineLabel = find.text('Timeline');
    expect(timelineLabel, findsOneWidget);
    expect(
      MediaQuery.textScalerOf(tester.element(timelineLabel)).scale(1),
      2,
      reason: 'the rendered Day control should use the requested 200% scale',
    );

    final host = find.byKey(const Key('personal-day-middle-viewport'));
    final header = find.byKey(const Key('compact-navigation-header'));
    final dock = find.byKey(const Key('compact-navigation-dock'));
    expect(tester.getRect(header).top, greaterThanOrEqualTo(24));
    expect(tester.getRect(dock).bottom, lessThanOrEqualTo(526));
    expect(
      tester.getRect(header).bottom,
      lessThanOrEqualTo(tester.getRect(host).top),
    );
    expect(
      tester.getRect(host).bottom,
      lessThanOrEqualTo(tester.getRect(dock).top),
    );
    expect(find.byTooltip('More').hitTestable(), findsOneWidget);
    expect(find.byTooltip('Today').hitTestable(), findsOneWidget);
    expect(find.byTooltip('Tasks').hitTestable(), findsOneWidget);
    expect(find.byTooltip('Notes').hitTestable(), findsOneWidget);
    _expectCompactDockLabelsFit(tester);

    await tester.tap(find.text('Timeline'));
    await tester.pumpAndSettle();
    final status = find.byKey(const Key('day-status-warning'));
    expect(status, findsOneWidget);
    // The toolbar action is visible on first render; it does not depend on
    // moving keyboard focus through the clipped Timeline header.
    expect(status.hitTestable(), findsOneWidget);
    expect(find.byTooltip('Calendar status unavailable'), findsOneWidget);
    final statusRect = tester.getRect(status);
    expect(statusRect.top, greaterThanOrEqualTo(tester.getRect(host).top));
    expect(statusRect.bottom, lessThanOrEqualTo(tester.getRect(host).bottom));
    expect(statusRect.width, greaterThanOrEqualTo(44));
    expect(statusRect.height, greaterThanOrEqualTo(44));
    expect(tester.getSemantics(status).flagsCollection.isLiveRegion, isTrue);
    expect(tester.getSemantics(status).label, 'Calendar status unavailable');
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox.shrink());
  });

  for (final width in [320.0, 390.0]) {
    for (final scale in [1.0, 2.0]) {
      for (final isToday in [true, false]) {
        testWidgets(
          'compact labels and selected date fit at $width px, ${scale}x, today=$isToday',
          (tester) async {
            final now = DateTime.now();
            final selectedDate = isToday
                ? now
                : DateTime(now.year, now.month, now.day - 1, 12);
            final query = DayQuery.local(
              personId: _personId,
              date: selectedDate,
              now: now,
            );
            _setViewport(tester, width: width, height: 844, textScale: scale);
            await tester.pumpWidget(
              _app(gateway: FakeDayGateway(), query: query, textScale: scale),
            );
            await tester.pumpAndSettle();

            _expectCompactDockLabelsFit(tester);
            final semantics = tester.ensureSemantics();
            try {
              expect(
                tester
                    .getSemantics(find.byTooltip('Today'))
                    .flagsCollection
                    .isSelected,
                ui.Tristate.isTrue,
              );

              final dateFinder = find.byKey(const Key('day-selected-date'));
              final dateText = tester.widget<Text>(dateFinder);
              final locale = Localizations.localeOf(tester.element(dateFinder));
              final fullDate = DateFormat.MMMEd(locale.toString())
                  .format(query.date);
              final shortDate = DateFormat.MMMd(locale.toString())
                  .format(query.date);
              final dateContext = tester.element(dateFinder);
              final dateStyle = DefaultTextStyle.of(dateContext).style
                  .merge(FloeType.headline);
              final fullDatePainter = TextPainter(
                text: TextSpan(text: fullDate, style: dateStyle),
                textDirection: Directionality.of(dateContext),
                textScaler: MediaQuery.textScalerOf(dateContext),
                locale: locale,
                maxLines: 1,
              )..layout();
              final dateWidth = tester.getSize(dateFinder).width;
              expect(
                dateText.data,
                fullDatePainter.width > dateWidth ? shortDate : fullDate,
              );
              expect(dateText.semanticsLabel, fullDate);
              expect(tester.getSemantics(dateFinder).label, fullDate);
              fullDatePainter.dispose();
              final displayedDatePainter = TextPainter(
                text: TextSpan(text: dateText.data, style: dateStyle),
                textDirection: Directionality.of(dateContext),
                textScaler: MediaQuery.textScalerOf(dateContext),
                locale: locale,
                maxLines: 1,
              )..layout();
              expect(displayedDatePainter.width, lessThanOrEqualTo(dateWidth));
              displayedDatePainter.dispose();

              final toolbarToday = find.byKey(const Key('day-go-today'));
              if (isToday) {
                expect(
                  toolbarToday,
                  findsNothing,
                  reason: 'omit the disabled redundant Today control',
                );
              } else {
                expect(toolbarToday, findsOneWidget);
                expect(
                  tester.widget<FloeButton>(toolbarToday).onPressed,
                  isNotNull,
                  reason:
                      'return-to-Today remains available off the current day',
                );
              }
            } finally {
              semantics.dispose();
            }
            expect(tester.takeException(), isNull);
            await tester.pumpWidget(const SizedBox.shrink());
          },
        );
      }
    }
  }

  testWidgets('fixed long date restores its full form when resized', (
    tester,
  ) async {
    final query = DayQuery.local(
      personId: _personId,
      date: DateTime(2026, 9, 30, 12),
      now: DateTime.now(),
    );
    _setViewport(tester, width: 320, height: 844, textScale: 2);
    await tester.pumpWidget(
      _app(gateway: FakeDayGateway(), query: query, textScale: 2),
    );
    await tester.pumpAndSettle();

    final dateFinder = find.byKey(const Key('day-selected-date'));
    final fullDate = DateFormat.MMMEd('en').format(query.date);
    final shortDate = DateFormat.MMMd('en').format(query.date);
    final semantics = tester.ensureSemantics();
    try {
      expect(fullDate, 'Wed, Sep 30');
      expect(tester.widget<Text>(dateFinder).data, shortDate);
      expect(tester.widget<Text>(dateFinder).semanticsLabel, fullDate);
      expect(tester.getSemantics(dateFinder).label, fullDate);

      tester.view.physicalSize = const Size(390, 844);
      await tester.pumpAndSettle();
      expect(tester.widget<Text>(dateFinder).data, fullDate);
      expect(tester.widget<Text>(dateFinder).semanticsLabel, fullDate);
      expect(tester.getSemantics(dateFinder).label, fullDate);
      expect(tester.takeException(), isNull);
    } finally {
      semantics.dispose();
    }
    await tester.pumpWidget(const SizedBox.shrink());
  });

  testWidgets('compact dock keeps destination focus and selected semantics', (
    tester,
  ) async {
    _setViewport(tester, width: 320, height: 844, textScale: 2);
    await tester.pumpWidget(_app(gateway: FakeDayGateway(), textScale: 2));
    await tester.pumpAndSettle();
    final semantics = tester.ensureSemantics();
    try {
      expect(
        tester.getSemantics(find.byTooltip('Today')).flagsCollection.isSelected,
        ui.Tristate.isTrue,
      );
      var dockFocused = false;
      for (var tab = 0; tab < 32 && !dockFocused; tab++) {
        await tester.sendKeyEvent(LogicalKeyboardKey.tab);
        await tester.pumpAndSettle();
        dockFocused = _hasKeyAncestor(
          FocusManager.instance.primaryFocus?.context,
          const Key('compact-navigation-dock'),
        );
      }
      expect(dockFocused, isTrue, reason: 'Tab reaches the compact dock');

      await tester.tap(find.byTooltip('Tasks'));
      await tester.pumpAndSettle();
      expect(
        tester.getSemantics(find.byTooltip('Tasks')).flagsCollection.isSelected,
        ui.Tristate.isTrue,
      );
      await tester.tap(find.byTooltip('Notes'));
      await tester.pumpAndSettle();
      expect(
        tester.getSemantics(find.byTooltip('Notes')).flagsCollection.isSelected,
        ui.Tristate.isTrue,
      );
      _expectCompactDockLabelsFit(tester);
      expect(tester.takeException(), isNull);
    } finally {
      semantics.dispose();
    }
    await tester.pumpWidget(const SizedBox.shrink());
  });

  testWidgets('desktop rail retains its 8px button insets', (tester) async {
    _setViewport(tester, width: 1200, height: 1000);
    await tester.pumpWidget(_app(gateway: FakeDayGateway()));
    await tester.pumpAndSettle();

    final rail = find.byKey(const Key('desktop-navigation-scroll'));
    final tasks = find.descendant(of: rail, matching: find.text('Tasks'));
    final tasksButton = find
        .ancestor(of: tasks, matching: find.byType(TextButton))
        .first;
    final tasksStyle = tester.widget<TextButton>(tasksButton).style!;
    expect(
      tasksStyle.padding!.resolve(const <WidgetState>{}),
      const EdgeInsets.symmetric(horizontal: 8, vertical: 8),
    );

    final assistant = find.byKey(const Key('assistant-entry'));
    final assistantButton = find
        .descendant(of: assistant, matching: find.byType(TextButton))
        .first;
    final assistantStyle = tester.widget<TextButton>(assistantButton).style!;
    expect(
      assistantStyle.padding!.resolve(const <WidgetState>{}),
      const EdgeInsets.symmetric(horizontal: 8, vertical: 8),
    );
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox.shrink());
  });

  testWidgets('search, page position and Settings section survive resize', (
    tester,
  ) async {
    _setViewport(tester, width: 390, height: 844);
    final runtime = RuntimeController(
      gateway: _ReadyRuntimeGateway(),
      personId: _personId,
    );
    await runtime.open();
    final owners = LocalOwnerGateways(
      runtime: runtime,
      operationAuthorization: _SyntheticPolicyGateway(),
    );
    addTearDown(runtime.dispose);

    await tester.pumpWidget(
      _app(
        gateway: FakeDayGateway(
          initialItems: [..._manyNotes(), ..._manyTasks()],
        ),
        owners: owners,
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Notes'));
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextField).first, 'Note');
    await tester.pumpAndSettle();
    expect(find.text('Matching notes · 16'), findsOneWidget);

    final notesPage = find.byKey(
      const PageStorageKey<String>('workspace-notes-list'),
    );
    final notesScroll = tester.state<ScrollableState>(
      find.descendant(of: notesPage, matching: find.byType(Scrollable)).first,
    );
    notesScroll.position.jumpTo(260);
    await tester.pumpAndSettle();
    final beforeResize = notesScroll.position.pixels;
    final host = find.byKey(const Key('personal-day-middle-viewport'));
    final hostElement = tester.element(host);

    tester.view.physicalSize = const Size(900, 844);
    await tester.pumpAndSettle();
    expect(tester.element(host), same(hostElement));
    expect(find.text('Matching notes · 16'), findsOneWidget);
    expect(
      tester.widget<TextField>(find.byType(TextField).first).controller!.text,
      'Note',
    );
    expect(notesScroll.position.pixels, closeTo(beforeResize, 1));

    tester.view.physicalSize = const Size(390, 844);
    await tester.pumpAndSettle();
    expect(tester.element(host), same(hostElement));
    expect(find.text('Matching notes · 16'), findsOneWidget);
    expect(notesScroll.position.pixels, closeTo(beforeResize, 1));

    await tester.tap(find.byTooltip('Tasks'));
    await tester.pumpAndSettle();
    final tasksPage = find.byKey(
      const PageStorageKey<String>('workspace-tasks-list'),
    );
    final tasksScroll = tester.state<ScrollableState>(
      find.descendant(of: tasksPage, matching: find.byType(Scrollable)).first,
    );
    tasksScroll.position.jumpTo(240);
    await tester.pumpAndSettle();
    final tasksOffset = tasksScroll.position.pixels;
    tester.view.physicalSize = const Size(900, 844);
    await tester.pumpAndSettle();
    expect(tester.element(host), same(hostElement));
    expect(tasksScroll.position.pixels, closeTo(tasksOffset, 1));
    tester.view.physicalSize = const Size(390, 844);
    await tester.pumpAndSettle();
    expect(tester.element(host), same(hostElement));
    expect(tasksScroll.position.pixels, closeTo(tasksOffset, 1));
    await tester.tap(find.byTooltip('Notes'));
    await tester.pumpAndSettle();
    expect(
      tester.widget<TextField>(find.byType(TextField).first).controller!.text,
      'Note',
    );
    final restoredScroll = tester.state<ScrollableState>(
      find
          .descendant(
            of: find.byKey(
              const PageStorageKey<String>('workspace-notes-list'),
            ),
            matching: find.byType(Scrollable),
          )
          .first,
    );
    expect(restoredScroll.position.pixels, closeTo(beforeResize, 1));

    await tester.tap(find.byTooltip('More'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Settings'));
    await tester.pumpAndSettle();
    final remoteSection = find.byKey(const ValueKey('settings-remoteServer'));
    expect(remoteSection, findsOneWidget);
    await tester.tap(remoteSection);
    await tester.pumpAndSettle();
    expect(
      find.text('Connections are available in the native Floe app.'),
      findsOneWidget,
    );
    final semantics = tester.ensureSemantics();
    try {
      void expectRemoteServerSelected() => expect(
        tester
            .getSemantics(remoteSection)
            .getSemanticsData()
            .flagsCollection
            .isSelected,
        ui.Tristate.isTrue,
      );

      expectRemoteServerSelected();
      for (final width in [900.0, 390.0]) {
        tester.view.physicalSize = Size(width, 844);
        await tester.pumpAndSettle();
        expect(tester.element(host), same(hostElement));
        expectRemoteServerSelected();
        expect(
          find.text('Connections are available in the native Floe app.'),
          findsOneWidget,
        );
        expect(
          find.byKey(const ValueKey('settings-content-scroll')),
          findsOneWidget,
        );
      }
    } finally {
      semantics.dispose();
    }
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox.shrink());
  });
}

final class _ReadyRuntimeGateway implements RuntimeGateway {
  @override
  Future<RuntimeReadinessSnapshot> readiness(String requestId) async =>
      const RuntimeReadinessSnapshot(state: RuntimeReadinessState.ready);

  @override
  dynamic noSuchMethod(Invocation invocation) =>
      throw StateError('Unexpected Runtime command in synthetic host test.');
}

final class _SyntheticPolicyGateway implements OperationAuthorizationGateway {
  @override
  Future<ActionAuthority> loadAuthority() async => const ActionAuthority(
    revision: 1,
    calendarCreate: ActionAuthorityMode.ask,
  );

  @override
  Future<ActionAuthority> setAuthority({
    required String commandId,
    required ActionAuthorityMode mode,
    required int expectedRevision,
  }) async => throw StateError(
    'Unexpected authority mutation in the responsive host test.',
  );
}
