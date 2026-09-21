import '../../support/app_host.dart';

import 'dart:io';

import 'package:floe_client/features/day/application/native_day_gateway.dart';
import 'package:floe_client/features/day/application/calendar_gateway.dart';
import 'package:floe_client/features/day/domain/day_models.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'Android selection uses the persisted device connection identity',
    () async {
      final library = File('../../target/debug/libfloe_ffi.dylib').absolute;
      if (!library.existsSync()) {
        markTestSkipped('cargo build -p floe-ffi가 필요합니다.');
        return;
      }
      final temporaryDirectory = await Directory.systemTemp.createTemp(
        'floe-android-calendar-binding-test-',
      );
      final now = DateTime.utc(2026, 9, 3, 9);
      final query = DayQuery(
        personId: localPersonId,
        date: now,
        now: now,
        timezoneOffsetSeconds: 0,
      );
      final gateway = await TestAppHost.open(
        libraryPath: library.path,
        databasePath: '${temporaryDirectory.path}/floe.db',
        clock: () => now,
        deviceId: 'local-00000000-0000-4000-8000-000000000011',
      );
      final snapshot = await gateway.day.selectCalendars(const [
        CalendarChoice(
          'android-selected',
          'Selected Android calendars',
          provider: 'android',
        ),
      ], query);
      expect(
        snapshot.calendar!.connectionId,
        '00000000-0000-4000-8000-000000000011',
      );
      expect(
        snapshot.calendar!.deviceId,
        'local-00000000-0000-4000-8000-000000000011',
      );
      expect(snapshot.calendar!.provider, 'android');
      expect(snapshot.calendar!.selectedCalendarIds, ['android-selected']);
      await gateway.close();
      await temporaryDirectory.delete(recursive: true);
    },
  );

  test(
    'server Calendar identity and selector survive a Rust restart',
    () async {
      final library = File('../../target/debug/libfloe_ffi.dylib').absolute;
      if (!library.existsSync()) {
        markTestSkipped('cargo build -p floe-ffi가 필요합니다.');
        return;
      }
      final temporaryDirectory = await Directory.systemTemp.createTemp(
        'floe-calendar-binding-test-',
      );
      final now = DateTime.utc(2026, 9, 3, 9);
      final query = DayQuery(
        personId: localPersonId,
        date: now,
        now: now,
        timezoneOffsetSeconds: 0,
      );
      var gateway = await TestAppHost.open(
        libraryPath: library.path,
        databasePath: '${temporaryDirectory.path}/floe.db',
        clock: () => now,
        deviceId: 'paired-device',
      );
      var snapshot = await gateway.day.bindCalendarConnection(
        connectionId: '8a1d7fb0-435d-5d1e-aab4-53ed2894da61',
        connectionRevision: 7,
        deviceId: 'paired-device',
        provider: 'google_calendar',
        calendars: const [
          CalendarChoice(
            'primary@example.test',
            'Google Calendar',
            provider: 'google_calendar',
          ),
        ],
        query: query,
      );
      expect(
        snapshot.calendar!.connectionId,
        '8a1d7fb0-435d-5d1e-aab4-53ed2894da61',
      );
      expect(snapshot.calendar!.revision, 7);
      expect(snapshot.calendar!.deviceId, 'paired-device');
      expect(snapshot.calendar!.provider, 'google_calendar');
      expect(snapshot.calendar!.selectedCalendarIds, ['primary@example.test']);
      await gateway.close();

      gateway = await TestAppHost.open(
        libraryPath: library.path,
        databasePath: '${temporaryDirectory.path}/floe.db',
        clock: () => now,
        deviceId: 'paired-device',
      );
      snapshot = await gateway.day.loadDay(query);
      expect(
        snapshot.calendar!.connectionId,
        '8a1d7fb0-435d-5d1e-aab4-53ed2894da61',
      );
      expect(snapshot.calendar!.revision, 7);
      expect(snapshot.calendar!.provider, 'google_calendar');
      await gateway.close();
      await temporaryDirectory.delete(recursive: true);
    },
  );

  test(
    'changing the active provider replaces the Rust calendar binding',
    () async {
      final library = File('../../target/debug/libfloe_ffi.dylib').absolute;
      if (!library.existsSync()) {
        markTestSkipped('cargo build -p floe-ffi가 필요합니다.');
        return;
      }
      final temporaryDirectory = await Directory.systemTemp.createTemp(
        'floe-calendar-provider-switch-test-',
      );
      final now = DateTime.utc(2026, 9, 3, 9);
      final query = DayQuery(
        personId: localPersonId,
        date: now,
        now: now,
        timezoneOffsetSeconds: 0,
      );
      var gateway = await TestAppHost.open(
        libraryPath: library.path,
        databasePath: '${temporaryDirectory.path}/floe.db',
        clock: () => now,
        deviceId: 'paired-device',
      );
      await gateway.day.bindCalendarConnection(
        connectionId: '8a1d7fb0-435d-5d1e-aab4-53ed2894da61',
        connectionRevision: 7,
        deviceId: 'paired-device',
        provider: 'google_calendar',
        calendars: const [
          CalendarChoice(
            'primary@example.test',
            'Google Calendar',
            provider: 'google_calendar',
          ),
        ],
        query: query,
      );
      final switched = await gateway.day.bindCalendarConnection(
        connectionId: '3d2e7a71-194b-4b47-84cc-b58c5ce17772',
        connectionRevision: 11,
        deviceId: 'paired-device',
        provider: 'microsoft_calendar',
        calendars: const [
          CalendarChoice(
            'calendar@microsoft.test',
            'Microsoft Calendar',
            provider: 'microsoft_calendar',
          ),
        ],
        query: query,
      );
      expect(
        switched.calendar!.connectionId,
        '3d2e7a71-194b-4b47-84cc-b58c5ce17772',
      );
      expect(switched.calendar!.revision, 11);
      expect(switched.calendar!.provider, 'microsoft_calendar');
      expect(switched.calendar!.selectedCalendarIds, [
        'calendar@microsoft.test',
      ]);
      await gateway.close();

      gateway = await TestAppHost.open(
        libraryPath: library.path,
        databasePath: '${temporaryDirectory.path}/floe.db',
        clock: () => now,
        deviceId: 'paired-device',
      );
      final restored = await gateway.day.loadDay(query);
      expect(
        restored.calendar!.connectionId,
        '3d2e7a71-194b-4b47-84cc-b58c5ce17772',
      );
      expect(restored.calendar!.provider, 'microsoft_calendar');
      expect(restored.calendar!.selectedCalendarIds, [
        'calendar@microsoft.test',
      ]);
      await gateway.close();
      await temporaryDirectory.delete(recursive: true);
    },
  );

  test('Rust/Turso gateway persists the complete task lifecycle', () async {
    final library = File('../../target/debug/libfloe_ffi.dylib').absolute;
    if (!library.existsSync()) {
      markTestSkipped('cargo build -p floe-ffi가 필요합니다.');
      return;
    }

    final temporaryDirectory = await Directory.systemTemp.createTemp(
      'floe-ffi-test-',
    );
    final databasePath = '${temporaryDirectory.path}/floe.db';
    final now = DateTime.utc(2026, 9, 3, 9);
    final query = DayQuery(
      personId: localPersonId,
      date: now,
      now: now,
      timezoneOffsetSeconds: 0,
    );

    var gateway = await TestAppHost.open(
      libraryPath: library.path,
      databasePath: databasePath,
      clock: () => now,
      deviceId: 'test-device',
    );
    expect((await gateway.day.loadDay(query)).items, isEmpty);

    final capture = await gateway.day.submitCapture('Rust 연결 확인', query);
    var snapshot = await gateway.day.classifyCapture(
      capture,
      const TaskDraft(title: 'Rust 연결 확인'),
      query,
    );
    var task = snapshot.items.single as TaskItem;
    expect(task.title, 'Rust 연결 확인');
    expect(task.isCompleted, isFalse);

    snapshot = await gateway.day.setTaskCompleted(task, true, query);
    task = snapshot.items.single as TaskItem;
    expect(task.isCompleted, isTrue);
    snapshot = await gateway.day.setTaskCompleted(task, false, query);
    task = snapshot.items.single as TaskItem;
    expect(task.isCompleted, isFalse);

    final eventCapture = await gateway.day.submitCapture('아침 점검', query);
    snapshot = await gateway.day.classifyCapture(
      eventCapture,
      EventDraft(
        title: '아침 점검',
        startsAt: now.subtract(const Duration(minutes: 30)),
        endsAt: now.add(const Duration(minutes: 30)),
      ),
      query,
    );
    final event = snapshot.items.whereType<EventItem>().single;
    expect(snapshot.nowEventId, event.id);

    final noteCapture = await gateway.day.submitCapture('연결 메모', query);
    snapshot = await gateway.day.classifyCapture(
      noteCapture,
      const NoteDraft(content: '연결 메모'),
      query,
    );
    expect(snapshot.items.whereType<NoteItem>().single.title, '연결 메모');
    await gateway.close();

    gateway = await TestAppHost.open(
      libraryPath: library.path,
      databasePath: databasePath,
      clock: () => now,
      deviceId: 'test-device',
    );
    snapshot = await gateway.day.loadDay(query);
    expect(snapshot.items, hasLength(3));
    task = snapshot.items.whereType<TaskItem>().single;
    expect(task.title, 'Rust 연결 확인');
    for (final item in List<DayItem>.of(snapshot.items)) {
      snapshot = await gateway.day.deleteItem(item, query);
    }
    expect(snapshot.items, isEmpty);
    await gateway.close();

    gateway = await TestAppHost.open(
      libraryPath: library.path,
      databasePath: databasePath,
      clock: () => now,
      deviceId: 'test-device',
    );
    expect((await gateway.day.loadDay(query)).items, isEmpty);
    await gateway.close();
    await temporaryDirectory.delete(recursive: true);
  });
}
