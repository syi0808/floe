import 'package:intl/intl.dart';
import 'package:floe_client/l10n/app_localizations.dart';
import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_squircle.dart';
import 'package:floe_client/features/day/domain/day_models.dart';
import 'package:floe_client/features/day/presentation/calendar_layout.dart';

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
              event.calendarLabel ?? AppLocalizations.of(context).savedInFloe,
              style: FloeType.body.copyWith(color: FloePalette.neutral600),
            ),
          ),
        ],
      ),
      SizedBox(height: FloeSpace.lg),
      FloeSquircle(
        size: FloeSquircleSize.md,
        fill: FloePalette.primary50,
        borderWidth: 0,
        padding: EdgeInsets.all(20),
        child: Row(
          children: [
            Icon(LucideIcons.clock, color: FloePalette.primary600, size: 20),
            SizedBox(width: FloeSpace.base),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    calendarRange(
                      context,
                      event,
                      snapshot.timezoneOffsetSeconds,
                      date: snapshot.date,
                    ),
                    style: FloeType.titleLarge.copyWith(
                      color: FloePalette.primary700,
                    ),
                  ),
                  SizedBox(height: 6),
                  Text(
                    DateFormat.yMMMd(AppLocalizations.of(context).localeName)
                        .format(snapshot.date),
                    style: FloeType.bodySmall.copyWith(
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
      SizedBox(height: FloeSpace.lg),
      for (final entry in <String, String>{
        if (event.timezone != null) 'Timezone': event.timezone!,
        if (event.isAllDay)
          AppLocalizations.of(context)
              .allDayBoundary: AppLocalizations.of(context).exclusiveDate(
            DateFormat.yMMMd(AppLocalizations.of(context).localeName)
                .format(event.endsAt),
          ),
      }.entries)
        Padding(
          padding: EdgeInsets.symmetric(vertical: 10),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Expanded(
                child: Text(
                  entry.key,
                  style: FloeType.bodySmall.copyWith(
                    fontSize: 12,
                    color: FloePalette.neutral600,
                  ),
                ),
              ),
              Expanded(
                child: Text(
                  entry.value,
                  style: FloeType.bodySmall.copyWith(fontSize: 12),
                ),
              ),
            ],
          ),
        ),
    ],
  );
}
