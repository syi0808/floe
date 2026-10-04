import 'dart:async';

import 'package:flutter/foundation.dart';

import 'package:floe_client/app/floe_loading.dart';

import 'package:floe_client/features/day/domain/day_models.dart';
import 'package:floe_client/features/day/application/day_gateway.dart';
import 'package:floe_client/features/conversation/application/agent_request_id.dart';

enum DayLoadState { loading, ready, failure }

final class PersonalDayController extends ChangeNotifier {
  factory PersonalDayController({
    required DayGateway gateway,
    required DayQuery query,
  }) => PersonalDayController._(gateway, query);

  PersonalDayController._(DayGateway gateway, this._query)
    : _gateway = gateway,
      _refreshGateway = gateway is DayRefreshGateway
          ? gateway as DayRefreshGateway
          : null;

  final DayGateway _gateway;
  final DayRefreshGateway? _refreshGateway;
  bool get canRefresh => _refreshGateway != null;
  DayQuery _query;
  DayLoadState loadState = DayLoadState.loading;
  DaySnapshot? snapshot;
  CaptureReceipt? pendingCapture;
  String? errorMessage;
  bool commandPending = false;
  int _loadGeneration = 0;
  bool _disposed = false;
  String? _refreshCommandId;
  DayQuery? _refreshQuery;
  DayRefreshSnapshot? _refreshOperation;
  Future<void>? _refreshing;
  CompletedDayRefresh? lastRefreshAcknowledgement;

  DayQuery get query => _query;

  bool _automaticActive = false;
  Timer? _automaticTimer;
  Future<void>? _automaticRefresh;
  bool _automaticRefreshAgain = false;
  DayQuery? _lastAutomaticQuery;
  final Stopwatch _automaticAge = Stopwatch();

  /// Presentation freshness policy. Reads remain explicit Day commands; merely
  /// querying the owner never starts acquisition or resumes an interrupted job.
  void setAutomaticRefreshActive(bool active) {
    if (_disposed || _automaticActive == active) return;
    _automaticActive = active;
    _automaticTimer?.cancel();
    if (!active) return;
    _automaticTimer = Timer.periodic(const Duration(minutes: 3), (_) {
      unawaited(refreshIfStale());
    });
    unawaited(refreshIfStale());
  }

  Future<void> refreshIfStale({bool force = false}) async {
    if (_disposed || !canRefresh) return;
    if (force) {
      // Remember invalidation while hidden; returning to Day must not reuse
      // the previous source selection's freshness interval.
      _lastAutomaticQuery = null;
      _automaticAge.stop();
    }
    if (!_automaticActive) return;
    final active = _automaticRefresh;
    if (active != null) {
      if (force ||
          _lastAutomaticQuery == null ||
          !_sameQueryIntent(_lastAutomaticQuery!, _query)) {
        _automaticRefreshAgain = true;
      }
      await active;
      return;
    }
    if (!force &&
        _lastAutomaticQuery != null &&
        _sameQueryIntent(_lastAutomaticQuery!, _query) &&
        _automaticAge.isRunning &&
        _automaticAge.elapsed < const Duration(minutes: 3))
      return;
    _automaticRefresh = _runAutomaticRefresh();
    try {
      await _automaticRefresh;
    } finally {
      _automaticRefresh = null;
    }
  }

  Future<void> _runAutomaticRefresh() async {
    do {
      _automaticRefreshAgain = false;
      _lastAutomaticQuery = _query;
      _automaticAge
        ..reset()
        ..start();
      await refresh();
      // Source/date changes while observing coalesce into one subsequent read.
      // An unresolved request still rejoins its original command in _refresh.
    } while (!_disposed && _automaticActive && _automaticRefreshAgain);
  }

  Future<void> load() => _load(false);

  Future<void> refresh() => _load(true);

