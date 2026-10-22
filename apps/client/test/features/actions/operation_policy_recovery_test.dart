import 'dart:ui' as ui;
import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:flutter/material.dart';
import 'package:flutter/semantics.dart';
import 'package:floe_client/app/floe_motion.dart';
import 'package:floe_client/features/settings/presentation/settings_screen.dart';
import 'package:floe_client/l10n/app_localizations.dart';
import 'package:floe_client/app/runtime/app_runtime.dart';
import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/app/runtime/runtime_controller.dart';
import 'package:floe_client/app/runtime/runtime_gateway.dart';
import 'package:floe_client/features/actions/application/calendar_action_controller.dart';
import 'package:floe_client/features/actions/application/calendar_action_gateway.dart';
import 'package:floe_client/features/actions/domain/calendar_action.dart';

void main() {
  for (final outcome in [
    CommandOutcome.notApplied,
    CommandOutcome.notAdmitted,
  ]) {
    test(
      '150 settled $outcome changes do not exhaust correlation capacity',
      () async {
        final gateway = _Gateway();
        gateway.onSet = (_, _, _) async => throw AppRuntimeException(
          'conflict',
          'synthetic settled failure',
          commandOutcome: outcome,
        );
        final controller = await _controller(gateway);
        addTearDown(controller.dispose);
        for (var revision = 1; revision <= 150; revision++) {
          gateway.authority = ActionAuthority(
            revision: revision,
            calendarCreate: ActionAuthorityMode.ask,
          );
          await controller.load();
          await expectLater(
            controller.setAuthority(ActionAuthorityMode.allow),
            throwsA(isA<AppRuntimeException>()),
          );
        }
        expect(gateway.calls, hasLength(150));
        expect(gateway.calls.map((call) => call.id).toSet(), hasLength(150));
      },
    );
  }

  test(
    'current-policy reload cannot authorize a different unresolved change',
    () async {
      final gateway = _Gateway();
      gateway.onSet = (_, _, _) async => throw const AppRuntimeException(
        'timeout',
        'lost acknowledgement',
        commandOutcome: CommandOutcome.indeterminate,
      );
      final controller = await _controller(gateway);
      addTearDown(controller.dispose);
      await expectLater(
        controller.setAuthority(ActionAuthorityMode.allow),
        throwsA(isA<AppRuntimeException>()),
      );
      gateway.authority = const ActionAuthority(
        revision: 2,
        calendarCreate: ActionAuthorityMode.allow,
      );
      await controller.load();
      await expectLater(
        controller.setAuthority(ActionAuthorityMode.deny),
        throwsStateError,
      );
      expect(gateway.calls, hasLength(1));
    },
  );
  for (final outcome in [
    CommandOutcome.admitted,
    CommandOutcome.indeterminate,
    null,
  ]) {
    test(
      'uncertain $outcome survives a later NotAdmitted until exact settlement',
      () async {
        final gateway = _Gateway();
        var attempt = 0;
        gateway.onSet = (_, mode, revision) async {
          attempt++;
          throw AppRuntimeException(
            'conflict',
            'synthetic failure',
            commandOutcome: attempt == 1
                ? outcome
                : attempt == 2
                ? CommandOutcome.notAdmitted
                : CommandOutcome.notApplied,
          );
        };
        final controller = await _controller(gateway);
        addTearDown(controller.dispose);
        await expectLater(
          controller.setAuthority(ActionAuthorityMode.allow),
          throwsA(isA<AppRuntimeException>()),
        );
        expect(controller.hasPendingChange, isTrue);
        await expectLater(
          controller.retryPendingChange(),
          throwsA(isA<AppRuntimeException>()),
        );
        expect(controller.hasPendingChange, isTrue);
        await expectLater(
          controller.retryPendingChange(),
          throwsA(isA<AppRuntimeException>()),
        );
        expect(controller.hasPendingChange, isFalse);
        expect(gateway.calls.map((call) => call.id).toSet(), hasLength(1));
        gateway.onSet = null;
        await controller.setAuthority(ActionAuthorityMode.allow);
        expect(gateway.calls.last.id, isNot(gateway.calls.first.id));
      },
    );
  }

  test('recreated controller replays original revision without regressing newer observation', () async {
    final gateway = _Gateway();
    gateway.onSet = (_, _, _) async => throw const AppRuntimeException(
      'timeout',
      'lost',
      commandOutcome: CommandOutcome.indeterminate,
    );
    final first = await _controller(gateway);
    await expectLater(
      first.setAuthority(ActionAuthorityMode.allow),
      throwsA(isA<AppRuntimeException>()),
    );
    first.dispose();
    gateway.authority = const ActionAuthority(
      revision: 7,
      calendarCreate: ActionAuthorityMode.deny,
    );
    gateway.onSet = null;
    final recreated = await _controller(gateway);
    addTearDown(recreated.dispose);
    expect(recreated.hasPendingChange, isTrue);
    final receipt = await recreated.retryPendingChange();
    expect(gateway.calls[1], gateway.calls[0]);
    expect(receipt.revision, 2);
    expect(recreated.authority!.revision, 7);
    expect(recreated.authority!.calendarCreate, ActionAuthorityMode.deny);
    expect(recreated.hasPendingChange, isFalse);
    await recreated.setAuthority(ActionAuthorityMode.allow);
    expect(gateway.calls.last.revision, 7);
    expect(gateway.calls.last.id, isNot(gateway.calls.first.id));
  });

  test('malformed policy receipt keeps exact recovery identity', () async {
    final gateway = _Gateway();
    gateway.onSet = (_, _, _) async => const ActionAuthority(
      revision: 1,
      calendarCreate: ActionAuthorityMode.allow,
    );
    final controller = await _controller(gateway);
    addTearDown(controller.dispose);
    await expectLater(
      controller.setAuthority(ActionAuthorityMode.allow),
      throwsStateError,
    );
    expect(controller.hasPendingChange, isTrue);
    gateway.onSet = null;
    await controller.retryPendingChange();
    expect(gateway.calls[1], gateway.calls[0]);
    expect(controller.hasPendingChange, isFalse);
  });

  for (final lateOutcome in [
    CommandOutcome.notAdmitted,
    CommandOutcome.notApplied,
  ]) {
    test(
      'late $lateOutcome cannot release a newer overlapping observation',
      () async {
        final gateway = _Gateway();
        final firstResult = Completer<ActionAuthority>();
        final secondResult = Completer<ActionAuthority>();
        gateway.onSet = (_, _, _) => gateway.calls.length == 1
            ? firstResult.future
            : secondResult.future;
        final first = await _controller(gateway);
        final second = await _controller(gateway);
        addTearDown(first.dispose);
        addTearDown(second.dispose);
        final firstFailure = expectLater(
          first.setAuthority(ActionAuthorityMode.allow),
          throwsA(isA<AppRuntimeException>()),
        );
        final secondFailure = expectLater(
          second.retryPendingChange(),
          throwsA(isA<AppRuntimeException>()),
        );
        firstResult.completeError(
          AppRuntimeException('conflict', 'late', commandOutcome: lateOutcome),
        );
        await firstFailure;
        expect(second.hasPendingChange, isTrue);
        secondResult.completeError(
          const AppRuntimeException(
            'timeout',
            'lost',
            commandOutcome: CommandOutcome.indeterminate,
          ),
        );
        await secondFailure;
        gateway.onSet = null;
        await second.retryPendingChange();
        expect(gateway.calls.map((call) => call.id).toSet(), hasLength(1));
      },
    );
  }

  test('disposal does not forget a late policy result', () async {
    final gateway = _Gateway();
    final result = Completer<ActionAuthority>();
    gateway.onSet = (_, _, _) => result.future;
    final first = await _controller(gateway);
    final late = expectLater(
      first.setAuthority(ActionAuthorityMode.allow),
      throwsStateError,
    );
    first.dispose();
    result.complete(
      const ActionAuthority(
        revision: 2,
        calendarCreate: ActionAuthorityMode.allow,
      ),
    );
    await late;
    gateway.onSet = null;
    final second = await _controller(gateway);
    addTearDown(second.dispose);
    await second.retryPendingChange();
    expect(gateway.calls[1], gateway.calls[0]);
  });

  for (final reduced in [false, true]) {
    testWidgets(
      'leaving Settings policy retires old actions, reduced $reduced',
      (tester) async {
        final gateway = _Gateway();
        final controller = await _controller(gateway);
        addTearDown(controller.dispose);
        tester.view.physicalSize = const Size(1200, 1000);
        tester.view.devicePixelRatio = 1;
        addTearDown(tester.view.resetPhysicalSize);
        addTearDown(tester.view.resetDevicePixelRatio);
        final handle = tester.ensureSemantics();
        try {
          await tester.pumpWidget(
            MaterialApp(
              localizationsDelegates: AppLocalizations.localizationsDelegates,
              supportedLocales: AppLocalizations.supportedLocales,
              home: MediaQuery(
                data: MediaQueryData(disableAnimations: reduced),
                child: Scaffold(
                  body: SettingsScreen(
                    connectionsController: null,
                    operationPolicyController: controller,
                  ),
                ),
              ),
            ),
          );
          await tester.pumpAndSettle();
          final oldAction = find.byKey(
            const ValueKey('floe-selection-trigger'),
          );
          final oldPosition = tester.getCenter(oldAction);
          await tester.tap(find.text('Remote server'));
          await tester.pump();
          await tester.pump(const Duration(milliseconds: 40));
          expect(oldAction, findsOneWidget); // Still visible during exit.
          final nav = tester
              .getSemantics(find.byKey(const ValueKey('settings-remoteServer')))
              .getSemanticsData();
          expect(nav.flagsCollection.isEnabled, ui.Tristate.isTrue);
          expect(nav.flagsCollection.isSelected, ui.Tristate.isTrue);
          expect(nav.hasAction(ui.SemanticsAction.tap), isTrue);
          await tester.tapAt(oldPosition);
          await tester.pump();
          expect(
            find.byKey(const ValueKey('floe-selection-popup')),
            findsNothing,
          );
          expect(gateway.calls, isEmpty);
          expect(controller.authority!.calendarCreate, ActionAuthorityMode.ask);
          expect(
            _liveLabels(tester).join('\n'),
            isNot(contains('Create Calendar events')),
          );
          final swap = tester.widget<AnimatedSwitcher>(
            find.byType(AnimatedSwitcher),
          );
          expect(
            swap.duration,
            reduced
                ? FloeMotion.reducedMotionDuration
                : FloeMotion.selectionDuration,
          );
          await tester.pumpAndSettle();
          expect(oldAction, findsNothing);
          expect(tester.takeException(), isNull);
          await tester.pumpWidget(const SizedBox.shrink());
        } finally {
          handle.dispose();
        }
      },
    );
  }

  testWidgets('Settings offers exact retry after pure reload', (tester) async {
    final gateway = _Gateway();
    gateway.onSet = (_, _, _) async => throw const AppRuntimeException(
      'timeout',
      'lost',
      commandOutcome: CommandOutcome.indeterminate,
    );
    final controller = await _controller(gateway);
    addTearDown(controller.dispose);
    await expectLater(
      controller.setAuthority(ActionAuthorityMode.allow),
      throwsA(isA<AppRuntimeException>()),
    );
    gateway.authority = const ActionAuthority(
      revision: 3,
      calendarCreate: ActionAuthorityMode.deny,
    );
    tester.view.physicalSize = const Size(1200, 1000);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    await tester.pumpWidget(
      MaterialApp(
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: Scaffold(
          body: SettingsScreen(
            connectionsController: null,
            operationPolicyController: controller,
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Refresh current value'));
    await tester.pumpAndSettle();
    expect(gateway.calls, hasLength(1));
    expect(find.text('Retry pending change'), findsOneWidget);
    gateway.onSet = null;
    await tester.tap(find.text('Retry pending change'));
    await tester.pumpAndSettle();
    expect(gateway.calls[1], gateway.calls[0]);
    expect(controller.authority!.revision, 3);
    expect(find.text('Retry pending change'), findsNothing);
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox.shrink());
  });
  test(
    'a different gateway cannot inherit another gateway pending change',
    () async {
      final oldGateway = _Gateway();
      oldGateway.onSet = (_, _, _) async =>
          throw const AppRuntimeException('timeout', 'lost');
      final oldController = await _controller(oldGateway);
      addTearDown(oldController.dispose);
      await expectLater(
        oldController.setAuthority(ActionAuthorityMode.allow),
        throwsA(isA<AppRuntimeException>()),
      );
      final newGateway = _Gateway();
      final newController = await _controller(newGateway);
      addTearDown(newController.dispose);
      expect(newController.hasPendingChange, isFalse);
      await newController.setAuthority(ActionAuthorityMode.deny);
      expect(oldController.hasPendingChange, isTrue);
      expect(newGateway.calls.single.id, isNot(oldGateway.calls.single.id));
    },
  );

  test(
    'contradictory same-revision receipt cannot settle pending identity',
    () async {
      final gateway = _Gateway();
      gateway.onSet = (_, _, _) async =>
          throw const AppRuntimeException('timeout', 'lost');
      final controller = await _controller(gateway);
      addTearDown(controller.dispose);
      await expectLater(
        controller.setAuthority(ActionAuthorityMode.allow),
        throwsA(isA<AppRuntimeException>()),
      );
      gateway.authority = const ActionAuthority(
        revision: 2,
        calendarCreate: ActionAuthorityMode.deny,
      );
      await controller.load();
      gateway.onSet = null;
      await expectLater(controller.retryPendingChange(), throwsStateError);
      expect(controller.hasPendingChange, isTrue);
      expect(controller.authority!.calendarCreate, ActionAuthorityMode.deny);
      expect(gateway.calls[1], gateway.calls[0]);
    },
  );
}

Future<OperationPolicyController> _controller(_Gateway gateway) async {
  final runtime = RuntimeController(gateway: _Runtime(), personId: 'person');
  addTearDown(runtime.dispose);
  await runtime.open();
  final controller = OperationPolicyController(
    gateway: gateway,
    runtime: runtime,
  );
  await Future<void>.value();
  expect(controller.loaded, isTrue);
  return controller;
}

final class _Gateway implements OperationAuthorizationGateway {
  ActionAuthority authority = const ActionAuthority(
    revision: 1,
    calendarCreate: ActionAuthorityMode.ask,
  );
  final calls = <({String id, ActionAuthorityMode mode, int revision})>[];
  Future<ActionAuthority> Function(String, ActionAuthorityMode, int)? onSet;
  @override
  Future<ActionAuthority> loadAuthority() async => authority;
  @override
  Future<ActionAuthority> setAuthority({
    required String commandId,
    required ActionAuthorityMode mode,
    required int expectedRevision,
  }) async {
    calls.add((id: commandId, mode: mode, revision: expectedRevision));
    return onSet == null
        ? ActionAuthority(revision: expectedRevision + 1, calendarCreate: mode)
        : onSet!(commandId, mode, expectedRevision);
  }
}

final class _Runtime implements RuntimeGateway {
  @override
  Future<RuntimeReadinessSnapshot> readiness(String requestId) async =>
      const RuntimeReadinessSnapshot(state: RuntimeReadinessState.ready);
  @override
  dynamic noSuchMethod(Invocation invocation) =>
      throw StateError('Unexpected Runtime operation');
}

List<String> _liveLabels(WidgetTester tester) {
  final labels = <String>[];
  void visit(SemanticsNode node) {
    labels.add(node.label);
    node.visitChildren((child) {
      visit(child);
      return true;
    });
  }

  visit(
    tester
        .renderObject(find.byType(Scaffold))
        .owner!
        .semanticsOwner!
        .rootSemanticsNode!,
  );
  return labels;
}
