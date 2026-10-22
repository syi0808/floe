import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/features/day/application/day_gateway.dart';
import 'package:floe_client/features/day/application/personal_day_controller.dart';
import 'package:floe_client/features/day/domain/day_models.dart';

void main() {
  test(
    'failed latest observation stays stale even after dismissing its error',
    () async {
      var now = DateTime.utc(2026, 10, 22, 10);
      final gateway = _Gateway();
      final controller = PersonalDayController(
        gateway: gateway,
        query: _query(now),
        clock: () => now,
      );
      addTearDown(controller.dispose);
      await controller.load();
      expect(controller.hasCurrentObservation, isTrue);
      final observed = controller.snapshot;
      now = now.add(const Duration(minutes: 2));
      gateway.failRead = true;
      await controller.load();
      expect(controller.snapshot, same(observed));
      expect(controller.hasCurrentObservation, isFalse);
      controller.clearError();
      expect(controller.hasCurrentObservation, isFalse);
      gateway.failRead = false;
      await controller.load();
      expect(controller.hasCurrentObservation, isTrue);
    },
  );

  test(
    'new reads advance now without changing selected date or DST bounds',
    () async {
      var now = DateTime.utc(2026, 11, 1, 5, 30);
      final selected = DayQuery(
        personId: 'person',
        date: DateTime(2026, 11, 1),
        now: now,
        timezoneOffsetSeconds: -14400,
        endTimezoneOffsetSeconds: -18000,
      );
      final gateway = _Gateway();
      final controller = PersonalDayController(
        gateway: gateway,
        query: selected,
        clock: () => now,
      );
      addTearDown(controller.dispose);
      await controller.load();
      now = now.add(const Duration(hours: 2));
      await controller.load();
      final read = gateway.reads.last;
      expect(read.now, now);
      expect(controller.snapshot!.generatedAt, now);
      expect(read.date, selected.date);
      expect(read.startsAt, selected.startsAt);
      expect(read.endsAt, selected.endsAt);
      expect(read.endsAt.difference(read.startsAt), const Duration(hours: 25));
    },
  );

  test(
    'refresh pins admission time but reads current time after completion',
    () async {
      var now = DateTime.utc(2026, 10, 14, 9);
      final gateway = _Gateway();
      final controller = PersonalDayController(
        gateway: gateway,
        query: _query(now),
        clock: () => now,
      );
      addTearDown(controller.dispose);
      now = now.add(const Duration(minutes: 3));
      final admittedAt = now;
      gateway.onRefresh = (query) async {
        now = now.add(const Duration(minutes: 2));
        return _complete(query);
      };
      await controller.refresh();
      expect(gateway.refreshes.single.now, admittedAt);
      expect(
        controller.lastRefreshAcknowledgement!.day.generatedAt,
        admittedAt,
      );
      expect(gateway.reads.single.now, now);
      expect(controller.snapshot!.generatedAt, now);
    },
  );

  test(
    'uncertain refresh rejoins exact intent while live reads advance',
    () async {
      var now = DateTime.utc(2026, 10, 14, 9);
      final gateway = _Gateway();
      final controller = PersonalDayController(
        gateway: gateway,
        query: _query(now),
        clock: () => now,
      );
      addTearDown(controller.dispose);
      gateway.onRefresh = (_) async => throw TimeoutException('response lost');
      await controller.refresh();
      final admitted = gateway.refreshes.single;
      now = now.add(const Duration(minutes: 7));
      await controller.load();
      expect(gateway.reads.last.now, now);
      gateway.onRefresh = (query) async => _complete(query);
      await controller.refresh();
      expect(gateway.commandIds[1], gateway.commandIds[0]);
      expect(identical(gateway.refreshes[1], admitted), isTrue);
      expect(gateway.refreshes[1].now, admitted.now);
      expect(gateway.reads.last.now, now);
      now = now.add(const Duration(minutes: 1));
      await controller.refresh();
      expect(gateway.commandIds[2], isNot(gateway.commandIds[0]));
      expect(gateway.refreshes[2].now, now);
    },
  );

  test(
    'automatic invalidation uses fresh time without moving selected day',
    () async {
      var now = DateTime(2026, 10, 14, 23, 59);
      final gateway = _Gateway();
      final controller = PersonalDayController(
        gateway: gateway,
        query: _query(now),
        clock: () => now,
      );
      addTearDown(controller.dispose);
      controller.setAutomaticRefreshActive(true);
      await controller.refreshIfStale();
      final selected = controller.query.date;
      now = now.add(const Duration(minutes: 4));
      await controller.refreshIfStale(force: true);
      expect(gateway.refreshes, hasLength(2));
      expect(gateway.refreshes.last.now, now);
      expect(controller.query.date, selected);
      expect(controller.snapshot!.generatedAt, now);
      controller.setAutomaticRefreshActive(false);
    },
  );

  test(
    'Today and date navigation use the same injected device clock',
    () async {
      var now = DateTime(2026, 10, 14, 23, 59);
      final gateway = _Gateway();
      final controller = PersonalDayController(
        gateway: gateway,
        query: _query(now),
        clock: () => now,
      );
      addTearDown(controller.dispose);
      now = DateTime(2026, 10, 15, 0, 2);
      await controller.moveDay(-1);
      expect(controller.query.date, DateTime(2026, 10, 13));
      expect(gateway.reads.last.now, now);
      await controller.goToday();
      expect(controller.query.date, DateTime(2026, 10, 15));
      expect(gateway.reads.last.now, now);
    },
  );

  test(
    'post-mutation display refreshes time without changing its receipt',
    () async {
      var now = DateTime.utc(2026, 10, 14, 9);
      final gateway = _Gateway();
      final controller = PersonalDayController(
        gateway: gateway,
        query: _query(now),
        clock: () => now,
      );
      addTearDown(controller.dispose);
      now = now.add(const Duration(minutes: 4));
      final admittedAt = now;
      gateway.onMutation = (query) async {
        now = now.add(const Duration(minutes: 2));
        return _snapshot(query);
      };
      final result = await controller.setTaskCompleted(
        TaskItem(id: 'task', title: 'test', revision: 1, createdAt: now),
        true,
      );
      expect(gateway.mutations.single.now, admittedAt);
      expect(result!.generatedAt, admittedAt);
      expect(controller.snapshot!.generatedAt, now);
    },
  );
  test('queued different-day refresh samples time only at admission', () async {
    var now = DateTime(2026, 10, 14, 9);
    final gateway = _Gateway();
    final firstResult = Completer<DayRefreshSnapshot>();
    gateway.onRefresh = (query) => gateway.refreshes.length == 1
        ? firstResult.future
        : Future.value(_complete(query));
    final controller = PersonalDayController(
      gateway: gateway,
      query: _query(now),
      clock: () => now,
    );
    addTearDown(controller.dispose);
    final first = controller.refresh();
    final firstQuery = gateway.refreshes.single;
    await controller.moveDay(1);
    now = now.add(const Duration(minutes: 5));
    final second = controller.refresh();
    expect(gateway.refreshes, hasLength(1));
    now = now.add(const Duration(minutes: 5));
    firstResult.complete(_complete(firstQuery));
    await Future.wait([first, second]);
    expect(gateway.refreshes, hasLength(2));
    expect(gateway.refreshes.first.now, firstQuery.now);
    expect(gateway.refreshes.last.now, now);
    expect(gateway.refreshes.last.date, DateTime(2026, 10, 15));
    expect(gateway.commandIds.last, isNot(gateway.commandIds.first));
    expect(controller.snapshot!.date, DateTime(2026, 10, 15));
    expect(controller.snapshot!.generatedAt, now);
  });
}

