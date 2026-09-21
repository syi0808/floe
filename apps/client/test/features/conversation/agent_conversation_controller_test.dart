import 'dart:async';

import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/features/conversation/application/agent_controller.dart';
import 'package:floe_client/features/conversation/application/agent_conversation_gateway.dart';

import '../../support/agent_fixture_gateway.dart';

import 'package:floe_client/features/conversation/presentation/agent_panel.dart';
import 'package:floe_client/app/runtime/agent_vault_gateway.dart';
import 'package:floe_client/features/conversation/application/conversation_runtime_gateway.dart';
import 'package:floe_client/l10n/app_localizations.dart';
import 'package:floe_client/app/runtime/floe_client.dart';
import 'package:floe_client/app/runtime/app_read_model.dart';
import 'package:flutter/material.dart';
import 'package:flutter_markdown_plus/flutter_markdown_plus.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../support/agent_vault_gateway.dart';

void main() {
  test(
    'starts a conversation without a Calendar refresh prerequisite',
    () async {
      final gateway = _ConversationGateway();
      final controller = AgentController(gateway: gateway, personId: 'person');
      addTearDown(controller.dispose);
      await controller.load();
      await controller.sendText('Hello Floe');
      expect(gateway.turns, hasLength(1));
      expect(controller.needsReload, isFalse);
    },
  );

  testWidgets('general conversation accepts free-form messages', (
    tester,
  ) async {
    final gateway = _ConversationGateway();
    final controller = AgentController(gateway: gateway, personId: 'person');
    addTearDown(controller.dispose);
    await controller.load();
    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: Scaffold(
          body: AgentPanel(controller: controller, onClose: () {}),
        ),
      ),
    );
    expect(find.text('Conversation'), findsNothing);
    expect(
      tester.getSize(find.byType(TextFormField)).height,
      lessThanOrEqualTo(50),
    );
    expect(find.byType(FilledButton), findsNothing);
    expect(find.byTooltip('Ask Floe'), findsOneWidget);
    await tester.enterText(find.byType(TextFormField), 'Hello Floe');
    await tester.tap(find.byTooltip('Ask Floe'));
    await tester.pump(const Duration(milliseconds: 500));
    await tester.pumpAndSettle();
    expect(gateway.turns.single.text, 'Hello Floe');
    final markdown = tester.widget<MarkdownBody>(find.byType(MarkdownBody));
    expect(
      markdown.data,
      '**Hello** back\n\n- First item\n- Second item\n\n'
      '[Details](https://example.com)\n\n'
      '![Remote diagram](https://example.com/image.png)',
    );
    expect(markdown.selectable, isTrue);
    expect(markdown.imageBuilder, isNotNull);
    expect(markdown.onTapLink, isNull);
    expect(markdown.styleSheet?.blockSpacing, FloeSpace.xs);
    expect(find.byType(Image), findsNothing);
    final renderedBlocks = tester
        .widgetList<SelectableText>(
          find.descendant(
            of: find.byType(MarkdownBody),
            matching: find.byType(SelectableText),
          ),
        )
        .map((widget) => widget.textSpan?.toPlainText())
        .whereType<String>();
    expect(renderedBlocks, contains('Hello back'));
  });

  test('product controller uses StartTurn and explicit Cancel only', () async {
    final gateway = _RuntimeConversationGateway();
    final controller = AgentController(gateway: gateway, personId: 'person');
    addTearDown(() {
      controller.dispose();
      gateway.runtime.readModel.dispose();
    });
    await controller.load();

    final sending = controller.sendText('Cancel this');
    await gateway.runtime.started.future;
    await controller.stop();
    await sending;

    expect(gateway.runtime.turns, 1);
    expect(gateway.runtime.cancellations, 1);
  });

  test('product retry sends a new command with explicit Run lineage', () async {
    final gateway = _ConversationGateway(failFirst: true);
    final controller = AgentController(gateway: gateway, personId: 'person');
    addTearDown(() {
      controller.dispose();
      gateway.runtime.readModel.dispose();
    });
    await controller.load();

    await controller.sendText('Retry this read');
    expect(controller.failure, 'server_model_unavailable');
    expect(controller.recoveryAction, 'retry_read');
    expect(controller.canRetry, isTrue);

    await controller.retry();

    expect(gateway.turns, hasLength(2));
    expect(gateway.turns.last.retryOf, '00000000-0000-4000-8000-000000000403');
    expect(controller.failure, isNull);
  });

  test(
    'terminal runtime interruption does not clear the Vault session',
    () async {
      final gateway = _ConversationGateway(
        failFirst: true,
        failureReason: 'interrupted',
      );
      final controller = AgentController(gateway: gateway, personId: 'person');
      addTearDown(() {
        controller.dispose();
        gateway.runtime.readModel.dispose();
      });
      await controller.load();

      await controller.sendText('Observe interruption');

      expect(controller.failure, 'interrupted');
      expect(controller.session, isNotNull);
      expect(controller.vaultState, AgentVaultState.ready);
      expect(controller.needsReload, isFalse);
    },
  );

  test('disposing the view does not cancel the backend Run', () async {
    final gateway = _RuntimeConversationGateway();
    final controller = AgentController(gateway: gateway, personId: 'person');
    await controller.load();
    final sending = controller.sendText('Keep running');
    await gateway.runtime.started.future;

    controller.dispose();

    expect(gateway.runtime.cancellations, 0);
    gateway.runtime.finish();
    await sending;
    gateway.runtime.readModel.dispose();
  });

  testWidgets('oversized emoji input stays in the composer', (tester) async {
    final gateway = _ConversationGateway();
    final controller = AgentController(gateway: gateway, personId: 'person');
    addTearDown(() {
      controller.dispose();
      gateway.runtime.readModel.dispose();
    });
    await controller.load();
    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: Scaffold(
          body: AgentPanel(controller: controller, onClose: () {}),
        ),
      ),
    );
    final text = List.filled(3000, '🧊').join();
    expect(controller.acceptsConversationText(text), isFalse);

    await tester.enterText(find.byType(TextFormField), text);
    await tester.tap(find.byTooltip('Ask Floe'));
    await tester.pump();

    expect(
      tester.widget<TextFormField>(find.byType(TextFormField)).controller!.text,
      text,
    );
    expect(controller.failure, 'invalid_input');
    expect(gateway.turns, isEmpty);
  });
}

