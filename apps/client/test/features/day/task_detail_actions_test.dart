import 'dart:async';
import 'dart:convert';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_app.dart';
import 'package:floe_client/app/floe_button.dart';
import 'package:floe_client/app/runtime/local_owner_gateways_scope.dart';
import 'package:floe_client/app/floe_selection.dart';
import 'package:floe_client/features/day/application/day_gateway.dart';
import 'package:floe_client/features/day/domain/day_models.dart';
import 'package:floe_client/l10n/app_localizations.dart';

const _personId = 'v035-task-actions';
final _now = DateTime(2026, 10, 23, 10);
final _query = DayQuery.local(personId: _personId, date: _now, now: _now);
const _firstTaskId = 'launch-brief';
const _firstTaskTitle = 'Prepare the launch brief for partner review';

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

Widget _app(_TaskActionGateway gateway, {double textScale = 1}) => FloeApp(
  personId: _personId,
  gateway: gateway,
  query: _query,
  clock: () => _now,
  ownerGateways: const LocalOwnerGateways(),
  builder: (context, child) => MediaQuery(
    data: MediaQuery.of(context)
        .copyWith(textScaler: TextScaler.linear(textScale)),
    child: RepaintBoundary(
      key: const Key('v035-task-actions-capture'),
      child: child!,
    ),
  ),
);

void _setViewport(WidgetTester tester, double width, double height) {
  tester.view.physicalSize = Size(width, height);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
}

Future<void> _openTasks(
  WidgetTester tester,
  _TaskActionGateway gateway, {
  double width = 390,
  double height = 844,
  double textScale = 1,
}) async {
  _setViewport(tester, width, height);
  await tester.pumpWidget(_app(gateway, textScale: textScale));
  await tester.pumpAndSettle();
  await tester.tap(find.byTooltip('Tasks'));
  await tester.pumpAndSettle();
}

Future<void> _openDetail(WidgetTester tester, String title) async {
  await tester.ensureVisible(find.text(title));
  await tester.pumpAndSettle();
  await tester.tap(find.text(title));
  await tester.pumpAndSettle();
}

Finder _rowButton(String title) => find
    .ancestor(of: find.text(title), matching: find.byType(TextButton))
    .first;

Future<void> _confirmDelete(
  WidgetTester tester,
  AppLocalizations copy, {
  String title = _firstTaskTitle,
}) async {
  await tester.tap(find.byKey(const Key('task-detail-delete')));
  await tester.pumpAndSettle();
  expect(find.text(copy.deleteThisItem), findsOneWidget);
  expect(find.text(copy.deleteItemMessage(title)), findsOneWidget);
  await tester.tap(find.text(copy.delete));
}

