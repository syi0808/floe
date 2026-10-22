import 'package:floe_client/app/floe_squircle.dart';
import 'package:flutter_markdown_plus/flutter_markdown_plus.dart';
import 'package:floe_client/app/floe_app.dart';
import 'package:floe_client/features/day/application/fake_day_gateway.dart';
import 'package:floe_client/features/day/domain/day_models.dart';
import 'package:floe_client/features/actions/presentation/agent_proposal_card.dart';
import 'package:floe_client/app/floe_button.dart';
import 'package:floe_client/app/floe_motion.dart';
import 'package:floe_client/features/conversation/domain/agent_session.dart';
import 'package:floe_client/features/actions/domain/calendar_action.dart';

import 'dart:async';

import 'package:floe_client/features/conversation/application/conversation_observation.dart';

import '../../support/interaction_page_fixture.dart';

import 'package:floe_client/features/conversation/presentation/agent_interaction_card.dart';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/l10n/app_localizations.dart';
import 'package:floe_client/app/runtime/app_read_model.dart';
import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/app/runtime/local_owner_gateways_scope.dart';
import 'package:floe_client/app/runtime/runtime_controller.dart';
import 'package:floe_client/app/runtime/runtime_gateway.dart';
import 'package:floe_client/features/conversation/application/conversation_controller.dart';
import 'package:floe_client/features/conversation/infrastructure/app_wire_conversation_client.dart';
import 'package:floe_client/features/conversation/infrastructure/app_wire_conversation_gateway.dart';
import 'package:floe_client/features/conversation/presentation/agent_panel.dart';
import 'package:floe_client/features/conversation/presentation/conversation_composer_draft.dart';
import 'package:floe_client/features/day/application/day_gateway.dart';

