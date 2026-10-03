import 'package:flutter/foundation.dart';

import 'package:floe_client/app/runtime/agent_vault_gateway.dart';
import 'package:floe_client/features/experts/domain/agent_registry.dart';

final class AgentRegistryController extends ChangeNotifier {
  AgentRegistryController({
    required this.gateway,
    required this.canOperate,
    required this.onFatalFailure,
  });

  final AgentRegistryGateway? gateway;
  final bool Function() canOperate;
  final void Function(AgentVaultException failure) onFatalFailure;

  int _operationGeneration = 0;
  bool _disposed = false;

  AgentDirectorySnapshot? directory;
  AgentBindingInspection? inspection;
  AgentBindingReview? review;
  String? failure;
  String? reviewFailure;
  bool loaded = false;
  bool busy = false;

  bool get available => gateway != null;
  AgentRegistryCommandKind? get pendingCommandKind =>
      gateway?.pendingCommandKind;
  bool get canRead => available && !busy && canOperate();
  bool get canManage =>
      available && !busy && canOperate() && pendingCommandKind == null;
  bool get canRetryPending =>
      available && !busy && canOperate() && pendingCommandKind != null;

  Future<void> load() async {
    final generation = _operationGeneration;
    if (!_isCurrent(generation) || !canRead) return;
    busy = true;
    failure = null;
    _notifyIfCurrent(generation);
    try {
      final loadedDirectory = await gateway!.readDirectory();
      if (!_isCurrent(generation)) return;
      directory = loadedDirectory;
      loaded = true;
    } on Object catch (error) {
      if (!_isCurrent(generation)) return;
      directory = null;
      loaded = false;
      failure = _failure(error);
      _reportFatal(error, generation);
    } finally {
      if (_isCurrent(generation, requireCanOperate: false)) {
        busy = false;
        notifyListeners();
      }
    }
  }

  Future<void> setInstallationEnabled(
    AgentInstallation installation,
    bool enabled,
  ) async {
    final generation = _operationGeneration;
    final current = directory;
    if (!_isCurrent(generation) || !canManage || current == null) return;
    await _run(generation, (operationGeneration) async {
      final result = await gateway!.setInstallationEnabled(
        installationRef: installation.installationRef,
        expectedRevision: current.revision,
        enabled: enabled,
      );
      if (!_isCurrent(operationGeneration)) return;
      final updated = result.installations
          .where((entry) => entry.installationRef == installation.installationRef)
          .singleOrNull;
      if (updated == null || updated.enabled != enabled) {
        throw const FormatException('Installation result scope mismatch.');
      }
      directory = result;
      loaded = true;
    });
  }

  Future<void> inspectBinding(
    AgentAssignment assignment,
    AgentSourceRequirement requirement,
  ) async {
    final generation = _operationGeneration;
    if (!_isCurrent(generation) || !canRead) return;
    await _run(generation, (operationGeneration) async {
      final result = await gateway!.inspectBinding(
        assignmentRef: assignment.assignmentRef,
        requirementRef: requirement.requirementRef,
      );
      if (!_isCurrent(operationGeneration)) return;
      if (result.assignmentRef != assignment.assignmentRef ||
          result.requirementRef != requirement.requirementRef) {
        throw const FormatException('Expert binding scope mismatch.');
      }
      inspection = result;
    }, reviewOperation: true);
  }

  Future<void> prepareReview(
    AgentAssignment assignment,
    AgentSourceRequirement requirement,
  ) async {
    final generation = _operationGeneration;
    if (!_isCurrent(generation) || !canManage) return;
    await _run(generation, (operationGeneration) async {
      final currentBinding = await gateway!.inspectBinding(
        assignmentRef: assignment.assignmentRef,
        requirementRef: requirement.requirementRef,
      );
      if (!_isCurrent(operationGeneration)) return;
      if (currentBinding.assignmentRef != assignment.assignmentRef ||
          currentBinding.requirementRef != requirement.requirementRef) {
        throw const FormatException('Expert binding scope mismatch.');
      }
      inspection = currentBinding;
      final prepared = await gateway!.prepareBindingReview(
        assignmentRef: assignment.assignmentRef,
        requirementRef: requirement.requirementRef,
        expectedBindingRevision: currentBinding.bindingRevision,
      );
      if (!_isCurrent(operationGeneration)) return;
      if (prepared.assignmentRef != assignment.assignmentRef ||
          prepared.requirementRef != requirement.requirementRef ||
          prepared.bindingRevision != currentBinding.bindingRevision) {
        throw const FormatException('Expert binding review scope mismatch.');
      }
      review = prepared;
    }, reviewOperation: true);
  }

