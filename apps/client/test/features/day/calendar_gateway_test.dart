import 'dart:io';

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/features/connections/domain/calendar_source_connection.dart';
import 'package:floe_client/features/day/application/calendar_gateway.dart';
import 'package:floe_client/features/day/application/calendar_observation_publisher.dart';
import 'package:floe_client/features/day/application/native_day_gateway.dart';
import 'package:floe_client/features/day/domain/day_models.dart';

import '../../support/app_host.dart';

final class FixtureCalendarAdapter implements CalendarAdapter {
  List<CalendarChoice> inventory = const [CalendarChoice('home', 'Home')];
  List<Map<String, dynamic>> records = [
    {
      'external_id': 'event-1',
      'external_revision': '1',
      'can_modify': false,
      'title': 'Meeting',
      'schedule': {
        'kind': 'timed',
        'starts_at': '2026-09-03T15:00:00Z',
        'ends_at': '2026-09-03T16:00:00Z',
        'timezone': 'Asia/Seoul',
      },
    },
  ];
  String? deniedCalendarId;

  @override
  Future<List<CalendarChoice>> calendars({bool requestAccess = true}) async =>
      inventory;

  @override
  Future<List<Map<String, dynamic>>> read(
    String calendarId,
    DayQuery query,
  ) async {
    if (deniedCalendarId == calendarId) {
      throw PlatformException(code: 'permission_denied');
    }
    return records;
  }

  @override
  Future<void> openSettings() async {}
}

final query = DayQuery(
  personId: localPersonId,
  date: DateTime.utc(2026, 9, 4),
  now: DateTime.utc(2026, 9, 4),
  timezoneOffsetSeconds: 32400,
);

Future<CalendarSourceConnection> _select(
  TestAppHost host,
  List<CalendarChoice> calendars, {
  bool all = false,
}) async {
  final resources = [
    for (final calendar in calendars)
      CalendarSourceResource(handle: calendar.id, label: calendar.name),
  ];
  final current = await host.runtime.calendarSource.inspectNative(
    query.personId,
  );
  return current == null
      ? host.runtime.calendarSource.establishNative(
          query.personId,
          resourceMode: all ? 'all_available' : 'selected',
          resources: resources,
        )
      : host.runtime.calendarSource.configureNative(
          query.personId,
          current: current,
          resourceMode: all ? 'all_available' : 'selected',
          resources: resources,
        );
}

void main() {
  test(
    'sync updates Day mirror without changing Connections authority',
    () async {
      final library = File('../../target/debug/libfloe_ffi.dylib').absolute;
      if (!library.existsSync()) {
        markTestSkipped('cargo build -p floe-ffi required');
        return;
      }
      final directory = await Directory.systemTemp.createTemp(
        'floe-calendar-mirror-',
      );
      final adapter = FixtureCalendarAdapter();
      Future<TestAppHost> open() => TestAppHost.open(
        libraryPath: library.path,
        databasePath: '${directory.path}/calendar.db',
        calendarAdapter: adapter,
        clock: () => query.now,
        deviceId: 'test-device',
      );
      var host = await open();
      try {
        final source = await _select(host, adapter.inventory);
        var snapshot = await host.day.syncCalendar(query);
        expect(snapshot.calendar!.sourceConnectionId, source.connectionId);
        expect(snapshot.calendar!.provider, 'event_kit');
        expect(snapshot.items.whereType<EventItem>(), hasLength(1));
        expect(snapshot.calendarMirrorRevision, 1);
        final afterSync = await host.runtime.calendarSource.inspectNative(
          query.personId,
        );
        expect(afterSync!.revision, source.revision);
        expect(afterSync.sourceAuthority, source.sourceAuthority);

        adapter.records.first['can_modify'] = true;
        snapshot = await host.day.syncCalendar(query);
        expect(snapshot.items.whereType<EventItem>().single.canModify, isTrue);
        expect(snapshot.calendarMirrorRevision, 2);
        final afterSecondSync = await host.runtime.calendarSource.inspectNative(
          query.personId,
        );
        expect(afterSecondSync!.revision, source.revision);
        expect(afterSecondSync.sourceAuthority, source.sourceAuthority);

        await host.close();
        host = await open();
        final restored = await host.day.loadDay(query);
        expect(restored.calendar!.sourceConnectionId, source.connectionId);
        expect(restored.items.whereType<EventItem>().single.canModify, isTrue);
        expect(
          (await host.runtime.calendarSource.inspectNative(query.personId))!
              .resources
              .single
              .handle,
          'home',
        );
      } finally {
        await host.close();
        await directory.delete(recursive: true);
      }
    },
  );

  test(
    'all-available inventory changes source authority; read failure does not',
    () async {
      final library = File('../../target/debug/libfloe_ffi.dylib').absolute;
      if (!library.existsSync()) {
        markTestSkipped('cargo build -p floe-ffi required');
        return;
      }
      final directory = await Directory.systemTemp.createTemp(
        'floe-calendar-inventory-',
      );
      final adapter = FixtureCalendarAdapter();
      final host = await TestAppHost.open(
        libraryPath: library.path,
        databasePath: '${directory.path}/calendar.db',
        calendarAdapter: adapter,
        clock: () => query.now,
        deviceId: 'test-device',
      );
      try {
        final original = await _select(host, adapter.inventory, all: true);
        await host.day.syncCalendar(query);
        adapter.inventory = const [
          CalendarChoice('home', 'Home'),
          CalendarChoice('work', 'Work'),
        ];
        final snapshot = await host.day.syncCalendar(query);
        final expanded = (await host.runtime.calendarSource.inspectNative(
          query.personId,
        ))!;
        expect(expanded.selectedCalendarIds, ['home', 'work']);
        expect(expanded.revision, original.revision + 1);
        expect(expanded.sourceAuthority, isNot(original.sourceAuthority));
        expect(snapshot.items.whereType<EventItem>(), hasLength(2));

        adapter.deniedCalendarId = 'work';
        final failed = await host.day.syncCalendar(query);
        expect(
          failed.calendar!.sourceStatuses['work']!.error,
          'permission_denied',
        );
        final afterFailure = (await host.runtime.calendarSource.inspectNative(
          query.personId,
        ))!;
        expect(afterFailure.revision, expanded.revision);
        expect(afterFailure.sourceAuthority, expanded.sourceAuthority);
        expect(afterFailure.selectedCalendarIds, ['home', 'work']);
      } finally {
        await host.close();
        await directory.delete(recursive: true);
      }
    },
  );

  test('large native source still imports although observation publication is bounded', () async {
    final library = File('../../target/debug/libfloe_ffi.dylib').absolute;
    if (!library.existsSync()) {
      markTestSkipped('cargo build -p floe-ffi required');
      return;
    }
    final directory = await Directory.systemTemp.createTemp(
      'floe-calendar-wide-',
    );
    final adapter = FixtureCalendarAdapter()
      ..inventory = List.generate(
        CalendarObservationPublisher.maxCalendarCount + 1,
        (index) => CalendarChoice('calendar-$index', 'Calendar $index'),
      );
    final host = await TestAppHost.open(
      libraryPath: library.path,
      databasePath: '${directory.path}/calendar.db',
      calendarAdapter: adapter,
      clock: () => query.now,
      deviceId: 'test-device',
    );
    try {
      await _select(host, adapter.inventory);
      final snapshot = await host.day.syncCalendar(query);
      expect(
        snapshot.items.whereType<EventItem>(),
        hasLength(adapter.inventory.length),
      );
      expect(snapshot.calendar!.error, isNull);
    } finally {
      await host.close();
      await directory.delete(recursive: true);
    }
  });
}