  Future<void> _load(bool sync) async {
    if (_disposed) return;
    final generation = ++_loadGeneration;
    final query = _query;
    final previous = snapshot;
    final keepPrevious = previous != null && _matchesQuery(previous, query);
    if (!keepPrevious) snapshot = null;
    loadState = DayLoadState.loading;
    errorMessage = null;
    notifyListeners();
    try {
      final result = await FloeLoading.run(
        () => sync ? _refresh(query) : _gateway.loadDay(query),
      );
      if (_disposed) return;
      if (generation != _loadGeneration) {
        // Acquisition completion invalidates an intervening mirror read for
        // this date. Re-query after commit rather than blocking independent
        // reads or publishing a possibly superseded snapshot.
        if (sync && _sameQueryIntent(query, _query)) await _load(false);
        return;
      }
      if (!_matchesQuery(result, query)) {
        throw const FormatException(
          'Day returned a snapshot for a different query.',
        );
      }
      snapshot = result;
      loadState = DayLoadState.ready;
    } on Object catch (error) {
      if (_disposed) return;
      if (generation != _loadGeneration) {
        if (sync &&
            error is _CurrentDayUnavailable &&
            _sameQueryIntent(query, _query))
          await _load(false);
        return;
      }
      if (error is _CurrentDayUnavailable) {
        // The command completed. Its historical receipt is retained separately;
        // unavailable live metadata cannot be presented as Current coverage.
        snapshot = null;
        loadState = DayLoadState.failure;
      } else {
        loadState = keepPrevious ? DayLoadState.ready : DayLoadState.failure;
      }
      errorMessage = error.toString();
    }
    notifyListeners();
  }

  Future<DaySnapshot> _refresh(DayQuery query) async {
    // Finish observing the retained command before admitting an explicitly
    // requested refresh for another date. Never reuse its result for that date.
    final retainedQuery = _refreshQuery ?? query;
    await (_refreshing ??= _refreshOwner(query)
        .whenComplete(() => _refreshing = null));
    if (_disposed) throw StateError('Day observer detached.');
    if (!_sameQueryIntent(retainedQuery, query)) return _refresh(query);
    return _currentDayAfterAcknowledgement(query, 'Calendar refresh completed');
  }

  bool _sameQueryIntent(DayQuery left, DayQuery right) =>
      left.personId == right.personId &&
      left.date.year == right.date.year &&
      left.date.month == right.date.month &&
      left.date.day == right.date.day &&
      left.timezoneOffsetSeconds == right.timezoneOffsetSeconds &&
      (left.endTimezoneOffsetSeconds ?? left.timezoneOffsetSeconds) ==
          (right.endTimezoneOffsetSeconds ?? right.timezoneOffsetSeconds);

  Future<void> _refreshOwner(DayQuery query) async {
    final refresher = _refreshGateway;
    if (refresher == null)
      throw StateError('Day source refresh is not available.');
    _refreshCommandId ??= newAgentRequestId();
    _refreshQuery ??= query;
    final requestQuery = _refreshQuery!;
    final first =
        _refreshOperation ??
        await refresher.refreshDay(
          commandId: _refreshCommandId!,
          query: requestQuery,
        );
    _refreshOperation = first;
    try {
      final result = await awaitDayRefresh(
        refresher,
        first,
        onSnapshot: (value) => _refreshOperation = value,
        detached: () => _disposed,
      );
      if (!_matchesQuery(result, requestQuery)) {
        // Keep the exact admitted intent and request its immutable receipt again.
        _refreshOperation = null;
        throw const FormatException(
          'Day refresh returned a snapshot for a different query.',
        );
      }
      final completed = _refreshOperation;
      if (completed is! CompletedDayRefresh) {
        throw const FormatException(
          'Day refresh completion was not acknowledged.',
        );
      }
      lastRefreshAcknowledgement = completed;
      _refreshCommandId = null;
      _refreshQuery = null;
      _refreshOperation = null;
    } on Object {
      if (_refreshOperation is FailedDayRefresh ||
          _refreshOperation is InterruptedDayRefresh) {
        _refreshCommandId = null;
        _refreshQuery = null;
        _refreshOperation = null;
      }
      rethrow;
    }
  }

  bool _matchesQuery(DaySnapshot snapshot, DayQuery query) =>
      snapshot.personId == query.personId &&
      snapshot.date.year == query.date.year &&
      snapshot.date.month == query.date.month &&
      snapshot.date.day == query.date.day &&
      snapshot.timezoneOffsetSeconds == query.timezoneOffsetSeconds;

