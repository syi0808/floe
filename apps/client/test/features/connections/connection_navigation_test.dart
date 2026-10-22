import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_app.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/app/runtime/local_owner_gateways_scope.dart';
import 'package:floe_client/app/runtime/runtime_controller.dart';
import 'package:floe_client/app/runtime/runtime_gateway.dart';
import 'package:floe_client/features/connections/application/calendar_system_access_gateway.dart';
import 'package:floe_client/features/connections/application/connections_gateway.dart';
import 'package:floe_client/features/connections/domain/connection_models.dart';
import 'package:floe_client/features/connections/presentation/connections_controller.dart';
import 'package:floe_client/features/connections/presentation/connector_screen.dart';
import 'package:floe_client/features/day/application/fake_day_gateway.dart';
import 'package:floe_client/features/day/domain/day_models.dart';
import 'package:floe_client/l10n/app_localizations.dart';

const _sourcePrefix = 'Synthetic source';
const _integrationPrefix = 'Synthetic integration';
const _targetIndex = 24;
const _captureKey = ValueKey('connection-navigation-capture');

void main() {
  setUpAll(() async {
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
  });

  testWidgets(
    'selected Connections detail survives compact host breakpoint changes',
    (tester) async {
      final fixture = await _fixture();
      addTearDown(() {
        fixture.controller.dispose();
        fixture.runtime.dispose();
      });
      _setViewport(tester, width: 390, height: 844);
      const personId = 'synthetic-connection-navigation-person';
      final now = DateTime(2026, 9, 4, 12);
      await tester.pumpWidget(
        FloeApp(
          personId: personId,
          gateway: FakeDayGateway(),
          query: DayQuery.local(personId: personId, date: now, now: now),
          connectionsController: fixture.controller,
          ownerGateways: LocalOwnerGateways(runtime: fixture.runtime),
        ),
      );
      await _pump(tester);
      await tester.tap(find.byTooltip('More'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Connected services'));
      await tester.pumpAndSettle();

      const sourceName = '$_sourcePrefix 12';
      final source = find.text(sourceName);
      await tester.ensureVisible(source);
      await tester.pumpAndSettle();
      await tester.tap(source);
      await tester.pumpAndSettle();
      expect(find.text('Back to connections'), findsOneWidget);
      expect(find.text('Launch planning calendar 18'), findsOneWidget);

      final host = find.byKey(const Key('personal-day-middle-viewport'));
      final hostElement = tester.element(host);
      for (final width in [900.0, 390.0]) {
        tester.view.physicalSize = Size(width, 844);
        await tester.pumpAndSettle();
        expect(tester.element(host), same(hostElement));
        expect(find.text('Back to connections'), findsOneWidget);
        expect(find.text(sourceName), findsOneWidget);
        expect(find.text('Launch planning calendar 18'), findsOneWidget);
      }

      await tester.tap(find.text('Back to connections'));
      await tester.pumpAndSettle();
      expect(find.text(sourceName), findsOneWidget);
      expect(
        _visible(tester.getRect(source), 390, 844),
        isTrue,
        reason: 'the detail return should retain its source list position',
      );
      expect(fixture.gateway.unexpectedCalls, isEmpty);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox.shrink());
    },
  );

  testWidgets(
    'source detail opens at its heading and Back restores list position and focus',
    (tester) async {
      final fixture = await _fixture();
      final scroll = ScrollController();
      addTearDown(() {
        fixture.controller.dispose();
        fixture.runtime.dispose();
        scroll.dispose();
      });
      _setViewport(tester, width: 360, height: 720);

      await tester.pumpWidget(_app(fixture.controller, scroll));
      await _pump(tester);
      const sourceName = '$_sourcePrefix $_targetIndex';
      final sourceFinder = find.text(sourceName);
      await tester.ensureVisible(sourceFinder);
      await tester.pumpAndSettle();
      final sourceRectBefore = tester.getRect(sourceFinder);
      expect(_visible(sourceRectBefore, 360, 720), isTrue);

      const warning = 'Synthetic status warning stays above the detail.';
      fixture.controller.failure = warning;
      fixture.controller.notifyListeners();
      await tester.pump();
      final listOffset = scroll.offset;
      final sourceRectAtActivation = tester.getRect(sourceFinder);
      expect(_visible(sourceRectAtActivation, 360, 720), isTrue);

      final sourceFocus = _cardFocus(tester, sourceFinder);
      await _activate(tester, sourceFocus);
      await tester.pumpAndSettle();

      expect(find.text(sourceName), findsOneWidget);
      expect(find.text('Launch planning calendar 1'), findsOneWidget);
      expect(find.text('Launch planning calendar 18'), findsOneWidget);
      expect(find.text('Back to connections'), findsOneWidget);
      expect(find.text(warning), findsOneWidget);
      final backRect = tester.getRect(find.text('Back to connections'));
      final detailRect = tester.getRect(find.text(sourceName));
      final warningRect = tester.getRect(find.text(warning));
      expect(_visible(backRect, 360, 720), isTrue);
      expect(_visible(detailRect, 360, 720), isTrue);
      expect(_visible(warningRect, 360, 720), isTrue);
      expect(scroll.offset, 0);
      final detailMaxExtent = scroll.position.maxScrollExtent;
      expect(detailMaxExtent, greaterThan(0));
      expect(fixture.gateway.unexpectedCalls, isEmpty);
      await expectLater(
        find.byKey(_captureKey),
        matchesGoldenFile('goldens/connection-source-detail-360.png'),
      );

      await _activateBack(tester, width: 360, height: 720);
      await tester.pumpAndSettle();
      expect(scroll.offset, closeTo(listOffset, 1));
      final returnedSource = find.text(sourceName);
      final returnedRect = tester.getRect(returnedSource);
      expect(_visible(returnedRect, 360, 720), isTrue);
      final returnedFocus = _cardFocus(tester, returnedSource);
      expect(returnedFocus, same(sourceFocus));
      expect(returnedFocus.hasPrimaryFocus, isTrue);
      await expectLater(
        find.byKey(_captureKey),
        matchesGoldenFile('goldens/connection-source-list-restored-360.png'),
      );

      for (var cycle = 0; cycle < 2; cycle++) {
        await tester.ensureVisible(returnedSource);
        await tester.pumpAndSettle();
        final repeatedOffset = scroll.offset;
        await _activate(tester, sourceFocus);
        await tester.pumpAndSettle();
        expect(
          _visible(tester.getRect(find.text('Back to connections')), 360, 720),
          isTrue,
        );
        await _activateBack(tester, width: 360, height: 720);
        await tester.pumpAndSettle();
        expect(scroll.offset, closeTo(repeatedOffset, 1));
        expect(sourceFocus.hasPrimaryFocus, isTrue);
      }

      expect(fixture.gateway.unexpectedCalls, isEmpty);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'integration detail restores a wide large-text list with reduced motion',
    (tester) async {
      final fixture = await _fixture(kind: _CardKind.integration);
      final scroll = ScrollController();
      addTearDown(() {
        fixture.controller.dispose();
        fixture.runtime.dispose();
        scroll.dispose();
      });
      _setViewport(tester, width: 900, height: 760);

      await tester.pumpWidget(
        _app(
          fixture.controller,
          scroll,
          width: 900,
          height: 760,
          textScale: 2,
          disableAnimations: true,
        ),
      );
      await _pump(tester);
      const integrationName = '$_integrationPrefix $_targetIndex';
      final integrationFinder = find.text(integrationName);
      await tester.ensureVisible(integrationFinder);
      await tester.pumpAndSettle();
      final listOffset = scroll.offset;
      final integrationFocus = _cardFocus(tester, integrationFinder);
      await _activate(tester, integrationFocus, key: LogicalKeyboardKey.space);
      await tester.pump();

      expect(find.text(integrationName), findsOneWidget);
      expect(find.text('Back to connections'), findsOneWidget);
      expect(
        _visible(tester.getRect(find.text(integrationName)), 900, 760),
        isTrue,
      );
      expect(
        _visible(tester.getRect(find.text('Back to connections')), 900, 760),
        isTrue,
      );
      expect(scroll.offset, 0);
      expect(scroll.position.isScrollingNotifier.value, isFalse);
      expect(
        MediaQuery.disableAnimationsOf(
          tester.element(find.text(integrationName)),
        ),
        isTrue,
      );
      await expectLater(
        find.byKey(_captureKey),
        matchesGoldenFile('goldens/connection-integration-detail-900-2x.png'),
      );
      await _activateBack(tester, width: 900, height: 760);
      await tester.pump();
      expect(scroll.offset, closeTo(listOffset, 1));
      expect(integrationFocus.hasPrimaryFocus, isTrue);
      expect(tester.takeException(), isNull);
      expect(fixture.gateway.unexpectedCalls, isEmpty);
    },
  );

  testWidgets(
    'middle source remains visible after the host resizes while its detail is open',
    (tester) async {
      final fixture = await _fixture();
      final scroll = ScrollController();
      addTearDown(() {
        fixture.controller.dispose();
        fixture.runtime.dispose();
        scroll.dispose();
      });
      _setViewport(tester, width: 360, height: 720);

      await tester.pumpWidget(_app(fixture.controller, scroll));
      await _pump(tester);
      const sourceName = '$_sourcePrefix 12';
      final sourceFinder = find.text(sourceName);
      expect(tester.getRect(sourceFinder).top, greaterThan(720));
      await tester.ensureVisible(sourceFinder);
      await tester.pumpAndSettle();
      final listOffset = scroll.offset;
      expect(_visible(tester.getRect(sourceFinder), 360, 720), isTrue);
      final sourceFocus = _cardFocus(tester, sourceFinder);
      await _activate(tester, sourceFocus);
      await tester.pumpAndSettle();
      expect(find.text('Launch planning calendar 18'), findsOneWidget);
      expect(scroll.position.maxScrollExtent, greaterThan(0));
      expect(
        _visible(tester.getRect(find.text('Back to connections')), 360, 720),
        isTrue,
      );

      tester.view.physicalSize = const Size(900, 760);
      await tester.pumpWidget(
        _app(fixture.controller, scroll, width: 900, height: 760),
      );
      await tester.pumpAndSettle();
      await _activateBack(tester, width: 900, height: 760);
      await tester.pumpAndSettle();

      final returnedRect = tester.getRect(sourceFinder);
      expect(
        _visible(returnedRect, 900, 760),
        isTrue,
        reason: 'offset=$listOffset, returned=$returnedRect',
      );
      expect(scroll.offset, lessThan(listOffset));
      expect(sourceFocus.hasPrimaryFocus, isTrue);
      expect(fixture.gateway.unexpectedCalls, isEmpty);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'moved source stays visible after Back while preserving useful list offset',
    (tester) async {
      final fixture = await _fixture();
      final scroll = ScrollController();
      addTearDown(() {
        fixture.controller.dispose();
        fixture.runtime.dispose();
        scroll.dispose();
      });
      _setViewport(tester, width: 360, height: 720);

      await tester.pumpWidget(_app(fixture.controller, scroll));
      await _pump(tester);
      const sourceName = '$_sourcePrefix 12';
      final sourceFinder = find.text(sourceName);
      await tester.ensureVisible(sourceFinder);
      await tester.pumpAndSettle();
      final listOffset = scroll.offset;
      final sourceFocus = _cardFocus(tester, sourceFinder);
      await _activate(tester, sourceFocus);
      await tester.pumpAndSettle();
      expect(find.text('Launch planning calendar 18'), findsOneWidget);
      expect(scroll.position.maxScrollExtent, greaterThan(0));

      fixture.gateway.value = _overview(
        kind: _CardKind.source,
        sourcePrefix: fixture.sourcePrefix,
        moveMiddleSourceToEnd: true,
      );
      await fixture.controller.load();
      await tester.pumpAndSettle();
      await _activateBack(tester, width: 360, height: 720);
      await tester.pumpAndSettle();

      final returnedRect = tester.getRect(sourceFinder);
      expect(
        _visible(returnedRect, 360, 720),
        isTrue,
        reason: 'offset=$listOffset, returned=$returnedRect',
      );
      expect(returnedRect.bottom, greaterThan(660));
      expect(sourceFocus.hasPrimaryFocus, isTrue);
      expect(fixture.gateway.unexpectedCalls, isEmpty);
      expect(tester.takeException(), isNull);
    },
  );

  for (final (width, textScale) in [(360.0, 2.0), (900.0, 1.0)]) {
    testWidgets(
      'source detail entry fits ${width.toInt()}px at ${textScale}x text',
      (tester) async {
        final fixture = await _fixture();
        final scroll = ScrollController();
        addTearDown(() {
          fixture.controller.dispose();
          fixture.runtime.dispose();
          scroll.dispose();
        });
        _setViewport(tester, width: width, height: 820);

        await tester.pumpWidget(
          _app(
            fixture.controller,
            scroll,
            width: width,
            height: 820,
            textScale: textScale,
          ),
        );
        await _pump(tester);
        const sourceName = '$_sourcePrefix $_targetIndex';
        final sourceFinder = find.text(sourceName);
        await tester.ensureVisible(sourceFinder);
        await tester.pumpAndSettle();
        final sourceFocus = _cardFocus(tester, sourceFinder);
        await _activate(tester, sourceFocus);
        await tester.pumpAndSettle();

        expect(find.text('Back to connections'), findsOneWidget);
        expect(find.text('Launch planning calendar 18'), findsOneWidget);
        expect(
          _visible(
            tester.getRect(find.text('Back to connections')),
            width,
            820,
          ),
          isTrue,
        );
        expect(
          _visible(tester.getRect(find.text(sourceName)), width, 820),
          isTrue,
        );
        expect(tester.takeException(), isNull);
        expect(fixture.gateway.unexpectedCalls, isEmpty);
      },
    );
  }

  testWidgets('controller replacement cancels a pending detail entry', (
    tester,
  ) async {
    final first = await _fixture();
    final replacement = await _fixture(sourcePrefix: 'Replacement source');
    final scroll = ScrollController();
    addTearDown(() {
      first.controller.dispose();
      first.runtime.dispose();
      replacement.controller.dispose();
      replacement.runtime.dispose();
      scroll.dispose();
    });
    _setViewport(tester, width: 360, height: 720);

    await tester.pumpWidget(_app(first.controller, scroll));
    await _pump(tester);
    const originalName = '$_sourcePrefix $_targetIndex';
    final originalFinder = find.text(originalName);
    await tester.ensureVisible(originalFinder);
    await tester.pumpAndSettle();
    final originalOffset = scroll.offset;

    await tester.tap(originalFinder);
    await tester.pumpWidget(_app(replacement.controller, scroll));
    await tester.pumpAndSettle();

    expect(find.text(originalName), findsNothing);
    expect(find.text('Replacement source $_targetIndex'), findsOneWidget);
    expect(scroll.offset, greaterThan(originalOffset - 100));
    expect(replacement.gateway.unexpectedCalls, isEmpty);
    expect(tester.takeException(), isNull);
  });

  testWidgets('pending Back cannot steal focus into a replacement Scrollable', (
    tester,
  ) async {
    final fixture = await _fixture();
    final firstScroll = ScrollController();
    final replacementScroll = ScrollController();
    addTearDown(() {
      fixture.controller.dispose();
      fixture.runtime.dispose();
      firstScroll.dispose();
      replacementScroll.dispose();
    });
    _setViewport(tester, width: 360, height: 720);

    await tester.pumpWidget(_app(fixture.controller, firstScroll));
    await _pump(tester);
    const sourceName = '$_sourcePrefix $_targetIndex';
    final sourceFinder = find.text(sourceName);
    await tester.ensureVisible(sourceFinder);
    await tester.pumpAndSettle();
    final sourceFocus = _cardFocus(tester, sourceFinder);
    await _activate(tester, sourceFocus);
    await tester.pumpAndSettle();
    await _queueBack(tester, width: 360, height: 720);

    await tester.pumpWidget(
      _app(
        fixture.controller,
        replacementScroll,
        hostKey: const ValueKey('replacement-connections-host-scroll'),
      ),
    );
    await tester.pumpAndSettle();
    expect(replacementScroll.offset, 0);
    expect(sourceFocus.hasPrimaryFocus, isFalse);
    expect(find.text(sourceName), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('pending Back cannot target a replacement host controller', (
    tester,
  ) async {
    final fixture = await _fixture();
    final firstScroll = ScrollController();
    final replacementScroll = ScrollController();
    addTearDown(() {
      fixture.controller.dispose();
      fixture.runtime.dispose();
      firstScroll.dispose();
      replacementScroll.dispose();
    });
    _setViewport(tester, width: 360, height: 720);

    await tester.pumpWidget(_app(fixture.controller, firstScroll));
    await _pump(tester);
    const sourceName = '$_sourcePrefix $_targetIndex';
    final sourceFinder = find.text(sourceName);
    await tester.ensureVisible(sourceFinder);
    await tester.pumpAndSettle();
    final sourceFocus = _cardFocus(tester, sourceFinder);
    await _activate(tester, sourceFocus);
    await tester.pumpAndSettle();
    await _queueBack(tester, width: 360, height: 720);

    await tester.pumpWidget(_app(fixture.controller, replacementScroll));
    await tester.pumpAndSettle();
    expect(replacementScroll.offset, 0);
    expect(sourceFocus.hasPrimaryFocus, isFalse);
    expect(find.text(sourceName), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('pending Back is invalidated by controller replacement', (
    tester,
  ) async {
    final first = await _fixture();
    final replacement = await _fixture(sourcePrefix: 'Replacement source');
    final scroll = ScrollController();
    addTearDown(() {
      first.controller.dispose();
      first.runtime.dispose();
      replacement.controller.dispose();
      replacement.runtime.dispose();
      scroll.dispose();
    });
    _setViewport(tester, width: 360, height: 720);

    await tester.pumpWidget(_app(first.controller, scroll));
    await _pump(tester);
    const originalName = '$_sourcePrefix $_targetIndex';
    final originalFinder = find.text(originalName);
    await tester.ensureVisible(originalFinder);
    await tester.pumpAndSettle();
    final originalFocus = _cardFocus(tester, originalFinder);
    await _activate(tester, originalFocus);
    await tester.pumpAndSettle();
    await _queueBack(tester, width: 360, height: 720);

    await tester.pumpWidget(_app(replacement.controller, scroll));
    await tester.pumpAndSettle();
    expect(find.text(originalName), findsNothing);
    expect(find.text('Replacement source $_targetIndex'), findsOneWidget);
    expect(originalFocus.hasPrimaryFocus, isFalse);
    expect(replacement.gateway.unexpectedCalls, isEmpty);
    expect(tester.takeException(), isNull);
  });

  testWidgets('pending Back is invalidated by a new source identity', (
    tester,
  ) async {
    final fixture = await _fixture();
    final scroll = ScrollController();
    addTearDown(() {
      fixture.controller.dispose();
      fixture.runtime.dispose();
      scroll.dispose();
    });
    _setViewport(tester, width: 360, height: 720);

    await tester.pumpWidget(_app(fixture.controller, scroll));
    await _pump(tester);
    const originalName = '$_sourcePrefix $_targetIndex';
    final originalFinder = find.text(originalName);
    await tester.ensureVisible(originalFinder);
    await tester.pumpAndSettle();
    final originalFocus = _cardFocus(tester, originalFinder);
    await _activate(tester, originalFocus);
    await tester.pumpAndSettle();
    await _queueBack(tester, width: 360, height: 720);

    await tester.pumpWidget(
      _app(fixture.controller, scroll, initialSourceRef: SourceRef(_uuid(1))),
    );
    await tester.pumpAndSettle();
    expect(find.text('$_sourcePrefix 1'), findsOneWidget);
    expect(find.text('Launch planning calendar 1'), findsNothing);
    expect(originalFocus.hasPrimaryFocus, isFalse);
    expect(tester.takeException(), isNull);
  });

  testWidgets('unmount invalidates a pending Back restoration', (tester) async {
    final fixture = await _fixture();
    final scroll = ScrollController();
    addTearDown(() {
      fixture.controller.dispose();
      fixture.runtime.dispose();
      scroll.dispose();
    });
    _setViewport(tester, width: 360, height: 720);

    await tester.pumpWidget(_app(fixture.controller, scroll));
    await _pump(tester);
    const sourceName = '$_sourcePrefix $_targetIndex';
    final sourceFinder = find.text(sourceName);
    await tester.ensureVisible(sourceFinder);
    await tester.pumpAndSettle();
    final sourceFocus = _cardFocus(tester, sourceFinder);
    await _activate(tester, sourceFocus);
    await tester.pumpAndSettle();
    await _queueBack(tester, width: 360, height: 720);

    await tester.pumpWidget(const SizedBox.shrink());
    expect(sourceFocus.hasPrimaryFocus, isFalse);
    expect(tester.takeException(), isNull);
  });

  for (final kind in _CardKind.values) {
    testWidgets(
      '${kind.name} removal cancels stale focus and scroll restoration',
      (tester) async {
        final fixture = await _fixture(kind: kind);
        final scroll = ScrollController();
        addTearDown(() {
          fixture.controller.dispose();
          fixture.runtime.dispose();
          scroll.dispose();
        });
        _setViewport(tester, width: 360, height: 720);

        await tester.pumpWidget(_app(fixture.controller, scroll));
        await _pump(tester);
        final targetName = switch (kind) {
          _CardKind.source => '$_sourcePrefix $_targetIndex',
          _CardKind.integration => '$_integrationPrefix $_targetIndex',
        };
        final target = find.text(targetName);
        await tester.ensureVisible(target);
        await tester.pumpAndSettle();
        final staleFocus = _cardFocus(tester, target);

        await tester.tap(target);
        fixture.gateway.value = _overview(
          kind: kind,
          sourcePrefix: fixture.sourcePrefix,
          count: _targetIndex - 1,
        );
        await fixture.controller.load();
        await tester.pumpAndSettle();

        expect(
          find.text(
            'This connection is no longer in the current list. Refresh Connections.',
          ),
          findsOneWidget,
        );
        expect(staleFocus.hasPrimaryFocus, isFalse);
        expect(fixture.gateway.unexpectedCalls, isEmpty);
        await tester.tap(find.text('Back to connections'));
        await tester.pumpAndSettle();
        expect(find.text(targetName), findsNothing);
        expect(tester.takeException(), isNull);
      },
    );
  }
}

FocusNode _cardFocus(WidgetTester tester, Finder cardName) {
  final card = find.ancestor(of: cardName, matching: find.byType(InkWell));
  expect(card, findsOneWidget);
  return tester.widget<InkWell>(card).focusNode!;
}

Future<void> _activate(
  WidgetTester tester,
  FocusNode focusNode, {
  LogicalKeyboardKey key = LogicalKeyboardKey.enter,
}) async {
  focusNode.requestFocus();
  await tester.pump();
  expect(focusNode.hasPrimaryFocus, isTrue);
  await tester.sendKeyEvent(key);
  await tester.pumpAndSettle();
}

Future<void> _activateBack(
  WidgetTester tester, {
  required double width,
  required double height,
}) async {
  await _queueBack(tester, width: width, height: height);
  await tester.pumpAndSettle();
}

Future<void> _queueBack(
  WidgetTester tester, {
  required double width,
  required double height,
}) async {
  final back = find.text('Back to connections');
  final button = find.ancestor(of: back, matching: find.byType(TextButton));
  expect(button, findsOneWidget);
  final focusElements = find.descendant(
    of: button,
    matching: find.byType(Focus),
  );
  expect(focusElements, findsWidgets);
  final focusNode = _focusFor(tester, focusElements.last);
  for (var count = 0; count < 10 && !focusNode.hasPrimaryFocus; count++) {
    await tester.sendKeyEvent(LogicalKeyboardKey.tab);
    await tester.pump();
  }
  expect(focusNode.hasPrimaryFocus, isTrue);
  expect(_visible(tester.getRect(back), width, height), isTrue);
  await tester.sendKeyEvent(LogicalKeyboardKey.enter);
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

bool _visible(Rect rect, double width, double height) =>
    rect.left >= 0 &&
    rect.top >= 0 &&
    rect.right <= width &&
    rect.bottom <= height;

Widget _app(
  ConnectionsController controller,
  ScrollController scroll, {
  double width = 360,
  double height = 720,
  double textScale = 1,
  bool disableAnimations = false,
  SourceRef? initialSourceRef,
  Key hostKey = const ValueKey('connections-host-scroll'),
}) => MaterialApp(
  theme: FloeTheme.light,
  locale: const Locale('en'),
  localizationsDelegates: AppLocalizations.localizationsDelegates,
  supportedLocales: AppLocalizations.supportedLocales,
  builder: (context, child) => MediaQuery(
    data: MediaQuery.of(context).copyWith(
      textScaler: TextScaler.linear(textScale),
      disableAnimations: disableAnimations,
    ),
    child: child!,
  ),
  home: Scaffold(
    body: SizedBox.expand(
      child: RepaintBoundary(
        key: _captureKey,
        child: ColoredBox(
          color: FloeColor.canvas,
          child: SingleChildScrollView(
            key: hostKey,
            controller: scroll,
            padding: width < 620
                ? const EdgeInsets.fromLTRB(16, 64, 16, 96)
                : const EdgeInsets.fromLTRB(120, 24, 36, 24),
            child: ConnectorScreen(
              controller: controller,
              initialSourceRef: initialSourceRef,
            ),
          ),
        ),
      ),
    ),
  ),
);

void _setViewport(
  WidgetTester tester, {
  required double width,
  required double height,
}) {
  tester.view.physicalSize = Size(width, height);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
}

Future<void> _pump(WidgetTester tester) async {
  await tester.pump();
  await tester.pump(const Duration(milliseconds: 250));
}

enum _CardKind { source, integration }

Future<_Fixture> _fixture({
  _CardKind kind = _CardKind.source,
  String sourcePrefix = _sourcePrefix,
}) async {
  final runtime = RuntimeController(
    gateway: _ReadyRuntimeGateway(),
    personId: 'synthetic-connection-navigation-person',
  );
  await runtime.open();
  final gateway = _ProbeConnectionsGateway(
    _overview(kind: kind, sourcePrefix: sourcePrefix),
  );
  final controller = ConnectionsController(
    gateway,
    runtime: runtime,
    calendarSystemAccess: _UnusedCalendarSystemAccess(),
  )..overview = gateway.value;
  return _Fixture(controller, runtime, gateway, sourcePrefix);
}

ConnectionsOverview _overview({
  required _CardKind kind,
  String sourcePrefix = _sourcePrefix,
  int count = _targetIndex,
  bool moveMiddleSourceToEnd = false,
}) {
  final sources = <SourceSummary>[];
  final integrations = <IntegrationSummary>[];
  for (var index = 1; index <= count; index++) {
    final group = ResourceGroup(
      groupRef: ResourceGroupRef(_uuid(1000 + index)),
      label: 'Synthetic account $index',
    );
    if (kind == _CardKind.source) {
      sources.add(
        SourceSummary(
          sourceRef: SourceRef(_uuid(index)),
          revision: 1,
          displayLabels: ['$sourcePrefix $index'],
          availability: 'available',
          selectedResources: [
            for (
              var item = 1;
              item <= (index == _targetIndex || index == 12 ? 18 : 1);
              item++
            )
              ResourceSummary(
                resourceRef: ResourceRef(_uuid(10000 + index * 32 + item)),
                label: index == _targetIndex || index == 12
                    ? 'Launch planning calendar $item'
                    : 'Synthetic resource $item',
                group: group,
              ),
          ],
          observeState: 'disabled',
          allowedActions: const {},
        ),
      );
    } else {
      integrations.add(
        IntegrationSummary(
          serviceKind: 'hosted',
          integrationRef: IntegrationRef(_uuid(2000 + index)),
          revision: 1,
          displayName: '$_integrationPrefix $index',
          category: 'synthetic',
          state: 'connected',
          capabilities: const {},
        ),
      );
    }
  }
  if (moveMiddleSourceToEnd && sources.length > 12) {
    sources.add(sources.removeAt(11));
  }
  return ConnectionsOverview(
    gateways: const [],
    integrations: integrations,
    sources: sources,
  );
}

String _uuid(int number) =>
    '00000000-0000-4000-8000-${number.toString().padLeft(12, '0')}';

final class _Fixture {
  const _Fixture(
    this.controller,
    this.runtime,
    this.gateway,
    this.sourcePrefix,
  );
  final ConnectionsController controller;
  final RuntimeController runtime;
  final _ProbeConnectionsGateway gateway;
  final String sourcePrefix;
}

final class _ReadyRuntimeGateway implements RuntimeGateway {
  @override
  Future<RuntimeReadinessSnapshot> readiness(String requestId) async =>
      const RuntimeReadinessSnapshot(state: RuntimeReadinessState.ready);

  @override
  dynamic noSuchMethod(Invocation invocation) =>
      throw StateError('Unexpected Runtime command in synthetic test.');
}

final class _ProbeConnectionsGateway implements ConnectionsGateway {
  _ProbeConnectionsGateway(this.value);
  ConnectionsOverview value;
  final unexpectedCalls = <String>[];

  @override
  Future<ConnectionsOverview> overview() async => value;

  @override
  dynamic noSuchMethod(Invocation invocation) {
    unexpectedCalls.add(invocation.memberName.toString());
    throw StateError('Unexpected connection command in synthetic test.');
  }
}

final class _UnusedCalendarSystemAccess implements CalendarSystemAccessGateway {
  @override
  Future<CalendarSystemAccess> inspect() async =>
      CalendarSystemAccess.notRequested;

  @override
  Future<void> openSettings() async {
    throw StateError('Synthetic test must not open System Settings.');
  }
}
