import 'dart:async';

import 'package:floe_client/features/agent/agent_controller.dart';
import 'package:floe_client/features/agent/agent_fixture_gateway.dart';
import 'package:floe_client/features/agent/agent_vault_gateway.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/agent_vault_gateway.dart';

void main() {
  test(
    'secure storage setup and unlock are explicit, never automatic',
    () async {
      final gateway = TestVaultGateway();
      final controller = AgentController(gateway: gateway, personId: 'test');
      addTearDown(controller.dispose);
      await controller.load();
      expect(controller.vaultState, AgentVaultState.missing);
      expect(controller.canSend, isFalse);
      expect(gateway.creates, 0);
      await controller.unlock(create: true);
      final sessionId = controller.session!.id;
      expect(gateway.creates, 1);
      expect(controller.canSend, isTrue);
      await controller.send(AgentFixturePrompt.today);
      expect(controller.messages, isNotEmpty);
      await controller.closeView();
      expect(controller.messages, isEmpty);
      expect(controller.session, isNull);
      await controller.load();
      expect(controller.vaultState, AgentVaultState.locked);
      expect(gateway.unlocks, 0);
      await controller.unlock();
      expect(controller.session!.id, sessionId);
      expect(controller.messages, isNotEmpty);
      expect(gateway.creates, 1);
    },
  );

  test(
    'closing during a turn clears immediately and never republishes completion',
    () async {
      final gateway = TestVaultGateway()..hold = true;
      final controller = AgentController(gateway: gateway, personId: 'test');
      addTearDown(controller.dispose);
      await controller.unlock(create: true);
      final turn = controller.send(AgentFixturePrompt.today);
      await Future<void>.delayed(Duration.zero);
      expect(controller.messages, isNotEmpty);
      final closing = controller.closeView();
      expect(controller.messages, isEmpty);
      expect(controller.session, isNull);
      await Future.wait([turn, closing]);
      expect(controller.messages, isEmpty);
      expect(controller.canSend, isFalse);
      expect(gateway.locks, 1);
      expect(gateway.releases, 1);
    },
  );

  test(
    'key failure removes presented messages and cannot create a replacement',
    () async {
      final gateway = TestVaultGateway();
      final controller = AgentController(gateway: gateway, personId: 'test');
      addTearDown(controller.dispose);
      await controller.unlock(create: true);
      await controller.send(AgentFixturePrompt.today);
      gateway.unavailable = true;
      await controller.load();
      expect(controller.vaultState, AgentVaultState.unavailable);
      expect(controller.messages, isEmpty);
      expect(controller.failure, 'vault_unavailable');
      await controller.send(AgentFixturePrompt.today);
      expect(gateway.begins, 1);
      expect(gateway.creates, 1);
      gateway.unavailable = false;
    },
  );

  test('closing during unlock seals delayed loaded messages', () async {
    final gateway = TestVaultGateway()..resumeGate = Completer<void>();
    final controller = AgentController(gateway: gateway, personId: 'test');
    addTearDown(controller.dispose);
    final unlocking = controller.unlock(create: true);
    await Future<void>.delayed(Duration.zero);
    final closing = controller.closeView();
    gateway.resumeGate!.complete();
    await Future.wait([unlocking, closing]);
    expect(controller.messages, isEmpty);
    expect(controller.session, isNull);
    expect(controller.vaultState, AgentVaultState.locked);
    expect(gateway.locks, 1);
  });
}
