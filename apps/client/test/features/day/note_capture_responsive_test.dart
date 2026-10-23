import 'dart:async';
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/floe_app.dart';
import 'package:floe_client/app/floe_button.dart';
import 'package:floe_client/app/floe_loading.dart';
import 'package:floe_client/app/runtime/local_owner_gateways_scope.dart';
import 'package:floe_client/features/day/application/day_gateway.dart';
import 'package:floe_client/features/day/application/fake_day_gateway.dart';
import 'package:floe_client/features/day/domain/day_models.dart';
import 'package:floe_client/preview/product_fixture.dart';

const _personId = 'notes-capture-synthetic';
final _now = DateTime(2026, 10, 23, 10);
final _query = DayQuery.local(personId: _personId, date: _now, now: _now);
const _shortDraft =
    'Review the partner handoff. Confirm the two open decisions before sharing the launch brief.';
const _longDraft =
    'Launch review notes\n\nKeep the first pass focused on the partner handoff. Start with the two decisions that would change the schedule, then list the people who need context before the review.\n\nAfter the meeting, write down who owns the update and when it should be shared. If a decision is still open, capture the next check-in instead of filling the page with guesses.';

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

void _setViewport(
  WidgetTester tester, {
  required double width,
  required double height,
  double textScale = 1,
}) {
  tester.view.physicalSize = Size(width, height);
  tester.view.devicePixelRatio = 1;
  tester.platformDispatcher.textScaleFactorTestValue = textScale;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
  addTearDown(tester.platformDispatcher.clearTextScaleFactorTestValue);
}

List<NoteItem> _notes() => [
  NoteItem(
    id: 'launch-plan',
    title: 'Launch plan\nReview timing with the partner team.',
    revision: 1,
    createdAt: _now.subtract(const Duration(hours: 1)),
  ),
  NoteItem(
    id: 'reading-list',
    title: 'Reading list\nTwo articles to revisit after planning.',
    revision: 1,
    createdAt: _now.subtract(const Duration(hours: 2)),
  ),
];

List<NoteItem> _manyNotes() => [
  for (var index = 1; index <= 14; index++)
    NoteItem(
      id: 'synthetic-$index',
      title:
          'Synthetic note $index\nA short line for Notes scroll restoration.',
      revision: 1,
      createdAt: _now.subtract(Duration(minutes: index)),
    ),
];

Widget _app(
  _CaptureGateway gateway, {
  double textScale = 1,
  bool reduceMotion = false,
  Key? captureKey,
}) => previewAppearance(
  RepaintBoundary(
    key: captureKey,
    child: FloeApp(
      personId: _personId,
      gateway: gateway,
      query: _query,
      clock: () => _now,
      ownerGateways: const LocalOwnerGateways(),
      builder: (context, child) => MediaQuery(
        data: MediaQuery.of(context).copyWith(
          textScaler: TextScaler.linear(textScale),
          disableAnimations: reduceMotion,
        ),
        child: child!,
      ),
    ),
  ),
);

Future<void> _openComposer(WidgetTester tester) async {
  await tester.tap(find.byKey(const Key('notes-new-note')));
  await tester.pumpAndSettle();
}

final class _CaptureGateway implements DayGateway {
  _CaptureGateway({List<DayItem> initialItems = const []})
    : _delegate = FakeDayGateway(initialItems: initialItems);

  final FakeDayGateway _delegate;
  int submissions = 0;
  int classifications = 0;
  bool failFirstClassification = false;
  Completer<void>? classificationGate;
  final classificationStarted = Completer<void>();

  @override
  Future<DaySnapshot> loadDay(DayQuery query) => _delegate.loadDay(query);

  @override
  Future<List<ManualCalendarDestination>> loadExternalCalendarDestinations() =>
      _delegate.loadExternalCalendarDestinations();

  @override
  Future<ManualCalendarOperationPage> loadExternalCalendarOperations({
    String? cursor,
    int limit = 100,
  }) => _delegate.loadExternalCalendarOperations(cursor: cursor, limit: limit);

  @override
  Future<CaptureReceipt> submitCapture(String input, DayQuery query) {
    submissions++;
    return _delegate.submitCapture(input, query);
  }

