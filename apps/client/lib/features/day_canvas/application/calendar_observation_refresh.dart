import 'dart:async';

import '../domain/day_models.dart';

abstract interface class CalendarObservationRefreshTask {
  void cancel();
}

abstract interface class CalendarObservationRefreshScheduler {
  CalendarObservationRefreshTask schedule(
    Duration delay,
    Future<void> Function() callback,
  );
}

final class TimerCalendarObservationRefreshScheduler
    implements CalendarObservationRefreshScheduler {
  const TimerCalendarObservationRefreshScheduler();

  @override
  CalendarObservationRefreshTask schedule(
    Duration delay,
    Future<void> Function() callback,
  ) => _TimerRefreshTask(Timer(delay, () => unawaited(callback())));
}

final class CalendarObservationRefreshCoordinator {
  factory CalendarObservationRefreshCoordinator({
    required Future<DaySnapshot> Function() refresh,
    CalendarObservationRefreshScheduler scheduler =
        const TimerCalendarObservationRefreshScheduler(),
    Duration refreshInterval = const Duration(minutes: 3),
  }) => CalendarObservationRefreshCoordinator._(
    refresh,
    scheduler,
    refreshInterval,
  );

  CalendarObservationRefreshCoordinator._(
    this._refresh,
    this._scheduler,
    this.refreshInterval,
  );

  static const supportedProviders = {'event_kit', 'android'};

  final Future<DaySnapshot> Function() _refresh;
  final CalendarObservationRefreshScheduler _scheduler;
  final Duration refreshInterval;

  CalendarObservationRefreshTask? _scheduled;
  Future<void>? _refreshInFlight;
  bool _active = false;
  bool _disposed = false;

  bool get active => _active && !_disposed;

  void reconcile(DaySnapshot? snapshot) {
    final connection = snapshot?.calendar;
    _active =
        connection != null &&
        supportedProviders.contains(connection.provider) &&
        !_permissionRevoked(connection);
    if (!_active) {
      _scheduled?.cancel();
      _scheduled = null;
      return;
    }
    _scheduleNext();
  }

  Future<void> ensureFresh() {
    if (!active) return Future.value();
    final existing = _refreshInFlight;
    if (existing != null) return existing;
    _scheduled?.cancel();
    _scheduled = null;
    final pending = _runRefresh();
    _refreshInFlight = pending;
    return pending;
  }

  Future<void> _runRefresh() async {
    try {
      final snapshot = await _refresh();
      if (_disposed) return;
      reconcile(snapshot);
      final connection = snapshot.calendar;
      if (connection == null || !_active) {
        throw StateError('Device calendar observation is unavailable.');
      }
    } finally {
      _refreshInFlight = null;
      if (active && _scheduled == null) _scheduleNext();
    }
  }

  void _scheduleNext() {
    if (!active) return;
    _scheduled?.cancel();
    _scheduled = _scheduler.schedule(refreshInterval, () async {
      _scheduled = null;
      try {
        await ensureFresh();
      } on Object {
        if (active && _scheduled == null) _scheduleNext();
      }
    });
  }

  void dispose() {
    if (_disposed) return;
    _disposed = true;
    _active = false;
    _scheduled?.cancel();
    _scheduled = null;
  }
}

bool _permissionRevoked(CalendarConnection connection) =>
    connection.error == 'permission_denied' ||
    connection.calendars.isNotEmpty &&
        connection.calendars.every(
          (calendar) => calendar.error == 'permission_denied',
        );

final class _TimerRefreshTask implements CalendarObservationRefreshTask {
  const _TimerRefreshTask(this._timer);

  final Timer _timer;

  @override
  void cancel() => _timer.cancel();
}
