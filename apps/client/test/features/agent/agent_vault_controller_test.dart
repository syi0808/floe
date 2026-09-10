import 'dart:async';

import 'package:floe_client/features/agent/agent_controller.dart';
import 'package:floe_client/features/agent/agent_fixture_gateway.dart';
import 'package:floe_client/features/agent/agent_vault_gateway.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../support/agent_vault_gateway.dart';

void main() {
  test('secure storage setup and unlock happen automatically', () async {
    final gateway = TestVaultGateway();
    final controller = AgentController(gateway: gateway, personId: 'test');
    addTearDown(controller.dispose);
    await controller.load();
    expect(controller.vaultState, AgentVaultState.ready);
    expect(controller.canSend, isTrue);
    expect(gateway.creates, 1);
    final sessionId = controller.session!.id;
    expect(gateway.creates, 1);
    expect(controller.canSend, isTrue);
    await controller.send(AgentFixturePrompt.today);
    expect(controller.messages, isNotEmpty);
    await controller.closeView();
    expect(controller.messages, isEmpty);
    expect(controller.session, isNull);
    await controller.load();
    expect(controller.vaultState, AgentVaultState.ready);
    expect(gateway.unlocks, 1);
    expect(controller.session!.id, sessionId);
    expect(controller.messages, isNotEmpty);
    expect(gateway.creates, 1);
  });

  test('automatic reload waits for an in-flight vault lock', () async {
    final gateway = TestVaultGateway();
    final controller = AgentController(gateway: gateway, personId: 'test');
    addTearDown(controller.dispose);
    await controller.load();
    final closing = controller.closeView();
    final loading = controller.load();
    await Future.wait([closing, loading]);
    expect(controller.vaultState, AgentVaultState.ready);
    expect(controller.session, isNotNull);
    expect(gateway.locks, 1);
    expect(gateway.unlocks, 1);
  });

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

  test(
    'model failure keeps the unlocked vault and confirmed messages',
    () async {
      final gateway = TestVaultGateway()
        ..responseFailure = 'server_model_invalid_output'
        ..omitSessionOnFailure = true;
      final controller = AgentController(gateway: gateway, personId: 'test');
      addTearDown(controller.dispose);
      await controller.load();

      await controller.send(AgentFixturePrompt.today);

      expect(controller.failure, 'server_model_invalid_output');
      expect(controller.needsReload, isTrue);
      expect(controller.vaultState, AgentVaultState.ready);
      expect(controller.session, isNotNull);
      expect(controller.messages, isNotEmpty);
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
