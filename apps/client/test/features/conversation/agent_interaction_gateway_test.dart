import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/features/conversation/application/agent_interaction_gateway.dart';
import 'package:floe_client/features/conversation/domain/agent_interaction.dart';
import 'package:floe_client/features/conversation/infrastructure/app_wire_conversation_client.dart';

void main() {
  test('interaction resolve uses the strict protocol digest field', () async {
    final fixture = jsonDecode(
      File(
        '../../crates/bindings/protocol/tests/fixtures/app_wire_v2/interaction_resolve.json',
      ).readAsStringSync(),
    ) as Map<String, dynamic>;
    final fixtureCommand = fixture['command'] as Map<String, dynamic>;
    final fixtureDigest = fixtureCommand['target_digest'] as String;
    final transport = _RecordingTransport(
      const AppWireTransportException(
        'validation',
        'Request was not admitted.',
        commandOutcome: CommandOutcome.notAdmitted,
      ),
    );
    final client = AppWireConversationClient(
      transport,
      newId: _ids([
        fixture['command_id'] as String,
        fixture['request_id'] as String,
      ]),
    );
    final command = client.prepareInteractionResolve(
      interactionId: fixtureCommand['interaction_id'] as String,
      sessionId: fixtureCommand['session_id'] as String,
      expectedRevision: fixtureCommand['expected_revision'] as int,
      decision: AgentInteractionDecision.approve,
      targetDigest: fixtureDigest,
    );

    await expectLater(
      client.submitInteractionResolve(command),
      throwsA(isA<AppWireTransportException>()),
    );

    expect(transport.commands.single, fixture);
    final payload =
        transport.commands.single['command'] as Map<String, dynamic>;
    expect(payload['target_digest'], fixtureDigest);
    expect(payload.containsKey('reviewed_digest'), isFalse);
  });

  test(
    'uncertain interaction decisions and refreshes retry the same ID',
    () async {
      const indeterminate = AppWireTransportException(
        'internal',
        'The acknowledgement is uncertain.',
        commandOutcome: CommandOutcome.indeterminate,
      );
      final transport = _RecordingTransport(indeterminate);
      final client = AppWireConversationClient(transport, newId: _ids());
      final gateway = NativeAgentInteractionGateway(client);

      for (var attempt = 0; attempt < 2; attempt++) {
        await expectLater(
          gateway.decideInteraction(
            _snapshot,
            AgentInteractionDecision.approve,
          ),
          throwsA(isA<AppWireTransportException>()),
        );
      }
      expect(
        transport.commands[0]['command_id'],
        transport.commands[1]['command_id'],
      );

      final refreshTransport = _RecordingTransport(indeterminate);
      final refreshClient = AppWireConversationClient(
        refreshTransport,
        newId: _ids(),
      );
      final refreshGateway = NativeAgentInteractionGateway(refreshClient);
      for (var attempt = 0; attempt < 2; attempt++) {
        await expectLater(
          refreshGateway.refreshInteraction(_snapshot),
          throwsA(isA<AppWireTransportException>()),
        );
      }
      expect(
        refreshTransport.commands[0]['command_id'],
        refreshTransport.commands[1]['command_id'],
      );
    },
  );
}

final AgentInteractionSnapshot _snapshot = AgentInteractionSnapshot(
  id: '00000000-0000-4000-8000-000000000001',
  sessionId: '00000000-0000-4000-8000-000000000002',
  originRunId: '00000000-0000-4000-8000-000000000003',
  kind: AgentInteractionKind.sourceAccess,
  state: AgentInteractionState.pending,
  revision: 1,
  targetDigest: List<String>.filled(64, 'a').join(),
  createdAtUnixMs: 1,
  expiresAtUnixMs: 2,
  target: const AgentNavigationTarget(
    destination: 'connection_settings',
    sourceLabel: 'Test calendar',
  ),
  actions: const [AgentInteractionAction.allow],
);

String Function() _ids([List<String> ids = const []]) {
  var next = 0;
  return () {
    if (next < ids.length) return ids[next++];
    next++;
    return '00000000-0000-4000-8000-${next.toString().padLeft(12, '0')}';
  };
}

final class _RecordingTransport implements AppWireTransport {
  _RecordingTransport(this.failure);

  final AppWireTransportException failure;
  final List<Map<String, dynamic>> commands = [];

  @override
  Future<Map<String, dynamic>> commandV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) async {
    commands.add(Map<String, dynamic>.from(request));
    throw failure;
  }

  @override
  Future<Map<String, dynamic>> queryV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) async => throw UnimplementedError();

  @override
  Future<Map<String, dynamic>> eventsV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) async => throw UnimplementedError();

  @override
  Future<void> close() async {}
}