void main() {
  setUpAll(_loadBundledFonts);

  for (final (width, height, suffix) in [
    (1280.0, 900.0, '1280'),
    (390.0, 844.0, '390'),
  ]) {
    testWidgets('Task list and detail action layout at $suffix px', (
      tester,
    ) async {
      final gateway = _TaskActionGateway();
      _setViewport(tester, width, height);
      await tester.pumpWidget(_app(gateway));
      await tester.pumpAndSettle();
      await tester.tap(find.byTooltip('Tasks'));
      await tester.pumpAndSettle();

      final copy = AppLocalizations.of(tester.element(find.text('Tasks').last));
      expect(find.text(_firstTaskTitle), findsOneWidget);
      expect(find.text(copy.deleteTask), findsNothing);
      expect(
        find.byTooltip(copy.deleteItemLabel(_firstTaskTitle)),
        findsNothing,
      );
      expect(find.byType(FloeCheckbox), findsNWidgets(gateway.tasks.length));
      await expectLater(
        find.byKey(const Key('v035-task-actions-capture')),
        matchesGoldenFile('goldens/v035-task-list-$suffix.png'),
      );

      await _openDetail(tester, _firstTaskTitle);
      expect(find.text(copy.completeTask), findsOneWidget);
      expect(find.text(copy.deleteTask), findsOneWidget);
      expect(find.byKey(const Key('task-detail-back')), findsOneWidget);
      final deleteButton = find.byKey(const Key('task-detail-delete'));
      final deleteRect = tester.getRect(deleteButton);
      expect(deleteRect.width, greaterThanOrEqualTo(44));
      expect(deleteRect.height, greaterThanOrEqualTo(44));
      final deleteTextButton = tester.widget<TextButton>(
        find.descendant(of: deleteButton, matching: find.byType(TextButton)),
      );
      final deleteForeground = deleteTextButton.style!.foregroundColor!;
      expect(deleteForeground.resolve({}), FloePalette.neutral600);
      expect(
        deleteForeground.resolve({WidgetState.focused}),
        FloePalette.error700,
      );
      final semantics = tester.ensureSemantics();
      try {
        expect(find.bySemanticsLabel(copy.deleteTask), findsOneWidget);
      } finally {
        semantics.dispose();
      }
      await expectLater(
        find.byKey(const Key('v035-task-actions-capture')),
        matchesGoldenFile('goldens/v035-task-detail-$suffix.png'),
      );
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox.shrink());
    });
  }

  testWidgets('320px detail actions remain usable at 200% text', (
    tester,
  ) async {
    final gateway = _TaskActionGateway();
    await _openTasks(tester, gateway, width: 320, height: 560, textScale: 2);
    await _openDetail(tester, _firstTaskTitle);
    final deleteAction = find.byKey(const Key('task-detail-delete'));
    await tester.ensureVisible(deleteAction);
    await tester.pumpAndSettle();
    final actionRect = tester.getRect(deleteAction);
    final viewportRect = tester.getRect(
      find.byKey(const Key('personal-day-middle-viewport')),
    );
    expect(actionRect.height, greaterThanOrEqualTo(44));
    expect(deleteAction.hitTestable(), findsOneWidget);
    expect(actionRect.top, greaterThanOrEqualTo(viewportRect.top));
    expect(actionRect.bottom, lessThanOrEqualTo(viewportRect.bottom));
    expect(
      find.text(AppLocalizations.of(tester.element(deleteAction)).deleteTask),
      findsOneWidget,
    );
    await expectLater(
      find.byKey(const Key('v035-task-actions-capture')),
      matchesGoldenFile('goldens/v035-task-detail-320-200.png'),
    );
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox.shrink());
  });

  testWidgets('Enter and Space open detail; Back and cancel restore focus', (
    tester,
  ) async {
    final gateway = _TaskActionGateway.withManyTasks();
    await _openTasks(tester, gateway);
    const targetTitle = 'Task 08: prepare partner review';
    final title = find.text(targetTitle);
    await tester.ensureVisible(title);
    await tester.pumpAndSettle();
    final row = _rowButton(targetTitle);
    final rowFocus = tester.widget<TextButton>(row).focusNode!;
    rowFocus.requestFocus();
    await tester.pump();
    expect(rowFocus.hasPrimaryFocus, isTrue);
    final beforeOpenOffset = Scrollable.of(tester.element(title))
        .position
        .pixels;

    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pumpAndSettle();
    final copy = AppLocalizations.of(
      tester.element(find.byKey(const Key('task-detail-back'))),
    );
    expect(find.byKey(const Key('task-detail-back')), findsOneWidget);
    expect(find.text(copy.noDueDate), findsOneWidget);
    await tester.tap(find.byKey(const Key('task-detail-back')));
    await tester.pumpAndSettle();
    expect(rowFocus.hasPrimaryFocus, isTrue);
    final afterBackTitle = find.text(targetTitle);
    expect(
      Scrollable.of(tester.element(afterBackTitle)).position.pixels,
      closeTo(beforeOpenOffset, 1),
    );

    await tester.sendKeyEvent(LogicalKeyboardKey.space);
    await tester.pumpAndSettle();
    expect(find.text(copy.deleteTask), findsOneWidget);
    await tester.tap(find.byKey(const Key('task-detail-delete')));
    await tester.pumpAndSettle();
    await tester.tap(find.text(copy.cancel));
    await tester.pumpAndSettle();
    expect(find.byKey(const Key('task-detail-delete')), findsOneWidget);
    final deleteFocus = tester
        .widget<FloeButton>(find.byKey(const Key('task-detail-delete')))
        .focusNode!;
    expect(deleteFocus.hasPrimaryFocus, isTrue);
    expect(gateway.deleteCalls, 0);

    await tester.tap(find.byKey(const Key('task-detail-back')));
    await tester.pumpAndSettle();
    expect(rowFocus.hasPrimaryFocus, isTrue);

    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pumpAndSettle();
    await _confirmDelete(tester, copy, title: targetTitle);
    await tester.pumpAndSettle();
    expect(find.text(targetTitle), findsNothing);
    const followingTitle = 'Task 09: prepare partner review';
    final followingRow = _rowButton(followingTitle);
    final followingFocus = tester.widget<TextButton>(followingRow).focusNode!;
    expect(followingFocus.hasPrimaryFocus, isTrue);
    expect(
      Scrollable.of(tester.element(find.text(followingTitle))).position.pixels,
      greaterThan(0),
      reason: 'the collection returns to its scrolled region after deletion',
    );
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox.shrink());
  });

  testWidgets('pending task actions ignore repeated activation', (
    tester,
  ) async {
    final gateway = _TaskActionGateway()
      ..completionGate = Completer<void>()
      ..deleteGate = Completer<void>();
    await _openTasks(tester, gateway);
    await _openDetail(tester, _firstTaskTitle);
    final copy = AppLocalizations.of(
      tester.element(find.byKey(const Key('task-detail-delete'))),
    );

    await tester.tap(find.byKey(const Key('task-detail-complete')));
    await tester.pump();
    expect(gateway.completionCalls, 1);
    final completeButton = tester.widget<FloeButton>(
      find.byKey(const Key('task-detail-complete')),
    );
    expect(completeButton.onPressed, isNull);
    expect(completeButton.loading, isTrue);
    await tester.tap(
      find.byKey(const Key('task-detail-complete')),
      warnIfMissed: false,
    );
    await tester.pump();
    expect(gateway.completionCalls, 1);
    gateway.completionGate!.complete();
    await tester.pump(const Duration(milliseconds: 600));
    await tester.pumpAndSettle();

    await _confirmDelete(tester, copy);
    await tester.pump();
    expect(gateway.deleteCalls, 1);
    final deleteButton = tester.widget<FloeButton>(
      find.byKey(const Key('task-detail-delete')),
    );
    expect(deleteButton.onPressed, isNull);
    expect(deleteButton.loading, isTrue);
    await tester.tap(
      find.byKey(const Key('task-detail-delete')),
      warnIfMissed: false,
    );
    await tester.pump();
    expect(gateway.deleteCalls, 1);
    gateway.deleteGate!.complete();
    await tester.pump(const Duration(milliseconds: 600));
    await tester.pumpAndSettle();
    expect(find.text(_firstTaskTitle), findsNothing);
    expect(find.byKey(const Key('tasks-collection')), findsOneWidget);
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox.shrink());
  });

  testWidgets('a pending delete preserves a newer task detail', (tester) async {
    final gateway = _TaskActionGateway()..deleteGate = Completer<void>();
    await _openTasks(tester, gateway);
    await _openDetail(tester, _firstTaskTitle);
    final copy = AppLocalizations.of(
      tester.element(find.byKey(const Key('task-detail-delete'))),
    );

    await _confirmDelete(tester, copy);
    await tester.pump();
    expect(gateway.deleteCalls, 1);

    await tester.tap(find.byKey(const Key('task-detail-back')));
    await tester.pump();
    await tester.tap(find.byTooltip('Notes'));
    await tester.pump();
    await tester.tap(find.byTooltip('Tasks'));
    await tester.pump();

    const secondTaskTitle = 'Send the revised timeline to Maya';
    final semantics = tester.ensureSemantics();
    try {
      final row = _rowButton(secondTaskTitle);
      expect(row, findsOneWidget);
      expect(row.hitTestable(), findsNothing);
      final node = tester.getSemantics(row);
      expect(
        node.getSemanticsData().hasAction(ui.SemanticsAction.tap),
        isFalse,
        reason: 'the pending command blocks activation of another task',
      );
    } finally {
      semantics.dispose();
    }

    // Exercise the real row navigation callback as a newer screen transition
    // while the deferred delete is still unresolved. Normal pointer, keyboard,
    // and accessibility activation remain blocked by the loading overlay.
    tester.widget<TextButton>(_rowButton(secondTaskTitle)).onPressed!.call();
    await tester.pump();
    expect(find.text(secondTaskTitle), findsOneWidget);
    expect(find.byKey(const Key('task-detail-back')), findsOneWidget);
    gateway.deleteGate!.complete();
    await tester.pump(const Duration(milliseconds: 600));
    await tester.pumpAndSettle();
    expect(find.text(secondTaskTitle), findsOneWidget);
    expect(find.byKey(const Key('task-detail-back')), findsOneWidget);
    expect(find.byKey(const Key('task-detail-delete')), findsOneWidget);
    expect(
      tester
          .widget<FloeButton>(find.byKey(const Key('task-detail-delete')))
          .onPressed,
      isNotNull,
    );
    expect(gateway.tasks.any((task) => task.id == _firstTaskId), isFalse);
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox.shrink());
  });

  testWidgets('command failures stay in detail and can be retried', (
    tester,
  ) async {
    final gateway = _TaskActionGateway()..failNextCompletion = true;
    await _openTasks(tester, gateway);
    await _openDetail(tester, _firstTaskTitle);

    await tester.tap(find.text('Complete task'));
    await tester.pumpAndSettle();
    expect(
      find.text('Bad state: Synthetic completion rejected'),
      findsOneWidget,
    );
    expect(find.byKey(const Key('task-detail-delete')), findsOneWidget);
    await tester.tap(find.text('Close'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Complete task'));
    await tester.pumpAndSettle();
    expect(find.text('Mark incomplete'), findsOneWidget);

    gateway.failNextDelete = true;
    final copy = AppLocalizations.of(
      tester.element(find.byKey(const Key('task-detail-delete'))),
    );
    await _confirmDelete(tester, copy);
    await tester.pumpAndSettle();
    expect(find.text('Bad state: Synthetic delete rejected'), findsOneWidget);
    expect(find.byKey(const Key('task-detail-back')), findsOneWidget);
    await tester.tap(find.text(copy.close));
    await tester.pumpAndSettle();
    await _confirmDelete(tester, copy);
    await tester.pumpAndSettle();
    expect(find.text(_firstTaskTitle), findsNothing);
    expect(find.text('Send the revised timeline to Maya'), findsOneWidget);
    final nextRow = _rowButton('Send the revised timeline to Maya');
    expect(
      tester.widget<TextButton>(nextRow).focusNode!.hasPrimaryFocus,
      isTrue,
    );
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox.shrink());
  });

  testWidgets('deleting the last task leaves focus on the collection heading', (
    tester,
  ) async {
    final gateway = _TaskActionGateway()
      ..tasks.removeWhere((task) => task.id != _firstTaskId);
    await _openTasks(tester, gateway);
    await _openDetail(tester, _firstTaskTitle);
    final copy = AppLocalizations.of(
      tester.element(find.byKey(const Key('task-detail-delete'))),
    );
    await _confirmDelete(tester, copy);
    await tester.pumpAndSettle();
    expect(find.text(copy.noTasksYet), findsOneWidget);
    final heading = find.byKey(const Key('tasks-page-title'));
    expect(Focus.of(tester.element(heading)).hasPrimaryFocus, isTrue);
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox.shrink());
  });
}

