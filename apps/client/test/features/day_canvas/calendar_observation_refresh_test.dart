import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/features/day_canvas/application/calendar_observation_refresh.dart';
import 'package:floe_client/features/day_canvas/domain/day_models.dart';

void main() {
  test(
    'refreshes before observation expiry and rearms after success',
    () async {
      final scheduler = _ManualScheduler();
      var refreshes = 0;
      final coordinator = CalendarObservationRefreshCoordinator(
        scheduler: scheduler,
        refresh: () async {
          refreshes++;
          return _snapshot();
        },
      );

      coordinator.reconcile(_snapshot());
      expect(scheduler.delays, [const Duration(minutes: 3)]);

      await scheduler.fire();

      expect(refreshes, 1);
      expect(scheduler.delays, [
        const Duration(minutes: 3),
        const Duration(minutes: 3),
      ]);
      coordinator.dispose();
    },
  );

  test('coalesces simultaneous resume and expert refresh triggers', () async {
    final scheduler = _ManualScheduler();
    final completion = Completer<DaySnapshot>();
    var refreshes = 0;
    final coordinator = CalendarObservationRefreshCoordinator(
      scheduler: scheduler,
      refresh: () {
        refreshes++;
        return completion.future;
      },
    )..reconcile(_snapshot());

    final first = coordinator.ensureFresh();
    final second = coordinator.ensureFresh();
    expect(refreshes, 1);
    expect(identical(first, second), isTrue);

    completion.complete(_snapshot());
    await Future.wait([first, second]);
    expect(refreshes, 1);
    coordinator.dispose();
  });

  test('permission revocation cancels future refreshes', () async {
    final scheduler = _ManualScheduler();
    final coordinator = CalendarObservationRefreshCoordinator(
      scheduler: scheduler,
      refresh: () async => _snapshot(error: 'permission_denied'),
    )..reconcile(_snapshot());

    await expectLater(coordinator.ensureFresh(), throwsStateError);

    expect(coordinator.active, isFalse);
    expect(scheduler.activeTasks, isEmpty);
    await coordinator.ensureFresh();
    expect(scheduler.delays, hasLength(1));
    coordinator.dispose();
  });

  test('disconnect and dispose cancel scheduled refresh', () {
    final scheduler = _ManualScheduler();
    final coordinator = CalendarObservationRefreshCoordinator(
      scheduler: scheduler,
      refresh: () async => _snapshot(),
    )..reconcile(_snapshot());

    coordinator.reconcile(_snapshot(connected: false));
    expect(scheduler.activeTasks, isEmpty);

    coordinator.reconcile(_snapshot());
    coordinator.dispose();
    expect(scheduler.activeTasks, isEmpty);
  });
}

DaySnapshot _snapshot({bool connected = true, String? error}) => DaySnapshot(
  personId: 'person-1',
  date: DateTime.utc(2026, 9, 11),
  generatedAt: DateTime.utc(2026, 9, 11),
  timezoneOffsetSeconds: 0,
  items: const [],
  overdueTaskCount: 0,
  calendar: connected
      ? CalendarConnection(
          provider: 'event_kit',
          revision: 1,
          error: error,
          calendars: const [ConnectedCalendar(id: 'home', name: 'Home')],
        )
      : null,
);

final class _ManualScheduler implements CalendarObservationRefreshScheduler {
  final List<Duration> delays = [];
  final List<_ManualTask> tasks = [];

  Iterable<_ManualTask> get activeTasks =>
      tasks.where((task) => !task.cancelled);

  @override
  CalendarObservationRefreshTask schedule(
    Duration delay,
    Future<void> Function() callback,
  ) {
    delays.add(delay);
    final task = _ManualTask(callback);
    tasks.add(task);
    return task;
  }

  Future<void> fire() async {
    final task = activeTasks.first;
    task.cancelled = true;
    await task.callback();
  }
}

final class _ManualTask implements CalendarObservationRefreshTask {
  _ManualTask(this.callback);

  final Future<void> Function() callback;
  bool cancelled = false;

  @override
  void cancel() => cancelled = true;
}
