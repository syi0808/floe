import 'dart:async';

import 'package:floe_client/features/conversation/application/agent_conversation_gateway.dart';
import 'package:floe_client/features/conversation/application/conversation_runtime_gateway.dart';
import 'package:floe_client/app/runtime/app_read_model.dart';
import 'package:floe_client/app/runtime/floe_client.dart';

import '../support/agent_fixture_gateway.dart';

class TestAgentGateway
    implements
        AgentFixtureStreamingGateway,
        AgentConversationGateway,
        ConversationRuntimeProvider {
  TestAgentGateway({this.personId = 'test', this.personal = false});
  final bool personal;

  @override
  late final ConversationRuntimeGateway conversationRuntime =
      _TestConversationRuntime(this);

  AgentSession _personal(AgentFixtureResult result) {
    if (personal) saved!['data_classes'] = ['personal'];
    return _result().session;
  }

  @override
  Future<AgentSession> startConversation(String personId) async =>
      _personal(await startAgentFixture(personId));
  @override
  Future<AgentSession> resumeConversation(String personId) async =>
      _personal(await resumeAgentFixture(personId));
  @override
  Future<AgentSession> loadConversation(
    String personId,
    String sessionId,
  ) async => _personal(await loadAgentFixture(personId, sessionId));
  @override
  Future<AgentSession> recoverConversation(AgentSession session) async =>
      _personal(await recoverAgentFixture(session));

  final String personId;
  Map<String, Object?>? saved;
  bool hold = false;
  bool failLoad = false;
  bool hangLoad = false;
  Object? loadError;
  bool failPoll = false;
  String? responseFailure;
  String? responseRecoveryAction;
  bool omitSessionOnFailure = false;
  bool includeSessionWithFailure = false;
  String? capabilityOutput;
  int begins = 0;
  int stops = 0;
  int releases = 0;
  int recoveries = 0;
  int _sessions = 0;
  AgentSession? _original;
  final _events = <Map<String, Object?>>[];
  bool _done = false;

  void appendProgress(Map<String, Object?> event) => _event(event);

  @override
  Future<AgentFixtureResult> startAgentFixture(String personId) async {
    saved = {
      'schema_version': 1,
      'id': 'sample-${++_sessions}',
      'person_id': personId,
      'revision': 0,
      'active_turn': null,
      'last_outcome': null,
      'data_classes': ['synthetic'],
      'messages': <Map<String, Object?>>[],
    };
    return _result();
  }

  @override
  Future<AgentFixtureResult> resumeAgentFixture(String personId) async {
    if (failLoad) throw StateError('synthetic load failure');
    if (hangLoad) await Completer<void>().future;
    if (loadError != null) throw loadError!;
    return saved == null ? startAgentFixture(personId) : _result();
  }

  @override
  Future<AgentFixtureResult> loadAgentFixture(
    String personId,
    String sessionId,
  ) async => _result();

  AgentFixtureResult _result() =>
      AgentFixtureResult.fromJson({'session': saved, 'events': []});

  @override
  Future<AgentFixtureResult> recoverAgentFixture(AgentSession session) async {
    recoveries++;
    saved!['active_turn'] = null;
    saved!['revision'] = session.revision + 1;
    saved!['last_outcome'] = {'status': 'halted', 'reason': 'interrupted'};
    return _result();
  }

  @override
  Future<AgentFixtureResult> runAgentFixture(
    AgentSession session,
    AgentFixturePrompt prompt,
  ) async {
    await beginAgentFixtureRun(session, prompt);
    _finish();
    await releaseAgentFixtureRun(session);
    return _result();
  }

  @override
  Future<AgentRunUpdate> beginAgentFixtureRun(
    AgentSession session,
    AgentFixturePrompt prompt,
  ) async {
    if (_original != null) throw StateError('run already exists');
    begins++;
    _original = session;
    _events.clear();
    _done = false;
    saved!['active_turn'] = 'turn-$begins';
    saved!['last_outcome'] = null;
    _event({'kind': 'started'});
    _message({
      'kind': 'user',
      'turn_id': 'turn-$begins',
      'text': prompt.sampleText,
    });
    _event({
      'kind': 'model_started',
      'iteration': 0,
      'placement': 'device_local',
    });
    return _update(0);
  }

  @override
  Future<AgentRunUpdate> pollAgentFixtureRun(
    AgentSession session,
    int afterSequence,
  ) async {
    if (failPoll) {
      failPoll = false;
      throw StateError('synthetic response loss');
    }
    if (!hold && !_done) _finish(responseFailure);
    return _update(afterSequence);
  }

  @override
  Future<AgentRunUpdate> stopAgentFixtureRun(AgentSession session) async {
    stops++;
    if (!_done) _finish('cancelled');
    return _update(0);
  }

  @override
  Future<AgentRunUpdate> releaseAgentFixtureRun(AgentSession session) async {
    if (!_done) throw StateError('not finished');
    releases++;
    final result = _update(0);
    _original = null;
    return result;
  }

  void _finish([String? failure]) {
    if (failure == null) {
      _event({
        'kind': 'capability_started',
        'call_id': 'call-$begins',
        'capability_id': 'fixture.schedule.read',
      });
      _message({
        'kind': 'capability',
        'turn_id': 'turn-$begins',
        'call_id': 'call-$begins',
        'capability_id': 'fixture.schedule.read',
        'input': 'sample-day',
        'result': {
          'Ok': capabilityOutput ?? 'Synthetic timeline: Design review 10:00–11:00; free 11:00–12:00.',
        },
      });
      _message({
        'kind': 'assistant',
        'turn_id': 'turn-$begins',
        'text': 'Sample briefing: Design review is at 10:00. There is a free hour afterward. No connected sources were read.',
      });
    }
    saved!['active_turn'] = null;
    saved!['last_outcome'] = failure == null
        ? {'status': 'completed'}
        : {'status': 'halted', 'reason': failure};
    saved!['revision'] = (saved!['revision']! as int) + 1;
    _event({
      'kind': 'finished',
      'revision': saved!['revision'],
      'outcome': saved!['last_outcome'],
    });
    _done = true;
  }

  void _message(Map<String, Object?> message) {
    saved!['messages'] = [...saved!['messages']! as List, message];
    saved!['revision'] = (saved!['revision']! as int) + 1;
    _event({
      'kind': 'message_committed',
      'revision': saved!['revision'],
      'message': message,
    });
  }

  void _event(Map<String, Object?> event) => _events.add({
    'schema_version': 1,
    'session_id': _original!.id,
    'turn_id': 'turn-$begins',
    'event': event,
  });

  AgentRunUpdate _update(int afterSequence) => AgentRunUpdate.fromJson({
    'session_id': _original!.id,
    'expected_revision': _original!.revision,
    'next_sequence': _events.length,
    'events': _events.skip(afterSequence).toList(),
    'done': _done,
    'session':
        _done &&
            (includeSessionWithFailure ||
                !(omitSessionOnFailure && responseFailure != null))
        ? saved
        : null,
    'failure': _done && (omitSessionOnFailure || includeSessionWithFailure)
        ? responseFailure
        : null,
    'recovery_action':
        _done && (omitSessionOnFailure || includeSessionWithFailure)
        ? responseRecoveryAction
        : null,
  });
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
    final failure = owner.responseFailure;
    final run = AppRunSnapshot(
      runId: 'test-run-${owner.begins}',
      sessionId: request.session.id,
      revision: 1,
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
                    'reload_required': owner.omitSessionOnFailure.toString(),
                  },
                ),
              ],
        finalMessageRef: null,
      ),
    );
    readModel.applyRunSnapshot(run);
    onRun(run);
    return ConversationTurnCompletion(run: run, session: request.session);
  }

  @override
  Future<void> cancelConversationTurn(
    AgentConversationTurnRequest request,
  ) async {
    owner.stops++;
  }
}