final class _TaskActionGateway implements DayGateway {
  _TaskActionGateway() : tasks = _defaultTasks();
  _TaskActionGateway.withManyTasks()
    : tasks = List.generate(
        12,
        (index) => TaskItem(
          id: 'task-${index.toString().padLeft(2, '0')}',
          title:
              'Task ${index.toString().padLeft(2, '0')}: prepare partner review',
          revision: 1,
          createdAt: _now,
        ),
      );

  final List<TaskItem> tasks;
  int deleteCalls = 0;
  int completionCalls = 0;
  bool failNextDelete = false;
  bool failNextCompletion = false;
  Completer<void>? deleteGate;
  Completer<void>? completionGate;

  @override
  Future<DaySnapshot> loadDay(DayQuery query) async => _snapshot(query);

  @override
  Future<DaySnapshot> setTaskCompleted(
    TaskItem task,
    bool completed,
    DayQuery query,
  ) async {
    completionCalls++;
    final gate = completionGate;
    if (gate != null) {
      await gate.future;
      if (identical(completionGate, gate)) completionGate = null;
    }
    if (failNextCompletion) {
      failNextCompletion = false;
      throw StateError('Synthetic completion rejected');
    }
    final index = tasks.indexWhere((candidate) => candidate.id == task.id);
    if (index < 0) throw StateError('Synthetic task no longer exists');
    final current = tasks[index];
    tasks[index] = TaskItem(
      id: current.id,
      title: current.title,
      revision: current.revision + 1,
      createdAt: current.createdAt,
      deadline: current.deadline,
      completedAt: completed ? query.now : null,
    );
    return _snapshot(query);
  }

