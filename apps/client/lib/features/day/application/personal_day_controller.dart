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

  PersonalDayController._(this._gateway, this._query)
    : _refreshGateway = gateway is DayRefreshGateway
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
  Future<DaySnapshot>? _refreshing;

  DayQuery get query => _query;

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
      if (_disposed || generation != _loadGeneration) return;
      if (!_matchesQuery(result, query)) {
        throw const FormatException(
          'Day returned a snapshot for a different query.',
        );
      }
      snapshot = result;
      loadState = DayLoadState.ready;
    } on Object catch (error) {
      if (_disposed || generation != _loadGeneration) return;
      loadState = keepPrevious ? DayLoadState.ready : DayLoadState.failure;
      errorMessage = error.toString();
    }
    notifyListeners();
  }

  Future<DaySnapshot> _refresh(DayQuery query) async {
    // Finish observing the retained command before admitting an explicitly
    // requested refresh for another date. Never reuse its result for that date.
    final retainedQuery = _refreshQuery ?? query;
    final result = await (_refreshing ??= _refreshOwner(query)
        .whenComplete(() => _refreshing = null));
    if (_disposed) throw StateError('Day observer detached.');
    if (!_sameQueryIntent(retainedQuery, query)) return _refresh(query);
    return result;
  }

  bool _sameQueryIntent(DayQuery left, DayQuery right) =>
      left.personId == right.personId &&
      left.date.year == right.date.year &&
      left.date.month == right.date.month &&
      left.date.day == right.date.day &&
      left.timezoneOffsetSeconds == right.timezoneOffsetSeconds &&
      (left.endTimezoneOffsetSeconds ?? left.timezoneOffsetSeconds) ==
          (right.endTimezoneOffsetSeconds ?? right.timezoneOffsetSeconds);

  Future<DaySnapshot> _refreshOwner(DayQuery query) async {
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
      _refreshCommandId = null;
      _refreshQuery = null;
      _refreshOperation = null;
      return result;
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
    final generation = _loadGeneration;
    return _run(() async {
      final result = await _gateway.classifyCapture(capture, draft, query);
      _acceptMutationSnapshot(result, query, generation);
      if (!_disposed && identical(pendingCapture, capture)) pendingCapture = null;
    });
  }

  Future<DaySnapshot?> setTaskCompleted(TaskItem task, bool completed) async {
    final query = _query;
    final generation = _loadGeneration;
    DaySnapshot? acknowledged;
    final succeeded = await _run(() async {
      final result = await _gateway.setTaskCompleted(task, completed, query);
      _acceptMutationSnapshot(result, query, generation);
      acknowledged = result;
    });
    return succeeded ? acknowledged : null;
  }

  Future<void> deleteItem(DayItem item) async {
    final query = _query;
    final generation = _loadGeneration;
    await _run(() async {
      final result = await _gateway.deleteItem(item, query);
      _acceptMutationSnapshot(result, query, generation);
    });
  }

  void _acceptMutationSnapshot(
    DaySnapshot result,
    DayQuery query,
    int generation,
  ) {
    if (!_matchesQuery(result, query)) {
      throw const FormatException('Day mutation returned a different query.');
    }
    if (_disposed || generation != _loadGeneration ||
        !_sameQueryIntent(query, _query)) return;
    // A read admitted before this mutation must not replace its acknowledgement.
    _loadGeneration++;
    snapshot = result;
    loadState = DayLoadState.ready;
  }

  Future<void> moveDay(int offset) {
    _query = DayQuery.local(
      personId: _query.personId,
      date: DateTime(
        _query.date.year,
        _query.date.month,
        _query.date.day + offset,
      ),
      now: DateTime.now(),
    );
    return load();
  }

  Future<void> goToday() {
    final now = DateTime.now();
    _query = DayQuery.local(
      personId: _query.personId,
      date: DateTime(now.year, now.month, now.day),
      now: now,
    );
    return load();
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
