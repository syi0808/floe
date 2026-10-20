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
      transport.onCommand = (request) async {
        calls++;
        transport.commands.add(request);
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
