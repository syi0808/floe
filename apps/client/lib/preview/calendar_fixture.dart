import '../features/day_canvas/application/fake_day_gateway.dart';
import '../features/day_canvas/domain/day_models.dart';

final calendarPreviewDate = DateTime.utc(2026, 9, 4);
final calendarPreviewQuery = DayQuery(
  personId: 'visual-preview',
  date: calendarPreviewDate,
  now: DateTime.utc(2026, 9, 4, 14, 28),
  timezoneOffsetSeconds: 0,
);

FakeDayGateway calendarPreviewGateway() => FakeDayGateway(
  initialItems: [
    for (final entry in [
      ('making', 'A day for making', 'Personal'),
      ('research', 'Research week', 'Product team'),
    ])
      EventItem(
        id: entry.$1,
        title: entry.$2,
        revision: 1,
        createdAt: calendarPreviewDate,
        startsAt: calendarPreviewDate,
        endsAt: calendarPreviewDate.add(const Duration(days: 1)),
        isAllDay: true,
        calendarName: entry.$3,
        externalId: 'fixture:${entry.$1}',
        provider: 'fixture',
        timezone: 'UTC',
      ),
    for (final entry in [
      ('standup', 'A little alignment', 570, 600, 'Work'),
      ('review', 'Make room for the details', 660, 720, 'Work'),
      ('planning', 'Plan the next step', 675, 735, 'Product team'),
      ('personal-call', 'A quick personal call', 690, 705, 'Personal'),
      ('reset', 'Take a breath', 750, 755, 'Personal'),
      ('zones', 'Across time zones', 960, 1005, 'Product team'),
    ])
      EventItem(
        id: entry.$1,
        title: entry.$2,
        revision: 1,
        createdAt: calendarPreviewDate,
        startsAt: calendarPreviewDate.add(Duration(minutes: entry.$3)),
        endsAt: calendarPreviewDate.add(Duration(minutes: entry.$4)),
        calendarName: entry.$5,
        externalId: 'fixture:${entry.$1}',
        provider: 'fixture',
        timezone: 'UTC',
      ),
    TaskItem(
      id: 'feedback',
      title: 'Prepare launch brief',
      revision: 1,
      createdAt: calendarPreviewDate,
      deadline: calendarPreviewDate,
    ),
    NoteItem(
      id: 'launch-plan',
      title: 'Launch plan',
      revision: 1,
      createdAt: calendarPreviewDate,
    ),
  ],
);
