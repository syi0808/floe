import 'package:floe_client/features/day_canvas/application/fake_day_gateway.dart';
import 'package:floe_client/features/day_canvas/domain/day_models.dart';
import 'package:floe_client/features/day_canvas/presentation/day_appearance.dart';
import 'package:flutter/widgets.dart';

Widget prototypeAppearance(Widget child) => DayAppearance(
  tones: const {
    'focus': ItemTone.violet,
    'review': ItemTone.blue,
    'break': ItemTone.violet,
    'call': ItemTone.coral,
    'launch': ItemTone.blue,
    'retro': ItemTone.amber,
    'planning': ItemTone.violet,
    'feedback': ItemTone.violet,
    'email': ItemTone.blue,
    'deck': ItemTone.amber,
    'gift': ItemTone.mint,
  },
  dailyNote: 'Focus on finalizing the launch plan and getting feedback on the deck. Keep the afternoon light for deep work and planning.',
  notes: const {
    'launch-plan': NoteAppearance(
      category: 'Planning',
      excerpt: 'Finalizing timelines and aligning on priorities for a smooth launch next week.',
      timestamp: 'Today, 2:28 PM',
    ),
    'focus-choice': NoteAppearance(
      category: 'Ideas',
      excerpt: 'Protect your focus like it’s your most valuable asset. It shapes everything.',
      timestamp: 'Today, 9:12 AM',
      tone: ItemTone.blue,
    ),
    'design-feedback': NoteAppearance(
      category: 'Work',
      excerpt: 'Loved the direction. Small tweaks to spacing and hierarchy will make a big impact.',
      timestamp: 'Yesterday, 4:40 PM',
      tone: ItemTone.amber,
    ),
    'gratitude': NoteAppearance(
      category: 'Personal',
      excerpt: 'Grateful for the small wins, meaningful conversations, and moments of calm.',
      timestamp: 'Yesterday, 9:07 PM',
      tone: ItemTone.mint,
    ),
    'deep-work': NoteAppearance(
      category: 'Focus',
      excerpt: 'Morning is my best window for deep work. No meetings, no distractions.',
      timestamp: 'Yesterday, 8:30 AM',
    ),
    'design-systems': NoteAppearance(
      category: 'Learning',
      excerpt: 'Consistency creates clarity. Systems scale ideas into beautiful experiences.',
      timestamp: 'Sep 2, 6:15 PM',
    ),
    'customer-delight': NoteAppearance(
      category: 'Ideas',
      excerpt: 'The little details are the big difference. Make the experience feel effortless.',
      timestamp: 'Sep 2, 11:20 AM',
      tone: ItemTone.blue,
    ),
    'team-retro': NoteAppearance(
      category: 'Work',
      excerpt:
          'Celebrate wins, learn openly, and keep raising the bar together.',
      timestamp: 'Sep 1, 3:45 PM',
      tone: ItemTone.amber,
    ),
    'evening-reset': NoteAppearance(
      category: 'Personal',
      excerpt: 'Slow down, reflect, and prepare for a better tomorrow.',
      timestamp: 'Sep 1, 9:30 PM',
      tone: ItemTone.mint,
    ),
  },
  tasks: const {
    'feedback': TaskAppearance(
      description: 'Create a short launch brief for the product launch. Include key messaging, timeline, and audience. Share with the team for review.',
      project: 'Product launch',
      estimate: '1h 15m',
      timeContext: 'Before 3:30 PM · Team retro',
      suggestion:
          'Team retro starts at 3:30 PM. Review the launch brief first?',
      note: 'Focus on clarity and alignment with our Q3 goals. Keep it short and actionable.',
      subtasks: [
        (title: 'Draft key messages', duration: '30m', done: true),
        (title: 'Outline timeline', duration: '20m', done: false),
        (title: 'Identify audience', duration: '15m', done: false),
        (title: 'Share draft with team', duration: '10m', done: false),
      ],
    ),
  },
  child: child,
);

final prototypeNow = DateTime.utc(2026, 9, 3, 14, 28);
final prototypeQuery = DayQuery(
  personId: 'visual-preview',
  date: prototypeNow,
  now: prototypeNow,
  timezoneOffsetSeconds: 0,
);

FakeDayGateway prototypeGateway({bool detail = false}) => FakeDayGateway(
  initialItems: [
    EventItem(
      id: 'all-day',
      title: 'Product launch',
      revision: 0,
      createdAt: prototypeNow,
      startsAt: DateTime.utc(2026, 9, 3),
      endsAt: DateTime.utc(2026, 9, 4),
      isAllDay: true,
    ),
    for (final (id, title, start, duration) in [
      ('focus', 'Focus time', 0, 90),
      ('review', 'Design review', 90, 60),
      ('break', 'Break', 180, 15),
      ('call', 'Customer call', 240, 60),
      ('launch', 'Launch plan', 330, 90),
      ('retro', 'Team retro', 450, 45),
      ('planning', 'Weekly planning', 540, 45),
    ])
      EventItem(
        id: id,
        title: title,
        revision: 0,
        createdAt: prototypeNow,
        startsAt: DateTime.utc(2026, 9, 3, 8).add(Duration(minutes: start)),
        endsAt: DateTime.utc(
          2026,
          9,
          3,
          8,
        ).add(Duration(minutes: start + duration)),
      ),
    for (final (id, title) in [
      ('feedback', 'Design feedback'),
      ('email', 'Email Miguel'),
      ('deck', 'Prepare deck'),
      ('gift', 'Buy gift'),
    ])
      TaskItem(
        id: id,
        title: detail && id == 'feedback' ? 'Prepare launch brief' : title,
        revision: 0,
        createdAt: prototypeNow,
        deadline: id == 'gift' ? null : DateTime.utc(2026, 9, 3, 23, 59),
      ),
    for (final (id, title) in [
      ('launch-plan', 'Launch plan'),
      ('focus-choice', 'Focus is a choice'),
      ('design-feedback', 'Design feedback'),
      ('gratitude', 'Gratitude'),
      ('deep-work', 'Deep work block'),
      ('design-systems', 'Design systems'),
      ('customer-delight', 'Customer delight'),
      ('team-retro', 'Team retro'),
      ('evening-reset', 'Evening reset'),
    ])
      NoteItem(id: id, title: title, revision: 0, createdAt: prototypeNow),
  ],
);
