import 'package:floe_client/app/runtime/agent_vault_gateway.dart';
import 'package:floe_client/app/runtime/app_read_model.dart';
import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/app/runtime/floe_client.dart';
import 'package:floe_client/features/conversation/application/agent_controller.dart';
import 'package:floe_client/features/conversation/application/agent_conversation_gateway.dart';
import 'package:floe_client/features/conversation/application/agent_interaction_gateway.dart';
import 'package:floe_client/features/conversation/application/conversation_runtime_gateway.dart';
import 'package:floe_client/features/conversation/domain/agent_interaction.dart';
import 'package:floe_client/features/conversation/domain/agent_session.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../support/agent_vault_gateway.dart';

const _cardId = '00000000-0000-4000-8000-000000000023';

Map<String, dynamic> _snapshot({
  String state = 'pending',
  List<String> actions = const ['allow', 'deny', 'dismiss'],
  int revision = 1,
}) => {
  'interaction_id': _cardId,
  'session_id': 'session',
  'origin_run_id': '00000000-0000-4000-8000-000000000029',
  'interaction_kind': 'processing_recipient',
  'state': state,
  'revision': revision,
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
    'source_scopes': [
      {
        'connection_id': 'calendar-connection',
        'resources': ['personal'],
        'categories': ['metadata'],
        'operation': 'read',
        'purpose': 'scheduling',
        'consumer': 'builtin:floe.builtin.schedule',
      },
    ],
  },
  'actions': actions,
};