final class _ConversationGateway extends TestVaultGateway
    implements AgentConversationGateway, ConversationRuntimeProvider {
  _ConversationGateway({
    this.failFirst = false,
    this.failureReason = 'server_model_unavailable',
  }) {
    state = AgentVaultState.ready;
  }

  final bool failFirst;
  final String failureReason;
  late final _ImmediateConversationRuntime runtime =
      _ImmediateConversationRuntime(this);
  final turns = <AgentConversationTurnRequest>[];
  AgentConversationTurnRequest? active;

  @override
  ConversationRuntimeGateway get conversationRuntime => runtime;

  Map<String, Object?> session({int revision = 0}) => {
    'schema_version': 1,
    'id': 'session',
    'person_id': 'person',
    'revision': revision,
    'data_classes': ['personal'],
    'messages': revision == 0
        ? <Object?>[]
        : [
            {'kind': 'user', 'turn_id': 'turn', 'text': active!.text},
            {
              'kind': 'assistant',
              'turn_id': 'turn',
              'text':
                  '**Hello** back\n\n- First item\n- Second item\n\n'
                  '[Details](https://example.com)\n\n'
                  '![Remote diagram](https://example.com/image.png)',
            },
          ],
    'active_turn': null,
    'last_outcome': revision == 0 ? null : {'status': 'completed'},
  };

  @override
  Future<AgentSession> startConversation(String personId) async =>
      AgentSession.fromJson(session());

  @override
  Future<AgentSession> resumeConversation(String personId) =>
      startConversation(personId);

  @override
  Future<AgentSession> loadConversation(String personId, String sessionId) =>
      startConversation(personId);

  @override
  Future<AgentSession> recoverConversation(AgentSession value) async =>
      AgentSession.fromJson(session(revision: value.revision + 1));
}

