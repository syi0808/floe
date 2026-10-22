import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/floe_app.dart';
import 'package:floe_client/app/runtime/local_owner_gateways_scope.dart';
import 'package:floe_client/features/day/application/day_gateway.dart';
import 'package:floe_client/features/day/domain/day_models.dart';
import 'package:floe_client/preview/native_interaction_fixture.dart';

const _shortDiagnostic = 'Synthetic Day read timed out. Try again.';
const _longDiagnostic =
    'Synthetic Day read failed: the local store did not return a snapshot '
    'before the request deadline. Diagnostic details: read-only snapshot '
    'acquisition timed out after 3.0 seconds while querying the selected '
    'date; owner status was unavailable and no current Day snapshot was '
    'received. This message is synthetic diagnostic preview text. No event '
    'or calendar data was read and no command was submitted. Retrying checks '
    'the Day read again without repeating any prior command. Reference '
    'SYNTHETIC-DAY-READ-20261022-001.';
const _fallbackDiagnostic = 'An unknown error occurred.';
const _scrollTail = 'SYNTHETIC-DAY-READ-20261022-001.';

const _layoutCases =
    <
      ({
        String name,
        Size size,
        double textScale,
        double safeTop,
        double safeBottom,
        String diagnostic,
      })
    >[
      (
        name: 'desktop-normal-short',
        size: Size(1280, 720),
        textScale: 1,
        safeTop: 0,
        safeBottom: 0,
        diagnostic: _shortDiagnostic,
      ),
      (
        name: 'desktop-200-long',
        size: Size(1280, 720),
        textScale: 2,
        safeTop: 0,
        safeBottom: 0,
        diagnostic: _longDiagnostic,
      ),
      (
        name: 'narrow-390-normal-short',
        size: Size(390, 844),
        textScale: 1,
        safeTop: 28,
        safeBottom: 24,
        diagnostic: _shortDiagnostic,
      ),
      (
        name: 'narrow-390-200-long',
        size: Size(390, 844),
        textScale: 2,
        safeTop: 28,
        safeBottom: 24,
        diagnostic: _longDiagnostic,
      ),
      (
        name: 'short-320-normal-long',
        size: Size(320, 560),
        textScale: 1,
        safeTop: 24,
        safeBottom: 20,
        diagnostic: _longDiagnostic,
      ),
      (
        name: 'short-320-200-long',
        size: Size(320, 560),
        textScale: 2,
        safeTop: 24,
        safeBottom: 20,
        diagnostic: _longDiagnostic,
      ),
    ];

