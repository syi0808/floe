import 'dart:async';
import 'dart:convert';

import 'package:floe_client/features/agent/agent_controller.dart';
import 'package:floe_client/features/agent/agent_registry.dart';
import 'package:floe_client/features/agent/agent_vault_gateway.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../support/agent_registry.dart';

void main() {
  test(
    'overview validates identity cardinality counters and installation links',
    () {
      final view = AgentRegistryView.fromJson(registryFixture());
      expect(view.assignments.single.completedInvocations, 2);
      expect(() => view.assignments.clear(), throwsUnsupportedError);
      for (final mode in [0, 1, 2, 3, 4, 5]) {
        final fixture = registryFixture();
        final assignment = (fixture['assignments'] as List).single as Map;
        switch (mode) {
          case 0:
            fixture['schema_version'] = 2;
          case 1:
            fixture['instance_id'] = 'invalid';
          case 2:
            assignment['installation_id'] = registryInstance;
          case 3:
            assignment['granted_view_count'] = 5;
          case 4:
            assignment['state_revision'] = -1;
          case 5:
            (fixture['assignments'] as List).add(
              Map<String, Object>.from(assignment),
            );
        }
        expect(
          () => AgentRegistryView.fromJson(fixture),
          throwsFormatException,
        );
      }
    },
  );

  test('native gateway sends only instance revision and explicit enablement target', () async {
    final transport = RegistryTransport();
    final gateway = NativeAgentVaultGateway(
      transport.call,
      deviceId: 'test-device',
    );
    final before = (await gateway.readRegistry(registryPerson))!;
    final after = await gateway.configureRegistry(
      before,
      target: AgentRegistryTarget.assignment,
      id: registryAssignment,
      enabled: false,
    );
    expect(after.revision, 11);
    expect(after.assignments.single.enabled, false);
    expect(transport.lastChange, {
      'instance_id': registryInstance,
      'expected_revision': 10,
      'target': {
        'kind': 'assignment',
        'id': registryAssignment,
        'enabled': false,
      },
    });
    expect(transport.pending, isNull);
  });

  test(
    'lost mutation reply is drained and reread without replaying configuration',
    () async {
      final transport = RegistryTransport();
      final gateway = NativeAgentVaultGateway(
        transport.call,
        deviceId: 'test-device',
      );
      final before = (await gateway.readRegistry(registryPerson))!;
      transport.loseMutationReply = true;
      await expectLater(
        gateway.configureRegistry(
          before,
          target: AgentRegistryTarget.assignment,
          id: registryAssignment,
          enabled: false,
        ),
        throwsStateError,
      );
      final after = (await gateway.readRegistry(registryPerson))!;
      expect(after.revision, 11);
      expect(after.assignments.single.enabled, false);
      expect(transport.changes, 1);
    },
  );

  test(
    'foreign registry reply is never accepted by the native gateway',
    () async {
      final transport = RegistryTransport()
        ..snapshot['person_id'] = registryInstance;
      final gateway = NativeAgentVaultGateway(
        transport.call,
        deviceId: 'test-device',
      );
      await expectLater(
        gateway.readRegistry(registryPerson),
        throwsFormatException,
      );
    },
  );

  test(
    'controller serializes changes and does not optimistically toggle grants',
    () async {
      final gateway = TestRegistryGateway();
      final controller = AgentController(
        gateway: gateway,
        personId: registryPerson,
      );
      addTearDown(controller.dispose);
      await controller.load();
      await controller.loadRegistry();
      gateway.registryGate = Completer<void>();
      final operation = controller.configureRegistry(
        AgentRegistryTarget.assignment,
        registryAssignment,
        false,
      );
      expect(controller.busy, true);
      expect(controller.canSend, false);
      expect(controller.registry!.assignments.single.enabled, true);
      await controller.configureRegistry(
        AgentRegistryTarget.assignment,
        registryAssignment,
        true,
      );
      expect(gateway.changes, 1);
      gateway.registryGate!.complete();
      await operation;
      expect(controller.registry!.assignments.single.enabled, false);
      expect(controller.canSend, true);
    },
  );

  test('conflict clears stale registry without replacing or hiding the conversation', () async {
    final gateway = TestRegistryGateway();
    final controller = AgentController(
      gateway: gateway,
      personId: registryPerson,
    );
    addTearDown(controller.dispose);
    await controller.load();
    await controller.loadRegistry();
    final session = controller.session;
    gateway.registryError = 'conflict';
    await controller.configureRegistry(
      AgentRegistryTarget.installation,
      registryInstallation,
      false,
    );
    expect(controller.registry, isNull);
    expect(controller.registryFailure, 'conflict');
    expect(controller.session, same(session));
    gateway.registryError = null;
    await controller.loadRegistry();
    expect(controller.registryFailure, isNull);
    expect(controller.registry, isNotNull);
  });

  test('locking clears registry immediately and discards late read and mutation results', () async {
    for (final mutate in [false, true]) {
      final gateway = TestRegistryGateway();
      final controller = AgentController(
        gateway: gateway,
        personId: registryPerson,
      );
      await controller.load();
      await controller.loadRegistry();
      gateway.registryGate = Completer<void>();
      final operation = mutate
          ? controller.configureRegistry(
              AgentRegistryTarget.assignment,
              registryAssignment,
              false,
            )
          : controller.loadRegistry();
      final closing = controller.closeView();
      expect(controller.registry, isNull);
      expect(controller.session, isNull);
      gateway.registryGate!.complete();
      await operation;
      await closing;
      expect(controller.registry, isNull);
      expect(controller.vaultState, AgentVaultState.locked);
      expect(gateway.locks, 1);
      controller.dispose();
    }
  });

  test('key failure or worker interruption clears cached conversation and settings', () async {
    for (final failure in ['vault_unavailable', 'interrupted']) {
      final gateway = TestRegistryGateway();
      final controller = AgentController(
        gateway: gateway,
        personId: registryPerson,
      );
      addTearDown(controller.dispose);
      await controller.load();
      await controller.loadRegistry();
      gateway.registryError = failure;
      await controller.loadRegistry();
      expect(controller.registry, isNull);
      expect(controller.session, isNull);
      expect(controller.vaultState, AgentVaultState.unavailable);
    }
  });
}

