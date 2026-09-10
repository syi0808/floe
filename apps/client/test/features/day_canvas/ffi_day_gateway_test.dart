import 'dart:io';

import 'package:floe_client/features/day_canvas/application/ffi_day_gateway.dart';
import 'package:floe_client/features/day_canvas/domain/day_models.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
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

    var gateway = await FfiDayGateway.open(
      libraryPath: library.path,
      databasePath: databasePath,
      clock: () => now,
    );
    expect((await gateway.loadDay(query)).items, isEmpty);

    final capture = await gateway.submitCapture('Rust 연결 확인', query);
    var snapshot = await gateway.classifyCapture(
      capture,
      const TaskDraft(title: 'Rust 연결 확인'),
      query,
    );
    var task = snapshot.items.single as TaskItem;
    expect(task.title, 'Rust 연결 확인');
    expect(task.isCompleted, isFalse);

    snapshot = await gateway.setTaskCompleted(task, true, query);
    task = snapshot.items.single as TaskItem;
    expect(task.isCompleted, isTrue);
    snapshot = await gateway.setTaskCompleted(task, false, query);
    task = snapshot.items.single as TaskItem;
    expect(task.isCompleted, isFalse);

    final eventCapture = await gateway.submitCapture('아침 점검', query);
    snapshot = await gateway.classifyCapture(
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

    final noteCapture = await gateway.submitCapture('연결 메모', query);
    snapshot = await gateway.classifyCapture(
      noteCapture,
      const NoteDraft(content: '연결 메모'),
      query,
    );
    expect(snapshot.items.whereType<NoteItem>().single.title, '연결 메모');
    await gateway.close();

    gateway = await FfiDayGateway.open(
      libraryPath: library.path,
      databasePath: databasePath,
      clock: () => now,
    );
    snapshot = await gateway.loadDay(query);
    expect(snapshot.items, hasLength(3));
    task = snapshot.items.whereType<TaskItem>().single;
    expect(task.title, 'Rust 연결 확인');
    for (final item in List<DayItem>.of(snapshot.items)) {
      snapshot = await gateway.deleteItem(item, query);
    }
    expect(snapshot.items, isEmpty);
    await gateway.close();

    gateway = await FfiDayGateway.open(
      libraryPath: library.path,
      databasePath: databasePath,
      clock: () => now,
    );
    expect((await gateway.loadDay(query)).items, isEmpty);
    await gateway.close();
    await temporaryDirectory.delete(recursive: true);
  });
}
