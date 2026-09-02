import 'package:flutter/foundation.dart';

import '../domain/day_models.dart';
import 'day_gateway.dart';

enum DayLoadState { loading, ready, failure }

final class PersonalDayController extends ChangeNotifier {
  factory PersonalDayController({
    required DayGateway gateway,
    required DayQuery query,
  }) => PersonalDayController._(gateway, query);

  PersonalDayController._(this._gateway, this._query);

  final DayGateway _gateway;
  DayQuery _query;
  DayLoadState loadState = DayLoadState.loading;
  DaySnapshot? snapshot;
  CaptureReceipt? pendingCapture;
  String? errorMessage;
  bool commandPending = false;

  DayQuery get query => _query;

  Future<void> load() async {
    loadState = DayLoadState.loading;
    errorMessage = null;
    notifyListeners();
    try {
      snapshot = await _gateway.loadDay(_query);
      loadState = DayLoadState.ready;
    } on Object catch (error) {
      loadState = DayLoadState.failure;
      errorMessage = error.toString();
    }
    notifyListeners();
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

  void moveDay(int offset) {
    _query = DayQuery(
      personId: _query.personId,
      date: _query.date.add(Duration(days: offset)),
      now: DateTime.now(),
      timezoneOffsetSeconds: DateTime.now().timeZoneOffset.inSeconds,
    );
    load();
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
      await operation();
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