void main() {
  for (final width in [320.0, 388.0]) {
    for (final scale in [1.0, 2.0]) {
      for (final empty in [true, false]) {
        testWidgets(
          'compact short history anchors composer at $width, $scale, $empty',
          (tester) async {
            tester.view.physicalSize = Size(width, 640);
            tester.view.devicePixelRatio = 1;
            tester.platformDispatcher.textScaleFactorTestValue = scale;
            addTearDown(tester.view.resetPhysicalSize);
            addTearDown(tester.view.resetDevicePixelRatio);
            addTearDown(
              tester.platformDispatcher.clearTextScaleFactorTestValue,
            );
            final h = await _Harness.create(
              initialSession: _session(
                messages: empty
                    ? []
                    : [
                        {..._message(_m1), 'text': 'Keep it simple.'},
                      ],
              ),
            );
            addTearDown(h.dispose);
            await tester.pumpWidget(_panelApp(h.controller));
            await tester.pumpAndSettle();
            final composer = find.byKey(const Key('conversation-composer'));
            expect(tester.getRect(composer).bottom, closeTo(628, 1));
            final body = find
                .descendant(
                  of: find.byType(SingleChildScrollView).first,
                  matching: find.byType(Scrollable),
                )
                .first;
            expect(
              tester.state<ScrollableState>(body).position.maxScrollExtent,
              0,
            );
            const draft = 'First line\nSecond line\nThird line';
            await tester.enterText(find.byType(TextField), draft);
            await tester.pumpAndSettle();
            expect(tester.getRect(composer).bottom, closeTo(628, 1));
            expect(
              tester.widget<TextField>(find.byType(TextField)).controller!.text,
              draft,
            );
            expect(find.byTooltip('Close').hitTestable(), findsOneWidget);
            expect(h.transport.commands, isEmpty);
            expect(tester.takeException(), isNull);
            await tester.pumpWidget(const SizedBox.shrink());
          },
        );
      }
    }
  }
  for (final width in [320.0, 388.0]) {
    for (final scale in [1.0, 2.0]) {
      testWidgets(
        'conversation turn hierarchy stays readable at $width, $scale',
        (tester) async {
          tester.view.physicalSize = Size(width, 1000);
          tester.view.devicePixelRatio = 1;
          tester.platformDispatcher.textScaleFactorTestValue = scale;
          addTearDown(tester.view.resetPhysicalSize);
          addTearDown(tester.view.resetDevicePixelRatio);
          addTearDown(tester.platformDispatcher.clearTextScaleFactorTestValue);
          const userText = 'Keep the plan simple.';
          const longUserText =
              '오후에는 중요한 일 하나에 집중할게.\n회의가 끝나면 다음 단계를 짧고 명확하게 정리해 줘.';
          const answer =
              '## Start small\n\n- Write **one opening paragraph**.\n- Leave the details for later.';
          const partial = '첫 문단부터 작성하고 있어요.';
          final session = _session(
            messages: [
              {..._message(_m1), 'text': userText},
              {..._message(_m2), 'kind': 'assistant', 'text': answer},
              {..._message(_m3), 'text': longUserText},
              {..._message(_m4), 'kind': 'assistant', 'text': partial},
            ],
          );
          final surface = _surfaceEnvelope(session);
          final entries = (surface['snapshot']['messages'] as List)
              .cast<Map<String, dynamic>>();
          for (final index in [1, 3]) {
            entries[index]['content'] = {
              'kind': 'result',
              'run_id': _terminalRunId,
              'completeness': index == 3 ? 'partial' : 'complete',
              'text': index == 1 ? answer : partial,
            };
          }
          final h = await _Harness.create(
            initialSession: session,
            surfaceRead: (_) async => surface,
          );
          addTearDown(h.dispose);
          final semantics = tester.ensureSemantics();
          try {
            await tester.pumpWidget(_panelApp(h.controller));
            await tester.pumpAndSettle();
            final user = find.byKey(const ValueKey('conversation-turn-$_m1'));
            final assistant = find.byKey(
              const ValueKey('conversation-turn-$_m2'),
            );
            final partialTurn = find.byKey(
              const ValueKey('conversation-turn-$_m4'),
            );
            final bubble = find.descendant(
              of: user,
              matching: find.byType(FloeSquircle),
            );
            expect(bubble, findsOneWidget);
            final userRect = tester.getRect(user);
            final bubbleRect = tester.getRect(bubble);
            expect(
              bubbleRect.width,
              lessThanOrEqualTo(userRect.width * .88 + .01),
            );
            expect(bubbleRect.right, closeTo(userRect.right, .01));
            expect(
              find.descendant(
                of: assistant,
                matching: find.byType(FloeSquircle),
              ),
              findsNothing,
            );
            expect(find.text('You'), findsNothing);
            expect(find.text('Floe'), findsOneWidget);
            expect(find.text('Partial result'), findsOneWidget);
            expect(
              find.descendant(
                of: partialTurn,
                matching: find.text('Partial result'),
              ),
              findsOneWidget,
            );
            expect(tester.getSemantics(user).label, 'You');
            expect(tester.getSemantics(assistant).label, 'Floe');
            final selectedUser = find.descendant(
              of: user,
              matching: find.byType(SelectableText),
            );
            expect(selectedUser, findsOneWidget);
            expect(tester.widget<SelectableText>(selectedUser).data, userText);
            expect(tester.getSemantics(selectedUser).value, userText);
            final markdown = find.descendant(
              of: assistant,
              matching: find.byType(MarkdownBody),
            );
            expect(tester.widget<MarkdownBody>(markdown).selectable, isTrue);
            expect(tester.widget<MarkdownBody>(markdown).data, answer);
            expect(
              find.descendant(
                of: assistant,
                matching: find.byType(SelectableText),
              ),
              findsWidgets,
            );
            expect(find.text(longUserText), findsOneWidget);
            expect(tester.takeException(), isNull);
            expect(h.transport.commands, isEmpty);
            await tester.pumpWidget(const SizedBox.shrink());
          } finally {
            semantics.dispose();
          }
        },
      );
    }
  }

  for (final width in [320.0, 388.0]) {
    for (final scale in [1.0, 2.0]) {
      for (final homeInset in [0.0, 34.0]) {
        testWidgets(
          'mobile conversation clears the keyboard at $width, $scale, $homeInset',
          (tester) async {
            tester.view.physicalSize = Size(width, 740);
            tester.view.devicePixelRatio = 1;
            tester.view.padding = FakeViewPadding(top: 24, bottom: homeInset);
            tester.view.viewPadding = FakeViewPadding(
              top: 24,
              bottom: homeInset,
            );
            tester.platformDispatcher.textScaleFactorTestValue = scale;
            addTearDown(tester.view.resetPhysicalSize);
            addTearDown(tester.view.resetDevicePixelRatio);
            addTearDown(tester.view.resetPadding);
            addTearDown(tester.view.resetViewPadding);
            addTearDown(tester.view.resetViewInsets);
            addTearDown(
              tester.platformDispatcher.clearTextScaleFactorTestValue,
            );
            final h = await _Harness.create();
            addTearDown(h.dispose);
            final now = DateTime(2026, 10, 22, 12);
            await tester.pumpWidget(
              FloeApp(
                personId: _personId,
                gateway: FakeDayGateway(),
                query: DayQuery.local(personId: _personId, date: now, now: now),
                agentGateway: h.controller.gateway,
                ownerGateways: LocalOwnerGateways(runtime: h.runtime),
              ),
            );
            await tester.pumpAndSettle();
            await tester.tap(find.text('Floe').last);
            await tester.pumpAndSettle();
            const text = 'Short first line.\nA clear next step.';
            await tester.enterText(find.byType(TextField), text);
            await tester.pumpAndSettle();
            expect(
              tester
                  .getRect(find.byKey(const Key('conversation-composer')))
                  .bottom,
              lessThanOrEqualTo(740 - homeInset),
            );
            tester.view.padding = const FakeViewPadding(top: 24);
            tester.view.viewInsets = const FakeViewPadding(bottom: 320);
            await tester.pumpAndSettle();
            final composer = find.byKey(const Key('conversation-composer'));
            final close = find.byTooltip('Close');
            expect(tester.getRect(composer).bottom, closeTo(408, 0.01));
            expect(tester.getRect(close).top, greaterThanOrEqualTo(24));
            expect(tester.getRect(close).bottom, lessThan(420));
            expect(
              tester.widget<TextField>(find.byType(TextField)).controller!.text,
              text,
            );
            expect(h.transport.commands, isEmpty);
            expect(tester.takeException(), isNull);
            tester.platformDispatcher.textScaleFactorTestValue = scale == 1
                ? 2
                : 1;
            await tester.pumpAndSettle();
            expect(tester.getRect(composer).bottom, closeTo(408, 0.01));
            expect(
              tester.widget<TextField>(find.byType(TextField)).controller!.text,
              text,
            );
            tester.view.viewInsets = const FakeViewPadding();
            tester.view.padding = FakeViewPadding(top: 24, bottom: homeInset);
            await tester.pumpAndSettle();
            expect(
              tester.getRect(composer).bottom,
              lessThanOrEqualTo(740 - homeInset),
            );
            expect(
              tester.widget<TextField>(find.byType(TextField)).controller!.text,
              text,
            );
            await tester.tap(close);
            await tester.pumpAndSettle();
            await tester.ensureVisible(find.text('Timeline'));
            await tester.tap(find.text('Timeline'));
            await tester.pumpAndSettle();
            await tester.ensureVisible(
              find.byTooltip('Tasks and calendar status'),
            );
            await tester.tap(find.byTooltip('Tasks and calendar status'));
            await tester.pumpAndSettle();
            tester.view.padding = const FakeViewPadding(top: 24);
            tester.view.viewInsets = const FakeViewPadding(bottom: 320);
            await tester.pumpAndSettle();
            expect(find.text('Tasks and calendar status'), findsOneWidget);
            expect(
              tester.getRect(find.byTooltip('Close')).bottom,
              lessThan(420),
            );
            expect(tester.takeException(), isNull);
            await tester.pumpWidget(const SizedBox.shrink());
          },
        );
      }
    }
  }

  testWidgets('expanded recent work stays below the compact Close control', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(320, 260);
    tester.view.devicePixelRatio = 1;
    tester.platformDispatcher.textScaleFactorTestValue = 2;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    addTearDown(tester.platformDispatcher.clearTextScaleFactorTestValue);
    final h = await _Harness.create(surfaceSequence: 20);
    addTearDown(h.dispose);
    h.transport.returnTerminalFailure = true;
    h.transport.workingRun = true;
    expect(
      await h.controller.sendText(
        'A longer synthetic work item to inspect safely',
      ),
      isTrue,
    );
    h.transport.extraWorks = [
      for (var i = 1; i <= 5; i++)
        {
          'receipt': {
            'command_id':
                '10000000-0000-4000-8000-${(100 + i).toString().padLeft(12, '0')}',
            'work_id':
                '10000000-0000-4000-8000-${(200 + i).toString().padLeft(12, '0')}',
            'execution_command_id':
                '10000000-0000-4000-8000-${(300 + i).toString().padLeft(12, '0')}',
            'input_id':
                '10000000-0000-4000-8000-${(400 + i).toString().padLeft(12, '0')}',
            'anchor': _surfaceAnchor(_sessionId, i),
            'origin': {'kind': 'text'},
            'runtime_epoch': 1,
          },
          'input_text': 'Queued synthetic task $i for a clear next step',
          'state': {'kind': 'queued'},
          'current_run_id': null,
        },
    ];
    await h.controller.refreshSurface();
    await tester.pumpWidget(_panelApp(h.controller));
    await tester.pumpAndSettle();
    final close = find.byTooltip('Close');
    final initialRect = tester.getRect(close);
    final work = find.textContaining('Recent work');
    await tester.ensureVisible(work);
    await tester.pumpAndSettle();
    await tester.tap(work);
    await tester.pumpAndSettle();
    expect(tester.takeException(), isNull);
    final workList = find.byKey(
      const PageStorageKey<Object>((
        'conversation-work-scroll',
        _personId,
        _sessionId,
      )),
    );
    expect(tester.getSize(workList).height, 220);
    final cancel = find.byTooltip('Cancel').first;
    await tester.ensureVisible(cancel);
    await tester.pumpAndSettle();
    // Settle the inner list before revealing the target in the outer viewport.
    await tester.ensureVisible(cancel);
    await tester.pumpAndSettle();
    expect(cancel.hitTestable(), findsOneWidget);
    expect(tester.getSize(cancel).width, greaterThanOrEqualTo(44));
    final workTile = find.byKey(
      const PageStorageKey<Object>((
        'conversation-recent-work',
        _personId,
        _sessionId,
      )),
    );
    final workStop = find.descendant(
      of: workTile,
      matching: find.byTooltip('Stop response'),
    );
    await tester.scrollUntilVisible(
      workStop,
      100,
      scrollable: find
          .descendant(of: workList, matching: find.byType(Scrollable))
          .first,
    );
    await tester.pumpAndSettle();
    await tester.ensureVisible(workStop);
    await tester.pumpAndSettle();
    expect(workStop.hitTestable(), findsOneWidget);
    final composerAction = find.byKey(
      const Key('conversation-composer-action'),
    );
    await tester.ensureVisible(composerAction);
    await tester.pumpAndSettle();
    expect(composerAction.hitTestable(), findsOneWidget);
    expect(tester.widget<FloeButton>(composerAction).onPressed, isNotNull);
    expect(tester.takeException(), isNull);
    expect(tester.getRect(close), initialRect);
    expect(initialRect.top, greaterThanOrEqualTo(0));
    await tester.pumpWidget(const SizedBox.shrink());
  });

  for (final width in [320.0, 1200.0]) {
    for (final scale in [1.0, 2.0]) {
      testWidgets(
        'composer keeps one label and truthful send state at $width, $scale',
        (tester) async {
          tester.view.physicalSize = Size(width, 740);
          tester.view.devicePixelRatio = 1;
          tester.platformDispatcher.textScaleFactorTestValue = scale;
          addTearDown(tester.view.resetPhysicalSize);
          addTearDown(tester.view.resetDevicePixelRatio);
          addTearDown(tester.platformDispatcher.clearTextScaleFactorTestValue);
          final h = await _Harness.create();
          addTearDown(h.dispose);
          final handle = tester.ensureSemantics();
          try {
            await tester.pumpWidget(_panelApp(h.controller));
            await tester.pumpAndSettle();
            final input = find.byKey(const Key('conversation-composer-input'));
            final action = find.byKey(
              const Key('conversation-composer-action'),
            );
            expect(tester.widget<FloeButton>(action).onPressed, isNull);
            expect(find.text('Message Floe'), findsOneWidget);
            expect(
              tester
                  .widget<TextField>(find.byType(TextField))
                  .decoration!
                  .label,
              isNull,
            );
            expect(
              tester.widget<TextField>(find.byType(TextField)).decoration!.hint,
              isNotNull,
            );
            expect(tester.getSize(action).width, greaterThanOrEqualTo(44));
            expect(tester.getSize(action).height, greaterThanOrEqualTo(44));
            await tester.enterText(input, '   ');
            await tester.pump();
            expect(tester.widget<FloeButton>(action).onPressed, isNull);
            await tester.enterText(input, 'A draft\nwith a second line');
            await tester.pumpAndSettle();
            expect(tester.widget<FloeButton>(action).onPressed, isNotNull);
            final field = find.descendant(
              of: input,
              matching: find.byType(TextField),
            );
            final node = tester.getSemantics(field);
            expect(node.label, contains('Message Floe'));
            expect(node.value, 'A draft\nwith a second line');
            expect(
              tester.widget<TextField>(field).decoration!.floatingLabelBehavior,
              FloatingLabelBehavior.never,
            );
            expect(h.transport.commands, isEmpty);
            expect(tester.takeException(), isNull);
            await tester.pumpWidget(const SizedBox.shrink());
          } finally {
            handle.dispose();
          }
        },
      );
    }
  }

  for (final scale in [1.0, 2.0]) {
    testWidgets('compact conversation keeps Close visible at $scale', (
      tester,
    ) async {
      // Use a short viewport so this fixture exercises real scrolling.
      tester.view.physicalSize = const Size(320, 400);
      tester.view.devicePixelRatio = 1;
      tester.platformDispatcher.textScaleFactorTestValue = scale;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      addTearDown(tester.platformDispatcher.clearTextScaleFactorTestValue);
      final h = await _Harness.create();
      addTearDown(h.dispose);
      var closes = 0;
      await tester.pumpWidget(_panelApp(h.controller, onClose: () => closes++));
      await tester.pumpAndSettle();
      await tester.enterText(
        find.byType(TextField),
        'First line\nSecond line\nThird line\nFourth line',
      );
      await tester.pumpAndSettle();
      final close = find.byTooltip('Close');
      final initialRect = tester.getRect(close);
      expect(initialRect.top, greaterThanOrEqualTo(0));
      final scrollable = find
          .descendant(
            of: find.byType(SingleChildScrollView).first,
            matching: find.byType(Scrollable),
          )
          .first;
      final position = tester.state<ScrollableState>(scrollable).position;
      expect(position.maxScrollExtent, greaterThan(0));
      position.jumpTo(position.maxScrollExtent);
      await tester.pump();
      expect(tester.getRect(close), initialRect);
      position.jumpTo(0);
      await tester.pump();
      expect(tester.getRect(close), initialRect);
      await tester.tap(close);
      expect(closes, 1);
      expect(h.transport.commands, isEmpty);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox.shrink());
    });
  }

  test('a synchronous readiness failure retains one Work intent and permits exact recovery', () async {
    var attempts = 0;
    final h = await _Harness.create(
      beforeAdmission: () {
        if (++attempts == 1) throw StateError('synthetic readiness failure');
        return Future<void>.value();
      },
    );
    addTearDown(h.dispose);
    final accepted = await h.controller.sendText('retain this exact input');
    expect(accepted, isFalse);
    expect(h.controller.hasPendingWorkAdmission, isTrue);
    expect(h.transport.commands, isEmpty);
    final retainedId = h
        .controller
        .gateway
        .conversationRuntime
        .pendingWorkAdmission!
        .commandId;
    await h.controller.refreshSurface();
    expect(attempts, 1, reason: 'Read-only polling never retries admission.');
    expect(h.transport.commands, isEmpty);
    h.transport.returnTerminalFailure = true;
    h.transport.terminalSession = _session(
      revision: 13,
      messages: [_message(_m5), _message(_m6)],
    );
    await h.controller.retryPendingWorkAdmission();
    expect(attempts, 2);
    expect(h.controller.hasPendingWorkAdmission, isFalse);
    expect(h.transport.commands, hasLength(1));
    expect(h.transport.commandIds.single, retainedId);
    expect(
      h.transport.commands.single['kind'],
      'conversation.work.submit_text',
    );
    expect(h.transport.commands.single['text'], 'retain this exact input');
    expect(
      h.controller.surfaceMessages.whereType<AppSurfaceInput>().last.text,
      'retain this exact input',
    );
  });

  testWidgets('empty composer keeps Stop available for running work', (
    tester,
  ) async {
    _usePanelViewport(tester);
    final h = await _Harness.create();
    addTearDown(h.dispose);
    h.transport.returnTerminalFailure = true;
    h.transport.workingRun = true;
    expect(await h.controller.sendText('synthetic running work'), isTrue);
    await tester.pumpWidget(_panelApp(h.controller));
    await tester.pumpAndSettle();
    final action = tester.widget<FloeButton>(
      find.byKey(const Key('conversation-composer-action')),
    );
    expect(action.tooltip, 'Stop response');
    expect(action.onPressed, isNotNull);
    expect(
      tester.widget<TextField>(find.byType(TextField)).controller!.text,
      isEmpty,
    );
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox.shrink());
  });

  test('a working Manager does not disable new text admission', () async {
    final h = await _Harness.create();
    addTearDown(h.dispose);
    h.transport.returnTerminalFailure = true;
    h.transport.workingRun = true;
    expect(await h.controller.sendText('a retained Work input'), isTrue);
    expect(h.controller.running, isTrue);
    expect(h.controller.hasPendingWorkAdmission, isFalse);
    expect(h.controller.canSend, isTrue);
    expect(h.controller.workItems.single.state, AppSurfaceWorkState.linked);
    await h.controller.refreshSurface();
    expect(
      h.transport.commands,
      hasLength(1),
      reason: 'Status polling never dispatches Work.',
    );
  });

  test(
    'an explicit Run observer never follows an unrelated queued Work',
    () async {
      final h = await _Harness.create();
      addTearDown(h.dispose);
      h.transport.returnTerminalFailure = true;
      h.transport.terminalSession = _session(
        revision: 13,
        messages: [_message(_m5), _message(_m6)],
      )..['active_turn'] = _m4;
      final completion = await h.controller.gateway.conversationRuntime
          .observeConversationRun(
            const AppCommandReceipt(
              commandId: _m1,
              runId: _terminalRunId,
              sessionRevision: 12,
              runtimeEpoch: 1,
            ),
            h.controller.session!,
            onRun: (_) {},
            observation: ConversationObservation(),
          );
      expect(completion.run.runId, _terminalRunId);
      expect(completion.session.activeTurn, _m4);
      expect(h.transport.runQueries, [_terminalRunId]);
      expect(h.transport.commands, isEmpty);
    },
  );

  test('an older in-flight read cannot re-enable sending behind an admitted prefix', () async {
    final h = await _Harness.create();
    addTearDown(h.dispose);
    h.transport.returnTerminalFailure = true;
    h.transport.workingRun = true;
    final requested = Completer<void>();
    final response = Completer<Map<String, dynamic>>();
    h.transport.surfaceRead = (_) {
      h.transport.surfaceRead = null;
      requested.complete();
      return response.future;
    };
    final reading = h.controller.refreshSurface();
    await requested.future;
    expect(
      await h.controller.sendText('preserve the committed prefix'),
      isTrue,
    );
    expect(h.controller.canSend, isFalse);
    response.complete(_surfaceEnvelope(h.transport.initialSession, head: 4));
    await reading;
    expect(
      h.controller.canSend,
      isFalse,
      reason: 'The committed Work is newer than this read.',
    );
    await h.controller.refreshSurface();
    expect(h.controller.canSend, isTrue);
    expect(h.transport.commands, hasLength(1));
  });

  for (final invalidation in ['session', 'runtime', 'dispose']) {
    test('proposal observation fences $invalidation replacement', () async {
      final h = await _Harness.create();
      addTearDown(h.dispose);
      final response = Completer<Map<String, dynamic>>();
      final requested = Completer<void>();
      h.transport.proposalPreview = (_) {
        requested.complete();
        return response.future;
      };
      final pending = h.controller.previewCalendarProposal(
        receipt: _proposalReceipt(),
        artifactId: _m3,
      );
      await requested.future;
      final rejected = expectLater(pending, throwsStateError);
      switch (invalidation) {
        case 'session':
          h.transport.replacementSession = _session(
            id: _replacementSessionId,
            messages: [_message(_replacementMessageId)],
          );
          await h.controller.load(newSession: true);
        case 'runtime':
          h.runtime.closeAdmission();
        case 'dispose':
          h.disposeController();
      }
      response.complete(
        _proposalPreviewEnvelope(_readyProposal('Old generation')),
      );
      await rejected;
    });
  }

  testWidgets(
    'execution details reveals complete delegation sources and read-only proposal review',
    (tester) async {
      tester.view.physicalSize = const Size(320, 720);
      tester.view.devicePixelRatio = 1;
      tester.platformDispatcher.textScaleFactorTestValue = 2;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      addTearDown(tester.platformDispatcher.clearTextScaleFactorTestValue);

      final output = List.generate(
        64,
        (index) =>
            'Line ${index + 1}: 제안 원문 stays complete and selectable at narrow widths.',
      ).join('\n');
      final userMessage = {
        ..._message(_m5),
        'text': 'Please review this calendar proposal.',
      };
      final assistantMessage = {
        ..._message(_m6),
        'kind': 'assistant',
        'text': 'I prepared a proposal for review.',
      };
      final initialSession = _session(
        messages: [
          userMessage,
          assistantMessage,
          _proposalDelegationMessage(output: output),
          _failedDelegationMessage(),
        ],
      );
      final surfaceSession = _session(
        messages: [userMessage, assistantMessage],
      );
      final h = await _Harness.create(
        initialSession: initialSession,
        surfaceRead: (_) async => _surfaceEnvelope(surfaceSession),
      );
      addTearDown(h.dispose);
      h.transport.proposalPreview = (_) async =>
          _proposalPreviewEnvelope(_readyProposal('Personal calendar'));

      final semantics = tester.ensureSemantics();
      try {
        await tester.pumpWidget(_panelApp(h.controller));
        await tester.pumpAndSettle();

        expect(find.text('Execution details'), findsOneWidget);
        expect(find.text('Source'), findsNothing);
        expect(find.text('View source'), findsNothing);
        expect(find.text('Review proposal'), findsNothing);
        expect(find.text('Add to calendar'), findsNothing);
        expect(find.byType(ExpansionTile), findsOneWidget);
        expect(h.transport.proposalQueries, isEmpty);
        expect(tester.takeException(), isNull);

        await tester.tap(find.text('Execution details'));
        await tester.pumpAndSettle();

        expect(find.text('Source'), findsNWidgets(2));
        expect(find.text('View source'), findsNothing);
        expect(find.byType(ExpansionTile), findsOneWidget);
        expect(find.text('This source is unavailable.'), findsOneWidget);
        expect(h.transport.proposalQueries, isEmpty);
        final sourceHeading = tester.getSemantics(find.text('Source').first);
        final executionDetails = tester.getSemantics(
          find.text('Execution details'),
        );
        expect(sourceHeading.label, 'Source');
        expect(sourceHeading.flagsCollection.isHeader, isTrue);
        expect(executionDetails.label, 'Execution details');
        expect(executionDetails.parent, isNot(same(sourceHeading)));
        final source = find.byWidgetPredicate(
          (widget) => widget is SelectableText && widget.data == output,
        );
        expect(source, findsOneWidget);
        expect(
          tester.getRect(source).right,
          lessThanOrEqualTo(tester.view.physicalSize.width),
        );
        expect(find.text('Review proposal'), findsOneWidget);
        expect(find.text('Add to calendar'), findsNothing);
        expect(tester.takeException(), isNull);

        await tester.ensureVisible(find.text('Review proposal'));
        await tester.tap(find.text('Review proposal'));
        await tester.pumpAndSettle();

        expect(h.transport.proposalQueries, hasLength(1));
        expect(
          h.transport.proposalQueries.single['receipt'],
          _proposalReceipt(),
        );
        expect(h.transport.proposalQueries.single['artifact_id'], _m3);
        expect(find.text('Add to calendar'), findsOneWidget);
        await tester.ensureVisible(find.text('Add to calendar'));
        await tester.pumpAndSettle();
        expect(find.text('Add to calendar').hitTestable(), findsOneWidget);
        expect(h.transport.commands, isEmpty);
        expect(tester.takeException(), isNull);
        await tester.pumpWidget(const SizedBox.shrink());
        await tester.pumpAndSettle();
        expect(tester.takeException(), isNull);
      } finally {
        semantics.dispose();
      }
    },
  );

  testWidgets(
    'exact proposal refresh rejects a different operation and preserves its current card',
    (tester) async {
      final h = await _Harness.create();
      addTearDown(h.dispose);
      h.transport.proposalPreview = (_) async => _proposalPreviewEnvelope({
        'kind': 'existing',
        'action': _proposalOperation(),
      });
      await tester.pumpWidget(
        _proposalApp(h.controller, _proposalMessage(_m3)),
      );
      await tester.tap(find.text('Review proposal'));
      await tester.pumpAndSettle();
      final replacement = _proposalOperation(title: 'Wrong replacement');
      replacement['action_ref'] = _m4;
      replacement['review_ref'] = {
        ...(replacement['review_ref'] as Map),
        'operation_id': _m4,
      };
      h.transport.proposalPreview = (_) async =>
          _proposalPreviewEnvelope({'kind': 'existing', 'action': replacement});
      await tester.tap(find.text('Refresh approval status'));
      await tester.pumpAndSettle();
      expect(find.text('Exact proposal operation'), findsOneWidget);
      expect(find.text('Wrong replacement'), findsNothing);
      expect(find.textContaining('identity changed'), findsWidgets);
      expect(h.transport.commands, isEmpty);
    },
  );

  testWidgets(
    'two artifacts from one Task remain separate across card replacement',
    (tester) async {
      final h = await _Harness.create();
      addTearDown(h.dispose);
      h.transport.proposalPreview = (query) async => _proposalPreviewEnvelope({
        'kind': 'existing',
        'action': _proposalOperation(
          title: query['artifact_id'] == _m3
              ? 'First artifact'
              : 'Second artifact',
        ),
      });
      await tester.pumpWidget(
        _proposalApp(h.controller, _proposalMessage(_m3)),
      );
      await tester.tap(find.text('Review proposal'));
      await tester.pumpAndSettle();
      expect(find.text('First artifact'), findsOneWidget);
      await tester.pumpWidget(
        _proposalApp(h.controller, _proposalMessage(_m4, messageId: _m2)),
      );
      await tester.tap(find.text('Review proposal'));
      await tester.pumpAndSettle();
      expect(find.text('First artifact'), findsNothing);
      expect(find.text('Second artifact'), findsOneWidget);
      expect(h.transport.proposalQueries.map((q) => q['artifact_id']), [
        _m3,
        _m4,
      ]);
      expect(h.transport.commands, isEmpty);
    },
  );

  testWidgets(
    'proposal does not alias a different approval from the same Run',
    (tester) async {
      final h = await _Harness.create();
      addTearDown(h.dispose);
      h.transport.interactionPage = (_) async =>
          interactionPageFixture(_sessionId, [_proposalInteraction()]);
      await h.controller.refreshInteractions();
      h.transport.proposalPreview = (_) async =>
          _proposalPreviewEnvelope(_readyProposal('Exact proposal calendar'));
      await tester.pumpWidget(
        _proposalApp(h.controller, _proposalMessage(_m3)),
      );
      expect(find.text('Review proposal'), findsOneWidget);
      expect(find.text('Unrelated approval'), findsNothing);
      await tester.tap(find.text('Review proposal'));
      await tester.pumpAndSettle();
      expect(find.textContaining('Exact proposal calendar'), findsOneWidget);
      expect(h.transport.proposalQueries.single['receipt'], _proposalReceipt());
      expect(h.transport.proposalQueries.single['artifact_id'], _m3);
      expect(h.transport.commands, isEmpty);
    },
  );

  testWidgets(
    'reopened proposal reads and refreshes its exact operation without an interaction',
    (tester) async {
      final h = await _Harness.create();
      addTearDown(h.dispose);
      var revision = 1;
      h.transport.proposalPreview = (_) async => _proposalPreviewEnvelope({
        'kind': 'existing',
        'action': _proposalOperation(revision: revision++),
      });
      await tester.pumpWidget(
        _proposalApp(h.controller, _proposalMessage(_m3)),
      );
      await tester.tap(find.text('Review proposal'));
      await tester.pumpAndSettle();
      expect(find.text('Exact proposal operation'), findsOneWidget);
      await tester.tap(find.text('Refresh approval status'));
      await tester.pumpAndSettle();
      expect(h.transport.proposalQueries, hasLength(2));
      expect(h.transport.proposalQueries[0], h.transport.proposalQueries[1]);
      expect(h.controller.interactionSnapshots, isEmpty);
      expect(h.transport.commands, isEmpty);
    },
  );

  testWidgets('late proposal preview cannot populate a replacement artifact', (
    tester,
  ) async {
    final h = await _Harness.create();
    addTearDown(h.dispose);
    final old = Completer<Map<String, dynamic>>();
    h.transport.proposalPreview = (query) => query['artifact_id'] == _m3
        ? old.future
        : Future.value(
            _proposalPreviewEnvelope(_readyProposal('New artifact calendar')),
          );
    await tester.pumpWidget(_proposalApp(h.controller, _proposalMessage(_m3)));
    await tester.tap(find.text('Review proposal'));
    await tester.pump();
    await tester.pumpWidget(
      _proposalApp(h.controller, _proposalMessage(_m4, messageId: _m2)),
    );
    await tester.tap(find.text('Review proposal'));
    await tester.pumpAndSettle();
    old.complete(
      _proposalPreviewEnvelope(_readyProposal('Stale artifact calendar')),
    );
    await tester.pumpAndSettle();
    expect(find.textContaining('New artifact calendar'), findsOneWidget);
    expect(find.textContaining('Stale artifact calendar'), findsNothing);
    expect(h.transport.commands, isEmpty);
  });

  testWidgets(
    'proposal preview owner failure does not fall back to generic destinations or submit',
    (tester) async {
      final h = await _Harness.create();
      addTearDown(h.dispose);
      h.transport.proposalPreview = (_) async =>
          throw StateError('proposal evidence revoked');
      await tester.pumpWidget(
        _proposalApp(h.controller, _proposalMessage(_m3)),
      );
      await tester.tap(find.text('Review proposal'));
      await tester.pumpAndSettle();
      expect(find.textContaining('proposal evidence revoked'), findsWidgets);
      expect(h.transport.proposalQueries, hasLength(1));
      expect(h.transport.commands, isEmpty);
    },
  );

  test(
    'proposal preview is a pure typed query and validates response and receipt',
    () async {
      final h = await _Harness.create();
      addTearDown(h.dispose);
      h.transport.proposalPreview = (_) async =>
          _proposalPreviewEnvelope(_readyProposal('Scoped'));
      final preview = await h.controller.previewCalendarProposal(
        receipt: _proposalReceipt(),
        artifactId: _m3,
      );
      expect(preview, isA<ReadyActionProposal>());
      await expectLater(
        () => h.controller.previewCalendarProposal(
          receipt: {..._proposalReceipt(), 'digest': 'bad'},
          artifactId: _m3,
        ),
        throwsFormatException,
      );
      expect(h.transport.proposalQueries, hasLength(1));
      h.transport.proposalPreview = (_) async => {
        'kind': 'wrong',
        'preview': _readyProposal('Scoped'),
      };
      await expectLater(
        h.controller.previewCalendarProposal(
          receipt: _proposalReceipt(),
          artifactId: _m3,
        ),
        throwsFormatException,
      );
      expect(h.transport.commands, isEmpty);
    },
  );

  test('known interaction refresh uses its exact reference without restarting list discovery', () async {
    final h = await _Harness.create();
    addTearDown(h.dispose);
    var reads = 0;
    h.transport.interactionGet = (_) async => {
      'kind': 'interaction',
      ...interactionFixture(id: _m3, sessionId: _sessionId, revision: ++reads),
    };
    await h.controller.ensureInteraction(_m3);
    await h.controller.ensureInteraction(_m3);
    expect(reads, 1);
    final refreshed = await h.controller.ensureInteraction(_m3, reload: true);
    expect(refreshed!.revision, 2);
    expect(h.transport.interactionQueries, isEmpty);
    expect(h.transport.commands, isEmpty);
  });

  testWidgets(
    'visible exact-reference card reloads outside the current interaction page',
    (tester) async {
      final h = await _Harness.create();
      addTearDown(h.dispose);
      var gets = 0;
      h.transport.interactionGet = (_) async {
        gets++;
        return {
          'kind': 'interaction',
          ...interactionFixture(id: _m3, sessionId: _sessionId),
        };
      };
      await tester.pumpWidget(
        MaterialApp(
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          supportedLocales: AppLocalizations.supportedLocales,
          home: Scaffold(
            body: AgentInteractionCard(
              controller: h.controller,
              interactionId: _m3,
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();
      expect(gets, 1);
      await h.controller.refreshInteractions();
      await tester.pumpAndSettle();
      expect(gets, 2);
      expect(h.controller.interactionFor(_m3), isNotNull);
      expect(h.controller.hasMoreInteractions, isFalse);
    },
  );

  testWidgets(
    'failed first interaction page has a retry without blocking Conversation',
    (tester) async {
      _usePanelViewport(tester);
      final h = await _Harness.create();
      addTearDown(h.dispose);
      var calls = 0;
      h.transport.interactionPage = (_) async {
        if (++calls == 1) throw StateError('page temporarily unavailable');
        return interactionPageFixture(_sessionId, []);
      };
      await tester.pumpWidget(_panelApp(h.controller));
      await tester.pumpAndSettle();
      expect(find.text('Retry interaction history'), findsOneWidget);
      expect(h.controller.needsReload, isFalse);
      await tester.tap(find.text('Retry interaction history'));
      await tester.pumpAndSettle();
      expect(calls, 2);
      expect(find.text('Retry interaction history'), findsNothing);
      expect(h.transport.commands, isEmpty);
    },
  );

  test('interaction pages load explicitly and retain newer revisions across live-page overlap', () async {
    final h = await _Harness.create();
    addTearDown(h.dispose);
    var calls = 0;
    h.transport.interactionPage = (query) async {
      calls++;
      if (calls == 1) {
        return interactionPageFixture(_sessionId, [
          interactionFixture(id: _m4, sessionId: _sessionId, revision: 3),
          interactionFixture(id: _m3, sessionId: _sessionId),
        ], more: true);
      }
      expect(query['after'], isNotNull);
      return interactionPageFixture(_sessionId, [
        interactionFixture(
          id: _m4,
          sessionId: _sessionId,
          revision: 2,
          state: 'expired',
        ),
        interactionFixture(id: _m2, sessionId: _sessionId, state: 'expired'),
      ]);
    };
    await h.controller.refreshInteractions();
    expect(calls, 1);
    expect(h.controller.hasMoreInteractions, isTrue);
    expect(h.controller.interactionSnapshots, hasLength(2));
    await h.controller.loadMoreInteractions();
    expect(h.controller.hasMoreInteractions, isFalse);
    expect(h.controller.interactionSnapshots, hasLength(3));
    expect(h.controller.interactionFor(_m4)!.revision, 3);
    await h.controller.loadMoreInteractions();
    expect(calls, 2);
    expect(h.transport.commands, isEmpty);
  });

  test(
    'failed later interaction page retains the window and cursor for retry',
    () async {
      final h = await _Harness.create();
      addTearDown(h.dispose);
      var calls = 0;
      h.transport.interactionPage = (_) async {
        calls++;
        if (calls == 1) {
          return interactionPageFixture(_sessionId, [
            interactionFixture(id: _m3, sessionId: _sessionId),
          ], more: true);
        }
        if (calls == 2) throw StateError('page unavailable');
        return interactionPageFixture(_sessionId, [
          interactionFixture(id: _m2, sessionId: _sessionId),
        ]);
      };
      await h.controller.refreshInteractions();
      await h.controller.loadMoreInteractions();
      expect(h.controller.hasMoreInteractions, isTrue);
      expect(h.controller.interactionSnapshots.single.id, _m3);
      await h.controller.loadMoreInteractions();
      expect(h.controller.interactionSnapshots, hasLength(2));
      expect(h.controller.hasMoreInteractions, isFalse);
    },
  );

  test('duplicate interaction page requests join no extra work and Session replacement fences the reply', () async {
    final h = await _Harness.create();
    addTearDown(h.dispose);
    final response = Completer<Map<String, dynamic>>();
    final started = Completer<void>();
    h.transport.interactionPage = (_) {
      started.complete();
      return response.future;
    };
    final loading = h.controller.refreshInteractions();
    await started.future;
    await h.controller.refreshInteractions();
    expect(h.transport.interactionQueries, hasLength(1));
    h.transport.replacementSession = _session(
      id: _replacementSessionId,
      messages: [_message(_replacementMessageId)],
    );
    await h.controller.load(newSession: true);
    response.complete(
      interactionPageFixture(_sessionId, [
        interactionFixture(id: _m3, sessionId: _sessionId),
      ], more: true),
    );
    await loading;
    expect(h.controller.interactionSnapshots, isEmpty);
    expect(h.controller.hasMoreInteractions, isFalse);
  });

  testWidgets(
    'panel offers explicit load-more for remaining interaction history',
    (tester) async {
      _usePanelViewport(tester);
      final h = await _Harness.create();
      addTearDown(h.dispose);
      var calls = 0;
      h.transport.interactionPage = (_) async {
        calls++;
        return interactionPageFixture(_sessionId, [
          interactionFixture(id: calls == 1 ? _m3 : _m2, sessionId: _sessionId),
        ], more: calls == 1);
      };
      await tester.pumpWidget(_panelApp(h.controller));
      await tester.pumpAndSettle();
      expect(calls, 1);
      final button = find.text('Load more interactions');
      expect(button, findsOneWidget);
      await tester.ensureVisible(button);
      await tester.tap(button);
      await tester.pumpAndSettle();
      expect(calls, 2);
      expect(button, findsNothing);
      expect(h.transport.commands, isEmpty);
    },
  );

  test(
    'earlier pages prepend in order and advance the exclusive cursor',
    () async {
      final harness = await _Harness.create(surfaceSequence: 6);
      addTearDown(harness.dispose);
      harness.transport
        ..enqueuePage(
          _session(messages: [_message(_m3), _message(_m4)], hasEarlier: true),
        )
        ..enqueuePage(_session(messages: [_message(_m1), _message(_m2)]));

      await harness.controller.loadEarlierMessages();

      expect(_messageIds(harness.controller), [_m3, _m4, _m5, _m6]);
      expect(harness.controller.hasEarlierMessages, isTrue);
      expect(harness.controller.earlierFailure, isNull);

      await harness.controller.loadEarlierMessages();

      expect(_messageIds(harness.controller), [_m1, _m2, _m3, _m4, _m5, _m6]);
      expect(harness.controller.hasEarlierMessages, isFalse);
      expect(harness.controller.earlierFailure, isNull);
      expect(
        harness.transport.historyQueries
            .map((query) => (query['through'] as Map)['sequence'])
            .toList(),
        [4, 2],
      );
    },
  );

  testWidgets(
    'failed send does not restore a draft into a replacement panel controller',
    (tester) async {
      _usePanelViewport(tester);

      final original = await _Harness.create();
      final replacement = await _Harness.create(
        sessionId: _replacementSessionId,
      );
      addTearDown(original.dispose);
      addTearDown(replacement.dispose);
      final draft = ConversationComposerDraft();
      addTearDown(draft.dispose);

      await tester.pumpWidget(_panelApp(original.controller, draft: draft));
      await tester.pumpAndSettle();
      await tester.enterText(
        find.byType(TextFormField),
        'old conversation draft',
      );
      await tester.pump();
      await tester.tap(find.byTooltip('Ask Floe'));
      await tester.pump();
      await original.transport.startTurnRequested.future.timeout(
        const Duration(seconds: 1),
      );

      expect(
        tester
            .widget<TextFormField>(find.byType(TextFormField))
            .controller!
            .text,
        'old conversation draft',
      );
      await tester.pumpWidget(_panelApp(replacement.controller, draft: draft));
      await tester.pumpAndSettle();

      original.transport.pendingStartTurn.completeError(
        const AppWireTransportException(
          'synthetic_failure',
          'Synthetic delayed send failure.',
          commandOutcome: CommandOutcome.notAdmitted,
        ),
      );
      await tester.pumpAndSettle();

      expect(replacement.controller.session?.id, _replacementSessionId);
      expect(
        tester
            .widget<TextFormField>(find.byType(TextFormField))
            .controller!
            .text,
        isEmpty,
      );
    },
  );

  testWidgets('failed send restores the draft for its original session', (
    tester,
  ) async {
    _usePanelViewport(tester);
    final harness = await _Harness.create();
    addTearDown(harness.dispose);

    await tester.pumpWidget(_panelApp(harness.controller));
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextFormField), 'keep this draft');
    await tester.pump();
    await tester.tap(find.byTooltip('Ask Floe'));
    await tester.pump();
    await harness.transport.startTurnRequested.future.timeout(
      const Duration(seconds: 1),
    );

    harness.transport.pendingStartTurn.completeError(
      const AppWireTransportException(
        'synthetic_failure',
        'Synthetic delayed send failure.',
        commandOutcome: CommandOutcome.notAdmitted,
      ),
    );
    await tester.pumpAndSettle();

    expect(harness.controller.failure, isNotNull);
    expect(
      tester.widget<TextFormField>(find.byType(TextFormField)).controller!.text,
      'keep this draft',
    );
  });

  testWidgets('failed send while dismissed retains the draft for reopen', (
    tester,
  ) async {
    _usePanelViewport(tester);
    final harness = await _Harness.create();
    addTearDown(harness.dispose);
    final draft = ConversationComposerDraft();
    addTearDown(draft.dispose);

    await tester.pumpWidget(
      _draftPresenceApp(harness.controller, draft: draft),
    );
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextFormField), 'retain on failure');
    await tester.pump();
    await tester.tap(find.byTooltip('Ask Floe'));
    await tester.pump();
    expect(harness.transport.startTurnRequested.isCompleted, isTrue);
    await harness.transport.startTurnRequested.future.timeout(
      const Duration(seconds: 1),
    );

    await tester.tap(find.text('Dismiss panel'));
    await tester.pumpAndSettle();
    harness.transport.pendingStartTurn.completeError(
      const AppWireTransportException(
        'synthetic_failure',
        'Synthetic delayed send failure.',
        commandOutcome: CommandOutcome.notAdmitted,
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Open panel'));
    await tester.pumpAndSettle();

    expect(harness.controller.failure, isNotNull);
    expect(
      tester.widget<TextFormField>(find.byType(TextFormField)).controller!.text,
      'retain on failure',
    );
    expect(
      harness.transport.commands.where(
        (command) => command['kind'] == 'conversation.work.submit_text',
      ),
      hasLength(1),
    );
  });

  testWidgets('accepted send while dismissed clears the stale draft', (
    tester,
  ) async {
    _usePanelViewport(tester);
    final harness = await _Harness.create();
    addTearDown(harness.dispose);
    final draft = ConversationComposerDraft();
    addTearDown(draft.dispose);
    final originalSession = harness.controller.session!;
    harness.transport.terminalSession = _session(
      revision: originalSession.revision + 1,
      messages: [_message(_m5), _message(_m6)],
    );

    await tester.pumpWidget(
      _draftPresenceApp(harness.controller, draft: draft),
    );
    await tester.pumpAndSettle();
    expect(harness.controller.canSend, isTrue);
    await tester.enterText(find.byType(TextFormField), 'accepted before close');
    await tester.pump();
    expect(draft.controller.text, 'accepted before close');
    expect(harness.controller.busy, isFalse);
    expect(
      tester
          .widget<FloeButton>(
            find.byKey(const Key('conversation-composer-action')),
          )
          .onPressed,
      isNotNull,
    );
    await tester.tap(find.byTooltip('Ask Floe'));
    await tester.pump();
    expect(harness.transport.startTurnRequested.isCompleted, isTrue);
    expect(
      harness.transport.commands.where(
        (command) => command['kind'] == 'conversation.work.submit_text',
      ),
      hasLength(1),
    );

    await tester.tap(find.text('Dismiss panel'));
    await tester.pumpAndSettle();
    harness.transport.returnTerminalFailure = true;
    harness.transport.completePendingStartTurnAsAccepted();
    await tester.pumpAndSettle();
    await tester.tap(find.text('Open panel'));
    await tester.pumpAndSettle();

    expect(harness.transport.admittedText, 'accepted before close');
    expect(
      tester.widget<TextFormField>(find.byType(TextFormField)).controller!.text,
      isEmpty,
    );
    expect(
      harness.transport.commands.where(
        (command) => command['kind'] == 'conversation.work.submit_text',
      ),
      hasLength(1),
    );
  });

  test('accepted completion does not clear a newer draft revision', () async {
    final harness = await _Harness.create();
    addTearDown(harness.dispose);
    final draft = ConversationComposerDraft();
    addTearDown(draft.dispose);
    draft.reconcile(harness.controller);
    draft.controller.text = 'submitted text';
    final submission = draft.capture(harness.controller)!;
    expect(submission.text, 'submitted text');

    draft.controller.text = 'intermediate edit';
    draft.controller.text = 'submitted text';

    expect(draft.clearAccepted(submission), isFalse);
    expect(draft.controller.text, 'submitted text');

    final latestSubmission = draft.capture(harness.controller)!;
    draft.controller.selection = const TextSelection.collapsed(offset: 3);
    expect(draft.clearAccepted(latestSubmission), isTrue);
    expect(draft.controller.text, isEmpty);
  });

  testWidgets(
    'accepted Work failure does not restore an already submitted draft',
    (tester) async {
      _usePanelViewport(tester);
      final harness = await _Harness.create();
      addTearDown(harness.dispose);
      final originalSession = harness.controller.session!;
      harness.transport
        ..returnTerminalFailure = true
        ..terminalSession = _session(
          revision: originalSession.revision + 1,
          messages: [_message(_m5), _message(_m6)],
        );

      await tester.pumpWidget(_panelApp(harness.controller));
      await tester.pumpAndSettle();
      await tester.enterText(find.byType(TextFormField), 'keep terminal draft');
      await tester.pump();
      await tester.tap(find.byTooltip('Ask Floe'));
      await tester.pump();
      await harness.transport.startTurnRequested.future.timeout(
        const Duration(seconds: 1),
      );
      await tester.pumpAndSettle();

      expect(harness.controller.failure, 'model_unavailable');
      expect(harness.controller.session?.id, originalSession.id);
      expect(
        harness.controller.session?.revision,
        originalSession.revision + 1,
      );
      expect(identical(originalSession, harness.controller.session), isFalse);
      expect(
        tester
            .widget<TextFormField>(find.byType(TextFormField))
            .controller!
            .text,
        isEmpty,
      );
    },
  );

  testWidgets('dismissed panel restores exact multiline draft and selection', (
    tester,
  ) async {
    _usePanelViewport(tester);
    final harness = await _Harness.create();
    addTearDown(harness.dispose);
    final draft = ConversationComposerDraft();
    addTearDown(draft.dispose);

    await tester.pumpWidget(
      _draftPresenceApp(harness.controller, draft: draft),
    );
    await tester.pumpAndSettle();
    const text = 'First line\nSecond line';
    await tester.enterText(find.byType(TextFormField), text);
    final selection = const TextSelection(baseOffset: 4, extentOffset: 16);
    draft.controller.selection = selection;
    await tester.pump();

    await tester.tap(find.text('Dismiss panel'));
    await tester.pumpAndSettle();
    expect(find.byType(AgentPanel), findsNothing);

    await tester.tap(find.text('Open panel'));
    await tester.pumpAndSettle();
    final restored = tester
        .widget<TextFormField>(find.byType(TextFormField))
        .controller!;
    expect(restored.text, text);
    expect(restored.selection.baseOffset, selection.baseOffset);
    expect(restored.selection.extentOffset, selection.extentOffset);
    expect(harness.transport.commands, isEmpty);
    expect(tester.takeException(), isNull);
  });

  testWidgets('dismissed draft is cleared after the session changes', (
    tester,
  ) async {
    _usePanelViewport(tester);
    final harness = await _Harness.create();
    addTearDown(harness.dispose);
    final draft = ConversationComposerDraft();
    addTearDown(draft.dispose);

    await tester.pumpWidget(
      _draftPresenceApp(harness.controller, draft: draft),
    );
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextFormField), 'old session draft');
    await tester.tap(find.text('Dismiss panel'));
    await tester.pumpAndSettle();

    harness.transport.replacementSession = _session(
      id: _replacementSessionId,
      messages: [_message(_replacementMessageId)],
    );
    await harness.controller.load(newSession: true);
    expect(harness.controller.session?.id, _replacementSessionId);
    final commandsBeforeOpen = harness.transport.commands.length;

    await tester.tap(find.text('Open panel'));
    await tester.pumpAndSettle();
    expect(
      tester.widget<TextFormField>(find.byType(TextFormField)).controller!.text,
      isEmpty,
    );
    expect(harness.transport.commands, hasLength(commandsBeforeOpen));
    expect(tester.takeException(), isNull);
  });

  testWidgets('dismissed draft is cleared for a replacement controller', (
    tester,
  ) async {
    _usePanelViewport(tester);
    final original = await _Harness.create();
    final replacement = await _Harness.create();
    addTearDown(original.dispose);
    addTearDown(replacement.dispose);
    final draft = ConversationComposerDraft();
    addTearDown(draft.dispose);
    const hostKey = ValueKey('draft-presence-host');

    await tester.pumpWidget(
      _draftPresenceApp(original.controller, draft: draft, key: hostKey),
    );
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextFormField), 'old owner draft');
    await tester.tap(find.text('Dismiss panel'));
    await tester.pumpAndSettle();

    await tester.pumpWidget(
      _draftPresenceApp(replacement.controller, draft: draft, key: hostKey),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Open panel'));
    await tester.pumpAndSettle();
    expect(
      tester.widget<TextFormField>(find.byType(TextFormField)).controller!.text,
      isEmpty,
    );
    expect(replacement.transport.commands, isEmpty);
    expect(tester.takeException(), isNull);
  });

  testWidgets('Load earlier shows pending and ignores repeated taps', (
    tester,
  ) async {
    _usePanelViewport(tester);
    final harness = await _Harness.create();
    addTearDown(harness.dispose);
    final response = Completer<Map<String, dynamic>>();
    final requested = Completer<void>();
    harness.transport.enqueuePendingPage(response, requested);

    await tester.pumpWidget(_panelApp(harness.controller));
    await tester.pumpAndSettle();
    final loadEarlier = find.text('Load earlier messages');
    await tester.tap(loadEarlier);
    await tester.tap(loadEarlier);
    await requested.future.timeout(const Duration(seconds: 1));
    await tester.pump();

    expect(find.text('Loading earlier messages…'), findsOneWidget);
    expect(harness.controller.loadingEarlier, isTrue);
    expect(harness.transport.historyQueries, hasLength(1));

    response.complete(
      _sessionEnvelope(_session(messages: [_message(_m3), _message(_m4)])),
    );
    await tester.pumpAndSettle();
    expect(_messageIds(harness.controller), [_m3, _m4, _m5, _m6]);
    expect(harness.transport.commands, isEmpty);
  });

  testWidgets(
    'overlapping history error is visible and keeps messages intact',
    (tester) async {
      _usePanelViewport(tester);
      final harness = await _Harness.create();
      addTearDown(harness.dispose);
      final before = _messageIds(harness.controller);
      harness.transport.enqueuePage(
        _session(messages: [_message(_m1), _message(_m6)]),
      );

      await tester.pumpWidget(_panelApp(harness.controller));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Load earlier messages'));
      await tester.pumpAndSettle();

      expect(
        find.text('Earlier messages could not be loaded. Try again.'),
        findsOneWidget,
      );
      expect(_messageIds(harness.controller), before);
      expect(find.text('synthetic $_m5'), findsOneWidget);
      expect(find.text('synthetic $_m6'), findsOneWidget);
      expect(harness.transport.commands, isEmpty);
    },
  );

  testWidgets('unmount during history read leaves it alive without dispatch', (
    tester,
  ) async {
    _usePanelViewport(tester);
    final harness = await _Harness.create();
    addTearDown(harness.dispose);
    final response = Completer<Map<String, dynamic>>();
    final requested = Completer<void>();
    final draft = ConversationComposerDraft();
    addTearDown(draft.dispose);
    harness.transport.enqueuePendingPage(response, requested);
    var panelVisible = true;

    await tester.pumpWidget(
      MaterialApp(
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: Scaffold(
          body: StatefulBuilder(
            builder: (context, setState) => panelVisible
                ? AgentPanel(
                    controller: harness.controller,
                    composerDraft: draft,
                    dayGateway: _UnusedDayGateway(),
                    onClose: () => setState(() => panelVisible = false),
                  )
                : const SizedBox.shrink(),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Load earlier messages'));
    await requested.future.timeout(const Duration(seconds: 1));
    await tester.pump();

    expect(response.isCompleted, isFalse);
    await tester.tap(find.byTooltip('Close'));
    await tester.pump();
    expect(find.byType(AgentPanel), findsNothing);

    response.complete(
      _sessionEnvelope(_session(messages: [_message(_m3), _message(_m4)])),
    );
    await tester.pumpAndSettle();

    expect(harness.transport.historyQueries, hasLength(1));
    expect(harness.transport.commands, isEmpty);
    expect(_messageIds(harness.controller), [_m3, _m4, _m5, _m6]);
  });

  testWidgets('controller replacement fences a pending history callback', (
    tester,
  ) async {
    _usePanelViewport(tester);
    final original = await _Harness.create();
    final replacement = await _Harness.create(sessionId: _replacementSessionId);
    addTearDown(original.dispose);
    addTearDown(replacement.dispose);
    final oldResponse = Completer<Map<String, dynamic>>();
    final oldRequested = Completer<void>();
    final newResponse = Completer<Map<String, dynamic>>();
    final newRequested = Completer<void>();
    original.transport.enqueuePendingPage(oldResponse, oldRequested);
    replacement.transport.enqueuePendingPage(newResponse, newRequested);

    await tester.pumpWidget(_panelApp(original.controller));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Load earlier messages'));
    await oldRequested.future.timeout(const Duration(seconds: 1));
    await tester.pump();

    await tester.pumpWidget(_panelApp(replacement.controller));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Load earlier messages'));
    await newRequested.future.timeout(const Duration(seconds: 1));
    await tester.pump();

    oldResponse.complete(
      _sessionEnvelope(_session(messages: [_message(_m3), _message(_m4)])),
    );
    await tester.pumpAndSettle();

    expect(find.text('Loading earlier messages…'), findsOneWidget);
    expect(replacement.controller.loadingEarlier, isTrue);
    expect(replacement.transport.historyQueries, hasLength(1));
    expect(replacement.controller.session?.id, _replacementSessionId);
    expect(_messageIds(replacement.controller), [_m5, _m6]);

    newResponse.complete(
      _sessionEnvelope(
        _session(
          id: _replacementSessionId,
          messages: [_message(_m1), _message(_m2)],
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(_messageIds(replacement.controller), [_m1, _m2, _m5, _m6]);
    expect(original.controller.earlierFailure, isNull);
  });

  test(
    'overlapping history page is rejected without duplicating messages',
    () async {
      final harness = await _Harness.create();
      addTearDown(harness.dispose);
      final before = _messageIds(harness.controller);
      // m6 overlaps the current window but is beyond the exclusive m5 cursor.
      harness.transport.enqueuePage(
        _session(messages: [_message(_m1), _message(_m6)]),
      );

      await harness.controller.loadEarlierMessages();

      expect(harness.controller.earlierFailure, isNotNull);
      expect(_messageIds(harness.controller), before);
      expect(_messageIds(harness.controller).toSet(), hasLength(before.length));
    },
  );

  test(
    'an empty earlier page cannot claim that more history remains',
    () async {
      final harness = await _Harness.create();
      addTearDown(harness.dispose);
      final before = _messageIds(harness.controller);
      harness.transport.enqueuePage(_session(messages: [], hasEarlier: true));

      await harness.controller.loadEarlierMessages();

      expect(harness.controller.earlierFailure, isNotNull);
      expect(_messageIds(harness.controller), before);
      expect(harness.controller.hasEarlierMessages, isTrue);
    },
  );

  for (final scenario in <_RejectedPageScenario>[
    _RejectedPageScenario(
      'malformed message window',
      _session(messages: [_message(_m1), _message(_m1)]),
    ),
    _RejectedPageScenario(
      'wrong session identity',
      _session(id: _otherSessionId, messages: [_message(_m1)]),
    ),
    _RejectedPageScenario(
      'foreign-person metadata in a Surface response',
      _session(personId: _otherPersonId, messages: [_message(_m1)]),
    ),
  ]) {
    test('rejects ${scenario.name} history page', () async {
      final harness = await _Harness.create();
      addTearDown(harness.dispose);
      final before = _messageIds(harness.controller);
      harness.transport.enqueuePage(scenario.response);

      await harness.controller.loadEarlierMessages();

      expect(harness.controller.earlierFailure, isNotNull);
      expect(_messageIds(harness.controller), before);
    });
  }

  test(
    'session replacement fences a pending earlier-page completion',
    () async {
      final harness = await _Harness.create();
      addTearDown(harness.dispose);
      final response = Completer<Map<String, dynamic>>();
      final requested = Completer<void>();
      harness.transport.enqueuePendingPage(response, requested);
      final earlierLoad = harness.controller.loadEarlierMessages();
      await requested.future.timeout(const Duration(seconds: 1));

      harness.transport.replacementSession = _session(
        id: _replacementSessionId,
        messages: [_message(_replacementMessageId)],
      );
      await harness.controller.load(newSession: true);
      expect(harness.controller.session?.id, _replacementSessionId);

      response.complete(
        _sessionEnvelope(_session(messages: [_message(_m1), _message(_m2)])),
      );
      await earlierLoad;

      expect(harness.controller.session?.id, _replacementSessionId);
      expect(_messageIds(harness.controller), [_replacementMessageId]);
      expect(harness.controller.earlierFailure, isNull);
    },
  );

  test(
    'runtime invalidation fences a pending earlier-page completion',
    () async {
      final harness = await _Harness.create();
      addTearDown(harness.dispose);
      final response = Completer<Map<String, dynamic>>();
      final requested = Completer<void>();
      harness.transport.enqueuePendingPage(response, requested);
      var notifications = 0;
      harness.controller.addListener(() => notifications++);
      final earlierLoad = harness.controller.loadEarlierMessages();
      await requested.future.timeout(const Duration(seconds: 1));

      harness.runtime.closeAdmission();
      expect(harness.controller.session, isNull);
      expect(harness.controller.messages, isEmpty);
      final notificationsAfterInvalidation = notifications;

      response.complete(
        _sessionEnvelope(_session(messages: [_message(_m1), _message(_m2)])),
      );
      await earlierLoad;

      expect(harness.controller.session, isNull);
      expect(harness.controller.messages, isEmpty);
      expect(harness.controller.earlierFailure, isNull);
      expect(notifications, notificationsAfterInvalidation);
    },
  );

  test(
    'dispose during history read prevents completion notification',
    () async {
      final harness = await _Harness.create();
      addTearDown(harness.dispose);
      final response = Completer<Map<String, dynamic>>();
      final requested = Completer<void>();
      harness.transport.enqueuePendingPage(response, requested);
      var notifications = 0;
      harness.controller.addListener(() => notifications++);
      final earlierLoad = harness.controller.loadEarlierMessages();
      await requested.future.timeout(const Duration(seconds: 1));

      harness.disposeController();
      final notificationsAfterDispose = notifications;
      response.complete(_sessionEnvelope(_session(messages: [_message(_m1)])));
      await earlierLoad;

      expect(notifications, notificationsAfterDispose);
    },
  );

  test(
    'detach and attach keep a history read alive without model dispatch',
    () async {
      final harness = await _Harness.create();
      addTearDown(harness.dispose);
      final response = Completer<Map<String, dynamic>>();
      final requested = Completer<void>();
      harness.transport.enqueuePendingPage(response, requested);
      final earlierLoad = harness.controller.loadEarlierMessages();
      await requested.future.timeout(const Duration(seconds: 1));

      harness.controller.detachView();
      harness.controller.attachView();
      expect(harness.transport.historyQueries, hasLength(1));
      expect(response.isCompleted, isFalse);
      expect(harness.transport.commands, isEmpty);

      response.complete(
        _sessionEnvelope(_session(messages: [_message(_m3), _message(_m4)])),
      );
      await earlierLoad;

      expect(harness.controller.earlierFailure, isNull);
      expect(_messageIds(harness.controller), [_m3, _m4, _m5, _m6]);
      expect(harness.transport.commands, isEmpty);
      expect(harness.transport.historyQueries, hasLength(1));
    },
  );
}

void _usePanelViewport(WidgetTester tester) {
  tester.view.physicalSize = const Size(1000, 1000);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
}

Widget _panelApp(
  ConversationController controller, {
  ConversationComposerDraft? draft,
  VoidCallback? onClose,
}) => MaterialApp(
  localizationsDelegates: AppLocalizations.localizationsDelegates,
  supportedLocales: AppLocalizations.supportedLocales,
  home: _PanelAppOwner(
    controller: controller,
    composerDraft: draft,
    onClose: onClose ?? () {},
  ),
);

final class _PanelAppOwner extends StatefulWidget {
  const _PanelAppOwner({
    required this.controller,
    required this.composerDraft,
    required this.onClose,
  });

  final ConversationController controller;
  final ConversationComposerDraft? composerDraft;
  final VoidCallback onClose;

  @override
  State<_PanelAppOwner> createState() => _PanelAppOwnerState();
}

final class _PanelAppOwnerState extends State<_PanelAppOwner> {
  ConversationComposerDraft? _ownedDraft;

  ConversationComposerDraft get _composerDraft =>
      widget.composerDraft ?? (_ownedDraft ??= ConversationComposerDraft());

  @override
  void didUpdateWidget(_PanelAppOwner oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.composerDraft != widget.composerDraft) {
      _ownedDraft?.dispose();
      _ownedDraft = null;
    }
  }

  @override
  void dispose() {
    _ownedDraft?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => Scaffold(
    body: AgentPanel(
      controller: widget.controller,
      composerDraft: _composerDraft,
      dayGateway: _UnusedDayGateway(),
      onClose: widget.onClose,
    ),
  );
}

Widget _draftPresenceApp(
  ConversationController controller, {
  required ConversationComposerDraft draft,
  Key? key,
}) => MaterialApp(
  localizationsDelegates: AppLocalizations.localizationsDelegates,
  supportedLocales: AppLocalizations.supportedLocales,
  home: _DraftPresenceHost(
    key: key,
    controller: controller,
    composerDraft: draft,
  ),
);

final class _DraftPresenceHost extends StatefulWidget {
  const _DraftPresenceHost({
    super.key,
    required this.controller,
    required this.composerDraft,
  });

  final ConversationController controller;
  final ConversationComposerDraft composerDraft;

  @override
  State<_DraftPresenceHost> createState() => _DraftPresenceHostState();
}

final class _DraftPresenceHostState extends State<_DraftPresenceHost> {
  bool _visible = true;

  void _close() {
    widget.controller.detachView();
    setState(() => _visible = false);
  }

  @override
  Widget build(BuildContext context) => Scaffold(
    body: Column(
      children: [
        Row(
          children: [
            TextButton(
              onPressed: _visible ? _close : null,
              child: const Text('Dismiss panel'),
            ),
            TextButton(
              onPressed: _visible
                  ? null
                  : () => setState(() => _visible = true),
              child: const Text('Open panel'),
            ),
          ],
        ),
        Expanded(
          child: FloePanelPresence(
            visible: _visible,
            child: AgentPanel(
              controller: widget.controller,
              composerDraft: widget.composerDraft,
              dayGateway: _UnusedDayGateway(),
              onClose: _close,
            ),
          ),
        ),
      ],
    ),
  );
}

final class _Harness {
  _Harness(this.runtime, this.readModel, this.transport, this.controller);

  final RuntimeController runtime;
  final AppReadModel readModel;
  final _ConversationTransport transport;
  final ConversationController controller;
  bool _disposed = false;
  bool _controllerDisposed = false;

  static Future<_Harness> create({
    String sessionId = _sessionId,
    int surfaceSequence = 4,
    Future<void> Function()? beforeAdmission,
    Map<String, Object?>? initialSession,
    Future<Map<String, dynamic>> Function(Map<String, dynamic>)? surfaceRead,
  }) async {
    final runtime = RuntimeController(
      gateway: _ReadyRuntimeGateway(),
      personId: _personId,
    );
    await runtime.open();
    final readModel = AppReadModel();
    final transport = _ConversationTransport(
      initialSession ??
          _session(
            id: sessionId,
            messages: [_message(_m5), _message(_m6)],
            hasEarlier: true,
          ),
      surfaceSequence: surfaceSequence,
    );
    transport.surfaceRead = surfaceRead;
    final client = AppWireConversationClient(transport, newId: _nextRequestId);
    final gateway = AppWireConversationGateway(
      transport,
      runtimeClient: client,
      readModel: readModel,
      beforeConversationStart: beforeAdmission,
    );
    final controller = ConversationController(
      gateway: gateway,
      personId: _personId,
      owners: LocalOwnerGateways(runtime: runtime),
    );
    final harness = _Harness(runtime, readModel, transport, controller);
    await controller.load();
    return harness;
  }

  void dispose() {
    if (_disposed) return;
    _disposed = true;
    disposeController();
    runtime.dispose();
    readModel.dispose();
  }

  void disposeController() {
    if (_controllerDisposed) return;
    _controllerDisposed = true;
    controller.dispose();
  }
}

final class _ConversationTransport implements AppWireTransport {
  _ConversationTransport(this.initialSession, {this.surfaceSequence = 4});
  final int surfaceSequence;
  Map<String, dynamic>? admittedWork;
  List<Map<String, dynamic>> extraWorks = [];
  String? admittedText;

  final Map<String, Object?> initialSession;
  Map<String, Object?>? replacementSession;
  final List<Map<String, dynamic>> proposalQueries = [];
  Future<Map<String, dynamic>> Function(Map<String, dynamic>)? proposalPreview;
  Future<Map<String, dynamic>> Function(Map<String, dynamic>)? surfaceRead;
  final List<Map<String, dynamic>> historyQueries = [];
  final List<Map<String, dynamic>> interactionQueries = [];
  Future<Map<String, dynamic>> Function(Map<String, dynamic>)? interactionPage;
  Future<Map<String, dynamic>> Function(Map<String, dynamic>)? interactionGet;
  final List<Map<String, dynamic>> commands = [];
  final List<String> commandIds = [];
  final List<String> runQueries = [];
  final Completer<void> startTurnRequested = Completer<void>();
  final Completer<Map<String, dynamic>> pendingStartTurn =
      Completer<Map<String, dynamic>>();
  bool returnTerminalFailure = false;
  bool workingRun = false;
  Map<String, Object?>? terminalSession;
  int _startTurnRequests = 0;
  final List<Future<Map<String, dynamic>> Function()> _historyReplies = [];

  void enqueuePage(Map<String, Object?> session) {
    _historyReplies.add(() async => _sessionEnvelope(session));
  }

  void enqueuePendingPage(
    Completer<Map<String, dynamic>> response,
    Completer<void> requested,
  ) {
    _historyReplies.add(() {
      if (!requested.isCompleted) requested.complete();
      return response.future;
    });
  }

  void completePendingStartTurnAsAccepted() {
    final command = commands.lastWhere(
      (command) => command['kind'] == 'conversation.work.submit_text',
    );
    final expected = Map<String, dynamic>.from(command['expected'] as Map);
    final commandId = commandIds.last;
    admittedText = command['text'] as String;
    admittedWork = {
      'command_id': commandId,
      'work_id': _m4,
      'execution_command_id': _turnId,
      'input_id': commandId,
      'anchor': _surfaceAnchor(
        expected['session_id'] as String,
        (expected['sequence'] as int) + 1,
      ),
      'origin': {'kind': 'text'},
      'runtime_epoch': 1,
    };
    pendingStartTurn.complete({
      'kind': 'conversation.work.receipt',
      'receipt': admittedWork,
    });
  }

  @override
  Future<Map<String, dynamic>> queryV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) async {
    final query = Map<String, dynamic>.from(request['query'] as Map);
    switch (query['kind']) {
      case 'conversation.calendar_proposal.preview':
        proposalQueries.add(query);
        return proposalPreview!(query);
      case 'conversation.surface.get':
        if (surfaceRead case final read?) {
          return read(query);
        }
        if (query['through'] != null) {
          historyQueries.add(query);
          if (_historyReplies.isEmpty) {
            throw StateError('No synthetic Surface page was queued.');
          }
          final result = await _historyReplies.removeAt(0)();
          return _surfaceEnvelope(
            Map<String, Object?>.from(result['session'] as Map),
            through: Map<String, dynamic>.from(query['through'] as Map),
          );
        }
        final selected =
            replacementSession != null &&
                replacementSession!['id'] == query['session_id']
            ? replacementSession!
            : initialSession;
        final fixture = Map<String, Object?>.from(selected);
        var head = selected['has_earlier_messages'] == true
            ? surfaceSequence
            : (selected['messages'] as List).length;
        if (admittedWork != null &&
            (admittedWork!['anchor'] as Map)['session_id'] ==
                query['session_id']) {
          fixture['messages'] = [
            ...(selected['messages'] as List),
            {
              'kind': 'user',
              'message_id': admittedWork!['input_id'],
              'text': admittedText,
            },
          ];
          head++;
        }
        return _surfaceEnvelope(fixture, head: head);
      case 'conversation.work.list':
        return {
          'kind': 'conversation.work.page',
          'page': {
            'items': [if (admittedWork != null) _workSnapshot(), ...extraWorks],
            'next': null,
          },
        };
      case 'conversation.work.get':
        return {
          'kind': 'conversation.work',
          'snapshot': admittedWork?['command_id'] == query['command_id']
              ? _workSnapshot()
              : null,
        };
      case 'conversation.session.resume':
        return _sessionEnvelope(initialSession);
      case 'conversation.session.get':
        if (terminalSession != null &&
            query['session_id'] == terminalSession!['id']) {
          return _sessionEnvelope(terminalSession!);
        }
        throw StateError('Unexpected Conversation session query: $query');
      case 'conversation.get_run':
        runQueries.add(query['run_id'] as String);
        if (returnTerminalFailure && query['run_id'] == _terminalRunId) {
          final run = _failedRunEnvelope(query['run_id'] as String);
          if (workingRun) {
            run['state'] = 'executing';
            run['progress'] = 'model';
            run['report'] = null;
          }
          return run;
        }
        throw StateError('Unexpected Conversation Run query: $query');
      case 'conversation.interaction.get':
        return interactionGet!(query);
      case 'conversation.interaction.list':
        interactionQueries.add(query);
        final handler = interactionPage;
        if (handler != null) return handler(query);
        return {
          'kind': 'interaction_list',
          'next_cursor': null,
          'session_id': query['session_id'],
          'interactions': const <Map<String, dynamic>>[],
        };
      case 'conversation.get_command':
        return {'kind': 'unknown_command', 'command_id': query['command_id']};
      default:
        throw StateError('Unexpected AppWire query: $query');
    }
  }

  @override
  Future<Map<String, dynamic>> eventsV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) async {
    if (!request.containsKey('cursor')) {
      return {
        'kind': 'resync_required',
        'runtime_epoch': 1,
        'snapshot_cursor': 0,
      };
    }
    return {
      'kind': 'events',
      'runtime_epoch': request['runtime_epoch'],
      'next_cursor': request['cursor'],
      'events': const <Map<String, dynamic>>[],
    };
  }

  @override
  Future<Map<String, dynamic>> commandV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) async {
    final command = Map<String, dynamic>.from(request['command'] as Map);
    commands.add(command);
    commandIds.add(request['command_id'] as String);
    if (command['kind'] == 'conversation.session.start') {
      return _sessionEnvelope(replacementSession ?? initialSession);
    }
    if (command['kind'] == 'conversation.work.submit_text') {
      _startTurnRequests++;
      if (returnTerminalFailure) {
        if (!startTurnRequested.isCompleted) startTurnRequested.complete();
        final expected = Map<String, dynamic>.from(command['expected'] as Map);
        admittedText = command['text'] as String;
        admittedWork = {
          'command_id': request['command_id'],
          'work_id': _m4,
          'execution_command_id': _turnId,
          'input_id': request['command_id'],
          'anchor': _surfaceAnchor(
            expected['session_id'] as String,
            (expected['sequence'] as int) + 1,
          ),
          'origin': {'kind': 'text'},
          'runtime_epoch': 1,
        };
        return {'kind': 'conversation.work.receipt', 'receipt': admittedWork};
      }
      if (_startTurnRequests == 1) {
        startTurnRequested.complete();
        return pendingStartTurn.future;
      }
      throw const AppWireTransportException(
        'synthetic_failure',
        'Synthetic delayed send failure.',
        commandOutcome: CommandOutcome.notAdmitted,
      );
    }
    throw StateError('Unexpected Conversation command: $command');
  }

  Map<String, dynamic> _workSnapshot() => {
    'receipt': admittedWork,
    'input_text': admittedText,
    'state': {'kind': 'linked', 'run_id': _terminalRunId},
    'current_run_id': _terminalRunId,
  };

  @override
  Future<void> close() async {}
}

final class _UnusedDayGateway implements DayGateway {
  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

final class _ReadyRuntimeGateway implements RuntimeGateway {
  @override
  Future<RuntimeReadinessSnapshot> readiness(String requestId) async =>
      const RuntimeReadinessSnapshot(state: RuntimeReadinessState.ready);

  @override
  Future<RuntimePreparationResult> prepare(String operationId) async =>
      RuntimePreparationResult(operationId: operationId, done: true);

  @override
  Future<RuntimePreparationResult> getPreparation(String operationId) async =>
      RuntimePreparationResult(operationId: operationId, done: true);

  @override
  Future<RuntimePreparationResult> acknowledge(String operationId) async =>
      RuntimePreparationResult(operationId: operationId, done: true);
}

final class _RejectedPageScenario {
  const _RejectedPageScenario(this.name, this.response);

  final String name;
  final Map<String, Object?> response;
}

List<String> _messageIds(ConversationController controller) =>
    controller.surfaceMessages.map((message) => message.messageId).toList();

// Synthetic codec boundaries only; Rust/Vault tests establish actual chain hashes.
Map<String, Object?> _surfaceAnchor(String session, int sequence) => {
  'session_id': session,
  'sequence': sequence,
  'digest': sequence.toRadixString(16).padLeft(64, '0'),
};
Map<String, dynamic> _surfaceEnvelope(
  Map<String, Object?> session, {
  Map<String, dynamic>? through,
  int? head,
}) {
  final entries = (session['messages'] as List).cast<Map<String, Object?>>();
  final sessionId = session['id'] as String;
  final sequence = through?['sequence'] as int? ?? head ?? entries.length;
  final first = sequence - entries.length;
  return {
    'kind': 'conversation.surface',
    'snapshot': {
      'through': _surfaceAnchor(sessionId, sequence),
      'older': session['has_earlier_messages'] == true
          ? _surfaceAnchor(sessionId, first)
          : null,
      if (session['person_id'] != _personId) 'person_id': session['person_id'],
      'messages': [
        for (var i = 0; i < entries.length; i++)
          {
            'message_id': entries[i]['message_id'],
            'before': _surfaceAnchor(sessionId, first + i),
            'through': _surfaceAnchor(sessionId, first + i + 1),
            'content': {
              'kind': 'input',
              'text': entries[i]['text'],
              'origin': 'text',
            },
          },
      ],
    },
  };
}

Map<String, dynamic> _sessionEnvelope(Map<String, Object?> session) => {
  'kind': 'conversation_session',
  'session': session,
};

Map<String, dynamic> _failedRunEnvelope(String runId) => {
  'kind': 'run_snapshot',
  'run_id': runId,
  'session_id': _sessionId,
  'revision': 1,
  'runtime_epoch': 1,
  'executor_generation': 1,
  'state': 'finished',
  'progress': 'failed',
  'task_refs': const <String>[],
  'attempt_refs': const <String>[],
  'report': {
    'execution': 'failed',
    'reply': 'not_produced',
    'issues': const [
      {'code': 'model_unavailable', 'message': 'Synthetic model failure.'},
    ],
    'action_refs': const <String>[],
    'interaction_refs': const <String>[],
    'final_message_ref': null,
  },
};

Map<String, Object?> _session({
  String id = _sessionId,
  String personId = _personId,
  int revision = 12,
  required List<Map<String, Object?>> messages,
  bool hasEarlier = false,
}) => {
  'id': id,
  'person_id': personId,
  'revision': revision,
  'usage': {
    'unknown_token_attempts': 0,
    'unknown_cost_attempts': 0,
    'model_attempts': 0,
    'estimated_tokens': 0,
    'iterations': 0,
    'capability_calls': 0,
    'tokens': 0,
    'cost_micros': 0,
    'estimated_cost_micros': 0,
  },
  'messages': messages,
  'has_earlier_messages': hasEarlier,
};

Map<String, Object?> _message(String id) => {
  'kind': 'user',
  'message_id': id,
  'turn_id': _turnId,
  'text': 'synthetic $id',
};

int _requestNumber = 1000;
String _nextRequestId() =>
    '00000000-0000-4000-8000-${(_requestNumber++).toString().padLeft(12, '0')}';

const _personId = '00000000-0000-4000-8000-000000000001';
const _otherPersonId = '00000000-0000-4000-8000-000000000002';
const _sessionId = '00000000-0000-4000-8000-000000000010';
const _otherSessionId = '00000000-0000-4000-8000-000000000011';
const _replacementSessionId = '00000000-0000-4000-8000-000000000012';
const _turnId = '00000000-0000-4000-8000-000000000020';
const _m1 = '00000000-0000-4000-8000-000000000101';
const _m2 = '00000000-0000-4000-8000-000000000102';
const _m3 = '00000000-0000-4000-8000-000000000103';
const _m4 = '00000000-0000-4000-8000-000000000104';
const _m5 = '00000000-0000-4000-8000-000000000105';
const _m6 = '00000000-0000-4000-8000-000000000106';
const _replacementMessageId = '00000000-0000-4000-8000-000000000107';
const _terminalRunId = '00000000-0000-4000-8000-000000000108';

Map<String, Object?> _proposalReceipt() => {
  'execution': {'task_id': _m1, 'execution_id': _m2, 'executor_generation': 1},
  'task_revision': 2,
  'journal_revision': 2,
  'digest': 'a' * 64,
};
Map<String, Object?> _proposalDelegationMessage({
  required String output,
  String artifact = _m3,
  String messageId = _m1,
}) => {
  'kind': 'delegation',
  'message_id': messageId,
  'turn_id': '00000000-0000-4000-8000-000000000099',
  'task': {
    'execution_receipt': _proposalReceipt(),
    'task_id': _m1,
    'agent_id': 'scheduler',
    'state': 'completed',
    'result': output,
    'artifacts': [
      {
        'artifact_id': artifact,
        'name': 'Schedule',
        'media_types': [
          'application/vnd.floe.actions.calendar-proposal+json;version=1',
        ],
      },
    ],
  },
};

Map<String, Object?> _failedDelegationMessage() => {
  'kind': 'delegation',
  'message_id': _replacementMessageId,
  'turn_id': _turnId,
  'task': {
    'execution_receipt': {
      'execution': {
        'task_id': _m4,
        'execution_id': _replacementMessageId,
        'executor_generation': 1,
      },
      'task_revision': 2,
      'journal_revision': 2,
      'digest': 'c' * 64,
    },
    'task_id': _m4,
    'agent_id': 'scheduler',
    'state': 'failed',
    'issue': {'code': 'failed', 'message': 'Synthetic proposal failure.'},
    'artifacts': const <Object?>[],
  },
};

AgentCapabilityMessage _proposalMessage(
  String artifact, {
  String messageId = _m1,
}) => AgentCapabilityMessage.fromDelegation(
  _proposalDelegationMessage(
    artifact: artifact,
    messageId: messageId,
    output: 'Proposal ready',
  ),
);
Widget _proposalApp(
  ConversationController controller,
  AgentCapabilityMessage message,
) => MaterialApp(
  localizationsDelegates: AppLocalizations.localizationsDelegates,
  supportedLocales: AppLocalizations.supportedLocales,
  home: Scaffold(
    body: SingleChildScrollView(
      child: AgentProposalCard(controller: controller, message: message),
    ),
  ),
);
Map<String, dynamic> _proposalPreviewEnvelope(Map<String, dynamic> preview) => {
  'kind': 'conversation.calendar_proposal.preview',
  'preview': preview,
};
Map<String, dynamic> _readyProposal(String label) => {
  'kind': 'ready',
  'title': 'Proposal',
  'schedule': {
    'starts_at': '2026-10-12T10:00:00Z',
    'ends_at': '2026-10-12T11:00:00Z',
    'timezone': 'UTC',
  },
  'destinations': [
    {'destination_ref': _m6, 'label': label},
  ],
};
Map<String, dynamic> _proposalOperation({
  int revision = 1,
  String title = 'Exact proposal operation',
}) => {
  'action_ref': _m5,
  'revision': revision,
  'origin': 'expert',
  'effect': {
    'kind': 'create',
    'destination_label': 'Personal Calendar',
    'title': title,
    'schedule': {
      'starts_at': '2026-10-12T10:00:00Z',
      'ends_at': '2026-10-12T11:00:00Z',
      'timezone': 'UTC',
    },
  },
  'review_ref': {
    'id': _m6,
    'operation_id': _m5,
    'effect_digest': 'a' * 64,
    'source_digest': 'b' * 64,
    'person_id': _personId,
    'device_id': 'synthetic-device',
    'policy_revision': 3,
    'created_at': '2026-10-10T10:00:00Z',
    'expires_at': '2026-10-10T11:00:00Z',
  },
  'created_at': '2026-10-10T10:00:00Z',
  'expires_at': '2026-10-10T11:00:00Z',
  'status': {'state': 'pending_review'},
  'allowed_actions': ['approve', 'reject'],
  'next_observation_after_ms': null,
};
Map<String, dynamic> _proposalInteraction() {
  final operation = _proposalOperation(title: 'Unrelated approval');
  return {
    ...interactionFixture(id: _m4, sessionId: _sessionId),
    'interaction_kind': 'operation_approval',
    'created_at': operation['created_at'],
    'expires_at': operation['expires_at'],
    'requirement': {
      'kind': 'operation_approval',
      'review_ref': operation['review_ref'],
    },
    'target': {'kind': 'operation_approval', 'operation': operation},
    'actions': ['allow'],
  };
}