final class _ImmediateConversationRuntime
    implements ConversationRuntimeGateway {
  _ImmediateConversationRuntime(this.owner);

  final _ConversationGateway owner;
  @override
  final AppReadModel readModel = AppReadModel();

  @override
  Future<void> synchronizeConversation(AgentSession session) async {
    if (readModel.conversation.syncState == AppReadSyncState.uninitialized) {
      readModel.bootstrap(
        cursor: const AppEventCursor(runtimeEpoch: 7, cursor: 0),
      );
    }
  }

  @override
  Future<ConversationTurnCompletion> runConversationTurn(
    AgentConversationTurnRequest request, {
    required void Function(AppRunSnapshot run) onRun,
  }) async {
    owner.active = request;
    owner.turns.add(request);
    final shouldFail = owner.failFirst && owner.turns.length == 1;
    final runId = owner.turns.length == 1
        ? '00000000-0000-4000-8000-000000000403'
        : '00000000-0000-4000-8000-000000000404';
    final run = AppRunSnapshot(
      runId: runId,
      sessionId: request.session.id,
      revision: 1,
      runtimeEpoch: 7,
      executorGeneration: 1,
      state: AppRunState.finished,
      progress: shouldFail ? 'failed' : 'completed',
      report: AppTurnReport(
        execution: shouldFail ? 'failed' : 'completed',
        reply: shouldFail ? 'not_produced' : 'generated',
        issues: shouldFail
            ? [
                AppWireIssue(
                  'unavailable',
                  'model unavailable',
                  metadata: {
                    'reason_code': owner.failureReason,
                    if (owner.failureReason == 'server_model_unavailable')
                      'recovery_action': 'retry_read',
                  },
                ),
              ]
            : const [],
        finalMessageRef: null,
      ),
    );
    readModel.applyRunSnapshot(run);
    onRun(run);
    return ConversationTurnCompletion(
      run: run,
      session: AgentSession.fromJson(
        owner.session(revision: owner.turns.length * 2),
      ),
    );
  }

  @override
  Future<void> cancelConversationTurn(
    AgentConversationTurnRequest request,
  ) async {}
}

final class _RuntimeConversationGateway extends TestVaultGateway
    implements AgentConversationGateway, ConversationRuntimeProvider {
  _RuntimeConversationGateway() {
    state = AgentVaultState.ready;
  }

  final runtime = _ControllerRuntime();

  @override
  ConversationRuntimeGateway get conversationRuntime => runtime;

  AgentSession session({int revision = 0}) => _runtimeSession(revision);

  @override
  Future<AgentSession> startConversation(String personId) async => session();

  @override
  Future<AgentSession> resumeConversation(String personId) async => session();

  @override
  Future<AgentSession> loadConversation(
    String personId,
    String sessionId,
  ) async => session();

  @override
  Future<AgentSession> recoverConversation(AgentSession value) async =>
      session(revision: value.revision + 1);
}

final class _ControllerRuntime implements ConversationRuntimeGateway {
  @override
  final AppReadModel readModel = AppReadModel();
  final Completer<void> started = Completer<void>();
  final Completer<void> cancelled = Completer<void>();
  int turns = 0;
  int cancellations = 0;

  @override
  Future<void> synchronizeConversation(AgentSession session) async {
    if (readModel.conversation.syncState == AppReadSyncState.uninitialized) {
      readModel.bootstrap(
        cursor: const AppEventCursor(runtimeEpoch: 7, cursor: 0),
      );
    }
  }

  @override
  Future<ConversationTurnCompletion> runConversationTurn(
    AgentConversationTurnRequest request, {
    required void Function(AppRunSnapshot run) onRun,
  }) async {
    turns++;
    final executing = _run(1, AppRunState.executing);
    readModel.applyRunSnapshot(executing);
    onRun(executing);
    started.complete();
    await cancelled.future;
    final finished = _run(2, AppRunState.finished);
    readModel.applyRunSnapshot(finished);
    onRun(finished);
    return ConversationTurnCompletion(
      run: finished,
      session: _runtimeSession(2),
    );
  }

  @override
  Future<void> cancelConversationTurn(
    AgentConversationTurnRequest request,
  ) async {
    cancellations++;
    if (!cancelled.isCompleted) cancelled.complete();
  }

  void finish() {
    if (!cancelled.isCompleted) cancelled.complete();
  }

  AppRunSnapshot _run(int revision, AppRunState state) => AppRunSnapshot(
    runId: '00000000-0000-4000-8000-000000000402',
    sessionId: '00000000-0000-4000-8000-000000000401',
    revision: revision,
    runtimeEpoch: 7,
    executorGeneration: 1,
    state: state,
    progress: state.name,
    report: state == AppRunState.finished
        ? const AppTurnReport(
            execution: 'cancelled',
            reply: 'not_produced',
            issues: [],
            finalMessageRef: null,
          )
        : null,
  );
}

AgentSession _runtimeSession(int revision) => AgentSession.fromJson({
  'schema_version': 1,
  'id': '00000000-0000-4000-8000-000000000401',
  'person_id': 'person',
  'revision': revision,
  'data_classes': ['personal'],
  'messages': const <Object?>[],
  'active_turn': null,
  'last_outcome': revision == 0 ? null : {'status': 'completed'},
  'continuation': null,
});
