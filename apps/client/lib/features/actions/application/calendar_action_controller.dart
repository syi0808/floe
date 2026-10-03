import 'dart:async';
import 'dart:convert';

import 'package:flutter/foundation.dart';

import 'package:floe_client/app/runtime/app_runtime.dart';
import 'package:floe_client/features/actions/application/calendar_action_gateway.dart';
import 'package:floe_client/features/actions/domain/calendar_action.dart';
import 'package:floe_client/features/actions/application/action_command_replay.dart';

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
    final kind = switch (code) {
      'vault_locked' => CalendarActionErrorKind.vaultLocked,
      'vault_unavailable' ||
      'storage_unavailable' => CalendarActionErrorKind.vaultUnavailable,
      'conflict' => CalendarActionErrorKind.conflict,
      'unavailable' ||
      'deadline_exceeded' ||
      'timeout' => CalendarActionErrorKind.unavailable,
      _ => CalendarActionErrorKind.other,
    };
    return CalendarActionError(kind: kind, code: code);
  }

  final CalendarActionErrorKind kind;
  final String code;

  bool get isVaultLocked => kind == CalendarActionErrorKind.vaultLocked;

  String get message => switch (kind) {
    CalendarActionErrorKind.vaultLocked =>
      'Unlock your Floe vault to review or change Actions.',
    CalendarActionErrorKind.vaultUnavailable =>
      'Actions are unavailable while the Floe vault is unavailable.',
    CalendarActionErrorKind.conflict =>
      'The Action changed. Refresh it before making another decision.',
    CalendarActionErrorKind.unavailable => 'The Action result could not be confirmed. Refresh or reconcile the same Action.',
    CalendarActionErrorKind.other =>
      'The Action could not be confirmed. Refresh before trying again.',
  };
}

/// UI state for the single Actions owner. Disposing this controller stops only
/// its scheduled observations; it never cancels work owned by Rust.
final class CalendarActionController extends ChangeNotifier {
  CalendarActionController({required this.gateway})
    : _commands = ActionCommandReplay.forGateway(gateway);

  final CalendarActionGateway gateway;
  List<CalendarAction> _actions = const [];
  ActionAuthority? _authority;
  List<ActionDestinationChoice> _destinations = const [];
  String? _nextCursor;
  CalendarActionError? _error;
  bool _busy = false;
  bool _loaded = false;
  bool _disposed = false;
  final ActionCommandReplay _commands;
  CalendarActionError? _destinationsError;
  bool _destinationsLoaded = false;
  final Map<String, Timer> _observations = {};

  List<CalendarAction> get actions => _actions;
  ActionAuthority? get authority => _authority;
  List<ActionDestinationChoice> get destinations => _destinations;
  String? get nextCursor => _nextCursor;
  CalendarActionError? get error => _error;
  CalendarActionError? get destinationsError => _destinationsError;
  bool get destinationsLoaded => _destinationsLoaded;

  /// Display availability observed from the owner. Every submitted command
  /// still needs the owner's current target and permission admission.
  bool get calendarChangesAvailable =>
      _destinationsLoaded &&
      _destinationsError == null &&
      _destinations.isNotEmpty;
  bool get busy => _busy;
  bool get loaded => _loaded;

  CalendarAction? find(String actionRef) {
    for (final action in _actions) {
      if (action.actionRef == actionRef) return action;
    }
    return null;
  }

  Future<void> load() async {
    if (_busy || _disposed) return;
    _busy = true;
    _error = null;
    _destinationsLoaded = false;
    _destinationsError = null;
    notifyListeners();
    try {
      // Reading durable history/authority must not depend on native Calendar
      // destination discovery succeeding.
      final page = await gateway.list();
      if (_disposed) return;
      _mergePage(page, replace: true);
      _loaded = true;
      final authority = await gateway.loadAuthority();
      if (_disposed) return;
      _acceptAuthority(authority);
      try {
        final destinations = await gateway.loadDestinations();
        if (_disposed) return;
        _destinations = List.unmodifiable(destinations);
        _destinationsLoaded = true;
        _destinationsError = null;
      } on Object catch (error) {
        if (_disposed) return;
        _destinations = const [];
        _destinationsLoaded = false;
        _destinationsError = CalendarActionError.from(error);
      }
      _error = null;
    } on Object catch (error) {
      if (!_disposed) _error = CalendarActionError.from(error);
    } finally {
      if (!_disposed) {
        _busy = false;
        notifyListeners();
      }
    }
  }

  Future<void> loadMore() async {
    final cursor = _nextCursor;
    if (_busy || _disposed || cursor == null) return;
    _busy = true;
    _error = null;
    notifyListeners();
    try {
      final page = await gateway.list(cursor: cursor);
      if (_disposed) return;
      _mergePage(page);
      _error = null;
    } on Object catch (error) {
      if (!_disposed) _error = CalendarActionError.from(error);
    } finally {
      if (!_disposed) {
        _busy = false;
        notifyListeners();
      }
    }
  }

