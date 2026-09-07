import 'dart:io';

import 'package:floe_client/features/agent/agent_fixture_gateway.dart';
import 'package:floe_client/features/agent/agent_expert_result.dart';
import 'package:floe_client/features/agent/agent_controller.dart';
import 'package:floe_client/features/day_canvas/application/ffi_day_gateway.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'native streaming controller stops, releases and resumes completed turns',
    () async {
      final library = File('../../target/debug/libfloe_ffi.dylib').absolute;
      expect(
        library.existsSync(),
        isTrue,
        reason: 'Run cargo build -p floe-ffi.',
      );
      final directory = await Directory.systemTemp.createTemp(
        'floe-agent-stream-',
      );
      final gateway = await FfiDayGateway.open(
        libraryPath: library.path,
        databasePath: '${directory.path}/fixture.db',
      );
      final controller = AgentController(
        gateway: gateway,
        personId: localPersonId,
      );
      addTearDown(() async {
        controller.dispose();
        await gateway.close();
        await directory.delete(recursive: true);
      });
      await controller.load();
      final running = controller.send(AgentFixturePrompt.today);
      final deadline = DateTime.now().add(const Duration(seconds: 5));
      while (controller.messages.isEmpty) {
        expect(DateTime.now().isBefore(deadline), isTrue);
        await Future<void>.delayed(const Duration(milliseconds: 10));
      }
      expect(controller.running, isTrue);
      expect(controller.messages, hasLength(1));
      await controller.stop();
      await running;
      expect(controller.failure, 'cancelled');
      expect(controller.needsReload, isFalse);
      await controller.retry();
      expect(controller.session!.lastOutcome!.completed, isTrue);
      expect(controller.messages, hasLength(4));
      final resumed = await gateway.resumeAgentFixture(localPersonId);
      expect(resumed.session.id, controller.session!.id);
      expect(resumed.session.revision, controller.session!.revision);
      expect(resumed.session.messages, hasLength(4));
    },
  );

  test(
    'Agent fixture crosses Dart/C ABI and resumes after core restart',
    () async {
      final library = File('../../target/debug/libfloe_ffi.dylib').absolute;
      expect(
        library.existsSync(),
        isTrue,
        reason: 'Run cargo build -p floe-ffi.',
      );
      final directory = await Directory.systemTemp.createTemp(
        'floe-agent-test-',
      );
      FfiDayGateway? gateway;
      addTearDown(() async {
        await gateway?.close();
        await directory.delete(recursive: true);
      });
      gateway = await FfiDayGateway.open(
        libraryPath: library.path,
        databasePath: '${directory.path}/fixture.db',
      );
      final initial = await gateway.startAgentFixture(localPersonId);
      expect(initial.session.messages, isEmpty);
      expect(initial.session.dataClasses, ['synthetic']);
      final first = await gateway.runAgentFixture(
        initial.session,
        AgentFixturePrompt.today,
      );
      expect(first.session.lastOutcome!.completed, isTrue);
      expect(first.session.messages, hasLength(3));
      expect(first.session.messages[1], isA<AgentCapabilityMessage>());
      final capability = first.session.messages[1] as AgentCapabilityMessage;
      final expert = AgentExpertResult.tryParse(
        capability.output,
        callId: capability.callId,
        personId: localPersonId,
      );
      expect(expert, isNotNull);
      expect(expert!.expert, 'floe.schedule');
      expect(expert.insights.first.title, 'Design review');
      expect(expert.insights.last.start!.hour, 11);
      expect(first.events.first.event, isA<AgentStarted>());
      expect(first.events.last.event, isA<AgentFinished>());
      expect(
        first.events.where((event) => event.event is AgentMessageCommitted),
        hasLength(3),
      );
      await expectLater(
        gateway.runAgentFixture(initial.session, AgentFixturePrompt.today),
        throwsA(
          isA<FfiDayGatewayException>().having(
            (error) => error.code,
            'code',
            'conflict',
          ),
        ),
      );
      await expectLater(
        gateway.loadAgentFixture(
          '00000000-0000-4000-8000-000000000002',
          first.session.id,
        ),
        throwsA(
          isA<FfiDayGatewayException>().having(
            (error) => error.code,
            'code',
            'not_found',
          ),
        ),
      );
      await gateway.close();
      gateway = null;
      gateway = await FfiDayGateway.open(
        libraryPath: library.path,
        databasePath: '${directory.path}/fixture.db',
      );
      final restored = await gateway.loadAgentFixture(
        localPersonId,
        first.session.id,
      );
      expect(restored.session.revision, first.session.revision);
      expect(restored.session.messages, hasLength(3));
      expect(
        (restored.session.messages[1] as AgentCapabilityMessage).output,
        capability.output,
      );
      expect(restored.events, isEmpty);
      final next = await gateway.runAgentFixture(
        restored.session,
        AgentFixturePrompt.followUp,
      );
      expect(next.session.messages, hasLength(5));
      final stalled = await gateway.runAgentFixture(
        next.session,
        AgentFixturePrompt.repeatedCall,
      );
      expect(stalled.session.lastOutcome!.failure, 'stalled');
      expect(stalled.session.activeTurn, isNull);
      final retry = await gateway.runAgentFixture(
        stalled.session,
        AgentFixturePrompt.today,
      );
      expect(retry.session.lastOutcome!.completed, isTrue);
    },
  );

  test('Agent event decoder rejects unknown versions and kinds', () {
    final event = <String, Object?>{
      'schema_version': 1,
      'session_id': 'session',
      'turn_id': 'turn',
      'event': {'kind': 'started'},
    };
    expect(AgentEvent.fromJson(event).event, isA<AgentStarted>());
    expect(
      () => AgentEvent.fromJson({...event, 'schema_version': 2}),
      throwsFormatException,
    );
    expect(
      () => AgentEvent.fromJson({
        ...event,
        'event': {'kind': 'execute_without_review'},
      }),
      throwsFormatException,
    );
  });

  test('Agent decoder rejects incomplete capability and outcome records', () {
    final message = <String, Object?>{
      'kind': 'capability',
      'turn_id': 'turn',
      'call_id': 'call',
      'capability_id': 'fixture.read',
      'input': 'sample',
    };
    for (final result in [
      {},
      {'Ok': 'result', 'Err': 'failure'},
    ]) {
      expect(
        () => AgentMessage.fromJson({...message, 'result': result}),
        throwsFormatException,
      );
    }
    expect(
      () => AgentOutcome.fromJson({'status': 'halted'}),
      throwsFormatException,
    );
    expect(
      () =>
          AgentOutcome.fromJson({'status': 'completed', 'reason': 'cancelled'}),
      throwsFormatException,
    );
  });
}
