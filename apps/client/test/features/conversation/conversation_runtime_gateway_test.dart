import 'dart:async';

import 'package:floe_client/features/conversation/application/agent_conversation_gateway.dart';

import 'package:floe_client/features/conversation/domain/agent_session.dart';

import 'package:floe_client/features/conversation/application/conversation_runtime_gateway.dart';
import 'package:floe_client/app/runtime/floe_client.dart';
import 'package:floe_client/app/runtime/app_read_model.dart';
import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'drives a product turn through command events and durable queries',
    () async {
      final transport = _ConversationTransport();
      final model = AppReadModel();
      addTearDown(model.dispose);
      final gateway = NativeConversationRuntimeGateway(
        client: FloeClient(transport, newId: _ids()),
        readModel: model,
        loadSession: (_, _) async => _session(revision: 2),
        observationInterval: Duration.zero,
      );
      final observed = <AppRunState>[];

      final completion = await gateway.runConversationTurn(
        AgentConversationTurnRequest(
          session: _session(),
          text: 'Hello',
          profileId: 'local-fast',
        ),
        onRun: (run) => observed.add(run.state),
      );

      expect(transport.commandKinds, ['conversation.start_turn']);
      expect((transport.commandRequests.single['command'] as Map)['profile'], {
        'kind': 'explicit',
        'profile_id': 'local-fast',
      });
      expect(transport.queryKinds, everyElement('conversation.get_run'));
      expect(transport.eventReads, greaterThanOrEqualTo(3));
      expect(observed, [AppRunState.executing, AppRunState.finished]);
      expect(completion.session.revision, 2);
      expect(completion.run.state, AppRunState.finished);
      expect(model.conversation.canSend(_sessionId), isTrue);
    },
  );

  test(
    'cancel before admission acknowledgement reuses one stable command',
    () async {
      final transport = _ConversationTransport(
        blockAdmission: true,
        loseCancelOnce: true,
      );
      final model = AppReadModel();
      addTearDown(model.dispose);
      final gateway = NativeConversationRuntimeGateway(
        client: FloeClient(transport, newId: _ids()),
        readModel: model,
        loadSession: (_, _) async => _session(revision: 2),
        observationInterval: Duration.zero,
      );
      final request = AgentConversationTurnRequest(
        session: _session(),
        text: 'Stop this turn',
      );
      final running = gateway.runConversationTurn(request, onRun: (_) {});
      await transport.admissionStarted.future;

      final firstCancel = gateway.cancelConversationTurn(request);
      final secondCancel = gateway.cancelConversationTurn(request);
      transport.releaseAdmission.complete();
      await Future.wait([firstCancel, secondCancel]);
      final completion = await running;

      expect(transport.commandKinds, [
        'conversation.start_turn',
        'conversation.cancel_run',
        'conversation.cancel_run',
      ]);
      expect(
        transport.commandRequests
            .skip(1)
            .map((request) => request['command_id']),
        everyElement(transport.commandRequests[1]['command_id']),
      );
      expect(completion.run.report!.execution, 'cancelled');
      expect(model.conversation.canSend(_sessionId), isTrue);
    },
  );

  test('lost admission acknowledgement recovers the same command', () async {
    final transport = _ConversationTransport(loseAdmissionOnce: true);
    final model = AppReadModel();
    addTearDown(model.dispose);
    final gateway = NativeConversationRuntimeGateway(
      client: FloeClient(transport, newId: _ids()),
      readModel: model,
      loadSession: (_, _) async => _session(revision: 2),
      observationInterval: Duration.zero,
    );

    final completion = await gateway.runConversationTurn(
      AgentConversationTurnRequest(session: _session(), text: 'Hello'),
      onRun: (_) {},
    );

    expect(
      transport.commandKinds.where((kind) => kind == 'conversation.start_turn'),
      hasLength(1),
    );
    expect(transport.queryKinds.first, 'conversation.get_command');
    expect(completion.run.state, AppRunState.finished);
  });

  test('continuation uses the durable source Run generation', () async {
    final transport = _ConversationTransport();
    final model = AppReadModel();
    addTearDown(model.dispose);
    final gateway = NativeConversationRuntimeGateway(
      client: FloeClient(transport, newId: _ids()),
      readModel: model,
      loadSession: (_, _) async => _session(revision: 2),
      observationInterval: Duration.zero,
    );

    await gateway.runConversationTurn(
      AgentConversationTurnRequest(
        session: _session(continuation: true),
        text: 'Continue',
        continuation: true,
      ),
      onRun: (_) {},
    );

    final command = transport.commandRequests.single;
    expect((command['command'] as Map)['mode'], {
      'kind': 'continue',
      'continuation_ref': {
        'run_id': _sourceRunId,
        'executor_generation': 9,
        'level': 2,
      },
    });
  });

  test(
    'retry validates and serializes a durable terminal source Run',
    () async {
      final transport = _ConversationTransport();
      final model = AppReadModel();
      addTearDown(model.dispose);
      final gateway = NativeConversationRuntimeGateway(
        client: FloeClient(transport, newId: _ids()),
        readModel: model,
        loadSession: (_, _) async => _session(revision: 2),
        observationInterval: Duration.zero,
      );

      await gateway.runConversationTurn(
        AgentConversationTurnRequest(
          session: _session(),
          text: 'Retry safely',
          retryOf: _sourceRunId,
        ),
        onRun: (_) {},
      );

      expect(
        transport.commandRequests.single['command'],
        containsPair('retry_of', _sourceRunId),
      );
      expect(transport.queryKinds.first, 'conversation.get_run');
    },
  );

  test('unsupported host route fails before StartTurn admission', () async {
    final transport = _ConversationTransport();
    final model = AppReadModel();
    addTearDown(model.dispose);
    final gateway = NativeConversationRuntimeGateway(
      client: FloeClient(transport, newId: _ids()),
      readModel: model,
      loadSession: (_, _) async => _session(revision: 2),
      beforeStartTurn: () async => throw StateError('route unsupported'),
      observationInterval: Duration.zero,
    );

    await expectLater(
      gateway.runConversationTurn(
        AgentConversationTurnRequest(session: _session(), text: 'Hello'),
        onRun: (_) {},
      ),
      throwsStateError,
    );

    expect(transport.commandKinds, isEmpty);
    expect(model.conversation.syncState, AppReadSyncState.synchronized);
  });
}

