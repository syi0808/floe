import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/features/agent/agent_controller.dart';
import 'package:floe_client/features/agent/agent_conversation_gateway.dart';
import 'package:floe_client/features/agent/agent_fixture_gateway.dart';
import 'package:floe_client/features/agent/agent_panel.dart';
import 'package:floe_client/features/agent/agent_vault_gateway.dart';
import 'package:floe_client/l10n/app_localizations.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/agent_vault_gateway.dart';

void main() {
  testWidgets('encrypted general conversation replaces the sample fallback', (
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
    expect(find.text('Private conversation'), findsOneWidget);
    expect(find.text('Sample assistant'), findsNothing);
    await tester.enterText(find.byType(TextFormField), 'Hello Floe');
    await tester.tap(find.text('Ask Floe'));
    await tester.pump(const Duration(milliseconds: 500));
    await tester.pumpAndSettle();
    expect(gateway.turns.single.text, 'Hello Floe');
    expect(find.text('Hello back'), findsOneWidget);
  });
}

final class _ConversationGateway extends TestVaultGateway
    implements AgentConversationGateway {
  _ConversationGateway() {
    state = AgentVaultState.ready;
  }

  final turns = <AgentConversationTurnRequest>[];
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
            {'kind': 'assistant', 'turn_id': 'turn', 'text': 'Hello back'},
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
