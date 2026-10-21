import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/features/voice/infrastructure/app_wire_voice_gateway.dart';

void main() {
  test(
    'Start reuses an admitted command after an uncertain response',
    () async {
      final transport = _Transport();
      final surface = 'd3c5b1bc-555e-4cd3-8f17-e694f9a4c3b9';
      var calls = 0;
      transport.onQuery = (request) async => {
        'kind': request['query']['kind'],
        'status': null,
      };
      transport.onCommand = (request) async {
        calls++;
        if (calls == 1) {
          throw const AppWireTransportException(
            'capability_unavailable',
            'Native audio is unavailable.',
            commandOutcome: CommandOutcome.admitted,
          );
        }
        return {
          'kind': 'voice.start',
          'status': {
            'command_id': request['command_id'],
            'surface_session_id': surface,
            'state': 'cancelled',
            'media': null,
            'remote': null,
            'stop_requested': null,
            'local_lease_released': null,
          },
        };
      };

      final gateway = AppWireVoiceGateway(transport);
      await expectLater(
        gateway.start(surface),
        throwsA(isA<AppWireTransportException>()),
      );
      final pending = gateway.pendingCommandId;
      expect(pending, isNotNull);

      final replay = await gateway.start(surface);
      expect(replay.commandId, pending);
      expect(replay.state.name, 'cancelled');
      expect(gateway.pendingCommandId, isNull);
      expect(transport.commands, hasLength(2));
      expect(transport.commands[0]['command_id'], pending);
      expect(transport.commands[1]['command_id'], pending);
      expect(transport.commands[0]['command'], {
        'kind': 'voice.start',
        'surface_session_id': surface,
      });
      expect(
        transport.commands[1]['command'],
        transport.commands[0]['command'],
      );
    },
  );

  test('status lookup is read-only and accepts a missing receipt', () async {
    final transport = _Transport()
      ..onQuery = (request) async {
        return {'kind': 'voice.start.status', 'status': null};
      };
    final gateway = AppWireVoiceGateway(transport);

    final status = await gateway.readStatus(
      'd3c5b1bc-555e-4cd3-8f17-e694f9a4c3b9',
    );

    expect(status, isNull);
    expect(transport.commands, isEmpty);
    expect(transport.queries, hasLength(1));
    expect(transport.queries.single['query'], {
      'kind': 'voice.start.status',
      'command_id': 'd3c5b1bc-555e-4cd3-8f17-e694f9a4c3b9',
    });
  });

  test(
    'Stop carries the exact bound call and clears it only after readback',
    () async {
      const surfaceId = 'fb0a4201-8e82-4a18-bfc1-0cf6bd35806e';
      final transport = _Transport();
      transport.onQuery = (request) async => {
        'kind': request['query']['kind'],
        'status': null,
      };
      transport.onCommand = (request) async {
        final kind = request['command']['kind'];
        final startId = kind == 'voice.start'
            ? request['command_id']
            : request['command']['voice_session_id'];
        return {
          'kind': kind,
          'status': {
            'command_id': startId,
            'surface_session_id': surfaceId,
            'state': 'bound',
            'media': kind == 'voice.stop' ? 'stopped' : 'muted',
            'remote': kind == 'voice.stop' ? 'closed' : 'connected',
            'stop_requested': kind == 'voice.stop',
            'local_lease_released': kind == 'voice.stop',
          },
        };
      };
      final gateway = AppWireVoiceGateway(transport);

      final started = await gateway.start(surfaceId);
      final stopped = await gateway.stop();

      expect(started.state.name, 'bound');
      expect(stopped.localLeaseReleased, isTrue);
      expect(gateway.pendingCommandId, isNull);
      expect(transport.commands, hasLength(2));
      expect(transport.commands.last['command']['kind'], 'voice.stop');
      expect(
        transport.commands.last['command']['voice_session_id'],
        started.commandId,
      );
      expect(
        transport.commands.last['command']['surface_session_id'],
        surfaceId,
      );
    },
  );

  test(
    'process restart recovers the journal command before allocating a new ID',
    () async {
      const commandId = 'd3c5b1bc-555e-4cd3-8f17-e694f9a4c3b9';
      const surfaceId = 'fb0a4201-8e82-4a18-bfc1-0cf6bd35806e';
      final transport = _Transport()
        ..onQuery = (request) async {
          expect(request['query'], {'kind': 'voice.start.recoverable'});
          return {
            'kind': 'voice.start.recoverable',
            'status': _startStatus(commandId, surfaceId, 'preparing'),
          };
        }
        ..onCommand = (request) async => {
          'kind': 'voice.start',
          'status': _startStatus(request['command_id'], surfaceId, 'preparing'),
        };
      final gateway = AppWireVoiceGateway(transport);

      final recovered = await gateway.recoverPendingStart();
      expect(recovered?.commandId, commandId);
      expect(gateway.pendingCommandId, commandId);
      expect(
        transport.commands,
        isEmpty,
        reason: 'recovery is a read-only query',
      );

      final replay = await gateway.start(surfaceId);
      expect(replay.commandId, commandId);
      expect(transport.commands.single['command_id'], commandId);
      expect(transport.commands.single['command']['kind'], 'voice.start');
    },
  );

  test(
    'recovered cancellation is exact and the next Start gets a new ID',
    () async {
      const commandId = 'd3c5b1bc-555e-4cd3-8f17-e694f9a4c3b9';
      const surfaceId = 'fb0a4201-8e82-4a18-bfc1-0cf6bd35806e';
      Map<String, Object?>? recoverable = _startStatus(
        commandId,
        surfaceId,
        'preparing',
      );
      final transport = _Transport()
        ..onQuery = (request) async {
          final kind = request['query']['kind'];
          if (kind == 'voice.start.recoverable') {
            return {'kind': kind, 'status': recoverable};
          }
          if (kind == 'voice.start.status') {
            return {
              'kind': kind,
              'status': _startStatus(commandId, surfaceId, 'preparing'),
            };
          }
          throw StateError('Unexpected query kind: $kind');
        }
        ..onCommand = (request) async {
          final kind = request['command']['kind'];
          if (kind == 'voice.start.cancel') {
            recoverable = null;
            return {
              'kind': kind,
              'status': _startStatus(commandId, surfaceId, 'cancelled'),
            };
          }
          return {
            'kind': kind,
            'status': _startStatus(
              request['command_id'],
              surfaceId,
              'cancelled',
            ),
          };
        };
      final gateway = AppWireVoiceGateway(transport);

      final cancelled = await gateway.cancelPendingStart();
      expect(cancelled.commandId, commandId);
      expect(cancelled.state.name, 'cancelled');
      expect(transport.commands.single['command_id'], commandId);
      expect(transport.commands.single['command'], {
        'kind': 'voice.start.cancel',
        'surface_session_id': surfaceId,
      });
      expect(gateway.pendingCommandId, isNull);

      final fresh = await gateway.start(surfaceId);
      expect(fresh.state.name, 'cancelled');
      expect(transport.commands, hasLength(2));
      expect(transport.commands.last['command']['kind'], 'voice.start');
      expect(transport.commands.last['command_id'], isNot(commandId));
    },
  );

  test('ended bound Start is not recovered for reuse', () async {
    const endedCommandId = 'd3c5b1bc-555e-4cd3-8f17-e694f9a4c3b9';
    final transport = _Transport()
      ..onQuery = (request) async {
        return {'kind': request['query']['kind'], 'status': null};
      }
      ..onCommand = (request) async => {
        'kind': 'voice.start',
        'status': _startStatus(
          request['command_id'],
          'd3c5b1bc-555e-4cd3-8f17-e694f9a4c3b9',
          'cancelled',
        ),
      };
    final gateway = AppWireVoiceGateway(transport);
    const surfaceId = 'd3c5b1bc-555e-4cd3-8f17-e694f9a4c3b9';

    final fresh = await gateway.start(surfaceId);

    expect(fresh.state.name, 'cancelled');
    expect(transport.queries.single['query'], {
      'kind': 'voice.start.recoverable',
    });
    expect(transport.commands.single['command_id'], isNot(endedCommandId));
  });

  test('bound status requires all call lifecycle fields', () async {
    final transport = _Transport()
      ..onQuery = (request) async => {
        'kind': 'voice.start.status',
        'status': {
          'command_id': request['query']['command_id'],
          'surface_session_id': 'd3c5b1bc-555e-4cd3-8f17-e694f9a4c3b9',
          'state': 'bound',
          'media': 'muted',
          'remote': 'not_started',
          'stop_requested': true,
          'local_lease_released': null,
        },
      };
    final gateway = AppWireVoiceGateway(transport);

    await expectLater(
      gateway.readStatus('d3c5b1bc-555e-4cd3-8f17-e694f9a4c3b9'),
      throwsA(isA<FormatException>()),
    );
    expect(transport.commands, isEmpty);
  });

  test(
    'unbound status still requires explicit null lifecycle fields',
    () async {
      final transport = _Transport()
        ..onQuery = (request) async => {
          'kind': 'voice.start.status',
          'status': {
            'command_id': request['query']['command_id'],
            'surface_session_id': 'd3c5b1bc-555e-4cd3-8f17-e694f9a4c3b9',
            'state': 'cancelled',
          },
        };
      final gateway = AppWireVoiceGateway(transport);

      await expectLater(
        gateway.readStatus('d3c5b1bc-555e-4cd3-8f17-e694f9a4c3b9'),
        throwsA(isA<FormatException>()),
      );
      expect(transport.commands, isEmpty);
    },
  );
}

final class _Transport implements AppWireTransport {
  final List<Map<String, dynamic>> commands = [];
  final List<Map<String, dynamic>> queries = [];
  Future<Map<String, dynamic>> Function(Map<String, dynamic>)? onCommand;
  Future<Map<String, dynamic>> Function(Map<String, dynamic>)? onQuery;

  @override
  Future<Map<String, dynamic>> commandV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) async {
    commands.add(request);
    final handler = onCommand;
    if (handler == null) throw StateError('Unexpected command.');
    return handler(request);
  }

  @override
  Future<Map<String, dynamic>> queryV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) async {
    queries.add(request);
    final handler = onQuery;
    if (handler == null) throw StateError('Unexpected query.');
    return handler(request);
  }

  @override
  Future<Map<String, dynamic>> eventsV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) async => throw StateError('Unexpected event request.');

  @override
  Future<void> close() async {}
}

Map<String, Object?> _startStatus(
  Object? commandId,
  String surfaceId,
  String state,
) => {
  'command_id': commandId,
  'surface_session_id': surfaceId,
  'state': state,
  'media': null,
  'remote': null,
  'stop_requested': null,
  'local_lease_released': null,
};
