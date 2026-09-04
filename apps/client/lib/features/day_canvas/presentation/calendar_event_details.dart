import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../../app/design_tokens.dart';
import '../../../app/floe_feedback.dart';
import '../../../app/floe_squircle.dart';
import '../domain/day_models.dart';
import 'calendar_layout.dart';

Future<void> openCalendarEvent(
  BuildContext context,
  EventItem event,
  DaySnapshot snapshot,
) => showFloeDialog<void>(
  context,
  (context) => CalendarEventDetails(event: event, snapshot: snapshot),
);

class CalendarEventDetails extends StatelessWidget {
  const CalendarEventDetails({
    super.key,
    required this.event,
    required this.snapshot,
  });
  final EventItem event;
  final DaySnapshot snapshot;
  @override
  Widget build(BuildContext context) => FloeDetailDialog(
    title: event.title,
    children: [
      Row(
        children: [
          Expanded(
            child: Text(
              event.calendarName ?? 'Saved in Floe',
              style: const TextStyle(color: FloePalette.neutral600),
            ),
          ),
          if (event.externalId != null) const FloeReadOnlyPill(),
        ],
      ),
      const SizedBox(height: 24),
      FloeSquircle(
        size: FloeSquircleSize.field,
        fill: FloePalette.primary50,
        borderWidth: 0,
        padding: const EdgeInsets.all(20),
        child: Row(
          children: [
            const Icon(
              LucideIcons.clock,
              color: FloePalette.primary600,
              size: 20,
            ),
            const SizedBox(width: 16),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    calendarRange(event, snapshot.timezoneOffsetSeconds),
                    style: const TextStyle(
                      fontSize: 18,
                      fontWeight: FontWeight.w600,
                      color: FloePalette.primary700,
                    ),
                  ),
                  const SizedBox(height: 6),
                  Text(
                    '${snapshot.date.month}/${snapshot.date.day} · ${event.timezone ?? 'Local time'}',
                    style: const TextStyle(
                      fontSize: 12,
                      color: FloePalette.primary700,
                    ),
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
      const SizedBox(height: 24),
      for (final entry in <String, String>{
        'Source time zone': event.timezone ?? 'Local time',
        if (snapshot.calendar?.lastSuccessAt != null)
          'Last collected': snapshot.calendar!.lastSuccessAt!
              .toLocal()
              .toString(),
        if (event.isAllDay)
          'All-day boundary':
              '${event.endsAt.toIso8601String().split('T').first} · exclusive',
      }.entries)
        Padding(
          padding: const EdgeInsets.symmetric(vertical: 10),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Expanded(
                child: Text(
                  entry.key,
                  style: const TextStyle(
                    fontSize: 12,
                    color: FloePalette.neutral600,
                  ),
                ),
              ),
              Expanded(
                child: Text(entry.value, style: const TextStyle(fontSize: 12)),
              ),
            ],
          ),
        ),
      if (event.externalId != null) ...[
        const SizedBox(height: 24),
        const FloeInfoNote(
          icon: LucideIcons.lockKeyhole,
          text: 'Manage this event in its original calendar. Floe has no edit or delete action for imported events.',
        ),
        const SizedBox(height: 16),
        Theme(
          data: Theme.of(context).copyWith(dividerColor: Colors.transparent),
          child: ExpansionTile(
            tilePadding: EdgeInsets.zero,
            title: const Text('Source details', style: TextStyle(fontSize: 12)),
            children: [
              FloeSquircle(
                size: FloeSquircleSize.field,
                fill: FloePalette.neutral50,
                borderWidth: 0,
                padding: const EdgeInsets.all(16),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    for (final entry in <String, String>{
                      'Connection / Person':
                          '${snapshot.calendar?.id ?? '—'} / ${snapshot.personId}',
                      'External occurrence ID': event.externalId!,
                      'Revision': '${event.revision}',
                      'Integration': event.provider ?? 'Calendar',
                    }.entries)
                      Padding(
                        padding: EdgeInsets.only(
                          top: entry.key == 'Connection / Person' ? 0 : 12,
                        ),
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(
                              entry.key,
                              style: const TextStyle(
                                fontSize: 11,
                                color: FloePalette.neutral600,
                              ),
                            ),
                            const SizedBox(height: 5),
                            Text(
                              entry.value,
                              style: const TextStyle(
                                fontSize: 10,
                                fontFamily: 'monospace',
                              ),
                            ),
                          ],
                        ),
                      ),
                  ],
                ),
              ),
            ],
          ),
        ),
      ],
      const SizedBox(height: 24),
      Align(
        alignment: Alignment.centerRight,
        child: FilledButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('Back to my day'),
        ),
      ),
    ],
  );
}
