import '../../support/app_host.dart';

import 'dart:io';

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/features/day/application/calendar_gateway.dart';
import 'package:floe_client/features/day/application/calendar_observation_publisher.dart';
import 'package:floe_client/features/day/application/native_day_gateway.dart';
import 'package:floe_client/features/day/domain/day_models.dart';

class FixtureCalendarAdapter implements CalendarAdapter {
  bool denied = false;
  String? deniedCalendarId;
  List<CalendarChoice> inventory = [
    const CalendarChoice('fixture', 'Test calendar', provider: 'fixture'),
  ];
  List<Map<String, dynamic>> records = [
    {
      'external_id': 'fixture-1',
      'can_modify': false,
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
      'can_modify': false,
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
  Future<List<CalendarChoice>> calendars({bool requestAccess = true}) async {
    if (denied) throw PlatformException(code: 'permission_denied');
    return inventory;
  }

  @override
  Future<List<Map<String, dynamic>>> read(
    String calendarId,
    DayQuery query,
  ) async {
    if (denied || deniedCalendarId == calendarId) {
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

void main() {
  test('device calendar sync publishes a bound Rust observation', () async {
    final library = File('../../target/debug/libfloe_ffi.dylib').absolute;
    if (!library.existsSync()) {
      markTestSkipped('cargo build -p floe-ffi required');
      return;
    }
    final directory = await Directory.systemTemp.createTemp(
      'floe-calendar-observation-',
    );
    final now = DateTime.now().toUtc();
    final publicationQuery = DayQuery(
      personId: localPersonId,
      date: DateTime.utc(now.year, now.month, now.day),
      now: now,
      timezoneOffsetSeconds: 0,
    );
    final adapter = FixtureCalendarAdapter()
      ..inventory = const [
        CalendarChoice('device-calendar', 'Device', provider: 'event_kit'),
      ]
      ..records = [
        {
          'external_id': 'device-event',
          'can_modify': false,
          'external_revision': '1',
          'title': 'Device event',
          'schedule': {
            'kind': 'timed',
            'starts_at': publicationQuery.startsAt
                .add(const Duration(hours: 1))
                .toIso8601String(),
            'ends_at': publicationQuery.startsAt
                .add(const Duration(hours: 2))
                .toIso8601String(),
            'timezone': 'UTC',
          },
        },
      ];
    final gateway = await TestAppHost.open(
      libraryPath: library.path,
      databasePath: '${directory.path}/calendar.db',
      calendarAdapter: adapter,
      deviceId: 'device-1',
    );
    try {
      await gateway.day.selectCalendars(adapter.inventory, publicationQuery);
      final snapshot = await gateway.day.syncCalendar(publicationQuery);

      expect(snapshot.calendar!.provider, 'event_kit');
      expect(snapshot.calendar!.revision, greaterThan(0));
    } finally {
      await gateway.close();
      await directory.delete(recursive: true);
    }
  });

  test(
    'wide calendar sync succeeds when Agent observation scope is bounded',
    () async {
      final library = File('../../target/debug/libfloe_ffi.dylib').absolute;
      if (!library.existsSync()) {
        markTestSkipped('cargo build -p floe-ffi required');
        return;
      }
      final directory = await Directory.systemTemp.createTemp(
        'floe-wide-calendar-observation-',
      );
      final adapter = FixtureCalendarAdapter()
        ..inventory = List.generate(
          CalendarObservationPublisher.maxCalendarCount + 1,
          (index) => CalendarChoice(
            'calendar-$index',
            'Calendar $index',
            provider: 'event_kit',
          ),
        );
      final gateway = await TestAppHost.open(
        libraryPath: library.path,
        databasePath: '${directory.path}/calendar.db',
        calendarAdapter: adapter,
        clock: () => query.now,
        deviceId: 'device-1',
      );
      try {
        await gateway.day.selectCalendars(
          adapter.inventory,
          query,
          includeAll: true,
        );
        final snapshot = await gateway.day.syncCalendar(query);

        expect(snapshot.calendar!.error, isNull);
        expect(
          snapshot.calendar!.calendars.every(
            (calendar) => calendar.error == null,
          ),
          isTrue,
        );
        expect(snapshot.items, hasLength(adapter.inventory.length * 2));
      } finally {
        await gateway.close();
        await directory.delete(recursive: true);
      }
    },
  );

  test(
    'refresh updates cached drag capability through the native JSON bridge',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'floe-drag-capability-',
      );
      final adapter = FixtureCalendarAdapter()
        ..inventory = [const CalendarChoice('fixture', 'Test calendar')];
      final gateway = await TestAppHost.open(
        libraryPath: File('../../target/debug/libfloe_ffi.dylib').absolute.path,
        databasePath: '${directory.path}/calendar.db',
        calendarAdapter: adapter,
        clock: () => query.now,
        deviceId: 'test-device',
      );
      try {
        var snapshot = await gateway.day.selectCalendars(
          adapter.inventory,
          query,
        );
        expect(
          snapshot.items.whereType<EventItem>().where(
            (event) => event.canModify,
          ),
          isEmpty,
        );
        adapter.records.first['can_modify'] = true;
        snapshot = await gateway.day.syncCalendar(query);
        final event = snapshot.items.whereType<EventItem>().firstWhere(
          (event) => !event.isAllDay,
        );
        expect(event.provider, 'event_kit');
        expect(event.canModify, isTrue);
        final cached = await gateway.day.loadDay(query);
        expect(
          cached.items
              .whereType<EventItem>()
              .firstWhere((item) => item.id == event.id)
              .canModify,
          isTrue,
        );
        adapter.records.first['can_modify'] = false;
        snapshot = await gateway.day.syncCalendar(query);
        expect(
          snapshot.items.whereType<EventItem>().where(
            (event) => event.canModify,
          ),
          isEmpty,
        );
      } finally {
        await gateway.close();
        await directory.delete(recursive: true);
      }
    },
  );
  test('selected scope stays fixed; all scope discovers and preserves unavailable source cache after reopen', () async {
    final library = File('../../target/debug/libfloe_ffi.dylib').absolute;
    expect(
      library.existsSync(),
      isTrue,
      reason: 'cargo build -p floe-ffi required',
    );
    final directory = await Directory.systemTemp.createTemp('floe-scope-');
    final adapter = FixtureCalendarAdapter();
    Future<TestAppHost> open() => TestAppHost.open(
      libraryPath: library.path,
      databasePath: '${directory.path}/scope.db',
      calendarAdapter: adapter,
      clock: () => query.now,
      deviceId: 'test-device',
    );
    var gateway = await open();
    try {
      await gateway.day.selectCalendars(adapter.inventory, query);
      await gateway.day.syncCalendar(query);
      adapter.inventory = [
        ...adapter.inventory,
        const CalendarChoice('new', 'New', provider: 'fixture'),
      ];
      var snapshot = await gateway.day.syncCalendar(query);
      expect(snapshot.calendar!.includeAll, isFalse);
      expect(snapshot.calendar!.selectedCalendarIds, ['fixture']);
      expect(snapshot.items, hasLength(2));
      await gateway.day.selectCalendars(
        [adapter.inventory.first],
        query,
        includeAll: true,
      );
      snapshot = await gateway.day.syncCalendar(query);
      expect(snapshot.items, hasLength(4));
      final identifiers = snapshot.items.map((item) => item.id).toSet();
      await gateway.close();
      gateway = await open();
      snapshot = await gateway.day.loadDay(query);
      expect(snapshot.calendar!.includeAll, isTrue);
      adapter.inventory = [adapter.inventory.last];
      snapshot = await gateway.day.syncCalendar(query);
      expect(snapshot.items.map((item) => item.id).toSet(), identifiers);
      expect(
        snapshot.calendar!.connectedCalendars
            .firstWhere((source) => source.id == 'fixture')
            .error,
        'calendar_unavailable',
      );
      expect(
        snapshot.calendar!.connectedCalendars
            .firstWhere((source) => source.id == 'new')
            .error,
        isNull,
      );
    } finally {
      await gateway.close();
      await directory.delete(recursive: true);
    }
  });
  test(
    'multiple calendars preserve sources, selection, and atomic cache',
    () async {
      final library = File('../../target/debug/libfloe_ffi.dylib').absolute;
      if (!library.existsSync()) {
        markTestSkipped('cargo build -p floe-ffi required');
        return;
      }
      final directory = await Directory.systemTemp.createTemp('floe-multiple-');
      final adapter = FixtureCalendarAdapter();
      Future<TestAppHost> open() => TestAppHost.open(
        libraryPath: library.path,
        databasePath: '${directory.path}/calendar.db',
        calendarAdapter: adapter,
        clock: () => query.now,
        deviceId: 'test-device',
      );
      var gateway = await open();
      const calendars = [
        CalendarChoice('home', 'Home', provider: 'fixture'),
        CalendarChoice('work', 'Work', provider: 'fixture'),
      ];
      adapter.inventory = calendars;
      try {
        await gateway.day.selectCalendars(calendars, query);
        var snapshot = await gateway.day.syncCalendar(query);
        expect(snapshot.items, hasLength(4));
        expect(
          snapshot.items
              .whereType<EventItem>()
              .map((event) => event.calendarName)
              .toSet(),
          {'Home', 'Work'},
        );
        final identifiers = snapshot.items.map((item) => item.id).toSet();
        snapshot = await gateway.day.syncCalendar(query);
        expect(snapshot.items.map((item) => item.id).toSet(), identifiers);
        await gateway.close();
        gateway = await open();
        snapshot = await gateway.day.loadDay(query);
        expect(snapshot.calendar!.selectedCalendarIds, ['home', 'work']);
        expect(
          snapshot.calendar!.connectedCalendars.map(
            (calendar) => calendar.name,
          ),
          ['Home', 'Work'],
        );
        adapter.records = [];
        adapter.deniedCalendarId = 'work';
        snapshot = await gateway.day.syncCalendar(query);
        expect(snapshot.calendar!.error, 'permission_denied');
        expect(snapshot.items, hasLength(2));
        expect(
          snapshot.items.whereType<EventItem>().every(
            (event) => event.calendarName == 'Work',
          ),
          isTrue,
        );
        snapshot = await gateway.day.selectCalendars([calendars.first], query);
        expect(snapshot.items, isEmpty);
        expect(
          snapshot.items.every((item) => identifiers.contains(item.id)),
          isTrue,
        );
        snapshot = await gateway.day.syncCalendar(query);
        expect(snapshot.items, isEmpty);
        expect(snapshot.calendar!.error, isNull);
      } finally {
        await gateway.close();
        await directory.delete(recursive: true);
      }
    },
  );

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
      var gateway = await TestAppHost.open(
        libraryPath: library.path,
        databasePath: '${directory.path}/calendar.db',
        calendarAdapter: adapter,
        clock: () => query.now,
        deviceId: 'test-device',
      );
      try {
        await gateway.day.selectCalendar(
          (await gateway.day.calendars()).single,
          query,
        );
        var snapshot = await gateway.day.syncCalendar(query);
        expect(snapshot.items, hasLength(2));
        final event = snapshot.items.whereType<EventItem>().first;
        expect(event.externalId, isNotNull);
        expect(event.sourceLabel, 'Fixture · Test calendar');
        final identifiers = snapshot.items.map((item) => item.id).toSet();
        snapshot = await gateway.day.syncCalendar(query);
        expect(snapshot.items.map((item) => item.id).toSet(), identifiers);
        adapter.denied = true;
        snapshot = await gateway.day.syncCalendar(query);
        expect(snapshot.calendar!.error, 'permission_denied');
        expect(snapshot.items, hasLength(2));
        await gateway.close();
        gateway = await TestAppHost.open(
          libraryPath: library.path,
          databasePath: '${directory.path}/calendar.db',
          calendarAdapter: adapter,
          clock: () => query.now,
          deviceId: 'test-device',
        );
        snapshot = await gateway.day.loadDay(query);
        expect(snapshot.calendar!.error, 'permission_denied');
        expect(snapshot.items.map((item) => item.id).toSet(), identifiers);
        adapter.denied = false;
        adapter.records = [];
        snapshot = await gateway.day.syncCalendar(query);
        expect(snapshot.items, isEmpty);
        expect(snapshot.calendar!.error, isNull);
      } finally {
        await gateway.close();
        await directory.delete(recursive: true);
      }
    },
  );
}
