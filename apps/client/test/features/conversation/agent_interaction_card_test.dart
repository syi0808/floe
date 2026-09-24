import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/app/runtime/agent_vault_gateway.dart';
import 'package:floe_client/app/runtime/app_read_model.dart';
import 'package:floe_client/app/runtime/floe_client.dart';
import 'package:floe_client/features/conversation/application/agent_controller.dart';
import 'package:floe_client/features/conversation/application/agent_conversation_gateway.dart';
import 'package:floe_client/features/conversation/application/agent_interaction_gateway.dart';
import 'package:floe_client/features/conversation/application/conversation_runtime_gateway.dart';
import 'package:floe_client/features/conversation/domain/agent_interaction.dart';
import 'package:floe_client/features/conversation/domain/agent_session.dart';
import 'package:floe_client/features/conversation/presentation/agent_interaction_card.dart';
import 'package:floe_client/l10n/app_localizations.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../support/agent_vault_gateway.dart';

const _cardId = '00000000-0000-4000-8000-000000000023';

Map<String, dynamic> _snapshot({
  String state = 'pending',
  List<String> actions = const ['allow', 'deny', 'dismiss'],
}) => {
  'interaction_id': _cardId,
  'session_id': 'session',
  'origin_run_id': '00000000-0000-4000-8000-000000000029',
  'interaction_kind': 'processing_recipient',
  'state': state,
  'revision': 1,
  'target_digest': List<int>.generate(32, (index) => index + 1),
  'created_at_unix_ms': 1700000000000,
  'expires_at_unix_ms': 1700003600000,
  'target': {
    'kind': 'recipient_consent',
    'recipient': 'model.example',
    'profile_id': 'server-model',
    'purpose': 'everyday_assistance',
    'consumer': 'conversation.root',
    'input_data_classes': ['personal'],
    'source_scopes': [],
  },
  'actions': actions,
};

void main() {
  testWidgets('card renders backend actions and decides on tap', (
    tester,
  ) async {
    final gateway = _CardGateway();
    final controller = AgentController(gateway: gateway, personId: 'person');
    addTearDown(controller.dispose);
    await controller.load();
    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: Scaffold(
          body: AgentInteractionCard(
            controller: controller,
            message: controller.messages.single as AgentInteractionMessage,
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('Model request'), findsOneWidget);
    expect(find.textContaining('model.example'), findsOneWidget);
    expect(find.text('Waiting for review'), findsOneWidget);
    // Only the backend-projected actions render; nothing is derived.
    expect(find.text('Allow'), findsOneWidget);
    expect(find.text('Deny'), findsOneWidget);
    expect(find.text('Not now'), findsOneWidget);
    expect(find.text('Continue'), findsNothing);

    await tester.tap(find.text('Allow'));
    await tester.pumpAndSettle();
    expect(gateway.decisions.single, AgentInteractionDecision.approve);
    expect(find.text('Resolved'), findsOneWidget);
    expect(find.text('Allow'), findsNothing);
    expect(find.text('Continue'), findsOneWidget);
  });
}

final class _CardGateway extends TestVaultGateway
    implements
        AgentConversationGateway,
        ConversationRuntimeProvider,
        AgentInteractionProvider {
  _CardGateway() {
    state = AgentVaultState.ready;
  }

  final decisions = <AgentInteractionDecision>[];
  bool decided = false;
  final _runtime = _CardRuntime();

  @override
  ConversationRuntimeGateway get conversationRuntime => _runtime;

  @override
  AgentInteractionGateway get interactionGateway => _CardInteractions(this);

  Map<String, Object?> _session() => {
    'schema_version': 1,
    'id': 'session',
    'person_id': 'person',
    'revision': 0,
    'data_classes': ['personal'],
    'messages': [
      {
        'kind': 'interaction',
        'turn_id': 'origin',
        'interaction_id': _cardId,
        'interaction_kind': 'processing_recipient',
      },
    ],
    'active_turn': null,
    'last_outcome': {'status': 'completed'},
  };

  @override
  Future<AgentSession> startConversation(String personId) async =>
      AgentSession.fromJson(_session());

  @override
  Future<AgentSession> resumeConversation(String personId) =>
      startConversation(personId);

  @override
  Future<AgentSession> loadConversation(String personId, String sessionId) =>
      startConversation(personId);

  @override
  Future<AgentSession> recoverConversation(AgentSession value) =>
      startConversation(value.personId);
}

final class _CardInteractions implements AgentInteractionGateway {
  _CardInteractions(this._parent);

  final _CardGateway _parent;

  Map<String, dynamic> _current() => _parent.decided
      ? _snapshot(state: 'resolved', actions: ['continue_request'])
      : _snapshot();

  @override
  Future<AgentInteractionSnapshot?> loadInteraction(
    String interactionId,
  ) async => AgentInteractionSnapshot.parse(_current());

  @override
  Future<List<AgentInteractionSnapshot>> loadSessionInteractions(
    String sessionId,
  ) async => [AgentInteractionSnapshot.parse(_current())];

  @override
  Future<AgentInteractionResolveResult> decideInteraction(
    AgentInteractionSnapshot snapshot,
    AgentInteractionDecision decision,
  ) async {
    _parent.decisions.add(decision);
    _parent.decided = true;
    return AgentInteractionResolveResult.parse({
      'kind': 'interaction_operation',
      'command_id': 'decision',
      'outcome': 'resolved',
      'snapshot': _current(),
    });
  }

  @override
  Future<AgentInteractionRefreshResult> refreshInteraction(
    AgentInteractionSnapshot snapshot,
  ) async => AgentInteractionRefreshResult.parse({
    'kind': 'interaction_refresh',
    'command_id': 'refresh',
    'outcome': 'still_pending',
    'snapshot': _current(),
  });

  @override
  Future<AppCommandReceipt> resumeInteraction({
    required String sessionId,
    required String originRunId,
    required int expectedRevision,
  }) async => throw UnimplementedError();
}

final class _CardRuntime implements ConversationRuntimeGateway {
  @override
  AppReadModel get readModel => _model;
  final AppReadModel _model = AppReadModel();

  @override
  Future<void> synchronizeConversation(AgentSession session) async {}

  @override
  Future<ConversationTurnCompletion> runConversationTurn(
    AgentConversationTurnRequest request, {
    required void Function(AppRunSnapshot run) onRun,
  }) async => throw UnimplementedError();

  @override
  Future<ConversationTurnCompletion> observeConversationRun(
    AppCommandReceipt receipt,
    AgentSession session, {
    required void Function(AppRunSnapshot run) onRun,
  }) async => throw UnimplementedError();

  @override
  Future<void> cancelConversationTurn(
    AgentConversationTurnRequest request,
  ) async {}
}
