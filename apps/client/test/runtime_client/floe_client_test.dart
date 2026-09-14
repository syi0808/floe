import 'dart:async';
import 'dart:collection';

import 'package:floe_client/runtime_client/floe_client.dart';
import 'package:floe_client/runtime_client/transport/app_wire_transport.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'StartTurn keeps its command identity across transport retries',
    () async {
      final transport = FakeTransport();
      final identifiers = Queue.of([
        '00000000-0000-4000-8000-000000000201',
        '00000000-0000-4000-8000-000000000202',
        '00000000-0000-4000-8000-000000000203',
      ]);
      final client = FloeClient(transport, newId: identifiers.removeFirst);
      final command = client.prepareStartTurn(
        sessionId: '00000000-0000-4000-8000-000000000204',
        expectedRevision: 3,
        text: 'hello',
      );
      transport.command = (request) async => {
        'kind': 'command_receipt',
        'command_id': request['command_id'],
        'admission': 'accepted',
        'run_id': '00000000-0000-4000-8000-000000000205',
        'session_revision': 4,
      };

      final first = await client.submitStartTurn(command);
      final second = await client.submitStartTurn(command);
      expect(first.commandId, command.commandId);
      expect(second.runId, first.runId);
      expect(
        transport.commandRequests.map((request) => request['request_id']),
        containsAll([
          '00000000-0000-4000-8000-000000000202',
          '00000000-0000-4000-8000-000000000203',
        ]),
      );
      for (final request in transport.commandRequests) {
        expect(request['command_id'], command.commandId);
        expect(request.containsKey('person_id'), isFalse);
        expect(request.containsKey('device_id'), isFalse);
        expect(
          (request['command'] as Map).containsKey('remote_route'),
          isFalse,
        );
      }
    },
  );

  test(
    'query decoding restores command, Run, report, and final message',
    () async {
      final identifiers = Queue.of([
        '00000000-0000-4000-8000-000000000211',
        '00000000-0000-4000-8000-000000000212',
        '00000000-0000-4000-8000-000000000213',
      ]);
      final transport = FakeTransport();
      final client = FloeClient(transport, newId: identifiers.removeFirst);
      transport.query = (request) async {
        final query = request['query'] as Map;
        return switch (query['kind']) {
          'conversation.get_command' => {
            'kind': 'command_receipt',
            'command_id': query['command_id'],
            'admission': 'accepted',
            'run_id': '00000000-0000-4000-8000-000000000214',
            'session_revision': 8,
          },
          'conversation.get_run' => {
            'kind': 'run_snapshot',
            'run_id': query['run_id'],
            'session_id': '00000000-0000-4000-8000-000000000215',
            'revision': 2,
            'runtime_epoch': 7,
            'executor_generation': 1,
            'state': 'finished',
            'progress': 'completed',
            'task_refs': <Object?>[],
            'attempt_refs': <Object?>[],
            'report': {
              'execution': 'completed',
              'reply': 'generated',
              'issues': <Object?>[],
              'action_refs': <Object?>[],
              'final_message_ref': '00000000-0000-4000-8000-000000000214',
            },
          },
          'conversation.get_message' => {
            'kind': 'message',
            'message_id': query['message_id'],
            'role': 'assistant',
            'text': 'done',
          },
          _ => throw StateError('unexpected query'),
        };
      };

      final receipt = await client.getCommand(
        '00000000-0000-4000-8000-000000000216',
      );
      final run = await client.getRun(receipt!.runId);
      final message = await client.getMessage(run.report!.finalMessageRef!);
      expect(run.state, AppRunState.finished);
      expect(run.runtimeEpoch, 7);
      expect(message.text, 'done');
    },
  );

  test('CancelRun keeps command identity across transport retries', () async {
    final identifiers = Queue.of([
      '00000000-0000-4000-8000-000000000231',
      '00000000-0000-4000-8000-000000000232',
      '00000000-0000-4000-8000-000000000233',
    ]);
    final transport = FakeTransport();
    final client = FloeClient(transport, newId: identifiers.removeFirst);
    final command = client.prepareCancelRun(
      '00000000-0000-4000-8000-000000000234',
    );
    transport.command = (request) async => {
      'kind': 'cancel_run_receipt',
      'command_id': request['command_id'],
      'run_id': (request['command'] as Map)['run_id'],
      'outcome': 'accepted',
    };

    final first = await client.submitCancelRun(command);
    final second = await client.submitCancelRun(command);
    expect(first.outcome, AppCancelRunOutcome.accepted);
    expect(second.commandId, command.commandId);
    for (final request in transport.commandRequests) {
      expect(request['command_id'], command.commandId);
      expect((request['command'] as Map)['reason'], 'user_requested');
    }
  });

  test('close settles a pending request once and rejects new work', () async {
    final transport = FakeTransport();
    final response = Completer<Map<String, dynamic>>();
    transport.command = (_) => response.future;
    final identifiers = Queue.of([
      '00000000-0000-4000-8000-000000000221',
      '00000000-0000-4000-8000-000000000222',
    ]);
    final client = FloeClient(transport, newId: identifiers.removeFirst);
    final command = client.prepareStartTurn(
      sessionId: '00000000-0000-4000-8000-000000000223',
      expectedRevision: 0,
      text: 'hello',
    );
    final pending = client.submitStartTurn(command);
    final settled = expectLater(pending, throwsStateError);
    await client.close();
    await settled;
    response.complete({
      'kind': 'command_receipt',
      'command_id': command.commandId,
      'admission': 'accepted',
      'run_id': '00000000-0000-4000-8000-000000000224',
      'session_revision': 1,
    });
    await expectLater(client.getCommand(command.commandId), throwsStateError);
    expect(transport.closed, isTrue);
  });

  test(
    'events require resync then enforce epoch revision and cursor order',
    () async {
      final identifiers = Queue.of([
        '00000000-0000-4000-8000-000000000241',
        '00000000-0000-4000-8000-000000000242',
      ]);
      final transport = FakeTransport();
      final client = FloeClient(transport, newId: identifiers.removeFirst);
      transport.events = (request) async {
        if (!request.containsKey('cursor')) {
          return {
            'kind': 'resync_required',
            'runtime_epoch': 7,
            'snapshot_cursor': 10,
          };
        }
        return {
          'kind': 'events',
          'runtime_epoch': 7,
          'next_cursor': 11,
          'events': [
            {
              'cursor': 11,
              'aggregate_revision': 2,
              'runtime_epoch': 7,
              'event': {
                'kind': 'run_updated',
                'run': {
                  'run_id': '00000000-0000-4000-8000-000000000243',
                  'session_id': '00000000-0000-4000-8000-000000000244',
                  'revision': 2,
                  'runtime_epoch': 7,
                  'executor_generation': 1,
                  'state': 'finished',
                  'progress': 'cancelled',
                  'task_refs': <Object?>[],
                  'attempt_refs': <Object?>[],
                  'report': {
                    'execution': 'cancelled',
                    'reply': 'not_produced',
                    'issues': <Object?>[],
                    'action_refs': <Object?>[],
                  },
                },
              },
            },
          ],
        };
      };

      final initial = await client.readEvents();
      final cursor = (initial as AppEventsResyncRequired).snapshotCursor;
      final page = await client.readEvents(after: cursor) as AppEventsPage;
      expect(page.cursor.cursor, 11);
      expect((page.events.single as AppRunUpdated).run.progress, 'cancelled');
    },
  );
}

final class FakeTransport implements AppWireTransport {
  Future<Map<String, dynamic>> Function(Map<String, dynamic>)? command;
  Future<Map<String, dynamic>> Function(Map<String, dynamic>)? query;
  Future<Map<String, dynamic>> Function(Map<String, dynamic>)? events;
  final List<Map<String, dynamic>> commandRequests = [];
  bool closed = false;

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
  }) => events!(request);

  @override
  Future<void> close() async {
    closed = true;
  }
}