void main() {
  group('interaction snapshot parsing', () {
    test('parses a consent card with scopes and backend actions', () {
      final snapshot = AgentInteractionSnapshot.parse(_snapshot());
      expect(snapshot.id, '00000000-0000-4000-8000-000000000023');
      expect(snapshot.sessionId, 'session');
      expect(snapshot.kind, AgentInteractionKind.processingRecipient);
      expect(snapshot.state, AgentInteractionState.pending);
      expect(snapshot.targetDigest, hasLength(32));
      final target = snapshot.target as AgentRecipientConsentTarget;
      expect(target.recipient, 'model.example');
      expect(target.inputDataClasses, ['personal']);
      expect(target.sourceScopes.single.connectionId, 'calendar-connection');
      expect(snapshot.actions, [
        AgentInteractionAction.allow,
        AgentInteractionAction.deny,
        AgentInteractionAction.dismiss,
      ]);
      expect(snapshot.terminal, isFalse);
    });

    test('parses observe and navigation targets', () {
      final observe = _snapshot()
        ..['interaction_kind'] = 'source_access'
        ..['target'] = {
          'kind': 'inline_observe',
          'connection_id': 'calendar-connection',
          'source_id': 'floe.source.calendar',
          'consumer': 'floe.builtin.schedule',
          'purpose': 'scheduling',
          'members': [
            {'member_id': 'personal', 'resource': 'personal'},
          ],
        };
      final observeSnapshot = AgentInteractionSnapshot.parse(observe);
      final observeTarget = observeSnapshot.target as AgentInlineObserveTarget;
      expect(observeTarget.members.single.resource, 'personal');

      final navigation = _snapshot()
        ..['target'] = {
          'kind': 'navigation_only',
          'destination': 'connection_settings',
          'source_id': 'floe.source.calendar',
          'consumer': 'floe.builtin.schedule',
          'purpose': 'scheduling',
        };
      final navigationSnapshot = AgentInteractionSnapshot.parse(navigation);
      expect(
        (navigationSnapshot.target as AgentNavigationOnlyTarget).destination,
        AgentNavigationDestination.connectionSettings,
      );
    });

    test('parses navigation-only Expert binding without source authority', () {
      final binding = _snapshot(actions: [
        'open_expert_settings',
        'refresh',
        'dismiss',
      ])
        ..['interaction_kind'] = 'expert_binding'
        ..['target'] = {
          'kind': 'expert_binding',
          'assignment_id': 'assignment-1',
          'package_id': 'example.test.expert',
          'package_version': '1.0.0',
          'requirement_key': 'required_attention',
          'capability': 'attention.coarse',
        };
      final snapshot = AgentInteractionSnapshot.parse(binding);
      expect(snapshot.kind, AgentInteractionKind.expertBinding);
      expect(snapshot.actions, [
        AgentInteractionAction.openExpertSettings,
        AgentInteractionAction.refresh,
        AgentInteractionAction.dismiss,
      ]);
      final target = snapshot.target as AgentExpertBindingTarget;
      expect(target.packageId, 'example.test.expert');
      expect(target.requirementKey, 'required_attention');
      expect(binding['target'], isNot(contains('connector_id')));
      expect(binding['target'], isNot(contains('grant_id')));
    });

    test('rejects unknown states, actions, targets and digests', () {
      for (final patch in [
        (map) => map['state'] = 'waiting_on_user',
        (map) => map['interaction_kind'] = 'device_pairing',
        (map) => map['actions'] = ['allow', 'auto_approve'],
        (map) => map['target'] = {'kind': 'admin_override'},
        (map) => map['target_digest'] = [1, 2, 3],
        (map) => map['target_digest'] = List.filled(32, 500),
      ]) {
        final raw = _snapshot();
        patch(raw);
        expect(
          () => AgentInteractionSnapshot.parse(raw),
          throwsFormatException,
        );
      }
    });

    test('parses resolve results with linked children', () {
      final result = AgentInteractionResolveResult.parse({
        'kind': 'interaction_operation',
        'command_id': 'command',
        'outcome': 'resolved',
        'snapshot': _snapshot(state: 'resolved', actions: ['continue_request']),
        'linked_run': {
          'command_id': 'resume-command',
          'runtime_epoch': 7,
          'admission': 'accepted',
          'run_id': 'child-run',
          'session_revision': 4,
        },
      });
      expect(result.outcome, AgentInteractionResolveOutcome.resolved);
      expect(result.snapshot.state, AgentInteractionState.resolved);
      expect(result.linkedRun!.runId, 'child-run');

      final stale = AgentInteractionResolveResult.parse({
        'kind': 'interaction_operation',
        'command_id': 'command',
        'outcome': 'stale',
        'snapshot': _snapshot(revision: 2),
      });
      expect(stale.outcome, AgentInteractionResolveOutcome.stale);
      expect(stale.snapshot.revision, 2);
      expect(stale.linkedRun, isNull);

      expect(
        () => AgentInteractionResolveResult.parse({
          'kind': 'interaction_operation',
          'command_id': 'command',
          'outcome': 'maybe',
          'snapshot': _snapshot(),
        }),
        throwsFormatException,
      );
    });

    test('parses interaction message references', () {
      final message = AgentMessage.fromJson({
        'kind': 'interaction',
        'turn_id': 'turn',
        'interaction_id': _cardId,
        'interaction_kind': 'processing_recipient',
      });
      expect(message, isA<AgentInteractionMessage>());
      expect((message as AgentInteractionMessage).interactionId, _cardId);
      expect(
        () => AgentMessage.fromJson({
          'kind': 'interaction',
          'turn_id': 'turn',
          'interaction_id': 'card',
          'interaction_kind': 'admin_override',
        }),
        throwsFormatException,
      );
    });
  });

  group('interaction client', () {
    test('resolve sends only the reviewed binding', () async {
      final transport = _FakeTransport();
      final client = FloeClient(transport, newId: () => 'decision');
      final command = client.prepareInteractionResolve(
        interactionId: 'card',
        sessionId: 'session',
        expectedRevision: 1,
        decision: AgentInteractionDecision.approve,
        targetDigest: List.filled(32, 2),
      );
      transport.command = (request) async => {
        'kind': 'interaction_operation',
        'command_id': 'decision',
        'outcome': 'resolved',
        'snapshot': _snapshot(state: 'resolved', actions: ['continue_request']),
      };
      final result = await client.submitInteractionResolve(command);
      expect(result.outcome, AgentInteractionResolveOutcome.resolved);
      final sent =
          transport.commandRequests.single['command'] as Map<String, dynamic>;
      expect(sent['kind'], 'conversation.interaction.resolve');
      expect(sent['decision'], 'approve');
      expect(sent.keys, hasLength(6));
      expect(sent.containsKey('recipient'), isFalse);
      expect(sent.containsKey('original_text'), isFalse);
    });

    test('resume omits caller text and observes the receipt', () async {
      final transport = _FakeTransport();
      final client = FloeClient(transport, newId: () => 'resume');
      final command = client.prepareInteractionResume(
        sessionId: 'session',
        originRunId: 'origin',
        expectedRevision: 4,
      );
      transport.command = (request) async => {
        'kind': 'command_receipt',
        'command_id': 'resume',
        'runtime_epoch': 7,
        'admission': 'accepted',
        'run_id': 'child',
        'session_revision': 5,
      };
      final receipt = await client.submitInteractionResume(command);
      expect(receipt.runId, 'child');
      final sent =
          transport.commandRequests.single['command'] as Map<String, dynamic>;
      expect(sent.keys, hasLength(4));
      expect(sent.containsKey('text'), isFalse);
    });

    test('get and list scope their snapshots', () async {
      final transport = _FakeTransport();
      final client = FloeClient(transport, newId: () => 'q');
      transport.query = (request) async => {
        'kind': 'interaction',
        ..._snapshot(),
      };
      final snapshot = await client.getInteraction(_cardId);
      expect(snapshot!.id, _cardId);

      transport.query = (request) async => {
        'kind': 'unknown_interaction',
        'interaction_id': 'missing',
      };
      expect(await client.getInteraction('missing'), isNull);

      transport.query = (request) async => {
        'kind': 'interaction_list',
        'session_id': 'session',
        'interactions': [_snapshot()],
      };
      final listed = await client.listInteractions('session');
      expect(listed, hasLength(1));
    });
  });

  group('interaction gateway', () {
    test('resubmits the identical decision after transport loss', () async {
      var attempts = 0;
      final transport = _FakeTransport();
      final client = FloeClient(transport, newId: () => 'decision');
      final gateway = NativeAgentInteractionGateway(client);
      transport.command = (request) async {
        attempts += 1;
        if (attempts == 1) throw StateError('transport lost');
        return {
          'kind': 'interaction_operation',
          'command_id': 'decision',
          'outcome': 'resolved',
          'snapshot': _snapshot(
            state: 'resolved',
            actions: ['continue_request'],
          ),
        };
      };
      final result = await gateway.decideInteraction(
        AgentInteractionSnapshot.parse(_snapshot()),
        AgentInteractionDecision.approve,
      );
      expect(result.outcome, AgentInteractionResolveOutcome.resolved);
      expect(attempts, 2);
      expect(
        transport.commandRequests.map((request) => request['command_id']),
        ['decision', 'decision'],
      );
    });
  });

  group('interaction controller', () {
    test('decide observes the linked child and reloads the card', () async {
      final gateway = _InteractionConversationGateway(
        outcome: AgentInteractionResolveOutcome.resolved,
        linked: true,
      );
      final controller = AgentController(gateway: gateway, personId: 'person');
      addTearDown(controller.dispose);
      await controller.load();
      await controller.refreshInteractions();
      final card = controller.interactionFor(_cardId)!;
      expect(card.actions, contains(AgentInteractionAction.allow));
      await controller.decideInteraction(
        card,
        AgentInteractionDecision.approve,
      );
      expect(gateway.decisions.single.$3, AgentInteractionDecision.approve);
      expect(gateway.observedRuns, ['child-run']);
      expect(controller.messages, hasLength(2));
      // The linked observation accepts a new session, which drops the
      // card snapshot; the panel reloads it lazily.
      expect(controller.interactionFor(_cardId), isNull);
      await controller.refreshInteractions();
      expect(
        controller.interactionFor(_cardId)!.state,
        AgentInteractionState.resolved,
      );
    });

    test('stale review keeps the current card for a new tap', () async {
      final gateway = _InteractionConversationGateway(
        outcome: AgentInteractionResolveOutcome.stale,
      );
      final controller = AgentController(gateway: gateway, personId: 'person');
      addTearDown(controller.dispose);
      await controller.load();
      await controller.refreshInteractions();
      final card = controller.interactionFor(_cardId)!;
      await controller.decideInteraction(
        card,
        AgentInteractionDecision.approve,
      );
      expect(controller.interactionFailureFor(_cardId), 'interaction_stale');
      expect(controller.interactionFor(_cardId)!.revision, 2);
      expect(controller.needsReload, isFalse);
    });

    test('continue claims the slot at the current revision', () async {
      final gateway = _InteractionConversationGateway(
        outcome: AgentInteractionResolveOutcome.resolved,
      );
      final controller = AgentController(gateway: gateway, personId: 'person');
      addTearDown(controller.dispose);
      await controller.load();
      await controller.refreshInteractions();
      final resolved = AgentInteractionSnapshot.parse(
        _snapshot(
          state: 'resolved',
          actions: ['continue_request'],
          revision: 3,
        ),
      );
      await controller.continueInteraction(resolved);
      expect(gateway.resumes.single.$3, 0);
      expect(gateway.observedRuns, ['child-run']);
    });
  });
}