  Future<void> loadReview(AgentBindingReviewRef reviewRef) async {
    final generation = _operationGeneration;
    if (!_isCurrent(generation) || !canRead) return;
    await _run(generation, (operationGeneration) async {
      final loadedReview = await gateway!.inspectBindingReview(reviewRef);
      if (!_isCurrent(operationGeneration)) return;
      if (!loadedReview.reviewRef.matches(reviewRef)) {
        throw const FormatException('Expert review reference mismatch.');
      }
      review = loadedReview;
    }, reviewOperation: true);
  }

  void usePreparedReview(AgentBindingReview preparedReview) {
    final generation = _operationGeneration;
    if (!_isCurrent(generation)) return;
    review = preparedReview;
    inspection = null;
    _notifyIfCurrent(generation);
  }

  Future<void> refreshReview(
    AgentAssignment assignment,
    AgentSourceRequirement requirement,
  ) async {
    final generation = _operationGeneration;
    final currentReview = review;
    if (!_isCurrent(generation) ||
        !canManage ||
        currentReview == null ||
        !currentReview.canRefresh) {
      return;
    }
    await prepareReview(assignment, requirement);
    if (!_isCurrent(generation)) return;
  }

  Future<bool> replaceBinding(Set<String> candidateRefs) async {
    final generation = _operationGeneration;
    final currentReview = review;
    if (!_isCurrent(generation) ||
        !canManage ||
        currentReview == null ||
        !currentReview.canReplace) {
      return false;
    }
    final assignment = directory?.assignments
        .where((entry) => entry.assignmentRef == currentReview.assignmentRef)
        .singleOrNull;
    final requirement = assignment?.requirements
        .where((entry) => entry.requirementRef == currentReview.requirementRef)
        .singleOrNull;
    if (requirement == null || candidateRefs.length > 16) {
      if (!_isCurrent(generation)) return false;
      reviewFailure = 'conflict';
      _notifyIfCurrent(generation);
      return false;
    }
    return _run(generation, (operationGeneration) async {
      final updated = await gateway!.replaceBinding(
        review: currentReview,
        candidateRefs: candidateRefs.toList()..sort(),
      );
      if (!_isCurrent(operationGeneration)) return;
      if (!updated.assignments.any(
        (entry) => entry.assignmentRef == currentReview.assignmentRef,
      )) {
        throw const FormatException('Assignment missing from owner reply.');
      }
      directory = updated;
      loaded = true;
      review = null;
      inspection = null;
    }, reviewOperation: true);
  }

  Future<AgentRegistryCommandResult?> retryPendingCommand() async {
    final generation = _operationGeneration;
    if (!_isCurrent(generation) || !canRetryPending) return null;
    AgentRegistryCommandResult? result;
    final succeeded = await _run(generation, (operationGeneration) async {
      final retried = await gateway!.retryPendingCommand();
      if (!_isCurrent(operationGeneration)) return;
      result = retried;
      switch (retried) {
        case AgentDirectoryCommandResult(:final directory, :final kind):
          this.directory = directory;
          loaded = true;
          if (kind == AgentRegistryCommandKind.bindingReplace) {
            review = null;
            inspection = null;
          }
        case AgentBindingReviewCommandResult(:final review):
          this.review = review;
      }
    }, reviewOperation: true);
    if (!_isCurrent(generation) || !succeeded) return null;
    return result;
  }

  void clear() {
    _operationGeneration++;
    if (_disposed) return;
    directory = null;
    inspection = null;
    review = null;
    failure = null;
    reviewFailure = null;
    loaded = false;
    busy = false;
    notifyListeners();
  }

  @override
  void dispose() {
    _operationGeneration++;
    _disposed = true;
    super.dispose();
  }

  Future<bool> _run(
    int generation,
    Future<void> Function(int generation) operation, {
    bool reviewOperation = false,
  }) async {
    if (!_isCurrent(generation) || busy) return false;
    busy = true;
    failure = null;
    reviewFailure = null;
    _notifyIfCurrent(generation);
    try {
      await operation(generation);
      return _isCurrent(generation);
    } on Object catch (error) {
      if (!_isCurrent(generation)) return false;
      final message = _failure(error);
      if (reviewOperation) {
        reviewFailure = message;
      } else {
        failure = message;
      }
      _reportFatal(error, generation);
      return false;
    } finally {
      if (_isCurrent(generation, requireCanOperate: false)) {
        busy = false;
        notifyListeners();
      }
    }
  }

  String _failure(Object error) =>
      error is AgentVaultException ? error.failure : 'storage_unavailable';

  bool _isCurrent(int generation, {bool requireCanOperate = true}) =>
      !_disposed &&
      generation == _operationGeneration &&
      (!requireCanOperate || canOperate());

  void _notifyIfCurrent(int generation) {
    if (_isCurrent(generation)) notifyListeners();
  }

  void _reportFatal(Object error, int generation) {
    if (error is AgentVaultException &&
        (error.reloadRequired == true || error.sealSession == true) &&
        _isCurrent(generation)) {
      onFatalFailure(error);
    }
  }
}