const _sessionId = '00000000-0000-4000-8000-000000000301';
const _runId = '00000000-0000-4000-8000-000000000303';
const _sourceRunId = '00000000-0000-4000-8000-000000000304';

AgentSession _session({int revision = 0, bool continuation = false}) =>
    AgentSession.fromJson({
      'schema_version': 1,
      'id': _sessionId,
      'person_id': 'person',
      'revision': revision,
      'data_classes': ['personal'],
      'messages': const <Object?>[],
      'active_turn': null,
      'last_outcome': revision == 0 ? null : {'status': 'completed'},
      'continuation': continuation
          ? {
              'turn_id': _sourceRunId,
              'level': 1,
              'placement': 'device_local',
              'usage': <String, Object?>{},
            }
          : null,
    });

String Function() _ids() {
  var next = 100;
  return () {
    final suffix = (next++).toString().padLeft(12, '0');
    return '00000000-0000-4000-8000-$suffix';
  };
}

final class _ConversationTransport implements AppWireTransport {
  _ConversationTransport({
    this.blockAdmission = false,
    this.loseAdmissionOnce = false,
    this.loseCancelOnce = false,
  });

  final bool blockAdmission;
  final bool loseAdmissionOnce;
  final bool loseCancelOnce;
  final Completer<void> admissionStarted = Completer<void>();
  final Completer<void> releaseAdmission = Completer<void>();
  final List<String> commandKinds = [];
  final List<Map<String, dynamic>> commandRequests = [];
  final List<String> queryKinds = [];
  int eventReads = 0;
  int runReads = 0;
  bool cancelled = false;
  bool admissionLost = false;
  bool cancelLost = false;
  String? acceptedCommandId;