  @override
  Future<DaySnapshot> deleteItem(DayItem item, DayQuery query) async {
    deleteCalls++;
    final gate = deleteGate;
    if (gate != null) {
      await gate.future;
      if (identical(deleteGate, gate)) deleteGate = null;
    }
    if (failNextDelete) {
      failNextDelete = false;
      throw StateError('Synthetic delete rejected');
    }
    tasks.removeWhere((task) => task.id == item.id);
    return _snapshot(query);
  }

  DaySnapshot _snapshot(DayQuery query) => DaySnapshot(
    personId: query.personId,
    date: query.date,
    generatedAt: query.now,
    timezoneOffsetSeconds: query.timezoneOffsetSeconds,
    items: List<DayItem>.unmodifiable(tasks),
  );

  @override
  dynamic noSuchMethod(Invocation invocation) => throw StateError(
    'Unexpected synthetic gateway call: ${invocation.memberName}',
  );
}

List<TaskItem> _defaultTasks() => [
  TaskItem(
    id: _firstTaskId,
    title: _firstTaskTitle,
    revision: 1,
    createdAt: _now,
  ),
  TaskItem(
    id: 'send-timeline',
    title: 'Send the revised timeline to Maya',
    revision: 1,
    createdAt: _now,
    deadline: DateTime(2026, 10, 24, 12),
  ),
  TaskItem(
    id: 'beta-feedback',
    title: 'Collect feedback from the beta team',
    revision: 1,
    createdAt: _now,
    deadline: DateTime(2026, 10, 23, 9),
  ),
];
