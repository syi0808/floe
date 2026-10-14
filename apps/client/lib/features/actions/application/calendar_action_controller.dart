import 'dart:async';

import 'package:flutter/foundation.dart';

import 'package:floe_client/app/runtime/app_runtime.dart';
import 'package:floe_client/app/runtime/app_owner_exception.dart';
import 'package:floe_client/app/runtime/runtime_controller.dart';
import 'package:floe_client/features/actions/application/operation_policy_command_replay.dart';
import 'package:floe_client/features/actions/application/calendar_action_gateway.dart';
import 'package:floe_client/features/actions/domain/calendar_action.dart';

enum CalendarActionErrorKind {
  vaultLocked,
  vaultUnavailable,
  conflict,
  unavailable,
  other,
}

final class CalendarActionError {
  const CalendarActionError({required this.kind, required this.code});

  factory CalendarActionError.from(Object error) {
    final code = error is AppRuntimeException ? error.code : 'internal';
    return CalendarActionError(
      code: code,
      kind: switch (code) {
        'vault_locked' => CalendarActionErrorKind.vaultLocked,
        'vault_unavailable' ||
        'storage_unavailable' => CalendarActionErrorKind.vaultUnavailable,
        'conflict' => CalendarActionErrorKind.conflict,
        'unavailable' ||
        'deadline_exceeded' ||
        'timeout' => CalendarActionErrorKind.unavailable,
        _ => CalendarActionErrorKind.other,
      },
    );
  }

  final CalendarActionErrorKind kind;
  final String code;

  String get message => switch (kind) {
    CalendarActionErrorKind.vaultLocked =>
      'Runtime preparation is required to change operation permissions.',
    CalendarActionErrorKind.vaultUnavailable => 'Operation permissions are unavailable until Runtime preparation completes.',
    CalendarActionErrorKind.conflict =>
      'The operation policy changed. Refresh before changing it again.',
    CalendarActionErrorKind.unavailable =>
      'The policy result could not be confirmed. Refresh its current value.',
    CalendarActionErrorKind.other =>
      'The operation policy could not be confirmed.',
  };
}

/// Settings projection for the Access-owned Expert operation policy.
final class OperationPolicyController extends ChangeNotifier {
  OperationPolicyController({required this.gateway, required this.runtime})
    : _commands = OperationPolicyCommandReplay.forGateway(gateway) {
    runtime.addListener(_readinessChanged);
    _wasReady = runtime.ready;
    if (_wasReady) unawaited(load());
  }

  final OperationAuthorizationGateway gateway;
  final RuntimeController runtime;
  final OperationPolicyCommandReplay _commands;
  bool _wasReady = false;
  bool _busy = false;
  bool _loaded = false;
  bool _disposed = false;
  int _generation = 0;
  ActionAuthority? _authority;
  CalendarActionError? _error;

  ActionAuthority? get authority => _authority;
  bool get hasPendingChange => _commands.pending != null;
  bool get busy => _busy || !runtime.ready;
  bool get loaded => _loaded;
  CalendarActionError? get error => runtime.ready
      ? _error
      : CalendarActionError(
          kind: runtime.failure?.failure == 'vault_locked'
              ? CalendarActionErrorKind.vaultLocked
              : CalendarActionErrorKind.vaultUnavailable,
          code: runtime.reasonCode ?? 'storage_unavailable',
        );

  bool _current(int generation) =>
      !_disposed && runtime.ready && generation == _generation;

  void _readinessChanged() {
    if (_disposed || _wasReady == runtime.ready) return;
    _wasReady = runtime.ready;
    _generation++;
    _authority = null;
    _error = null;
    _loaded = false;
    notifyListeners();
    if (runtime.ready && !_busy) unawaited(load());
  }

  void _reportStorageFailure(Object failure) {
    final owner = failure is AppRuntimeException ? failure.ownerFailure : null;
    if (owner != null) {
      runtime.reportFailure(
        AppOwnerException.fromAppWire(owner.reason, ownerFailure: owner),
      );
    }
  }

  Future<void> load() async {
    if (busy || _disposed) return;
    final generation = _generation;
    _busy = true;
    _error = null;
    notifyListeners();
    try {
      final authority = await gateway.loadAuthority();
      if (!_current(generation)) return;
      _acceptAuthority(authority);
      _loaded = true;
    } on Object catch (failure) {
      if (_current(generation)) {
        _reportStorageFailure(failure);
        _error = CalendarActionError.from(failure);
      }
    } finally {
      if (!_disposed) {
        _busy = false;
        notifyListeners();
        if (runtime.ready && generation != _generation) unawaited(load());
      }
    }
  }

  Future<ActionAuthority> setAuthority(ActionAuthorityMode mode) async {
    final current = _authority;
    if (current == null) {
      throw StateError('Access operation policy is unavailable until loaded.');
    }
    return _mutate(() => _commands.retain(mode: mode, before: current));
  }

  Future<ActionAuthority> retryPendingChange() {
    final pending = _commands.pending;
    if (pending == null) throw StateError('No policy change is pending.');
    return _mutate(() => pending);
  }

  Future<ActionAuthority> _mutate(
    OperationPolicyCommand Function() request,
  ) async {
    if (_disposed) throw StateError('Policy controller is disposed.');
    if (busy) throw StateError('Access operation policy is unavailable.');
    final generation = _generation;
    _busy = true;
    _error = null;
    notifyListeners();
    OperationPolicyCommand? command;
    int? submission;
    try {
      command = request();
      submission = _commands.begin(command);
      final result = await gateway.setAuthority(
        commandId: command.commandId,
        mode: command.mode,
        expectedRevision: command.before.revision,
      );
      if (!_current(generation)) {
        throw StateError('Policy result belongs to retired storage.');
      }
      if (result.calendarCreate != command.mode ||
          result.revision < command.before.revision ||
          (command.mode != command.before.calendarCreate &&
              result.revision == command.before.revision)) {
        throw StateError('Access policy response did not match the request.');
      }
      // An exact replay may return a historical receipt. Never regress a newer
      // current-value observation to that receipt just to retire correlation.
      if (_authority == null || result.revision >= _authority!.revision) {
        _acceptAuthority(result);
      }
      _commands.acknowledge(command);
      return result;
    } on Object catch (failure) {
      if (command != null && submission != null) {
        _commands.reject(
          command,
          submission,
          failure is AppRuntimeException ? failure.commandOutcome : null,
        );
      }
      if (_current(generation)) {
        _reportStorageFailure(failure);
        _error = CalendarActionError.from(failure);
      }
      rethrow;
    } finally {
      if (!_disposed) {
        _busy = false;
        notifyListeners();
        if (runtime.ready && generation != _generation) unawaited(load());
      }
    }
  }

  void _acceptAuthority(ActionAuthority next) {
    final current = _authority;
    if (current != null &&
        (next.revision < current.revision ||
            (next.revision == current.revision &&
                next.calendarCreate != current.calendarCreate))) {
      throw StateError('Operation policy revision regressed.');
    }
    _authority = next;
  }

  @override
  void dispose() {
    _disposed = true;
    runtime.removeListener(_readinessChanged);
    super.dispose();
  }
}