  Future<CalendarAction> inspect(String actionRef) async {
    try {
      final result = await gateway.inspect(actionRef);
      if (_disposed) return result;
      if (result.actionRef != actionRef) {
        throw StateError('Actions inspect returned another reference.');
      }
      _acceptSnapshot(result);
      _error = null;
      notifyListeners();
      return find(actionRef) ?? result;
    } on Object catch (error) {
      if (!_disposed) {
        _error = CalendarActionError.from(error);
        notifyListeners();
      }
      rethrow;
    }
  }

  Future<CalendarAction> submit(ActionIntent intent) async {
    final command = <String, Object?>{
      'kind': 'actions.submit',
      'intent': intent.toJson(),
    };
    return _mutateAction(
      command,
      (commandId) => gateway.submit(commandId: commandId, intent: intent),
      validate: (action) => _validateSubmittedAction(action, intent),
    );
  }

  Future<CalendarAction> decide(
    CalendarAction action,
    CalendarActionDecision decision,
  ) async {
    final current = _requireCurrent(action);
    final allowed = switch (decision) {
      CalendarActionDecision.approve => ActionAllowedAction.approve,
      CalendarActionDecision.reject => ActionAllowedAction.reject,
      CalendarActionDecision.cancel => ActionAllowedAction.cancel,
    };
    if (!current.allowedActions.contains(allowed)) {
      _error = const CalendarActionError(
        kind: CalendarActionErrorKind.conflict,
        code: 'conflict',
      );
      if (!_disposed) notifyListeners();
      throw StateError('The owner does not allow this Action decision.');
    }
    final command = <String, Object?>{
      'kind': 'actions.decide',
      'action_ref': current.actionRef,
      'review_ref': current.reviewRef.toJson(),
      'decision': decision.name,
      'expected_revision': current.revision,
    };
    return _mutateAction(
      command,
      (commandId) => gateway.decide(
        commandId: commandId,
        action: current,
        decision: decision,
      ),
      validate: (result) => _validateActionAdvance(result, current),
    );
  }

  Future<CalendarAction> reconcile(CalendarAction action) async {
    final current = _requireCurrent(action);
    if (!current.allowedActions.contains(ActionAllowedAction.reconcile)) {
      _error = const CalendarActionError(
        kind: CalendarActionErrorKind.conflict,
        code: 'conflict',
      );
      if (!_disposed) notifyListeners();
      throw StateError('The owner does not allow Action reconciliation.');
    }
    final command = <String, Object?>{
      'kind': 'actions.reconcile',
      'action_ref': current.actionRef,
      'expected_revision': current.revision,
    };
    return _mutateAction(
      command,
      (commandId) => gateway.reconcile(commandId: commandId, action: current),
      validate: (result) => _validateActionAdvance(result, current),
    );
  }

  Future<ActionAuthority> setAuthority(ActionAuthorityMode mode) async {
    final current = _authority;
    if (current == null) {
      throw StateError('Actions authority is unavailable until it is loaded.');
    }
    final command = <String, Object?>{
      'kind': 'actions.authority.set_calendar_create',
      'mode': mode.name,
      'expected_revision': current.revision,
    };
    final key = jsonEncode(command);
    return _mutate<ActionAuthority>(
      key,
      (commandId) => gateway.setAuthority(
        commandId: commandId,
        mode: mode,
        expectedRevision: current.revision,
      ),
      validate: (result) {
        if (result.calendarCreate != mode ||
            result.revision < current.revision ||
            (mode != current.calendarCreate &&
                result.revision == current.revision)) {
          throw StateError(
            'Actions authority snapshot did not match the request.',
          );
        }
        _authority = result;
      },
    );
  }

  Future<T> _mutate<T>(
    String commandKey,
    Future<T> Function(String commandId) send, {
    required void Function(T result) validate,
  }) async {
    if (_disposed) throw StateError('Actions controller is disposed.');
    if (_busy) throw StateError('Another Actions request is in progress.');
    _busy = true;
    _error = null;
    notifyListeners();
    try {
      final commandId = _commands.retain(commandKey);
      final result = await send(commandId);
      validate(result);
      _commands.acknowledge(commandKey, commandId);
      _error = null;
      return result;
    } on Object catch (error) {
      _error = CalendarActionError.from(error);
      rethrow;
    } finally {
      if (!_disposed) {
        _busy = false;
        notifyListeners();
      }
    }
  }

  Future<CalendarAction> _mutateAction(
    Map<String, Object?> command,
    Future<CalendarAction> Function(String commandId) send, {
    required void Function(CalendarAction result) validate,
  }) {
    final commandKey = jsonEncode(command);
    return _mutate<CalendarAction>(
      commandKey,
      send,
      validate: (result) {
        validate(result);
        _acceptSnapshot(result);
      },
    );
  }

