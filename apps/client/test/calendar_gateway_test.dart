import 'dart:io';

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/features/day_canvas/application/calendar_gateway.dart';
import 'package:floe_client/features/day_canvas/application/ffi_day_gateway.dart';
import 'package:floe_client/features/day_canvas/domain/day_models.dart';

class FixtureCalendarAdapter implements CalendarAdapter {
  bool denied = false;
  List<Map<String, dynamic>> records = [
    {
      'external_id': 'fixture-1',
      'external_revision': '1',
      'title': '외부 일정',
      'schedule': {
        'kind': 'timed',
        'starts_at': '2026-09-03T15:00:00Z',
        'ends_at': '2026-09-03T16:00:00Z',
        'timezone': 'Asia/Seoul',
      },
    },
    {
      'external_id': 'fixture-2',
      'external_revision': '1',
      'title': '종일 일정',
      'schedule': {
        'kind': 'all_day',
        'start_date': '2026-09-04',
        'end_date_exclusive': '2026-09-05',
      },
    },
  ];

  @override
  Future<List<CalendarChoice>> calendars() async => [
    const CalendarChoice('fixture', 'Test calendar', provider: 'fixture'),
  ];
  @override
  Future<List<Map<String, dynamic>>> read(
    String calendarId,
    DayQuery query,
  ) async {
    if (denied) throw PlatformException(code: 'permission_denied');
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

void main() {
  test(
    'fixture crosses native ABI, preserves provenance, failure, and restart',
    () async {
      final library = File('../../target/debug/libfloe_ffi.dylib').absolute;
      if (!library.existsSync()) {
        markTestSkipped('cargo build -p floe-ffi required');
        return;
      }
      final directory = await Directory.systemTemp.createTemp('floe-calendar-');
      final adapter = FixtureCalendarAdapter();
      var gateway = await FfiDayGateway.open(
        libraryPath: library.path,
        databasePath: '${directory.path}/calendar.db',
        calendarAdapter: adapter,
        clock: () => query.now,
      );
      try {
        await gateway.selectCalendar((await gateway.calendars()).single, query);
        var snapshot = await gateway.syncCalendar(query);
        expect(snapshot.items, hasLength(2));
        final event = snapshot.items.whereType<EventItem>().first;
        expect(event.externalId, isNotNull);
        expect(event.sourceLabel, 'Fixture · Test calendar');
        final identifiers = snapshot.items.map((item) => item.id).toSet();
        snapshot = await gateway.syncCalendar(query);
        expect(snapshot.items.map((item) => item.id).toSet(), identifiers);
        adapter.denied = true;
        snapshot = await gateway.syncCalendar(query);
        expect(snapshot.calendar!.error, 'permission_denied');
        expect(snapshot.items, hasLength(2));
        await gateway.close();
        gateway = await FfiDayGateway.open(
          libraryPath: library.path,
          databasePath: '${directory.path}/calendar.db',
          calendarAdapter: adapter,
          clock: () => query.now,
        );
        snapshot = await gateway.loadDay(query);
        expect(snapshot.calendar!.error, 'permission_denied');
        expect(snapshot.items.map((item) => item.id).toSet(), identifiers);
        adapter.denied = false;
        adapter.records = [];
        snapshot = await gateway.syncCalendar(query);
        expect(snapshot.items, isEmpty);
        expect(snapshot.calendar!.error, isNull);
      } finally {
        await gateway.close();
        await directory.delete(recursive: true);
      }
    },
  );
}
