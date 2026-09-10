import 'package:floe_client/features/agent/agent_controller.dart';
import 'package:floe_client/features/agent/agent_fixture_gateway.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../support/agent_gateway.dart';

void main() {
  test(
    'preambles remain ordered progress messages rather than final answers',
    () async {
      final gateway = TestAgentGateway()..hold = true;
      final controller = AgentController(gateway: gateway, personId: 'test');
      addTearDown(controller.dispose);
      await controller.load();
      final run = controller.send(AgentFixturePrompt.today);
      await Future<void>.delayed(Duration.zero);
      gateway.appendProgress({
        'kind': 'message_committed',
        'revision': 2,
        'message': {
          'kind': 'preamble',
          'turn_id': gateway.saved!['active_turn'],
          'text': 'Checking both sources.',
        },
      });
      await Future<void>.delayed(const Duration(milliseconds: 120));
      expect(controller.running, isTrue);
      expect(controller.messages, hasLength(2));
      expect(controller.messages.last.kind, AgentMessageKind.preamble);
      expect(
        (controller.messages.last as AgentTextMessage).text,
        'Checking both sources.',
      );
      await controller.stop();
      await run;
    },
  );

  test(
    'durable attempt events distinguish Expert reasoning and correction',
    () async {
      final gateway = TestAgentGateway()..hold = true;
      final controller = AgentController(gateway: gateway, personId: 'test');
      addTearDown(controller.dispose);
      await controller.load();
      final run = controller.send(AgentFixturePrompt.today);
      await Future<void>.delayed(Duration.zero);
      gateway.appendProgress({
        'kind': 'model_attempt',
        'record': {
          'id': 'attempt-1',
          'scope_id': 'expert-call',
          'attempt': 1,
          'state': 'started',
          'failure': null,
        },
      });
      await Future<void>.delayed(const Duration(milliseconds: 120));
      expect(controller.progress, AgentProgress.expertModel);
      gateway.appendProgress({
        'kind': 'model_attempt',
        'record': {
          'id': 'attempt-2',
          'scope_id': 'expert-call',
          'attempt': 2,
          'state': 'started',
          'failure': null,
        },
      });
      await Future<void>.delayed(const Duration(milliseconds: 120));
      expect(controller.progress, AgentProgress.correcting);
      expect(gateway.begins, 1);
      expect(controller.messages, hasLength(1));
      await controller.stop();
      await run;
      expect(controller.failure, 'cancelled');
    },
  );

  test('progress is visible before completion and stop retains only committed messages', () async {
    final gateway = TestAgentGateway()..hold = true;
    final controller = AgentController(gateway: gateway, personId: 'test');
    addTearDown(controller.dispose);
    await controller.load();
    final run = controller.send(AgentFixturePrompt.today);
    await Future<void>.delayed(Duration.zero);
    expect(controller.running, isTrue);
    expect(controller.messages, hasLength(1));
    expect(controller.progress, AgentProgress.model);
    await controller.send(AgentFixturePrompt.today);
    expect(gateway.begins, 1);
    await controller.stop();
    await run;
    expect(controller.failure, 'cancelled');
    expect(controller.messages, hasLength(1));
    expect(controller.canRetry, isTrue);
    expect(gateway.releases, 1);
    gateway.hold = false;
    await controller.retry();
    expect(controller.messages, hasLength(4));
    expect(controller.failure, isNull);
    expect(gateway.begins, 2);
  });

  test(
    'read-only reload resolves lost transport before another turn',
    () async {
      final gateway = TestAgentGateway()..failPoll = true;
      final controller = AgentController(gateway: gateway, personId: 'test');
      addTearDown(controller.dispose);
      await controller.load();
      await controller.send(AgentFixturePrompt.today);
      expect(controller.needsReload, isTrue);
      expect(controller.canSend, isFalse);
      expect(gateway.stops, 1);
      expect(gateway.releases, 1);
      await controller.retry();
      expect(gateway.begins, 1);
      await controller.load();
      expect(controller.needsReload, isFalse);
      expect(controller.messages, hasLength(1));
      expect(gateway.begins, 1);
    },
  );

  test(
    'interrupted resume requires explicit recovery without model replay',
    () async {
      final gateway = TestAgentGateway();
      await gateway.startAgentFixture('test');
      gateway.saved!['active_turn'] = 'abandoned';
      final controller = AgentController(gateway: gateway, personId: 'test');
      addTearDown(controller.dispose);
      await controller.load();
      expect(controller.needsRecovery, isTrue);
      expect(controller.canSend, isFalse);
      await controller.send(AgentFixturePrompt.today);
      expect(gateway.begins, 0);
      await controller.recover();
      expect(gateway.recoveries, 1);
      expect(controller.canSend, isTrue);
      expect(gateway.begins, 0);
    },
  );

  test(
    'dispose stops and releases an active run without later notifications',
    () async {
      final gateway = TestAgentGateway()..hold = true;
      final controller = AgentController(gateway: gateway, personId: 'test');
      await controller.load();
      final run = controller.send(AgentFixturePrompt.today);
      await Future<void>.delayed(Duration.zero);
      controller.dispose();
      await run;
      expect(gateway.stops, 1);
      expect(gateway.releases, 1);
      expect(gateway.saved!['active_turn'], isNull);
    },
  );

  test('load errors and typed model failure preserve retry intent across controller restart', () async {
    final gateway = TestAgentGateway()..failLoad = true;
    final controller = AgentController(gateway: gateway, personId: 'test');
    await controller.load();
    expect(controller.needsReload, isTrue);
    gateway.failLoad = false;
    gateway.responseFailure = 'model_unavailable';
    await controller.load();
    await controller.send(AgentFixturePrompt.followUp);
    expect(controller.failure, 'model_unavailable');
    controller.dispose();
    final restored = AgentController(gateway: gateway, personId: 'test');
    addTearDown(restored.dispose);
    await restored.load();
    expect(restored.canRetry, isTrue);
    gateway.responseFailure = null;
    await restored.retry();
    final questions = restored.messages.whereType<AgentTextMessage>().where(
      (message) => message.kind == AgentMessageKind.user,
    );
    expect(
      questions.map((message) => message.text),
      everyElement(AgentFixturePrompt.followUp.sampleText),
    );
  });
}