DayQuery _query(DateTime now) =>
    DayQuery.local(personId: 'person', date: now, now: now);
DaySnapshot _snapshot(DayQuery query) => DaySnapshot(
  personId: query.personId,
  date: query.date,
  generatedAt: query.now,
  timezoneOffsetSeconds: query.timezoneOffsetSeconds,
  items: const [],
);
CompletedDayRefresh _complete(DayQuery query) => CompletedDayRefresh(
  operationRef: 'refresh',
  revision: 1,
  day: _snapshot(query),
);

final class _Gateway implements DayGateway, DayRefreshGateway {
  bool failRead = false;
  final reads = <DayQuery>[];
  final refreshes = <DayQuery>[];
  final commandIds = <String>[];
  final mutations = <DayQuery>[];
  Future<DayRefreshSnapshot> Function(DayQuery)? onRefresh;
  Future<DaySnapshot> Function(DayQuery)? onMutation;
  @override
  Future<DaySnapshot> loadDay(DayQuery query) async {
    reads.add(query);
    if (failRead) throw StateError('Synthetic read failed');
    return _snapshot(query);
  }

  @override
  Future<DayRefreshSnapshot> refreshDay({
    required String commandId,
    required DayQuery query,
  }) async {
    commandIds.add(commandId);
    refreshes.add(query);
    return onRefresh == null ? _complete(query) : onRefresh!(query);
  }

  @override
  Future<DaySnapshot> setTaskCompleted(
    TaskItem task,
    bool completed,
    DayQuery query,
  ) async {
    mutations.add(query);
    return onMutation == null ? _snapshot(query) : onMutation!(query);
  }

  @override
  dynamic noSuchMethod(Invocation invocation) =>
      throw StateError('Unexpected gateway call: ${invocation.memberName}');
}
