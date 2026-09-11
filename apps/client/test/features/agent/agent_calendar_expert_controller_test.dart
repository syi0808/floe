import 'dart:async';

import 'package:floe_client/features/agent/agent_controller.dart';
import 'package:floe_client/features/agent/agent_vault_gateway.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../support/agent_calendar_experts.dart';
import '../../support/agent_registry.dart';

void main() {
  const connectionId = '00000000-0000-4000-8000-000000000010';
  test('controller serializes setup with chat and waits for confirmed installation', () async {
    final gateway = TestCalendarExpertGateway();
    final controller = AgentController(
      gateway: gateway,
      personId: registryPerson,
    );
    addTearDown(controller.dispose);
    await controller.load();
    await controller.loadCalendarExperts();
    gateway.gate = Completer<void>();
    final operation = controller.installCalendarExpert(
      setupId: connectionId,
      provider: 'event_kit',
      calendarIds: ['work', 'home'],
      connectionScope: 'selected',
      connectionRevision: 1,
    );
    expect(controller.busy, true);
    expect(controller.canSend, false);
    expect(controller.calendarExperts!.setups, isEmpty);
    expect(controller.pendingCalendarSetup, isNotNull);
    await controller.installCalendarExpert(
      setupId: connectionId,
      provider: 'event_kit',
      calendarIds: ['different'],
      connectionScope: 'selected',
      connectionRevision: 1,
    );
    await controller.retryCalendarSetup();
    expect(gateway.requests, hasLength(1));
    gateway.gate!.complete();
    await operation;
    expect(controller.calendarExperts!.views.single.enabled, true);
    expect(
      controller.calendarExperts!.accessEnabled(
        controller.calendarExperts!.setups.single,
      ),
      true,
    );
    expect(controller.pendingCalendarSetup, isNull);
    expect(controller.registry!.revision, 2);
    expect(controller.canSend, true);
  });

  test('uncertain setup retains exact intent for retry or read-only reconciliation', () async {
    for (final retry in [false, true]) {
      final gateway = TestCalendarExpertGateway();
      final controller = AgentController(
        gateway: gateway,
        personId: registryPerson,
      );
      addTearDown(controller.dispose);
      await controller.load();
      await controller.loadCalendarExperts();
      final session = controller.session;
      gateway.transport.loss = 'submit';
      await controller.installCalendarExpert(
        setupId: connectionId,
        provider: 'event_kit',
        calendarIds: ['home', 'work'],
        connectionScope: 'selected',
        connectionRevision: 1,
      );
      final pending = controller.pendingCalendarSetup;
      expect(pending, isNotNull);
      expect(controller.calendarExperts, isNull);
      expect(controller.calendarExpertFailure, 'storage_unavailable');
      expect(controller.session, same(session));
      await controller.installCalendarExpert(
        setupId: connectionId,
        provider: 'event_kit',
        calendarIds: ['different'],
        connectionScope: 'selected',
        connectionRevision: 1,
      );
      expect(gateway.requests, hasLength(1));
      if (retry) {
        await controller.retryCalendarSetup();
        expect(gateway.requests.last, same(pending));
      } else {
        await controller.loadCalendarExperts();
        expect(gateway.requests, hasLength(1));
      }
      expect(controller.calendarExpertFailure, isNull);
      expect(controller.pendingCalendarSetup, isNull);
      expect(
        controller.calendarExperts!.setups.single.setupId,
        pending!.setupId,
      );
      expect(gateway.transport.installations, 1);
    }
  });

  test('failed uncommitted intent can only be discarded after a successful refresh', () async {
    final gateway = TestCalendarExpertGateway();
    final controller = AgentController(
      gateway: gateway,
      personId: registryPerson,
    );
    addTearDown(controller.dispose);
    await controller.load();
    await controller.loadCalendarExperts();
    gateway.error = 'conflict';
    await controller.installCalendarExpert(
      setupId: connectionId,
      provider: 'event_kit',
      calendarIds: ['home', 'work'],
      connectionScope: 'selected',
      connectionRevision: 1,
    );
    controller.discardUncommittedCalendarSetup();
    expect(controller.pendingCalendarSetup, isNotNull);
    gateway.error = null;
    await controller.loadCalendarExperts();
    expect(controller.pendingCalendarSetup, isNotNull);
    controller.discardUncommittedCalendarSetup();
    expect(controller.pendingCalendarSetup, isNull);
    expect(gateway.transport.installations, 0);
  });

  test(
    'access pause is atomic and a lost response never replays the change',
    () async {
      final gateway = TestCalendarExpertGateway();
      final controller = AgentController(
        gateway: gateway,
        personId: registryPerson,
      );
      addTearDown(controller.dispose);
      await controller.load();
      await controller.loadCalendarExperts();
      await controller.installCalendarExpert(
        setupId: connectionId,
        provider: 'event_kit',
        calendarIds: ['home', 'work'],
        connectionScope: 'selected',
        connectionRevision: 1,
      );
      final setupId = controller.calendarExperts!.setups.single.setupId;
      gateway.gate = Completer<void>();
      final operation = controller.setCalendarAccessEnabled(setupId, false);
      expect(controller.calendarExperts!.views.single.enabled, true);
      expect(controller.busy, true);
      gateway.gate!.complete();
      await operation;
      expect(controller.calendarExperts!.views.single.enabled, false);
      expect(
        controller.calendarExperts!.registry.installations.every(
          (entry) => !entry.enabled,
        ),
        true,
      );
      gateway.transport.loss = 'release_after';
      await controller.setCalendarAccessEnabled(setupId, true);
      expect(controller.calendarExperts, isNull);
      await controller.loadCalendarExperts();
      expect(controller.calendarExperts!.views.single.enabled, true);
      expect(controller.calendarExperts!.registry.revision, 4);
    },
  );

  test('lock clears source scope and pending intent immediately and ignores late results', () async {
    for (final operationKind in ['read', 'install', 'configure']) {
      final gateway = TestCalendarExpertGateway();
      final controller = AgentController(
        gateway: gateway,
        personId: registryPerson,
      );
      await controller.load();
      await controller.loadCalendarExperts();
      if (operationKind == 'configure') {
        await controller.installCalendarExpert(
          setupId: connectionId,
          provider: 'event_kit',
          calendarIds: ['home', 'work'],
          connectionScope: 'selected',
          connectionRevision: 1,
        );
      }
      final setupId = operationKind == 'configure'
          ? controller.calendarExperts!.setups.single.setupId
          : null;
      gateway.gate = Completer<void>();
      final operation = switch (operationKind) {
        'read' => controller.loadCalendarExperts(),
        'install' => controller.installCalendarExpert(
          setupId: connectionId,
          provider: 'event_kit',
          calendarIds: ['home', 'work'],
          connectionScope: 'selected',
          connectionRevision: 1,
        ),
        _ => controller.setCalendarAccessEnabled(setupId!, false),
      };
      final closing = controller.closeView();
      expect(controller.calendarExperts, isNull);
      expect(controller.pendingCalendarSetup, isNull);
      expect(controller.session, isNull);
      expect(controller.vaultState, AgentVaultState.locked);
      gateway.gate!.complete();
      await operation;
      await closing;
      expect(controller.calendarExperts, isNull);
      expect(controller.registry, isNull);
      expect(controller.pendingCalendarSetup, isNull);
      controller.dispose();
    }
  });

  test('protected key failure clears all source and conversation state without fallback', () async {
    final gateway = TestCalendarExpertGateway();
    final controller = AgentController(
      gateway: gateway,
      personId: registryPerson,
    );
    addTearDown(controller.dispose);
    await controller.load();
    await controller.loadCalendarExperts();
    gateway.error = 'vault_unavailable';
    await controller.installCalendarExpert(
      setupId: connectionId,
      provider: 'event_kit',
      calendarIds: ['home', 'work'],
      connectionScope: 'selected',
      connectionRevision: 1,
    );
    expect(controller.vaultState, AgentVaultState.unavailable);
    expect(controller.session, isNull);
    expect(controller.calendarExperts, isNull);
    expect(controller.pendingCalendarSetup, isNull);
    expect(controller.canManageCalendarExperts, false);
  });
}
