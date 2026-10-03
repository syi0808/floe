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
    DayRefreshGateway? refreshGateway,
  }) => PersonalDayController._(gateway, query, refreshGateway);

  PersonalDayController._(this._gateway, this._query, this._refreshGateway);

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
    final generation = ++_loadGeneration;
    final query = _query;
    loadState = DayLoadState.loading;
    errorMessage = null;
    notifyListeners();
    try {
      final result = await FloeLoading.run(
        () => sync ? _refresh(query) : _gateway.loadDay(query),
      );
      if (_disposed || generation != _loadGeneration) return;
      snapshot = result;
      loadState = DayLoadState.ready;
    } on Object catch (error) {
      if (_disposed || generation != _loadGeneration) return;
      loadState = DayLoadState.failure;
      errorMessage = error.toString();
    }
    notifyListeners();
  }

  Future<DaySnapshot> _refresh(DayQuery query) =>
      _refreshing ??= _refreshOwner(query)
          .whenComplete(() => _refreshing = null);

  Future<DaySnapshot> _refreshOwner(DayQuery query) async {
    final refresher = _refreshGateway;
    if (refresher == null)
      throw StateError('Day source refresh is not available.');
    _refreshCommandId ??= newAgentRequestId();
    _refreshQuery ??= query;
    final first =
        _refreshOperation ??
        await refresher.refreshDay(
          commandId: _refreshCommandId!,
          query: _refreshQuery!,
        );
    _refreshOperation = first;
    try {
      final result = await awaitDayRefresh(
        refresher,
        first,
        onSnapshot: (value) => _refreshOperation = value,
        detached: () => _disposed,
      );
      final matches =
          result.personId == query.personId &&
          result.date.year == query.date.year &&
          result.date.month == query.date.month &&
          result.date.day == query.date.day &&
          result.timezoneOffsetSeconds == query.timezoneOffsetSeconds;
      _refreshCommandId = null;
      _refreshQuery = null;
      _refreshOperation = null;
      return matches ? result : await _refreshOwner(query);
    } on Object {
      if (_refreshOperation?.terminal == true) {
        _refreshCommandId = null;
        _refreshQuery = null;
        _refreshOperation = null;
      }
      rethrow;
    }
  }

  @override
  void dispose() {
    _disposed = true;
    _loadGeneration++;
    super.dispose();
  }

  Future<bool> submitCapture(String input) async {
    return _run(() async {
      pendingCapture = await _gateway.submitCapture(input, _query);
    });
  }

  Future<bool> classify(ClassificationDraft draft) async {
    final capture = pendingCapture;
    if (capture == null) return false;
    return _run(() async {
      snapshot = await _gateway.classifyCapture(capture, draft, _query);
      pendingCapture = null;
    });
  }

  Future<void> setTaskCompleted(TaskItem task, bool completed) async {
    await _run(() async {
      snapshot = await _gateway.setTaskCompleted(task, completed, _query);
    });
  }

  Future<void> deleteItem(DayItem item) async {
    await _run(() async {
      snapshot = await _gateway.deleteItem(item, _query);
    });
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
    errorMessage = null;
    notifyListeners();
  }

  Future<bool> _run(Future<void> Function() operation) async {
    commandPending = true;
    errorMessage = null;
    notifyListeners();
    try {
      await FloeLoading.run(operation);
      return true;
    } on Object catch (error) {
      errorMessage = error.toString().replaceFirst('FormatException: ', '');
      return false;
    } finally {
      commandPending = false;
      notifyListeners();
    }
  }
}
