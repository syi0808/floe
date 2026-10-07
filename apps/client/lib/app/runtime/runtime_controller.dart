import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:floe_client/app/runtime/app_owner_exception.dart';
import 'package:floe_client/app/runtime/native_transport.dart';
import 'package:floe_client/app/runtime/runtime_gateway.dart';
import 'package:floe_client/features/conversation/application/agent_request_id.dart';
import 'package:floe_client/infrastructure/diagnostics/app_diagnostics.dart';

/// App-lifetime observer of Rust-owned Runtime preparation and readiness.
/// It never exposes physical Vault create, unlock, or lock operations.
final class RuntimeController extends ChangeNotifier {
  RuntimeController({required this.gateway, required this.personId});

  final RuntimeGateway gateway;
  final String personId;

  RuntimeReadinessState _state = RuntimeReadinessState.unknown;
  AppOwnerException? _failure;
  String? _operationId;
  bool _operationSubmitted = false;
  bool _busy = false;
  bool _disposed = false;
  bool _closing = false;
  int _failureRevision = 0;
  int _operationEpoch = 0;
  bool _reobserveAfterBusy = false;
  final Set<String> _observedIncidents = {};
  Future<void>? _opening;

  RuntimeReadinessState get state => _state;
  AppOwnerException? get failure => _failure;
  bool get busy => _busy;
  bool get hasPendingOperation => _operationId != null;
  bool get ready =>
      !_disposed &&
      !_closing &&
      !_busy &&
      _state == RuntimeReadinessState.ready;
  bool get canRecover =>
      !_disposed &&
      !_closing &&
      !_busy &&
      (_operationId != null ||
          _failure?.safeActions.contains('retry') == true ||
          _failure?.ownerFailure?.recovery == 'reobserve');

  String? get reasonCode =>
      _safeToken(_failure?.reasonCode ?? _failure?.failure);
  String? get incidentId => _safeToken(_failure?.incidentId);

  /// Called once after native callback registrations. Day startup does not wait.
  Future<void> open() =>
      _opening ??= _run((guard) => _observe(allowPrepare: true, guard: guard));

  /// Rejoin uncertain work with its original UUID; allocate a new UUID only
  /// after the prior result was acknowledged and the owner permits Retry.
  Future<void> recover() async {
    if (!canRecover) return;
    // _run clears the current failure while the new observation is pending,
    // so capture the owner's Retry decision before entering it.
    final allowPrepare =
        _operationId == null && _failure?.safeActions.contains('retry') == true;
    await _run((guard) => _observe(allowPrepare: allowPrepare, guard: guard));
  }

  /// A feature can invalidate shared readiness only with the Runtime owner's
  /// canonical intrinsic Vault projection. Turn and provider failures stay local.
  void reportFailure(AppOwnerException error) {
    final owner = error.ownerFailure;
    if (_disposed ||
        _closing ||
        owner?.domain != 'vault' ||
        !{'vault_locked', 'vault_unavailable'}.contains(owner?.reason) ||
        (owner?.reloadRequired != true && owner?.sealSession != true)) {
      return;
    }
    final incident = owner!.incidentId;
    if (!_observedIncidents.add(incident)) return;
    _failureRevision++;
    _failure = error;
    _state = RuntimeReadinessState.unavailable;
    _reobserveAfterBusy = true;
    notifyListeners();
    _startPendingReobserve();
  }

  Future<void> _observe({
    required bool allowPrepare,
    required _RuntimeRunGuard guard,
  }) async {
    if (!_isCurrent(guard)) return;
    final operationId = _operationId;
    if (operationId != null) {
      await _finishOperation(
        operationId,
        rejoin: _operationSubmitted,
        guard: guard,
      );
      return;
    }
    final snapshot = await gateway.readiness(newAgentRequestId());
    if (!_isCurrent(guard)) return;
    _apply(snapshot, guard);
    if (snapshot.state == RuntimeReadinessState.ready ||
        snapshot.state == RuntimeReadinessState.unavailable ||
        !allowPrepare ||
        !_isCurrent(guard)) {
      return;
    }
    final id = newAgentRequestId();
    if (!_isCurrent(guard)) return;
    _operationId = id;
    _operationSubmitted = false;
    await _finishOperation(id, rejoin: false, guard: guard);
  }

  Future<void> _observeReadinessOnly(_RuntimeRunGuard guard) async {
    if (!_isCurrent(guard)) return;
    final snapshot = await gateway.readiness(newAgentRequestId());
    if (!_isCurrent(guard)) return;
    _apply(snapshot, guard);
  }

  Future<void> _finishOperation(
    String id, {
    required bool rejoin,
    required _RuntimeRunGuard guard,
  }) async {
    if (!_isCurrent(guard)) return;
    RuntimePreparationResult? result;
    try {
      _operationSubmitted = true;
      result = rejoin ? await _rejoin(id, guard) : await gateway.prepare(id);
    } on AppOwnerException catch (error) {
      if (_isCurrent(guard) && !rejoin && _definitelyNotAdmitted(error)) {
        _operationId = null;
        _operationSubmitted = false;
      }
      rethrow;
    }
    if (!_isCurrent(guard) || result == null) return;
    RuntimePreparationResult currentResult = result;
    _validateResult(currentResult, id);

    final elapsed = Stopwatch()..start();
    while (!currentResult.done) {
      if (elapsed.elapsed >= const Duration(seconds: 35)) {
        throw AppOwnerException(
          'deadline_exceeded',
          requestId: id,
          stage: 'runtime_prepare',
        );
      }
      await Future<void>.delayed(const Duration(milliseconds: 80));
      if (!_isCurrent(guard)) return;
      currentResult = await gateway.getPreparation(id);
      if (!_isCurrent(guard)) return;
      _validateResult(currentResult, id);
    }

    final operationFailure = currentResult.failure;
    final acknowledged = await gateway.acknowledge(id);
    if (!_isCurrent(guard)) return;
    _validateResult(acknowledged, id, requireDone: true);
    if (acknowledged.failure != operationFailure) {
      throw const FormatException('Runtime preparation receipt changed.');
    }
    _operationId = null;
    _operationSubmitted = false;

    final snapshot = await gateway.readiness(newAgentRequestId());
    if (!_isCurrent(guard)) return;
    if (operationFailure != null) {
      _apply(snapshot, guard);
      throw _HistoricalPreparationFailure(
        AppOwnerException.fromAppWire(
          operationFailure,
          requestId: id,
          stage: 'runtime_prepare',
          ownerFailure: snapshot.failure,
        ),
      );
    }
    _apply(snapshot, guard);
    if (snapshot.state != RuntimeReadinessState.ready) {
      throw _failure ?? const AppOwnerException('vault_unavailable');
    }
  }