void main() {
  setUpAll(_loadBundledFonts);

  const cutoffCases = <({double textScale, double screenHeight})>[
    (textScale: 2, screenHeight: 455),
    (textScale: 2, screenHeight: 456),
    (textScale: 3, screenHeight: 521),
    (textScale: 3, screenHeight: 522),
    (textScale: 3, screenHeight: 560),
  ];

  for (final cutoffCase in cutoffCases) {
    testWidgets('readable diagnostic and keyboard retry near cutoff at '
        '${cutoffCase.textScale}x, ${cutoffCase.screenHeight}px', (
      tester,
    ) async {
      final harness = await _mount(
        tester,
        size: Size(320, cutoffCase.screenHeight),
        textScale: cutoffCase.textScale,
        safeTop: 24,
        safeBottom: 20,
        diagnostic: _longDiagnostic,
      );
      addTearDown(harness.dispose);

      final middle = find.byKey(const Key('personal-day-middle-viewport'));
      final diagnosticScroll = find.byKey(
        const Key('day-failure-diagnostic-scroll'),
      );
      final cardScroll = find.byKey(const Key('day-failure-card-scroll'));
      expect(tester.getSize(middle).height, greaterThanOrEqualTo(280));
      expect(tester.takeException(), isNull);
      expect(
        tester.getSize(diagnosticScroll).height,
        greaterThanOrEqualTo(
          _twoLineDiagnosticHeight(tester, diagnosticScroll),
        ),
      );
      final diagnosticPosition = tester
          .state<ScrollableState>(_scrollableUnder(tester, diagnosticScroll))
          .position;
      expect(diagnosticPosition.maxScrollExtent, greaterThan(0));
      final cardPosition = tester
          .state<ScrollableState>(_scrollableUnder(tester, cardScroll))
          .position;
      expect(cardPosition.maxScrollExtent, greaterThan(0));
      final middleBounds = tester.getRect(middle);
      final diagnosticBounds = tester.getRect(diagnosticScroll);
      await tester.drag(
        cardScroll,
        Offset(0, middleBounds.center.dy - diagnosticBounds.center.dy),
      );
      await _pumpFrames(tester);
      expect(
        _containsRect(tester.getRect(middle), tester.getRect(diagnosticScroll)),
        isTrue,
      );
      expect(tester.takeException(), isNull);
      for (
        var attempt = 0;
        attempt < 4 &&
            diagnosticPosition.pixels < diagnosticPosition.maxScrollExtent - 1;
        attempt++
      ) {
        await tester.drag(diagnosticScroll, const Offset(0, -3000));
        await _pumpFrames(tester);
      }
      expect(
        diagnosticPosition.pixels,
        closeTo(diagnosticPosition.maxScrollExtent, 1),
      );
      final diagnosticSemantics = tester
          .getSemantics(find.byType(SelectableText))
          .getSemanticsData();
      expect(diagnosticSemantics.value, contains(_longDiagnostic));
      expect(diagnosticSemantics.value, contains(_scrollTail));

      final retryText = find.text('Try again');
      final retry = find.ancestor(
        of: retryText,
        matching: find.byType(FilledButton),
      );
      var focused = false;
      for (var tab = 0; tab < 40 && !focused; tab++) {
        await tester.sendKeyEvent(LogicalKeyboardKey.tab);
        await tester.pump();
        focused = Focus.of(
          tester.element(retryText),
          createDependency: false,
        ).hasPrimaryFocus;
      }
      expect(focused, isTrue);
      expect(
        _containsRect(tester.getRect(middle), tester.getRect(retry)),
        isTrue,
      );
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await _pumpFrames(tester);
      expect(harness.gateway.readCalls, 2);
      expect(harness.gateway.otherCalls, isEmpty);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox.shrink());
    });
  }

  for (final layout in _layoutCases) {
    testWidgets('initial Day failure fits ${layout.name}', (tester) async {
      final harness = await _mount(
        tester,
        size: layout.size,
        textScale: layout.textScale,
        safeTop: layout.safeTop,
        safeBottom: layout.safeBottom,
        diagnostic: layout.diagnostic,
      );
      addTearDown(harness.dispose);

      final middle = find.byKey(const Key('personal-day-middle-viewport'));
      final retryText = find.text('Try again');
      final retry = find.ancestor(
        of: retryText,
        matching: find.byType(FilledButton),
      );
      final middleBounds = tester.getRect(middle);
      final retryBounds = tester.getRect(retry);
      expect(find.text('Could not load your day'), findsOneWidget);
      expect(_containsRect(middleBounds, retryBounds), isTrue);
      expect(harness.gateway.readCalls, 1);
      expect(harness.gateway.otherCalls, isEmpty);
      expect(tester.takeException(), isNull);

      if (layout.name == 'short-320-200-long') {
        await expectLater(
          find.byKey(const ValueKey('day-failure-preview')),
          matchesGoldenFile('goldens/day_failure_320_200.png'),
        );
      }

      if (layout.diagnostic == _longDiagnostic) {
        final diagnosticScroll = find.byKey(
          const Key('day-failure-diagnostic-scroll'),
        );
        expect(diagnosticScroll, findsOneWidget);
        final scrollable = _scrollableUnder(tester, diagnosticScroll);
        final position = tester.state<ScrollableState>(scrollable).position;
        expect(position.maxScrollExtent, greaterThanOrEqualTo(0));
        if (position.maxScrollExtent > 0) {
          await tester.drag(diagnosticScroll, const Offset(0, -3000));
          await _pumpFrames(tester);
          expect(position.pixels, closeTo(position.maxScrollExtent, 1));

          final viewportBounds = tester.getRect(diagnosticScroll);
          final diagnosticBounds = tester.getRect(find.byType(SelectableText));
          expect(
            diagnosticBounds.bottom,
            lessThanOrEqualTo(viewportBounds.bottom + 1),
          );
          if (layout.name == 'short-320-200-long') {
            await expectLater(
              find.byKey(const ValueKey('day-failure-preview')),
              matchesGoldenFile('goldens/day_failure_320_200_scroll_end.png'),
            );
          }
        } else {
          final diagnosticBounds = tester.getRect(find.byType(SelectableText));
          expect(
            _containsRect(tester.getRect(diagnosticScroll), diagnosticBounds),
            isTrue,
          );
        }
        final semantics = tester
            .getSemantics(find.byType(SelectableText))
            .getSemanticsData();
        expect(semantics.value, contains(_longDiagnostic));
        expect(semantics.value, contains(_scrollTail));
        expect(
          _containsRect(tester.getRect(middle), tester.getRect(retry)),
          isTrue,
        );
      } else {
        expect(
          find.byKey(const Key('day-failure-diagnostic-scroll')),
          findsOneWidget,
        );
        final scrollable = _scrollableUnder(
          tester,
          find.byKey(const Key('day-failure-diagnostic-scroll')),
        );
        expect(
          tester.state<ScrollableState>(scrollable).position.maxScrollExtent,
          0,
        );
      }

      if (layout.name == 'desktop-normal-short') {
        await expectLater(
          find.byKey(const ValueKey('day-failure-preview')),
          matchesGoldenFile('goldens/day_failure_desktop.png'),
        );
      }
      await tester.tap(retry);
      await _pumpFrames(tester);
      expect(harness.gateway.readCalls, 2);
      expect(harness.gateway.otherCalls, isEmpty);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox.shrink());
    });
  }

  testWidgets('Retry remains keyboard reachable at 320px and 200%', (
    tester,
  ) async {
    final harness = await _mount(
      tester,
      size: const Size(320, 560),
      textScale: 2,
      safeTop: 24,
      safeBottom: 20,
      diagnostic: _longDiagnostic,
    );
    addTearDown(harness.dispose);

    final retryText = find.text('Try again');
    final retry = find.ancestor(
      of: retryText,
      matching: find.byType(FilledButton),
    );
    var focused = false;
    for (var tab = 0; tab < 40 && !focused; tab++) {
      await tester.sendKeyEvent(LogicalKeyboardKey.tab);
      await tester.pump();
      focused = Focus.of(
        tester.element(retryText),
        createDependency: false,
      ).hasPrimaryFocus;
    }
    expect(focused, isTrue);
    expect(
      _containsRect(
        tester.getRect(find.byKey(const Key('personal-day-middle-viewport'))),
        tester.getRect(retry),
      ),
      isTrue,
    );
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await _pumpFrames(tester);
    expect(harness.gateway.readCalls, 2);
    expect(harness.gateway.otherCalls, isEmpty);
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox.shrink());
  });

  testWidgets('exceptionally short middle viewport scrolls the whole card', (
    tester,
  ) async {
    final harness = await _mount(
      tester,
      size: const Size(320, 400),
      textScale: 2,
      safeTop: 24,
      safeBottom: 20,
      diagnostic: _longDiagnostic,
    );
    addTearDown(harness.dispose);

    final cardScroll = find.byKey(const Key('day-failure-card-scroll'));
    expect(cardScroll, findsOneWidget);
    final scrollable = _scrollableUnder(tester, cardScroll);
    final position = tester.state<ScrollableState>(scrollable).position;
    expect(position.maxScrollExtent, greaterThan(0));

    final retry = find.ancestor(
      of: find.text('Try again'),
      matching: find.byType(FilledButton),
    );
    expect(
      _containsRect(
        tester.getRect(find.byKey(const Key('personal-day-middle-viewport'))),
        tester.getRect(retry),
      ),
      isFalse,
    );
    await tester.drag(cardScroll, const Offset(0, -3000));
    await _pumpFrames(tester);
    expect(position.pixels, closeTo(position.maxScrollExtent, 1));
    expect(
      _containsRect(
        tester.getRect(find.byKey(const Key('personal-day-middle-viewport'))),
        tester.getRect(retry),
      ),
      isTrue,
    );
    await tester.tap(retry);
    await _pumpFrames(tester);
    expect(harness.gateway.readCalls, 2);
    expect(harness.gateway.otherCalls, isEmpty);
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox.shrink());
  });

  testWidgets('keyboard focus reveals retry in a short middle viewport', (
    tester,
  ) async {
    final harness = await _mount(
      tester,
      size: const Size(320, 400),
      textScale: 2,
      safeTop: 24,
      safeBottom: 20,
      diagnostic: _longDiagnostic,
    );
    addTearDown(harness.dispose);

    final retryText = find.text('Try again');
    final retry = find.ancestor(
      of: retryText,
      matching: find.byType(FilledButton),
    );
    var focused = false;
    for (var tab = 0; tab < 40 && !focused; tab++) {
      await tester.sendKeyEvent(LogicalKeyboardKey.tab);
      await tester.pump();
      focused = Focus.of(
        tester.element(retryText),
        createDependency: false,
      ).hasPrimaryFocus;
    }
    expect(focused, isTrue);
    expect(
      _containsRect(
        tester.getRect(find.byKey(const Key('personal-day-middle-viewport'))),
        tester.getRect(retry),
      ),
      isTrue,
    );
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await _pumpFrames(tester);
    expect(harness.gateway.readCalls, 2);
    expect(harness.gateway.otherCalls, isEmpty);
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox.shrink());
  });

  testWidgets('unknown diagnostic uses the localized fallback', (tester) async {
    final harness = await _mount(
      tester,
      size: const Size(390, 844),
      textScale: 1,
      safeTop: 28,
      safeBottom: 24,
      diagnostic: '',
      blankError: true,
    );
    addTearDown(harness.dispose);

    expect(find.text(_fallbackDiagnostic), findsOneWidget);
    expect(find.text('Could not load your day'), findsOneWidget);
    final retry = find.ancestor(
      of: find.text('Try again'),
      matching: find.byType(FilledButton),
    );
    expect(
      _containsRect(
        tester.getRect(find.byKey(const Key('personal-day-middle-viewport'))),
        tester.getRect(retry),
      ),
      isTrue,
    );
    await tester.tap(retry);
    await _pumpFrames(tester);
    expect(harness.gateway.readCalls, 2);
    expect(harness.gateway.otherCalls, isEmpty);
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox.shrink());
  });
}

