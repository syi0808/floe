import 'dart:async';

import 'package:floe_client/features/day/domain/day_models.dart';

/// Supplied only when the durable Day acquisition owner is assembled.
abstract interface class DayRefreshGateway {
  Future<DayRefreshSnapshot> refreshDay({
    required String commandId,
    required DayQuery query,
  });
  Future<DayRefreshSnapshot> observeDayRefresh(String operationRef);
}

abstract interface class DayGateway {
  Future<DaySnapshot> loadDay(DayQuery query);
  Future<List<ManualCalendarDestination>> loadExternalCalendarDestinations();
  Future<ManualCalendarOperationPage> loadExternalCalendarOperations({
    String? cursor,
    int limit = 100,
  });
  Future<ManualCalendarOperationReceipt> inspectExternalCalendarOperation(
    String operationRef,
  );
  Future<ManualCalendarOperationReceipt> reconcileExternalCalendarOperation(
    String operationRef,
    int expectedRevision,
  );
  Future<ManualCalendarOperationReceipt> executeExternalCalendarOperation(
    ManualCalendarOperationIntent operation,
  );
  Future<CaptureReceipt> submitCapture(String input, DayQuery query);
  Future<DaySnapshot> classifyCapture(
    CaptureReceipt capture,
    ClassificationDraft classification,
    DayQuery query,
  );
  Future<DaySnapshot> setTaskCompleted(
    TaskItem task,
    bool completed,
    DayQuery query,
  );
  Future<DaySnapshot> deleteItem(DayItem item, DayQuery query);
}

Future<DaySnapshot> awaitDayRefresh(
  DayRefreshGateway gateway,
  DayRefreshSnapshot initial, {
  void Function(DayRefreshSnapshot snapshot)? onSnapshot,
  Duration observationTimeout = const Duration(seconds: 35),
  bool Function()? detached,
}) async {
  var current = initial;
  final elapsed = Stopwatch()..start();
  while (true) {
    if (detached?.call() == true) throw StateError('Day observer detached.');
    onSnapshot?.call(current);
    switch (current) {
      case CompletedDayRefresh(:final day):
        return day;
      case FailedDayRefresh(:final failure):
        throw StateError('Day refresh failed: ${failure.name}');
      case InterruptedDayRefresh(:final failure):
        throw StateError('Day refresh interrupted: ${failure.name}');
      case PendingDayRefresh():
      case RunningDayRefresh():
        break;
    }
    if (elapsed.elapsed >= observationTimeout)
      throw TimeoutException('Day refresh observation timed out.');
    await Future<void>.delayed(const Duration(milliseconds: 100));
    final next = await gateway.observeDayRefresh(current.operationRef);
    if (next.operationRef != current.operationRef ||
        next.revision < current.revision ||
        (next.revision == current.revision &&
            next.runtimeType != current.runtimeType) ||
        (current is RunningDayRefresh && next is PendingDayRefresh)) {
      throw const FormatException(
        'Day refresh observation changed identity or regressed.',
      );
    }
    current = next;
  }
}
