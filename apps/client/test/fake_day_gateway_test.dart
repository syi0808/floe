import 'package:floe_client/features/day_canvas/application/fake_day_gateway.dart';
import 'package:floe_client/features/day_canvas/domain/day_models.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('capture classification replaces the batched snapshot', () async {
    final now = DateTime.utc(2026, 9, 2, 9);
    final query = DayQuery(
      personId: 'person-1',
      date: now,
      now: now,
      timezoneOffsetSeconds: 0,
    );
    final gateway = FakeDayGateway();

    final capture = await gateway.submitCapture('  Buy milk  ', query);
    final snapshot = await gateway.classifyCapture(
      capture,
      const TaskDraft(title: 'Buy milk'),
      query,
    );

    expect(capture.originalInput, 'Buy milk');
    expect(snapshot.items, hasLength(1));
    expect(snapshot.items.single, isA<TaskItem>());
    expect(snapshot.items.single.title, 'Buy milk');
  });

  test('empty capture is rejected before classification', () async {
    final now = DateTime.utc(2026, 9, 2, 9);
    final gateway = FakeDayGateway();
    final query = DayQuery(
      personId: 'person-1',
      date: now,
      now: now,
      timezoneOffsetSeconds: 0,
    );

    expect(gateway.submitCapture('  ', query), throwsFormatException);
  });
}