Future<void> _loadBundledFonts() async {
  final manifest = jsonDecode(
    await rootBundle.loadString('FontManifest.json'),
  ) as List<dynamic>;
  for (final rawEntry in manifest) {
    final entry = rawEntry as Map<String, dynamic>;
    final loader = FontLoader(entry['family'] as String);
    for (final rawFont in entry['fonts'] as List<dynamic>) {
      loader.addFont(
        rootBundle.load((rawFont as Map<String, dynamic>)['asset'] as String),
      );
    }
    await loader.load();
  }
}

Future<_Harness> _mount(
  WidgetTester tester, {
  required Size size,
  required double textScale,
  required String diagnostic,
  double safeTop = 0,
  double safeBottom = 0,
  bool blankError = false,
}) async {
  tester.view.physicalSize = size;
  tester.view.devicePixelRatio = 1;
  tester.view.viewPadding = FakeViewPadding(top: safeTop, bottom: safeBottom);
  tester.view.viewInsets = FakeViewPadding.zero;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
  addTearDown(tester.view.resetViewPadding);
  addTearDown(tester.view.resetViewInsets);

  final fixture = await NativeInteractionPreviewFixture.create();
  final gateway = _ReadFailureGateway(diagnostic, blankError: blankError);
  await tester.pumpWidget(
    RepaintBoundary(
      key: const ValueKey('day-failure-preview'),
      child: FloeApp(
        gateway: gateway,
        personId: nativePreviewPersonId,
        query: nativePreviewQuery,
        ownerGateways: LocalOwnerGateways(
          runtime: fixture.runtime,
          assistantFeatures: fixture.assistantFeatures,
          memory: fixture.memory,
          operationAuthorization: fixture.operationAuthorizationGateway,
        ),
        builder: (context, child) => MediaQuery(
          data: MediaQuery.of(context)
              .copyWith(textScaler: TextScaler.linear(textScale)),
          child: child!,
        ),
      ),
    ),
  );
  await _pumpFrames(tester);
  return _Harness(fixture, gateway);
}

