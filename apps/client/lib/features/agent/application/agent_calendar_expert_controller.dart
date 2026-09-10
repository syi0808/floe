import 'package:flutter/foundation.dart';

import '../agent_calendar_experts.dart';
import '../agent_registry.dart';
import '../agent_request_id.dart';
import '../agent_vault_gateway.dart';
import 'agent_registry_controller.dart';

final class AgentCalendarExpertController extends ChangeNotifier {
  AgentCalendarExpertController({
    required this.gateway,
    required this.registryGateway,
    required this.registryController,
    required this.personId,
    required this.canOperate,
    required this.onFatalFailure,
  });

  final AgentCalendarExpertGateway? gateway;
  final AgentRegistryGateway? registryGateway;
  final AgentRegistryController registryController;
  final String personId;
  final bool Function() canOperate;
  final void Function(String failure) onFatalFailure;

  AgentCalendarExperts? experts;
  String? failure;
  AgentCalendarSetup? _pendingSetup;
  bool busy = false;

  AgentCalendarSetup? get pendingSetup => _pendingSetup;
  bool get available => gateway != null && registryGateway != null;
  bool get canManage =>
      available && !busy && !registryController.busy && canOperate();

  Future<void> load() =>
      _operation(() => gateway!.readCalendarExperts(personId));

  Future<void> install({
    required String provider,
    required List<String> calendarIds,
  }) async {
    final current = experts;
    if (!canManage || current == null || _pendingSetup != null) {
      return;
    }
    try {
      _pendingSetup = AgentCalendarSetup(
        personId: personId,
        instanceId: current.registry.instanceId,
        expectedRevision: current.registry.revision,
        setupId: newAgentRequestId(),
        provider: provider,
        calendarIds: calendarIds,
      );
    } on FormatException {
      failure = 'invalid_input';
      notifyListeners();
      return;
    }
    await retrySetup();
  }

  Future<void> retrySetup() async {
    final pending = _pendingSetup;
    if (pending == null) return;
    await _operation(
      () => gateway!.installCalendarExpert(pending),
      submitted: pending,
    );
    if (_pendingSetup == null && experts != null) {
      await setCalendarAccessEnabled(pending.setupId, true);
    }
  }

  void discardUncommittedSetup() {
    final current = experts;
    final pending = _pendingSetup;
    if (!canManage ||
        current == null ||
        pending == null ||
        current.registry.instanceId != pending.instanceId ||
        current.setups.any((entry) => entry.setupId == pending.setupId)) {
      return;
    }
    _pendingSetup = null;
    notifyListeners();
  }

  Future<void> configureView(String handle, bool enabled) async {
    final current = experts;
    if (current == null || _pendingSetup != null) return;
    final before = current.views
        .where((entry) => entry.handle == handle)
        .singleOrNull;
    if (before == null) return;
    await _operation(() async {
      final configured = await registryGateway!.configureRegistry(
        current.registry,
        target: AgentRegistryTarget.calendarView,
        id: handle,
        enabled: enabled,
      );
      final next = await gateway!.readCalendarExperts(personId);
      final updated = next.views
          .where((entry) => entry.handle == handle)
          .singleOrNull;
      if (configured.instanceId != current.registry.instanceId ||
          configured.revision != current.registry.revision + 1 ||
          next.registry.instanceId != configured.instanceId ||
          next.registry.revision != configured.revision ||
          updated == null ||
          updated.enabled != enabled ||
          updated.provider != before.provider ||
          !listEquals(updated.calendarIds, before.calendarIds)) {
        throw const FormatException('Calendar configuration mismatch');
      }
      return next;
    });
  }

  Future<void> setCalendarAccessEnabled(String setupId, bool enabled) =>
      _configureCalendarAccess(
        setupId,
        operation: AgentCalendarAccessOperation.setEnabled,
        enabled: enabled,
      );

