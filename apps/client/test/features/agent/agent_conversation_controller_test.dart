import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/features/agent/agent_controller.dart';
import 'package:floe_client/features/agent/agent_conversation_gateway.dart';
import 'package:floe_client/features/agent/agent_fixture_gateway.dart';
import 'package:floe_client/features/agent/agent_panel.dart';
import 'package:floe_client/features/agent/agent_vault_gateway.dart';
import 'package:floe_client/l10n/app_localizations.dart';
import 'package:flutter/material.dart';
import 'package:flutter_markdown_plus/flutter_markdown_plus.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../support/agent_vault_gateway.dart';

void main() {
  test('refreshes device context before starting an expert turn', () async {
    var refreshed = false;
    final gateway = _ConversationGateway()
      ..beforeBegin = () => expect(refreshed, isTrue);
    final controller = AgentController(
      gateway: gateway,
      personId: 'person',
      beforeInvocation: () async => refreshed = true,
    );
    addTearDown(controller.dispose);

    await controller.load();
    await controller.sendText('Check my schedule');

    expect(gateway.turns, hasLength(1));
  });

  test('does not start a turn when device context refresh fails', () async {
    final gateway = _ConversationGateway();
    final controller = AgentController(
      gateway: gateway,
      personId: 'person',
      beforeInvocation: () async {
        throw StateError('calendar unavailable');
      },
    );
    addTearDown(controller.dispose);

    await controller.load();
    await controller.sendText('Check my schedule');

    expect(gateway.turns, isEmpty);
    expect(controller.failure, 'transport_unavailable');
    expect(controller.needsReload, isTrue);
  });

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
}

final class _ConversationGateway extends TestVaultGateway
    implements AgentConversationGateway {
  _ConversationGateway() {
    state = AgentVaultState.ready;
  }

  final turns = <AgentConversationTurnRequest>[];
  void Function()? beforeBegin;
  AgentConversationTurnRequest? active;
  bool done = false;

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

  @override
  Future<AgentRunUpdate> beginConversationTurn(
    AgentConversationTurnRequest request,
  ) async {
    beforeBegin?.call();
    active = request;
    turns.add(request);
    done = false;
    return update();
  }

  @override
  Future<AgentRunUpdate> pollConversationTurn(
    AgentConversationTurnRequest request,
    int afterSequence,
  ) async {
    done = true;
    return update();
  }

  @override
  Future<AgentRunUpdate> stopConversationTurn(
    AgentConversationTurnRequest request,
  ) async {
    done = true;
    return update();
  }

  @override
  Future<AgentRunUpdate> releaseConversationTurn(
    AgentConversationTurnRequest request,
  ) async => update();

  AgentRunUpdate update() => AgentRunUpdate.fromJson({
    'session_id': active!.session.id,
    'expected_revision': active!.session.revision,
    'events': <Object?>[],
    'next_sequence': 0,
    'done': done,
    'session': done ? session(revision: 1) : null,
    'failure': null,
  });
}
