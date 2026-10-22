import 'dart:convert';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/floe_app.dart';
import 'package:floe_client/app/runtime/local_owner_gateways_scope.dart';
import 'package:floe_client/app/runtime/runtime_controller.dart';
import 'package:floe_client/app/runtime/runtime_gateway.dart';
import 'package:floe_client/features/actions/application/calendar_action_gateway.dart';
import 'package:floe_client/features/actions/domain/calendar_action.dart';
import 'package:floe_client/features/day/application/fake_day_gateway.dart';
import 'package:floe_client/features/day/domain/day_models.dart';
import 'package:floe_client/preview/product_fixture.dart';

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
  double textScale = 1,
  Key captureKey = const Key('responsive-host-capture'),
}) => previewAppearance(
  FloeApp(
    personId: _personId,
    gateway: gateway,
    query: _query,
    ownerGateways: owners,
    builder: (context, child) => MediaQuery(
      data: MediaQuery.of(context)
          .copyWith(textScaler: TextScaler.linear(textScale)),
      child: RepaintBoundary(key: captureKey, child: child!),
    ),
  ),
);

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

    await tester.tap(find.text('Timeline'));
    await tester.pumpAndSettle();
    final status = find.byKey(const Key('day-status-details'));
    expect(status, findsOneWidget);
    // At 200%, the warning wraps into the bounded header scroll view. Moving
    // keyboard focus to it must reveal the whole action in the middle host.
    var statusFocused = false;
    for (var tab = 0; tab < 40 && !statusFocused; tab++) {
      await tester.sendKeyEvent(LogicalKeyboardKey.tab);
      await tester.pumpAndSettle();
      statusFocused = _hasKeyAncestor(
        FocusManager.instance.primaryFocus?.context,
        const Key('day-status-details'),
      );
    }
    expect(
      statusFocused,
      isTrue,
      reason: 'the status action should be tabbable',
    );
    final statusRect = tester.getRect(status);
    expect(status.hitTestable(), findsOneWidget);
    expect(statusRect.top, greaterThanOrEqualTo(tester.getRect(host).top));
    expect(statusRect.bottom, lessThanOrEqualTo(tester.getRect(host).bottom));
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
