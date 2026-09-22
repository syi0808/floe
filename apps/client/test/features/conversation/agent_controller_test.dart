import 'package:floe_client/features/conversation/application/agent_controller.dart';

import 'package:floe_client/app/runtime/agent_vault_gateway.dart';
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
    'interrupted resume requires explicit recovery without model replay',
    () async {
      final gateway = TestAgentGateway();
      await gateway.startConversation('test');
      gateway.saved!['active_turn'] = 'abandoned';
      final controller = AgentController(gateway: gateway, personId: 'test');
      addTearDown(controller.dispose);
      await controller.load();
      expect(controller.needsRecovery, isTrue);
      expect(controller.canSend, isFalse);
      await controller.sendText('Hello Floe');
      expect(gateway.begins, 0);
      await controller.recover();
      expect(gateway.recoveries, 1);
      expect(controller.canSend, isTrue);
      expect(gateway.begins, 0);
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
      await controller.sendText('Hello Floe');
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
      await controller.sendText('Hello Floe');
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
      await controller.sendText('Hello Floe');
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
    await controller.sendText('Hello Floe');
    expect(controller.canRetry, isTrue);
    gateway.responseFailure = null;
    gateway.responseRecoveryAction = null;
    await controller.retry();
    expect(controller.failure, isNull);
    expect(gateway.begins, 2);
  });
}
