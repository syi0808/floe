import 'dart:async';

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