class RegistryTransport {
  final snapshot = registryFixture();
  Map<String, dynamic>? pending;
  Map? lastChange;
  bool loseMutationReply = false;
  int changes = 0;

  Future<Map<String, dynamic>> call(Map<String, Object?> request) async {
    final operation = request['operation'] as Map;
    if (operation['kind'] == 'submit') {
      if (pending != null) throw const AgentVaultException('conflict');
      final action = operation['action'] as Map;
      expect(action['kind'], 'registry');
      final change = action['change'] as Map?;
      if (change != null) {
        lastChange = change;
        changes++;
        final target = change['target'] as Map;
        final entries =
            snapshot[target['kind'] == 'assignment'
                    ? 'assignments'
                    : 'installations']
                as List;
        (entries.single as Map)['enabled'] = target['enabled'];
        snapshot['revision'] = (snapshot['revision'] as int) + 1;
      }
      pending = {
        'request_id': request['request_id'],
        'done': true,
        'events': <Object?>[],
        'next_sequence': 0,
        'state': 'ready',
        'registry': jsonDecode(jsonEncode(snapshot)),
        'session': null,
        'failure': null,
      };
      if (change != null && loseMutationReply) {
        loseMutationReply = false;
        throw StateError('lost registry reply');
      }
    }
    if (pending == null) throw const AgentVaultException('not_found');
    final result = Map<String, dynamic>.from(pending!);
    if (operation['kind'] == 'release') pending = null;
    return result;
  }
}
