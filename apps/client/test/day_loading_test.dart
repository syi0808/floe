import 'dart:async';

import 'package:floe_client/features/day_canvas/application/calendar_gateway.dart';
import 'package:floe_client/features/day_canvas/application/day_gateway.dart';
import 'package:floe_client/features/day_canvas/application/personal_day_controller.dart';
import 'package:floe_client/features/day_canvas/domain/day_models.dart';
import 'package:flutter_test/flutter_test.dart';

class DelayedGateway implements DayGateway {
  final requests = <Completer<DaySnapshot>>[];
  @override
  Future<DaySnapshot> loadDay(DayQuery query) {
    final pending = Completer<DaySnapshot>();
    requests.add(pending);
    return pending.future;
  }

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

class DelayedCalendarGateway extends DelayedGateway implements CalendarGateway {
  final syncQueries = <DayQuery>[];
  final syncRequests = <Completer<DaySnapshot>>[];

  @override
  Future<DaySnapshot> syncCalendar(DayQuery query) {
    syncQueries.add(query);
    final pending = Completer<DaySnapshot>();
    syncRequests.add(pending);
    return pending.future;
  }
}

void main() {
  final date = DateTime.utc(2026, 9, 4);
  final query = DayQuery(
    personId: 'test',
    date: date,
    now: date,
    timezoneOffsetSeconds: 0,
  );
  DaySnapshot snapshot(DateTime day) => DaySnapshot(
    personId: 'test',
    date: day,
    generatedAt: date,
    timezoneOffsetSeconds: 0,
    items: const [],
  );
  test(
    'date navigation syncs the selected day and ignores older results',
    () async {
      final gateway = DelayedCalendarGateway();
      final controller = PersonalDayController(gateway: gateway, query: query);
      addTearDown(controller.dispose);
      final initial = controller.load();
      gateway.requests.single.complete(snapshot(date));
      await initial;

      controller.moveDay(1);
      expect(gateway.syncQueries.last.date, date.add(const Duration(days: 1)));
      expect(controller.loadState, DayLoadState.loading);
      controller.moveDay(-2);
      final previousDay = date.subtract(const Duration(days: 1));
      expect(gateway.syncQueries.last.date, previousDay);
      gateway.syncRequests.last.complete(snapshot(previousDay));
      await Future<void>.delayed(Duration.zero);
      gateway.syncRequests.first.complete(
        snapshot(date.add(const Duration(days: 1))),
      );
      await Future<void>.delayed(Duration.zero);
      expect(controller.snapshot!.date, previousDay);
      expect(controller.loadState, DayLoadState.ready);

      controller.goToday();
      final today = DateTime.now();
      final selected = gateway.syncQueries.last.date;
      expect(selected, DateTime(today.year, today.month, today.day));
      gateway.syncRequests.last.complete(snapshot(selected));
      await Future<void>.delayed(Duration.zero);
      expect(controller.snapshot!.date, selected);
      expect(gateway.requests, hasLength(1));
    },
  );

  test('older loads cannot overwrite a more recent date', () async {
    final gateway = DelayedGateway();
    final controller = PersonalDayController(gateway: gateway, query: query);
    final first = controller.load();
    controller.moveDay(1);
    gateway.requests[1].complete(snapshot(date.add(const Duration(days: 1))));
    await Future<void>.delayed(Duration.zero);
    gateway.requests[0].complete(snapshot(date));
    await first;
    expect(controller.snapshot!.date.day, 5);
    expect(controller.loadState, DayLoadState.ready);
    controller.dispose();
  });
  test('failure stops loading and a retry completes', () async {
    final gateway = DelayedGateway();
    final controller = PersonalDayController(gateway: gateway, query: query);
    final first = controller.load();
    gateway.requests.single.completeError(StateError('offline'));
    await first;
    expect(controller.loadState, DayLoadState.failure);
    final retry = controller.load();
    gateway.requests.last.complete(snapshot(date));
    await retry;
    expect(controller.loadState, DayLoadState.ready);
    expect(controller.errorMessage, isNull);
    controller.dispose();
  });
  test('load completion after disposal does not notify', () async {
    final gateway = DelayedGateway();
    final controller = PersonalDayController(gateway: gateway, query: query);
    final pending = controller.load();
    controller.dispose();
    gateway.requests.single.complete(snapshot(date));
    await pending;
  });
}
