import 'package:flutter/foundation.dart';

import 'package:floe_client/features/experts/domain/agent_registry.dart';
import 'package:floe_client/app/runtime/agent_vault_gateway.dart';

final class AgentRegistryController extends ChangeNotifier {
  AgentRegistryController({
    required this.gateway,
    required this.personId,
    required this.canOperate,
    required this.onFatalFailure,
  });

  final AgentRegistryGateway? gateway;
  final String personId;
  final bool Function() canOperate;
  final void Function(AgentVaultException failure) onFatalFailure;

  AgentRegistryView? registry;
  String? failure;
  bool loaded = false;
  bool busy = false;
  AgentCandidateCatalog? candidateCatalog;
  String? candidateFailure;
  bool candidateBusy = false;

  bool get available => gateway != null;
  bool get canManage => available && !busy && canOperate();

  Future<void> loadCandidates(
    String assignmentId,
    String requirementKey,
  ) async {
    if (!canManage || candidateBusy) return;
    candidateBusy = true;
    candidateFailure = null;
    notifyListeners();
    try {
      final result = await gateway!.readCandidates(
        personId,
        assignmentId: assignmentId,
        requirementKey: requirementKey,
      );
      if (!canOperate()) return;
      if (result.assignmentId != assignmentId ||
          result.requirementKey != requirementKey) {
        throw const FormatException('Expert candidate scope mismatch');
      }
      candidateCatalog = result;
    } on Object catch (error) {
      if (!canOperate()) return;
      candidateCatalog = null;
      candidateFailure = error is AgentVaultException
          ? error.failure
          : 'storage_unavailable';
      if (error is AgentVaultException &&
          (error.reloadRequired == true || error.sealSession == true)) {
        onFatalFailure(error);
      }
    } finally {
      candidateBusy = false;
      notifyListeners();
    }
  }

  Future<void> replaceSelection(
    AgentInstallation installation,
    AgentExpertDefinition definition,
    AgentAssignment assignment,
    AgentSourceRequirement requirement,
    List<String> candidateIds,
  ) async {
    if (!canManage || candidateBusy) return;
    final current = candidateCatalog;
    if (current == null ||
        current.assignmentId != assignment.id ||
        current.requirementKey != requirement.key ||
        current.bindingRevision != assignment.bindingRevision ||
        candidateIds.length > requirement.maximumSources ||
        candidateIds.toSet().length != candidateIds.length ||
        candidateIds.any(
          (id) => !current.candidates.any(
            (candidate) =>
                candidate.id == id && candidate.availability == 'available',
          ),
        )) {
      candidateFailure = 'conflict';
      notifyListeners();
      return;
    }
    candidateBusy = true;
    candidateFailure = null;
    notifyListeners();
    try {
      final result = await gateway!.replaceSelection(
        personId,
        installation: installation,
        definition: definition,
        assignment: assignment,
        requirement: requirement,
        candidateIds: candidateIds,
      );
      if (!canOperate()) return;
      if (result.assignmentId != assignment.id ||
          result.requirementKey != requirement.key ||
          result.bindingRevision != assignment.bindingRevision + 1) {
        throw const FormatException('Expert binding result mismatch');
      }
      candidateCatalog = result;
      try {
        registry = await gateway!.readRegistry(personId);
        loaded = true;
      } on Object {
        registry = null;
        loaded = false;
      }
    } on Object catch (error) {
      if (!canOperate()) return;
      candidateCatalog = null;
      candidateFailure = error is AgentVaultException
          ? error.failure
          : 'storage_unavailable';
      try {
        registry = await gateway!.readRegistry(personId);
        loaded = true;
      } on Object {
        registry = null;
        loaded = false;
      }
      if (error is AgentVaultException &&
          (error.reloadRequired == true || error.sealSession == true)) {
        onFatalFailure(error);
      }
    } finally {
      candidateBusy = false;
      notifyListeners();
    }
  }

  Future<void> load() => _operation(null);

  Future<void> configure(
    AgentRegistryTarget target,
    String id,
    bool enabled,
  ) async {
    final current = registry;
    final registryGateway = gateway;
    if (current == null || registryGateway == null) return;
    await _operation(
      () => registryGateway.configureRegistry(
        current,
        target: target,
        id: id,
        enabled: enabled,
      ),
    );
  }

  Future<bool> configureCapability(String installationId, bool enabled) async {
    final current = registry;
    final registryGateway = gateway;
    if (current == null || registryGateway == null) return false;
    final installation = current.installations
        .where((entry) => entry.id == installationId)
        .singleOrNull;
    if (installation == null) return false;
    final assignments = current.assignments
        .where((entry) => entry.installationId == installationId)
        .toList();
    final changes = <(AgentRegistryTarget, String)>[
      if (enabled && !installation.enabled)
        (AgentRegistryTarget.installation, installation.id),
      if (enabled)
        for (final assignment in assignments)
          if (!assignment.enabled)
            (AgentRegistryTarget.assignment, assignment.id),
      if (!enabled)
        for (final assignment in assignments)
          if (assignment.enabled)
            (AgentRegistryTarget.assignment, assignment.id),
      if (!enabled && installation.enabled)
        (AgentRegistryTarget.installation, installation.id),
    ];
    if (changes.isEmpty) return false;
    await _operation(() async {
      var next = current;
      for (final (target, id) in changes) {
        final configured = await registryGateway.configureRegistry(
          next,
          target: target,
          id: id,
          enabled: enabled,
        );
        if (configured.instanceId != next.instanceId ||
            configured.revision != next.revision + 1) {
          throw const FormatException('Registry configuration mismatch');
        }
        next = configured;
      }
      return next;
    }, expectedChanges: changes.length);
    return failure == null;
  }

  void replace(AgentRegistryView value) {
    registry = value;
    failure = null;
    loaded = true;
    notifyListeners();
  }

  void clear() {
    registry = null;
    failure = null;
    loaded = false;
    candidateCatalog = null;
    candidateFailure = null;
    notifyListeners();
  }

  Future<void> _operation(
    Future<AgentRegistryView> Function()? change, {
    int expectedChanges = 1,
  }) async {
    if (!canManage) return;
    final previous = registry;
    final registryGateway = gateway!;
    busy = true;
    failure = null;
    notifyListeners();
    try {
      final result = change == null
          ? await registryGateway.readRegistry(personId)
          : await change();
      if (!canOperate()) return;
      if (result != null && result.personId != personId) {
        throw const FormatException('Registry Person mismatch');
      }
      if (change != null &&
          (result == null ||
              previous == null ||
              result.instanceId != previous.instanceId ||
              result.revision != previous.revision + expectedChanges)) {
        throw const FormatException('Registry configuration mismatch');
      }
      registry = result;
      loaded = true;
    } on Object catch (error) {
      if (!canOperate()) return;
      registry = null;
      loaded = false;
      failure = error is AgentVaultException
          ? error.failure
          : 'storage_unavailable';
      if (error is AgentVaultException &&
          (error.reloadRequired == true || error.sealSession == true)) {
        onFatalFailure(error);
      }
    } finally {
      busy = false;
      notifyListeners();
    }
  }
}