Future<void> _pumpFrames(WidgetTester tester) async {
  for (var frame = 0; frame < 20; frame++) {
    await tester.pump(const Duration(milliseconds: 50));
  }
}

Finder _scrollableUnder(WidgetTester tester, Finder scrollView) =>
    find.descendant(of: scrollView, matching: find.byType(Scrollable)).first;

double _twoLineDiagnosticHeight(WidgetTester tester, Finder diagnosticScroll) {
  final context = tester.element(diagnosticScroll);
  final painter = TextPainter(
    text: TextSpan(text: 'M\nM', style: DefaultTextStyle.of(context).style),
    textDirection: Directionality.of(context),
    textScaler: MediaQuery.textScalerOf(context),
  )..layout(maxWidth: tester.getSize(diagnosticScroll).width);
  return painter.height;
}

bool _containsRect(Rect outer, Rect inner) =>
    inner.left >= outer.left &&
    inner.top >= outer.top &&
    inner.right <= outer.right &&
    inner.bottom <= outer.bottom;

final class _Harness {
  const _Harness(this.fixture, this.gateway);

  final NativeInteractionPreviewFixture fixture;
  final _ReadFailureGateway gateway;

  Future<void> dispose() => fixture.dispose();
}

final class _ReadFailureGateway implements DayGateway {
  _ReadFailureGateway(this.diagnostic, {this.blankError = false});

  final String diagnostic;
  final bool blankError;
  int readCalls = 0;
  final otherCalls = <String>[];

  @override
  Future<DaySnapshot> loadDay(DayQuery query) async {
    readCalls++;
    if (query.personId != nativePreviewPersonId) {
      throw StateError('Preview-only: unexpected Day person identity.');
    }
    if (blankError) throw const _BlankDiagnostic();
    throw StateError(diagnostic);
  }

  @override
  dynamic noSuchMethod(Invocation invocation) {
    otherCalls.add(invocation.memberName.toString());
    throw StateError('Preview-only: non-read Day operations are blocked.');
  }
}

final class _BlankDiagnostic implements Exception {
  const _BlankDiagnostic();

  @override
  String toString() => '';
}