final class _FakeTransport implements AppWireTransport {
  Future<Map<String, dynamic>> Function(Map<String, dynamic>)? command;
  Future<Map<String, dynamic>> Function(Map<String, dynamic>)? query;
  final commandRequests = <Map<String, dynamic>>[];

  @override
  Future<Map<String, dynamic>> commandV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) {
    commandRequests.add(request);
    return command!(request);
  }

  @override
  Future<Map<String, dynamic>> queryV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) => query!(request);

  @override
  Future<Map<String, dynamic>> eventsV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) => throw UnimplementedError();

  @override
  Future<void> close() async {}
}

final class _InteractionConversationGateway extends TestVaultGateway
    implements
        AgentConversationGateway,
        ConversationRuntimeProvider,
        AgentInteractionProvider {
  _InteractionConversationGateway({
    required this.outcome,
    this.linked = false,
  }) {
    state = AgentVaultState.ready;
  }

  final AgentInteractionResolveOutcome outcome;
  final bool linked;
  final decisions = <(String, int, AgentInteractionDecision)>[];
  final resumes = <(String, String, int)>[];
  final observedRuns = <String>[];

  late final AgentInteractionGateway _interactions = _Gateway(this);

  @override
  AgentInteractionGateway get interactionGateway => _interactions;

  @override
  ConversationRuntimeGateway get conversationRuntime => _Runtime(this);

  Map<String, Object?> _session({int revision = 0}) => {
    'schema_version': 1,
    'id': 'session',
    'person_id': 'person',
    'revision': revision,
    'data_classes': ['personal'],
    'messages': [
      {
        'kind': 'interaction',
        'turn_id': 'origin',
        'interaction_id': _cardId,
        'interaction_kind': 'processing_recipient',
      },
      if (revision > 0)
        {'kind': 'assistant', 'turn_id': 'child', 'text': 'Resumed.'},
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
  Future<AgentSession> loadConversation(
    String personId,
    String sessionId,
  ) async => AgentSession.fromJson(_session(revision: 1));

  @override
  Future<AgentSession> recoverConversation(AgentSession value) =>
      startConversation(value.personId);
}

final class _Gateway implements AgentInteractionGateway {
  _Gateway(this._parent);

  final _InteractionConversationGateway _parent;
  bool decided = false;

  @override
  Future<AgentInteractionSnapshot?> loadInteraction(
    String interactionId,
  ) async => AgentInteractionSnapshot.parse(_current());

  @override
  Future<List<AgentInteractionSnapshot>> loadSessionInteractions(
    String sessionId,
  ) async => [AgentInteractionSnapshot.parse(_current())];

  Map<String, dynamic> _current() =>
      _parent.outcome == AgentInteractionResolveOutcome.stale
      ? _snapshot(revision: 2)
      : decided
      ? _snapshot(state: 'resolved', actions: ['continue_request'])
      : _snapshot();

  @override
  Future<AgentInteractionResolveResult> decideInteraction(
    AgentInteractionSnapshot snapshot,
    AgentInteractionDecision decision,
  ) async {
    _parent.decisions.add((snapshot.id, snapshot.revision, decision));
    decided = _parent.outcome != AgentInteractionResolveOutcome.stale;
    return AgentInteractionResolveResult.parse({
      'kind': 'interaction_operation',
      'command_id': 'decision',
      'outcome': _parent.outcome.name,
      'snapshot': _current(),
      if (_parent.linked)
        'linked_run': {
          'command_id': 'resume-command',
          'runtime_epoch': 7,
          'admission': 'accepted',
          'run_id': 'child-run',
          'session_revision': 1,
        },
    });
  }

  @override
  Future<AgentInteractionRefreshResult> refreshInteraction(
    AgentInteractionSnapshot snapshot,
  ) async => AgentInteractionRefreshResult.parse({
    'kind': 'interaction_refresh',
    'command_id': 'refresh',
    'outcome': 'still_pending',
    'snapshot': _snapshot(),
  });

  @override
  Future<AppCommandReceipt> resumeInteraction({
    required String sessionId,
    required String originRunId,
    required int expectedRevision,
  }) async {
    _parent.resumes.add((sessionId, originRunId, expectedRevision));
    return const AppCommandReceipt(
      commandId: 'resume',
      runId: 'child-run',
      sessionRevision: 1,
      runtimeEpoch: 7,
    );
  }
}

final class _Runtime implements ConversationRuntimeGateway {
  _Runtime(this._parent);

  final _InteractionConversationGateway _parent;
  final AppReadModel _model = AppReadModel();

  @override
  AppReadModel get readModel => _model;

  @override
  Future<void> synchronizeConversation(AgentSession session) async {}

  @override
  Future<ConversationTurnCompletion> runConversationTurn(
    AgentConversationTurnRequest request, {
    required void Function(AppRunSnapshot run) onRun,
  }) async {
    throw UnimplementedError();
  }

  @override
  Future<ConversationTurnCompletion> observeConversationRun(
    AppCommandReceipt receipt,
    AgentSession session, {
    required void Function(AppRunSnapshot run) onRun,
  }) async {
    _parent.observedRuns.add(receipt.runId);
    final run = AppRunSnapshot(
      runId: receipt.runId,
      sessionId: session.id,
      revision: 1,
      runtimeEpoch: receipt.runtimeEpoch,
      executorGeneration: 1,
      state: AppRunState.finished,
      progress: 'done',
      report: null,
    );
    onRun(run);
    return ConversationTurnCompletion(
      run: run,
      session: await _parent.loadConversation(session.personId, session.id),
    );
  }

  @override
  Future<void> cancelConversationTurn(
    AgentConversationTurnRequest request,
  ) async {}
}
