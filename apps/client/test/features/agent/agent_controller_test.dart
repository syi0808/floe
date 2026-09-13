import 'package:floe_client/features/agent/agent_controller.dart';
import 'package:floe_client/features/agent/agent_fixture_gateway.dart';
import 'package:floe_client/features/agent/agent_vault_gateway.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../support/agent_gateway.dart';
import '../../support/agent_vault_gateway.dart';

void main() {
  test('vault status failure does not disable a fresh start', () async {
    final gateway = TestVaultGateway()..unavailable = true;
    final controller = AgentController(gateway: gateway, personId: 'test');
    addTearDown(controller.dispose);

    await controller.load();

    expect(controller.vaultState, AgentVaultState.unavailable);
    expect(controller.canStartConversation, isTrue);

    gateway.unavailable = false;
    await controller.load(newSession: true);
    expect(controller.session, isNotNull);
  });

  test('session load timeout releases the new conversation escape', () async {
    final gateway = TestAgentGateway()..hangLoad = true;
    final controller = AgentController(
      gateway: gateway,
      personId: 'test',
      loadTimeout: const Duration(milliseconds: 10),
    );
    addTearDown(controller.dispose);

    await controller.load();

    expect(controller.progress, AgentProgress.idle);
    expect(controller.failure, 'storage_unavailable');
    expect(controller.needsReload, isTrue);
    expect(controller.canStartConversation, isTrue);

    gateway.hangLoad = false;
    await controller.load(newSession: true);
    expect(controller.session, isNotNull);
  });

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
    expect(controller.canRetry, isFalse);
    expect(gateway.releases, 1);
    expect(gateway.begins, 1);
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

  test(
    'load errors do not manufacture retry intent across controller restart',
    () async {
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
      expect(restored.canRetry, isFalse);
    },
  );

  test(
    'source-local review failure preserves the conversation session',
    () async {
      final gateway = TestAgentGateway()
        ..omitSessionOnFailure = true
        ..includeSessionWithFailure = true
        ..responseFailure = 'capability_unavailable'
        ..responseRecoveryAction = 'review_source';
      final controller = AgentController(gateway: gateway, personId: 'test');
      addTearDown(controller.dispose);
      await controller.load();
      await controller.send(AgentFixturePrompt.today);
      expect(controller.failure, 'capability_unavailable');
      expect(controller.recoveryAction, 'review_source');
      expect(controller.needsReload, isFalse);
      expect(controller.session, isNotNull);
      expect(controller.canRetry, isFalse);
    },
  );

  test(
    'non-read recovery action cannot retry a possibly effectful turn',
    () async {
      final gateway = TestAgentGateway()
        ..omitSessionOnFailure = true
        ..responseFailure = 'deadline_exceeded'
        ..responseRecoveryAction = 'none';
      final controller = AgentController(gateway: gateway, personId: 'test');
      addTearDown(controller.dispose);
      await controller.load();
      await controller.send(AgentFixturePrompt.today);
      expect(controller.recoveryAction, 'none');
      expect(controller.canRetry, isFalse);
    },
  );

  test('explicit safe read recovery is the only retryable action', () async {
    final gateway = TestAgentGateway()
      ..omitSessionOnFailure = true
      ..includeSessionWithFailure = true
      ..responseFailure = 'capability_unavailable'
      ..responseRecoveryAction = 'retry_read';
    final controller = AgentController(gateway: gateway, personId: 'test');
    addTearDown(controller.dispose);
    await controller.load();
    await controller.send(AgentFixturePrompt.today);
    expect(controller.canRetry, isTrue);
    gateway.responseFailure = null;
    gateway.responseRecoveryAction = null;
    await controller.retry();
    expect(controller.failure, isNull);
    expect(gateway.begins, 2);
  });
}
