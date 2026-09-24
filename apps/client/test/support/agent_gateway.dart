import 'dart:async';

import 'package:floe_client/features/conversation/application/agent_conversation_gateway.dart';
import 'package:floe_client/features/conversation/application/conversation_runtime_gateway.dart';
import 'package:floe_client/features/conversation/domain/agent_session.dart';
import 'package:floe_client/app/runtime/app_read_model.dart';
import 'package:floe_client/app/runtime/floe_client.dart';

class TestAgentGateway
    implements AgentConversationGateway, ConversationRuntimeProvider {
  TestAgentGateway({this.personId = 'test', this.personal = true});
  final String personId;
  final bool personal;
  Map<String, Object?>? saved;
  bool hold = false;
  bool failLoad = false;
  bool hangLoad = false;
  Object? loadError;
  String? responseFailure;
  String? responseRecoveryAction;
  bool omitSessionOnFailure = false;
  bool includeSessionWithFailure = false;
  int begins = 0;
  int stops = 0;
  int recoveries = 0;
  int _sessions = 0;
  Completer<void>? turnGate;

  @override
  late final ConversationRuntimeGateway conversationRuntime =
      _TestConversationRuntime(this);

  @override
  Future<AgentSession> startConversation(String personId) async {
    saved = {
      'schema_version': 1,
      'id': 'session-${++_sessions}',
      'person_id': personId,
      'revision': 0,
      'active_turn': null,
      'last_outcome': null,
      'data_classes': [personal ? 'personal' : 'synthetic'],
      'messages': <Map<String, Object?>>[],
    };
    return currentSession;
  }

  @override
  Future<AgentSession> resumeConversation(String personId) async {
    if (failLoad) throw StateError('test load failure');
    if (hangLoad) await Completer<void>().future;
    if (loadError != null) throw loadError!;
    return saved == null ? startConversation(personId) : currentSession;
  }

  @override
  Future<AgentSession> loadConversation(
    String personId,
    String sessionId,
  ) async => currentSession;

  AgentSession get currentSession => AgentSession.fromJson(saved!);

  @override
  Future<AgentSession> recoverConversation(AgentSession previous) async {
    recoveries++;
    saved!['active_turn'] = null;
    saved!['revision'] = previous.revision + 1;
    saved!['last_outcome'] = {'status': 'halted', 'reason': 'interrupted'};
    return currentSession;
  }
}

final class _TestConversationRuntime implements ConversationRuntimeGateway {
  _TestConversationRuntime(this.owner);
  final TestAgentGateway owner;
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
    owner.begins++;
    final turnId = 'test-run-${owner.begins}';
    owner.saved!['messages'] = [
      ...owner.saved!['messages']! as List,
      {'kind': 'user', 'turn_id': turnId, 'text': request.text},
    ];
    owner.saved!['active_turn'] = turnId;
    owner.saved!['revision'] = (owner.saved!['revision']! as int) + 1;
    if (owner.hold) {
      final pending = AppRunSnapshot(
        runId: turnId,
        sessionId: request.session.id,
        revision: 1,
        runtimeEpoch: 7,
        executorGeneration: 1,
        state: AppRunState.accepted,
        progress: 'running',
        report: null,
      );
      readModel.applyRunSnapshot(pending);
      onRun(pending);
      owner.turnGate = Completer<void>();
      await owner.turnGate!.future;
    }
    final failure = owner.responseFailure;
    owner.saved!['active_turn'] = null;
    owner.saved!['last_outcome'] = failure == null
        ? {'status': 'completed'}
        : {'status': 'halted', 'reason': failure};
    if (failure == null) {
      owner.saved!['messages'] = [
        ...owner.saved!['messages']! as List,
        {'kind': 'assistant', 'turn_id': turnId, 'text': 'Test response.'},
      ];
    }
    owner.saved!['revision'] = (owner.saved!['revision']! as int) + 1;
    final run = AppRunSnapshot(
      runId: turnId,
      sessionId: request.session.id,
      revision: 2,
      runtimeEpoch: 7,
      executorGeneration: 1,
      state: AppRunState.finished,
      progress: failure == null ? 'completed' : 'failed',
      report: AppTurnReport(
        execution: failure == null ? 'completed' : 'failed',
        reply: failure == null ? 'generated' : 'not_produced',
        issues: failure == null
            ? []
            : [
                AppWireIssue(
                  'unavailable',
                  'Test owner failure',
                  metadata: {
                    'reason_code': failure,
                    'recovery_action': owner.responseRecoveryAction ?? 'none',
                    'reload_required':
                        (owner.omitSessionOnFailure &&
                                !owner.includeSessionWithFailure)
                            .toString(),
                  },
                ),
              ],
        finalMessageRef: null,
      ),
    );
    readModel.applyRunSnapshot(run);
    onRun(run);
    return ConversationTurnCompletion(run: run, session: owner.currentSession);
  }

  @override
  Future<ConversationTurnCompletion> observeConversationRun(
    AppCommandReceipt receipt,
    AgentSession session, {
    required void Function(AppRunSnapshot run) onRun,
  }) async {
    final run = AppRunSnapshot(
      runId: receipt.runId,
      sessionId: session.id,
      revision: 2,
      runtimeEpoch: receipt.runtimeEpoch,
      executorGeneration: 1,
      state: AppRunState.finished,
      progress: 'completed',
      report: const AppTurnReport(
        execution: 'completed',
        reply: 'generated',
        issues: [],
        finalMessageRef: null,
      ),
    );
    readModel.applyRunSnapshot(run);
    onRun(run);
    return ConversationTurnCompletion(run: run, session: session);
  }

  @override
  Future<void> cancelConversationTurn(
    AgentConversationTurnRequest request,
  ) async {
    owner.stops++;
    owner.responseFailure = 'cancelled';
    owner.turnGate?.complete();
  }
}