  Future<RuntimePreparationResult?> _rejoin(
    String id,
    _RuntimeRunGuard guard,
  ) async {
    try {
      final result = await gateway.getPreparation(id);
      if (!_isCurrent(guard)) return null;
      _validateResult(result, id);
      return result;
    } on AppOwnerException catch (error) {
      if (!_isCurrent(guard)) return null;
      if (!_isNotFound(error)) rethrow;
      // A missing result does not prove non-admission. Reissue the exact
      // immutable prepare intent with the retained UUID.
      if (!_isCurrent(guard)) return null;
      return gateway.prepare(id);
    }
  }

  void _validateResult(
    RuntimePreparationResult result,
    String id, {
    bool requireDone = false,
  }) {
    if (result.operationId != id || requireDone && !result.done) {
      throw const FormatException('Runtime preparation correlation mismatch.');
    }
  }

  bool _apply(RuntimeReadinessSnapshot snapshot, _RuntimeRunGuard guard) {
    if (!_isCurrent(guard)) return false;
    _state = snapshot.state;
    final owner = snapshot.failure;
    _failure = owner == null
        ? null
        : AppOwnerException.fromAppWire(owner.reason, ownerFailure: owner);
    return true;
  }

  bool _isCurrent(_RuntimeRunGuard guard) =>
      !_disposed &&
      !_closing &&
      guard.epoch == _operationEpoch &&
      guard.failureRevision == _failureRevision;

  bool _definitelyNotAdmitted(AppOwnerException error) =>
      error.commandDisposition == NativeCommandDisposition.notAdmitted ||
      error.commandDisposition == NativeCommandDisposition.notApplied;

  bool _isNotFound(AppOwnerException error) =>
      error.failure == 'not_found' ||
      error.metadata['reason_code'] == 'not_found';

  Future<void> _run(
    Future<void> Function(_RuntimeRunGuard guard) operation, {
    bool clearFailure = true,
  }) async {
    if (_disposed || _closing || _busy) return;
    final guard = _RuntimeRunGuard(++_operationEpoch, _failureRevision);
    _busy = true;
    if (clearFailure) _failure = null;
    notifyListeners();
    try {
      await operation(guard);
    } on Object catch (error, stackTrace) {
      final historicalFailure = error is _HistoricalPreparationFailure
          ? error.failure
          : null;
      final typed =
          historicalFailure ??
          (error is AppOwnerException
              ? error
              : const AppOwnerException('storage_unavailable'));
      if (_isCurrent(guard) && historicalFailure == null) {
        _failure = typed;
        if (_state == RuntimeReadinessState.ready) {
          _state = RuntimeReadinessState.unavailable;
        }
      }
      AppDiagnostics.error(
        component: 'runtime',
        operation: typed.stage ?? 'observe',
        error: StateError(
          'Runtime preparation failed: ${_safeToken(typed.reasonCode ?? typed.failure) ?? 'storage_unavailable'}',
        ),
        stackTrace: stackTrace,
        failure: _safeToken(typed.failure),
        reasonCode: _safeToken(typed.reasonCode),
        failureDomain: _safeToken(typed.domain),
        failureCategory: _safeToken(typed.category),
        incidentId: _safeToken(typed.incidentId),
        requestId: _safeToken(typed.requestId),
        safeActions: typed.safeActions,
      );
    } finally {
      // A feature failure changes the revision, so the run token is stale for
      // applying data. It still owns the busy slot until its await settles.
      if (!_disposed && !_closing && guard.epoch == _operationEpoch) {
        _busy = false;
        notifyListeners();
        _startPendingReobserve();
      }
    }
  }

  void _startPendingReobserve() {
    if (!_reobserveAfterBusy || _disposed || _closing || _busy) return;
    _reobserveAfterBusy = false;
    unawaited(_run(_observeReadinessOnly, clearFailure: false));
  }

  void closeAdmission() {
    if (_disposed || _closing) return;
    _closing = true;
    _operationEpoch++;
    _busy = false;
    _reobserveAfterBusy = false;
    notifyListeners();
  }

  @override
  void dispose() {
    _disposed = true;
    _operationEpoch++;
    _busy = false;
    _reobserveAfterBusy = false;
    super.dispose();
  }
}

final class _RuntimeRunGuard {
  const _RuntimeRunGuard(this.epoch, this.failureRevision);

  final int epoch;
  final int failureRevision;
}

final class _HistoricalPreparationFailure implements Exception {
  const _HistoricalPreparationFailure(this.failure);

  final AppOwnerException failure;
}

String? _safeToken(String? value) =>
    value != null && RegExp(r'^[a-zA-Z0-9_.:-]{1,128}$').hasMatch(value)
    ? value
    : null;