  CalendarAction _requireCurrent(CalendarAction action) {
    final current = find(action.actionRef);
    if (current == null ||
        current.revision != action.revision ||
        !current.hasSameImmutableIdentity(action)) {
      _error = const CalendarActionError(
        kind: CalendarActionErrorKind.conflict,
        code: 'conflict',
      );
      if (!_disposed) notifyListeners();
      throw StateError('Refresh the Action before making a decision.');
    }
    return current;
  }

  void _validateSubmittedAction(CalendarAction result, ActionIntent intent) {
    final matchesIntent = switch (intent) {
      DirectCreate(:final title, :final schedule) =>
        result.origin == CalendarActionOrigin.direct &&
            result.effect is CreateActionEffect &&
            (result.effect as CreateActionEffect).title == title &&
            _sameSchedule(
              (result.effect as CreateActionEffect).schedule,
              schedule,
            ),
      DirectUpdate(
        :final eventRef,
        :final expectedRevision,
        :final title,
        :final schedule,
      ) =>
        result.origin == CalendarActionOrigin.direct &&
            result.effect is UpdateActionEffect &&
            (result.effect as UpdateActionEffect).eventRef == eventRef &&
            (result.effect as UpdateActionEffect).expectedRevision ==
                expectedRevision &&
            (result.effect as UpdateActionEffect).title == title &&
            _sameSchedule(
              (result.effect as UpdateActionEffect).schedule,
              schedule,
            ),
      DirectDelete(:final eventRef, :final expectedRevision) =>
        result.origin == CalendarActionOrigin.direct &&
            result.effect is DeleteActionEffect &&
            (result.effect as DeleteActionEffect).eventRef == eventRef &&
            (result.effect as DeleteActionEffect).expectedRevision ==
                expectedRevision,
      ExpertProposal() => result.origin == CalendarActionOrigin.expert,
    };
    if (!matchesIntent) {
      throw StateError('Actions submit returned a mismatched Action.');
    }
    final previous = find(result.actionRef);
    if (previous != null && !result.isOlderObservationThan(previous)) {
      _validateActionAdvance(result, previous);
    }
  }

  bool _sameSchedule(ActionSchedule owner, ActionSchedule intent) =>
      owner.startsAt == intent.startsAt &&
      owner.endsAt == intent.endsAt &&
      owner.timezone == intent.timezone;

  void _validateActionAdvance(CalendarAction result, CalendarAction previous) {
    if (!result.follows(previous)) {
      throw StateError('Actions returned a mismatched or regressed snapshot.');
    }
  }

  void _acceptAuthority(ActionAuthority value) {
    final current = _authority;
    if (current != null &&
        (value.revision < current.revision ||
            (value.revision == current.revision &&
                value.calendarCreate != current.calendarCreate))) {
      throw StateError('Actions authority revision regressed.');
    }
    _authority = value;
  }

  void _acceptSnapshot(CalendarAction value) {
    final current = find(value.actionRef);
    if (current != null && value.isOlderObservationThan(current)) return;
    if (current != null) _validateActionAdvance(value, current);
    _actions = List.unmodifiable([
      if (current == null) value,
      for (final action in _actions)
        if (action.actionRef == value.actionRef) value else action,
    ]);
    _scheduleObservation(value);
  }

  void _mergePage(ActionsPage page, {bool replace = false}) {
    final previous = {for (final action in _actions) action.actionRef: action};
    final next = <String, CalendarAction>{
      if (!replace)
        for (final action in _actions) action.actionRef: action,
    };
    for (final action in page.actions) {
      final current = previous[action.actionRef] ?? next[action.actionRef];
      if (current != null && action.isOlderObservationThan(current)) {
        next[action.actionRef] = current;
      } else {
        if (current != null) _validateActionAdvance(action, current);
        next[action.actionRef] = action;
      }
    }
    _actions = List.unmodifiable(next.values);
    _nextCursor = page.nextCursor;
    for (final action in page.actions) {
      _scheduleObservation(next[action.actionRef]!);
    }
    if (replace) {
      final pageRefs = page.actions.map((action) => action.actionRef).toSet();
      for (final action in previous.values) {
        if (!pageRefs.contains(action.actionRef))
          _cancelObservation(action.actionRef);
      }
    }
  }

  void _scheduleObservation(CalendarAction action) {
    _cancelObservation(action.actionRef);
    final delay = action.nextObservationAfterMs;
    if (_disposed || delay == null || delay <= 0 || delay > 60000) return;
    _observations[action.actionRef] = Timer(Duration(milliseconds: delay), () {
      _observations.remove(action.actionRef);
      unawaited(_observeAfterDelay(action.actionRef));
    });
  }

  Future<void> _observeAfterDelay(String actionRef) async {
    if (_disposed) return;
    try {
      await inspect(actionRef);
    } on Object catch (error) {
      if (_disposed) return;
      _error = CalendarActionError.from(error);
      notifyListeners();
    }
  }

  void _cancelObservation(String actionRef) {
    _observations.remove(actionRef)?.cancel();
  }

  @override
  void dispose() {
    _disposed = true;
    for (final timer in _observations.values) {
      timer.cancel();
    }
    _observations.clear();
    super.dispose();
  }
}
