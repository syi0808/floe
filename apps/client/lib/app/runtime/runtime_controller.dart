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
  Future<void> open() => _opening ??= _run(() => _observe(allowPrepare: true));

  /// Rejoin uncertain work with its original UUID; allocate a new UUID only
  /// after the prior result was acknowledged and the owner permits Retry.
  Future<void> recover() async {
    if (!canRecover) return;
    await _run(
      () => _observe(
        allowPrepare:
            _operationId == null &&
            _failure?.safeActions.contains('retry') == true,
      ),
    );
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
    notifyListeners();
    if (!_busy) unawaited(_run(() => _observe(allowPrepare: false)));
  }

  Future<void> _observe({required bool allowPrepare}) async {
    final operationId = _operationId;
    if (operationId != null) {
      await _finishOperation(operationId, rejoin: _operationSubmitted);
      return;
    }
    final snapshot = await gateway.readiness(newAgentRequestId());
    _apply(snapshot);
    if (snapshot.state == RuntimeReadinessState.ready ||
        snapshot.state == RuntimeReadinessState.unavailable ||
        !allowPrepare) {
      return;
    }
    final id = newAgentRequestId();
    _operationId = id;
    _operationSubmitted = false;
    await _finishOperation(id, rejoin: false);
  }

  Future<void> _finishOperation(String id, {required bool rejoin}) async {
    RuntimePreparationResult result;
    try {
      _operationSubmitted = true;
      result = rejoin ? await _rejoin(id) : await gateway.prepare(id);
    } on AppOwnerException catch (error) {
      if (!rejoin && _definitelyNotAdmitted(error)) {
        _operationId = null;
        _operationSubmitted = false;
      }
      rethrow;
    }
    _validateResult(result, id);

    final elapsed = Stopwatch()..start();
    while (!result.done) {
      if (elapsed.elapsed >= const Duration(seconds: 35)) {
        throw AppOwnerException(
          'deadline_exceeded',
          requestId: id,
          stage: 'runtime_prepare',
        );
      }
      await Future<void>.delayed(const Duration(milliseconds: 80));
      result = await gateway.getPreparation(id);
      _validateResult(result, id);
    }

    final operationFailure = result.failure;
    final acknowledged = await gateway.acknowledge(id);
    _validateResult(acknowledged, id, requireDone: true);
    if (acknowledged.failure != operationFailure) {
      throw const FormatException('Runtime preparation receipt changed.');
    }
    _operationId = null;
    _operationSubmitted = false;

    final snapshot = await gateway.readiness(newAgentRequestId());
    if (operationFailure != null) {
      _apply(snapshot);
      final owner = snapshot.failure;
      _state = snapshot.state == RuntimeReadinessState.ready
          ? RuntimeReadinessState.unavailable
          : snapshot.state;
      _failure = AppOwnerException.fromAppWire(
        operationFailure,
        requestId: id,
        stage: 'runtime_prepare',
        ownerFailure: owner,
      );
      throw _failure!;
    }
    _apply(snapshot);
    if (snapshot.state != RuntimeReadinessState.ready) {
      throw _failure ?? const AppOwnerException('vault_unavailable');
    }
  }

  Future<RuntimePreparationResult> _rejoin(String id) async {
    try {
      final result = await gateway.getPreparation(id);
      _validateResult(result, id);
      return result;
    } on AppOwnerException catch (error) {
      if (!_isNotFound(error)) rethrow;
      // A missing result does not prove non-admission. Reissue the exact
      // immutable prepare intent with the retained UUID.
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

  void _apply(RuntimeReadinessSnapshot snapshot) {
    _state = snapshot.state;
    final owner = snapshot.failure;
    _failure = owner == null
        ? null
        : AppOwnerException.fromAppWire(owner.reason, ownerFailure: owner);
  }

  bool _definitelyNotAdmitted(AppOwnerException error) =>
      error.commandDisposition == NativeCommandDisposition.notAdmitted ||
      error.commandDisposition == NativeCommandDisposition.notApplied;

  bool _isNotFound(AppOwnerException error) =>
      error.failure == 'not_found' ||
      error.metadata['reason_code'] == 'not_found';

  Future<void> _run(Future<void> Function() operation) async {
    if (_disposed || _closing || _busy) return;
    final failureRevision = _failureRevision;
    _busy = true;
    _failure = null;
    notifyListeners();
    try {
      await operation();
    } on Object catch (error, stackTrace) {
      final typed = error is AppOwnerException
          ? error
          : const AppOwnerException('storage_unavailable');
      if (!_disposed && !_closing && failureRevision == _failureRevision) {
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
      _busy = false;
      if (!_disposed && !_closing) notifyListeners();
    }
  }

  void closeAdmission() {
    if (_disposed || _closing) return;
    _closing = true;
    notifyListeners();
  }

  @override
  void dispose() {
    _disposed = true;
    super.dispose();
  }
}

String? _safeToken(String? value) =>
    value != null && RegExp(r'^[a-zA-Z0-9_.:-]{1,128}$').hasMatch(value)
    ? value
    : null;