  @override
  Future<DaySnapshot> classifyCapture(
    CaptureReceipt capture,
    ClassificationDraft classification,
    DayQuery query,
  ) async {
    classifications++;
    if (!classificationStarted.isCompleted) classificationStarted.complete();
    await classificationGate?.future;
    if (failFirstClassification && classifications == 1) {
      throw StateError('synthetic classification failure');
    }
    return _delegate.classifyCapture(capture, classification, query);
  }

  @override
  dynamic noSuchMethod(Invocation invocation) => throw StateError(
    'Unexpected notes capture fixture operation: $invocation',
  );
}

void main() {
  setUpAll(_loadBundledFonts);

  testWidgets('capture is centered and spacious on desktop with long text', (
    tester,
  ) async {
    _setViewport(tester, width: 1280, height: 900);
    final gateway = _CaptureGateway(initialItems: _notes());
    await tester.pumpWidget(
      _app(gateway, captureKey: const Key('capture-screen')),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Notes'));
    await tester.pumpAndSettle();
    await _openComposer(tester);
    expect(
      tester
          .widget<FloeButton>(find.byKey(const Key('new-note-save')))
          .onPressed,
      isNull,
    );
    await tester.enterText(
      find.byKey(const Key('new-note-content')),
      _longDraft,
    );
    await tester.pumpAndSettle();

    final composer = find.byKey(const Key('new-note-composer'));
    expect(composer, findsOneWidget);
    final dialogSurface = find.descendant(
      of: composer,
      matching: find.byWidgetPredicate(
        (widget) => widget is Material && widget.type == MaterialType.card,
      ),
    );
    expect(dialogSurface, findsOneWidget);
    final rect = tester.getRect(dialogSurface);
    expect(rect.width, lessThanOrEqualTo(640));
    expect(rect.width, greaterThan(580));
    expect(
      find.descendant(of: composer, matching: find.text('Oct 23, 2026')),
      findsNothing,
    );
    expect(tester.takeException(), isNull);
    await expectLater(
      find.byKey(const Key('capture-screen')),
      matchesGoldenFile('goldens/v036-notes-capture-1280.png'),
    );
    await tester.pumpWidget(const SizedBox.shrink());
  });

  testWidgets('narrow capture fills the page and keeps actions available', (
    tester,
  ) async {
    _setViewport(tester, width: 390, height: 844);
    final gateway = _CaptureGateway(initialItems: _notes());
    await tester.pumpWidget(
      _app(gateway, captureKey: const Key('capture-screen')),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Notes'));
    await tester.pumpAndSettle();
    await _openComposer(tester);
    await tester.enterText(
      find.byKey(const Key('new-note-content')),
      _shortDraft,
    );
    await tester.pumpAndSettle();

    final composer = find.byKey(const Key('new-note-composer'));
    expect(tester.getSize(composer), const Size(390, 844));
    expect(find.byType(Dialog), findsNothing);
    expect(
      tester.getRect(find.byKey(const Key('new-note-save'))).bottom,
      lessThanOrEqualTo(844),
    );
    expect(
      find.descendant(of: composer, matching: find.text('Oct 23, 2026')),
      findsNothing,
    );
    expect(tester.takeException(), isNull);
    await expectLater(
      find.byKey(const Key('capture-screen')),
      matchesGoldenFile('goldens/v036-notes-capture-390.png'),
    );
    await tester.pumpWidget(const SizedBox.shrink());
  });

  testWidgets('320px at 200% leaves composer and actions above the keyboard', (
    tester,
  ) async {
    _setViewport(tester, width: 320, height: 740, textScale: 2);
    final gateway = _CaptureGateway(initialItems: _notes());
    await tester.pumpWidget(
      _app(
        gateway,
        textScale: 2,
        reduceMotion: true,
        captureKey: const Key('capture-screen'),
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Notes'));
    await tester.pumpAndSettle();
    await _openComposer(tester);
    final route = ModalRoute.of(
      tester.element(find.byKey(const Key('new-note-composer'))),
    )!;
    expect(route.transitionDuration, Duration.zero);
    expect(route.reverseTransitionDuration, Duration.zero);
    tester.view.viewInsets = const FakeViewPadding(bottom: 280);
    await tester.enterText(
      find.byKey(const Key('new-note-content')),
      _longDraft,
    );
    await tester.pumpAndSettle();

    final composer = find.byKey(const Key('new-note-composer'));
    final save = tester.getRect(find.byKey(const Key('new-note-save')));
    final cancel = tester.getRect(find.byKey(const Key('new-note-cancel')));
    final editor = tester.getRect(find.byKey(const Key('new-note-content')));
    expect(tester.getSize(composer), const Size(320, 740));
    expect(editor.height, greaterThan(0));
    expect(save.top, greaterThanOrEqualTo(editor.bottom));
    expect(save.bottom, lessThanOrEqualTo(460));
    expect(cancel.bottom, lessThanOrEqualTo(460));
    expect(tester.takeException(), isNull);
    await expectLater(
      find.byKey(const Key('capture-screen')),
      matchesGoldenFile('goldens/v036-notes-capture-320-200-keyboard.png'),
    );
    await tester.pumpWidget(const SizedBox.shrink());
  });

  testWidgets(
    'capture retry preserves draft and selection without resubmitting',
    (tester) async {
      _setViewport(tester, width: 390, height: 844);
      final gateway = _CaptureGateway(initialItems: _notes())
        ..failFirstClassification = true;
      await tester.pumpWidget(_app(gateway, reduceMotion: true));
      await tester.pumpAndSettle();
      await tester.tap(find.byTooltip('Notes'));
      await tester.pumpAndSettle();
      await _openComposer(tester);
      const draft = '  Keep this draft exactly while retrying.  ';
      final field = find.byKey(const Key('new-note-content'));
      await tester.enterText(field, draft);
      final controller = tester
          .widget<TextField>(
            find.descendant(of: field, matching: find.byType(TextField)),
          )
          .controller!;
      controller.selection = const TextSelection.collapsed(offset: 10);
      await tester.pump();
      await tester.tap(find.byKey(const Key('new-note-save')));
      await tester.pumpAndSettle();

      expect(gateway.submissions, 1);
      expect(gateway.classifications, 1);
      expect(find.text('Could not save. Please try again.'), findsOneWidget);
      expect(controller.text, draft);
      expect(controller.selection, const TextSelection.collapsed(offset: 10));
      await tester.tap(find.byKey(const Key('new-note-save')));
      await tester.pumpAndSettle();
      expect(gateway.submissions, 1);
      expect(gateway.classifications, 2);
      expect(find.byKey(const Key('new-note-content')), findsNothing);
      final saved = (await gateway.loadDay(_query)).items.whereType<NoteItem>();
      expect(saved.map((note) => note.title), contains(draft.trim()));
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox.shrink());
    },
  );

  testWidgets(
    'pending capture blocks close and late save removes only itself',
    (tester) async {
      _setViewport(tester, width: 390, height: 844);
      final gateway = _CaptureGateway(initialItems: _notes())
        ..classificationGate = Completer<void>();
      await tester.pumpWidget(_app(gateway, reduceMotion: true));
      await tester.pumpAndSettle();
      await tester.tap(find.byTooltip('Notes'));
      await tester.pumpAndSettle();
      await _openComposer(tester);
      await tester.enterText(
        find.byKey(const Key('new-note-content')),
        _shortDraft,
      );
      await tester.pump();
      await tester.tap(find.byKey(const Key('new-note-save')));
      await tester.pump(FloeLoading.minimumDuration);
      await gateway.classificationStarted.future;
      await tester.pump();

      expect(gateway.submissions, 1);
      expect(gateway.classifications, 1);
      expect(
        tester
            .widget<FloeButton>(find.byKey(const Key('new-note-save')))
            .onPressed,
        isNull,
      );
      await tester.tap(find.byKey(const Key('new-note-save')));
      await tester.pump();
      expect(gateway.submissions, 1);
      expect(gateway.classifications, 1);
      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await tester.pump();
      expect(find.byKey(const Key('new-note-content')), findsOneWidget);
      final navigator = Navigator.of(
        tester.element(find.byKey(const Key('new-note-content'))),
      );
      await navigator.maybePop();
      await tester.pump();
      expect(find.byKey(const Key('new-note-content')), findsOneWidget);

      navigator.push<void>(
        PageRouteBuilder<void>(
          transitionDuration: Duration.zero,
          reverseTransitionDuration: Duration.zero,
          pageBuilder: (context, _, _) => const Scaffold(
            body: Center(
              child: Text('Newer destination', key: Key('newer-destination')),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();
      gateway.classificationGate!.complete();
      await tester.pump(const Duration(seconds: 1));
      await tester.pumpAndSettle();
      expect(find.byKey(const Key('newer-destination')), findsOneWidget);
      expect(find.byKey(const Key('new-note-content')), findsNothing);
      expect(gateway.submissions, 1);
      expect(gateway.classifications, 1);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox.shrink());
    },
  );

  testWidgets('disposing during capture does not pop a later route', (
    tester,
  ) async {
    _setViewport(tester, width: 390, height: 844);
    final gateway = _CaptureGateway(initialItems: _notes())
      ..classificationGate = Completer<void>();
    await tester.pumpWidget(_app(gateway, reduceMotion: true));
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Notes'));
    await tester.pumpAndSettle();
    await _openComposer(tester);
    await tester.enterText(
      find.byKey(const Key('new-note-content')),
      _shortDraft,
    );
    await tester.pump();
    await tester.tap(find.byKey(const Key('new-note-save')));
    await tester.pump(FloeLoading.minimumDuration);
    await gateway.classificationStarted.future;
    await tester.pump();

    await tester.pumpWidget(const SizedBox.shrink());
    gateway.classificationGate!.complete();
    await tester.pump(const Duration(seconds: 1));
    expect(tester.takeException(), isNull);
  });

  testWidgets('cancel preserves the Notes query and Escape closes reader', (
    tester,
  ) async {
    _setViewport(tester, width: 390, height: 844);
    await tester.pumpWidget(_app(_CaptureGateway(initialItems: _notes())));
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Notes'));
    await tester.pumpAndSettle();
    final search = find.byType(TextField).first;
    await tester.enterText(search, 'launch');
    await tester.pumpAndSettle();
    final queryController = tester.widget<TextField>(search).controller!;
    await _openComposer(tester);
    final composerFocus = FocusManager.instance.primaryFocus;
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pumpAndSettle();
    expect(find.byKey(const Key('new-note-content')), findsNothing);
    expect(queryController.text, 'launch');
    expect(FocusManager.instance.primaryFocus, isNot(same(composerFocus)));

    await tester.tap(find.text('Launch plan'));
    await tester.pumpAndSettle();
    expect(find.byKey(const Key('note-original-content')), findsOneWidget);
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pumpAndSettle();
    expect(find.byKey(const Key('notes-page-title')), findsOneWidget);
    expect(queryController.text, 'launch');
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox.shrink());
  });

  testWidgets('composer return preserves the Notes collection scroll', (
    tester,
  ) async {
    _setViewport(tester, width: 390, height: 600);
    final gateway = _CaptureGateway(initialItems: _manyNotes());
    await tester.pumpWidget(_app(gateway, reduceMotion: true));
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Notes'));
    await tester.pumpAndSettle();
    final collection = find.byKey(
      const PageStorageKey<String>('workspace-notes-list'),
    );
    final scrollable = find
        .descendant(of: collection, matching: find.byType(Scrollable))
        .first;
    final scrollState = tester.state<ScrollableState>(scrollable);
    scrollState.position.jumpTo(360);
    await tester.pumpAndSettle();
    final offset = scrollState.position.pixels;
    final create = tester.widget<FloeButton>(
      find.byKey(const Key('notes-new-note')),
    );
    create.onPressed!.call();
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const Key('new-note-cancel')));
    await tester.pumpAndSettle();
    final returnedScroll = tester.state<ScrollableState>(scrollable);
    expect(returnedScroll.position.pixels, offset);
    expect(find.byKey(const Key('notes-page-title')), findsOneWidget);
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox.shrink());
  });
}
