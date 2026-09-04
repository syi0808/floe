import 'package:floe_client/features/day_canvas/domain/day_models.dart';
import 'package:floe_client/features/day_canvas/presentation/calendar_layout.dart';
import 'package:flutter_test/flutter_test.dart';

final date = DateTime.utc(2026, 9, 4);
EventItem event(String id, int start, int end) => EventItem(
  id: id,
  title: id,
  revision: 0,
  createdAt: date,
  startsAt: date.add(Duration(minutes: start)),
  endsAt: date.add(Duration(minutes: end)),
);

void main() {
  test(
    'five minute events retain exact duration and adjacent events share a lane',
    () {
      final result = layoutCalendarEvents(
        [event('short', 660, 665), event('next', 665, 720)],
        date,
        0,
      );
      expect(result.first.end - result.first.start, 5);
      expect(result.map((entry) => entry.columns), [1, 1]);
    },
  );
  test('overlap chains share columns and reuse the first free column', () {
    final result = layoutCalendarEvents(
      [
        event('first', 660, 720),
        event('second', 675, 735),
        event('third', 690, 705),
        event('fourth', 705, 725),
        event('last', 735, 780),
      ],
      date,
      0,
    );
    expect(result.map((entry) => entry.column), [0, 1, 2, 2, 0]);
    expect(result.map((entry) => entry.columns), [3, 3, 3, 3, 1]);
  });
  test('clips midnight crossings and excludes invalid/outside intervals', () {
    final result = layoutCalendarEvents(
      [
        event('before', -30, 30),
        event('after', 1420, 1500),
        event('invalid', 60, 60),
        event('outside', 1500, 1560),
      ],
      date,
      0,
    );
    expect(result.map((entry) => (entry.start, entry.end)), [
      (0, 30),
      (1420, 1440),
    ]);
  });
  test('positions UTC events using the selected display offset', () {
    final result = layoutCalendarEvents(
      [event('seoul', -540, -480)],
      date,
      32400,
    );
    expect(result.single.start, 0);
    expect(result.single.end, 60);
  });
}