  @override
  Future<Map<String, dynamic>> commandV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) async {
    final command = Map<String, dynamic>.from(request['command'] as Map);
    commandRequests.add(request);
    final kind = command['kind']! as String;
    commandKinds.add(kind);
    if (kind == 'conversation.start_turn') {
      if (!admissionStarted.isCompleted) admissionStarted.complete();
      if (blockAdmission) await releaseAdmission.future;
      acceptedCommandId = request['command_id']! as String;
      if (loseAdmissionOnce && !admissionLost) {
        admissionLost = true;
        throw StateError('Lost accepted admission response.');
      }
      return {
        'kind': 'command_receipt',
        'command_id': request['command_id'],
        'runtime_epoch': 7,
        'admission': 'accepted',
        'run_id': _runId,
        'session_revision': 1,
      };
    }
    cancelled = true;
    if (loseCancelOnce && !cancelLost) {
      cancelLost = true;
      throw StateError('Lost accepted cancellation response.');
    }
    return {
      'kind': 'cancel_run_receipt',
      'command_id': request['command_id'],
      'run_id': _runId,
      'runtime_epoch': 7,
      'outcome': 'accepted',
    };
  }

  @override
  Future<Map<String, dynamic>> eventsV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) async {
    eventReads++;
    if (request['cursor'] == null) {
      return {
        'kind': 'resync_required',
        'runtime_epoch': 7,
        'snapshot_cursor': 0,
      };
    }
    return {
      'kind': 'events',
      'runtime_epoch': 7,
      'next_cursor': request['cursor'],
      'events': const <Object?>[],
    };
  }

  @override
  Future<Map<String, dynamic>> queryV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) async {
    final query = Map<String, dynamic>.from(request['query'] as Map);
    final kind = query['kind']! as String;
    queryKinds.add(kind);
    if (kind == 'conversation.get_command') {
      return {
        'kind': 'command_receipt',
        'command_id': acceptedCommandId,
        'runtime_epoch': 7,
        'admission': 'accepted',
        'run_id': _runId,
        'session_revision': 1,
      };
    }
    if (kind != 'conversation.get_run') {
      throw StateError('Unexpected query: $kind');
    }
    final requestedRunId = query['run_id']! as String;
    if (requestedRunId == _sourceRunId) {
      return {
        'kind': 'run_snapshot',
        'run_id': _sourceRunId,
        'session_id': _sessionId,
        'revision': 2,
        'runtime_epoch': 7,
        'executor_generation': 9,
        'state': 'finished',
        'progress': 'partial',
        'task_refs': const <Object?>[],
        'attempt_refs': const <Object?>[],
        'report': {
          'execution': 'partial',
          'reply': 'generated',
          'issues': const <Object?>[],
          'action_refs': const <Object?>[],
        },
      };
    }
    runReads++;
    final finished = cancelled || runReads > 1;
    return {
      'kind': 'run_snapshot',
      'run_id': _runId,
      'session_id': _sessionId,
      'revision': finished ? 2 : 1,
      'runtime_epoch': 7,
      'executor_generation': 1,
      'state': finished ? 'finished' : 'executing',
      'progress': finished ? (cancelled ? 'cancelled' : 'completed') : 'model',
      'task_refs': const <Object?>[],
      'attempt_refs': const <Object?>[],
      'report': finished
          ? {
              'execution': cancelled ? 'cancelled' : 'completed',
              'reply': cancelled ? 'not_produced' : 'generated',
              'issues': const <Object?>[],
              'action_refs': const <Object?>[],
            }
          : null,
    };
  }

  @override
  Future<void> close() async {}
}
