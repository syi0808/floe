import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/features/connections/domain/calendar_source_connection.dart';
import 'package:floe_client/features/day/application/native_day_gateway.dart';
import 'package:floe_client/features/day/domain/day_models.dart';

import '../../support/app_host.dart';

void main() {
  test(
    'remote Calendar source persists independently of the Day mirror',
    () async {
      final library = File('../../target/debug/libfloe_ffi.dylib').absolute;
      if (!library.existsSync()) {
        markTestSkipped('cargo build -p floe-ffi required');
        return;
      }
      final directory = await Directory.systemTemp.createTemp(
        'floe-remote-source-',
      );
      Future<TestAppHost> open() => TestAppHost.open(
        libraryPath: library.path,
        databasePath: '${directory.path}/calendar.db',
        deviceId: 'paired-device',
      );
      var host = await open();
      try {
        final source = await host.runtime.calendarSource.bindRemote(
          localPersonId,
          connectorId: 'calendar.google',
          connectionId: '8a1d7fb0-435d-5d1e-aab4-53ed2894da61',
          resources: const [
            CalendarSourceResource(
              handle: 'primary@example.test',
              label: 'Google Calendar',
            ),
          ],
        );
        expect(source.revision, 1);
        expect(source.provider, 'google_calendar');
        expect(source.selectedCalendarIds, ['primary@example.test']);
        final query = DayQuery.local(
          personId: localPersonId,
          date: DateTime(2026, 9, 3),
          now: DateTime(2026, 9, 3),
        );
        expect((await host.day.loadDay(query)).calendar, isNull);

        await host.close();
        host = await open();
        final restored = (await host.runtime.calendarSource.inspectRemote(
          localPersonId,
        )).single;
        expect(restored.connectionId, source.connectionId);
        expect(restored.sourceAuthority, source.sourceAuthority);
        expect(restored.revision, source.revision);
        expect((await host.day.loadDay(query)).calendar, isNull);

        final renamed = await host.runtime.calendarSource.bindRemote(
          localPersonId,
          connectorId: 'calendar.google',
          connectionId: restored.connectionId,
          current: restored,
          resources: const [
            CalendarSourceResource(
              handle: 'primary@example.test',
              label: 'Renamed Calendar',
            ),
          ],
        );
        expect(renamed.revision, restored.revision + 1);
        expect(renamed.sourceAuthority, restored.sourceAuthority);
        final disconnected = await host.runtime.calendarSource.disconnectRemote(
          localPersonId,
          current: renamed,
        );
        expect(disconnected.state, 'disconnected');
        expect(disconnected.sourceAuthority, isNot(renamed.sourceAuthority));
        expect(
          await host.runtime.calendarSource.inspectRemote(localPersonId),
          isEmpty,
        );
      } finally {
        await host.close();
        await directory.delete(recursive: true);
      }
    },
  );

  test('Rust/Turso gateway persists the complete task lifecycle', () async {
    final library = File('../../target/debug/libfloe_ffi.dylib').absolute;
    if (!library.existsSync()) {
      markTestSkipped('cargo build -p floe-ffi required');
      return;
    }
    final directory = await Directory.systemTemp.createTemp('floe-day-tasks-');
    final now = DateTime.utc(2026, 9, 3, 9);
    final query = DayQuery(
      personId: localPersonId,
      date: now,
      now: now,
      timezoneOffsetSeconds: 0,
    );
    Future<TestAppHost> open() => TestAppHost.open(
      libraryPath: library.path,
      databasePath: '${directory.path}/floe.db',
      clock: () => now,
      deviceId: 'test-device',
    );
    var host = await open();
    try {
      expect((await host.day.loadDay(query)).items, isEmpty);
      final capture = await host.day.submitCapture('Rust 연결 확인', query);
      var snapshot = await host.day.classifyCapture(
        capture,
        const TaskDraft(title: 'Rust 연결 확인'),
        query,
      );
      var task = snapshot.items.single as TaskItem;
      expect(task.isCompleted, isFalse);
      snapshot = await host.day.setTaskCompleted(task, true, query);
      task = snapshot.items.single as TaskItem;
      expect(task.isCompleted, isTrue);
      snapshot = await host.day.setTaskCompleted(task, false, query);
      task = snapshot.items.single as TaskItem;
      expect(task.isCompleted, isFalse);

      final eventCapture = await host.day.submitCapture('아침 점검', query);
      snapshot = await host.day.classifyCapture(
        eventCapture,
        EventDraft(
          title: '아침 점검',
          startsAt: now.subtract(const Duration(minutes: 30)),
          endsAt: now.add(const Duration(minutes: 30)),
        ),
        query,
      );
      expect(
        snapshot.nowEventId,
        snapshot.items.whereType<EventItem>().single.id,
      );
      final noteCapture = await host.day.submitCapture('연결 메모', query);
      snapshot = await host.day.classifyCapture(
        noteCapture,
        const NoteDraft(content: '연결 메모'),
        query,
      );
      expect(snapshot.items.whereType<NoteItem>().single.title, '연결 메모');

      await host.close();
      host = await open();
      snapshot = await host.day.loadDay(query);
      expect(snapshot.items, hasLength(3));
      for (final item in List<DayItem>.of(snapshot.items)) {
        snapshot = await host.day.deleteItem(item, query);
      }
      expect(snapshot.items, isEmpty);
      await host.close();
      host = await open();
      expect((await host.day.loadDay(query)).items, isEmpty);
    } finally {
      await host.close();
      await directory.delete(recursive: true);
    }
  });
}