  @override
  void dispose() {
    _disposed = true;
    _loadGeneration++;
    _automaticTimer?.cancel();
    super.dispose();
  }

  Future<bool> submitCapture(String input) async {
    final query = _query;
    return _run(() async {
      final capture = await _gateway.submitCapture(input, query);
      if (!_disposed) pendingCapture = capture;
    });
  }

  Future<bool> classify(ClassificationDraft draft) async {
    final capture = pendingCapture;
    if (capture == null) return false;
    final query = _query;
    return _run(() async {
      final result = await _gateway.classifyCapture(capture, draft, query);
      if (!_disposed && identical(pendingCapture, capture))
        pendingCapture = null;
      await _displayAfterMutation(result, query);
    });
  }

  Future<DaySnapshot?> setTaskCompleted(TaskItem task, bool completed) async {
    final query = _query;
    DaySnapshot? acknowledged;
    final succeeded = await _run(() async {
      final result = await _gateway.setTaskCompleted(task, completed, query);
      acknowledged = result;
      await _displayAfterMutation(result, query);
    });
    return succeeded ? acknowledged : null;
  }

  Future<void> deleteItem(DayItem item) async {
    final query = _query;
    await _run(() async {
      final result = await _gateway.deleteItem(item, query);
      await _displayAfterMutation(result, query);
    });
  }

  Future<DaySnapshot> _currentDayAfterAcknowledgement(
    DayQuery query,
    String acknowledgement,
  ) async {
    try {
      final current = await _gateway.loadDay(query);
      if (!_matchesQuery(current, query)) {
        throw const FormatException('Day returned a different display query.');
      }
      return current;
    } on Object catch (error) {
      throw _CurrentDayUnavailable(acknowledgement, error);
    }
  }

  Future<void> _displayAfterMutation(
    DaySnapshot acknowledged,
    DayQuery query,
  ) async {
    if (!_matchesQuery(acknowledged, query)) {
      throw const FormatException('Day mutation returned a different query.');
    }
    if (_disposed || !_sameQueryIntent(query, _query)) return;
    // The receipt is an immutable historical acknowledgement. Read the current
    // display independently, without replaying a successful command on failure.
    final displayGeneration = ++_loadGeneration;
    loadState = DayLoadState.loading;
    try {
      final current = await _currentDayAfterAcknowledgement(
        query,
        'Change saved',
      );
      if (_disposed || displayGeneration != _loadGeneration) return;
      snapshot = current;
      loadState = DayLoadState.ready;
    } on _CurrentDayUnavailable catch (error) {
      if (_disposed || displayGeneration != _loadGeneration) return;
      snapshot = null;
      loadState = DayLoadState.failure;
      errorMessage = error.toString();
      // The mutation succeeded; only display observation failed. The caller
      // must close its save flow instead of retrying the already saved change.
    }
  }

  Future<void> moveDay(int offset) async {
    _query = DayQuery.local(
      personId: _query.personId,
      date: DateTime(
        _query.date.year,
        _query.date.month,
        _query.date.day + offset,
      ),
      now: DateTime.now(),
    );
    await load();
    await refreshIfStale();
  }

  Future<void> goToday() async {
    final now = DateTime.now();
    _query = DayQuery.local(
      personId: _query.personId,
      date: DateTime(now.year, now.month, now.day),
      now: now,
    );
    await load();
    await refreshIfStale();
  }

  void clearError() {
    if (_disposed) return;
    errorMessage = null;
    notifyListeners();
  }

  Future<bool> _run(Future<void> Function() operation) async {
    if (_disposed || commandPending) return false;
    commandPending = true;
    errorMessage = null;
    notifyListeners();
    try {
      await FloeLoading.run(operation);
      return true;
    } on Object catch (error) {
      if (!_disposed) {
        errorMessage = error.toString().replaceFirst('FormatException: ', '');
      }
      return false;
    } finally {
      commandPending = false;
      if (!_disposed) notifyListeners();
    }
  }
}

final class _CurrentDayUnavailable implements Exception {
  const _CurrentDayUnavailable(this.acknowledgement, this.cause);
  final String acknowledgement;
  final Object cause;
  @override
  String toString() =>
      '$acknowledgement, but the current Day could not be loaded. Retry loading Day. $cause';
}