  Future<void> changeCalendarAccessScope({
    required String setupId,
    required String provider,
    required List<String> calendarIds,
  }) async {
    final replacementSetupId = newAgentRequestId();
    await _configureCalendarAccess(
      setupId,
      operation: AgentCalendarAccessOperation.setScope,
      provider: provider,
      replacementSetupId: replacementSetupId,
      calendarIds: calendarIds,
    );
    if (experts?.setups.any((entry) => entry.setupId == replacementSetupId) ??
        false) {
      await setCalendarAccessEnabled(replacementSetupId, true);
    }
  }

  Future<void> removeCalendarAccess(String setupId) => _configureCalendarAccess(
    setupId,
    operation: AgentCalendarAccessOperation.remove,
  );

  Future<void> _configureCalendarAccess(
    String setupId, {
    required AgentCalendarAccessOperation operation,
    bool? enabled,
    String? provider,
    String? replacementSetupId,
    List<String>? calendarIds,
  }) async {
    final current = experts;
    if (current == null ||
        _pendingSetup != null ||
        !current.setups.any((entry) => entry.setupId == setupId)) {
      return;
    }
    final request = AgentCalendarAccessRequest(
      personId: personId,
      instanceId: current.registry.instanceId,
      expectedRevision: current.registry.revision,
      setupId: setupId,
      operation: operation,
      enabled: enabled,
      provider: provider,
      replacementSetupId: replacementSetupId,
      calendarIds: calendarIds,
    );
    await _operation(() async {
      final next = await gateway!.configureCalendarAccess(request);
      if (next.registry.instanceId != current.registry.instanceId ||
          next.registry.revision != current.registry.revision + 1) {
        throw const FormatException('Calendar access configuration mismatch');
      }
      final setup = next.setups
          .where((entry) => entry.setupId == setupId)
          .singleOrNull;
      switch (operation) {
        case AgentCalendarAccessOperation.setEnabled:
          if (setup == null || next.accessEnabled(setup) != enabled) {
            throw const FormatException('Calendar access state mismatch');
          }
        case AgentCalendarAccessOperation.setScope:
          final replacement = next.setups
              .where((entry) => entry.setupId == replacementSetupId)
              .singleOrNull;
          final view = replacement == null
              ? null
              : next.views
                    .where((entry) => entry.handle == replacement.viewHandle)
                    .singleOrNull;
          if (setup != null ||
              view == null ||
              view.provider != provider ||
              !listEquals(view.calendarIds, [...calendarIds!]..sort())) {
            throw const FormatException('Calendar access scope mismatch');
          }
        case AgentCalendarAccessOperation.remove:
          if (setup != null) {
            throw const FormatException('Calendar access removal mismatch');
          }
      }
      return next;
    });
  }

  Future<void> _operation(
    Future<AgentCalendarExperts> Function() operation, {
    AgentCalendarSetup? submitted,
  }) async {
    if (!canManage) return;
    busy = true;
    failure = null;
    notifyListeners();
    try {
      final result = await operation();
      if (!canOperate()) return;
      if (result.registry.personId != personId) {
        throw const FormatException('Calendar Person mismatch');
      }
      if (submitted != null && result.receiptFor(submitted) == null) {
        throw const FormatException('Missing Calendar setup receipt');
      }
      if (_pendingSetup case final pending?) {
        if (result.receiptFor(pending) != null) _pendingSetup = null;
      }
      experts = result;
      registryController.replace(result.registry);
    } on Object catch (error) {
      if (!canOperate()) return;
      experts = null;
      registryController.clear();
      failure = error is AgentVaultException
          ? error.failure
          : 'storage_unavailable';
      if (failure == 'vault_unavailable' || failure == 'interrupted') {
        onFatalFailure(failure!);
      }
    } finally {
      busy = false;
      notifyListeners();
    }
  }

  void clear() {
    experts = null;
    failure = null;
    _pendingSetup = null;
    notifyListeners();
  }
}
