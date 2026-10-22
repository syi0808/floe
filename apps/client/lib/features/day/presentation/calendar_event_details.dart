import 'package:flutter/material.dart';
import 'package:intl/intl.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_button.dart';
import 'package:floe_client/app/floe_feedback.dart';
import 'package:floe_client/features/day/domain/day_models.dart';
import 'package:floe_client/features/day/presentation/calendar_layout.dart';
import 'package:floe_client/l10n/app_localizations.dart';

enum _EventDetailAction { edit, delete }

Future<void> openCalendarEvent(
  BuildContext context,
  EventItem event,
  DaySnapshot snapshot, {
  VoidCallback? onEdit,
  VoidCallback? onDelete,
}) async {
  final action = await showFloeDialog<_EventDetailAction>(
    context,
    (context) => CalendarEventDetails(
      event: event,
      snapshot: snapshot,
      canEdit: event.actionTarget != null && onEdit != null,
      canDelete: event.actionTarget != null && onDelete != null,
    ),
  );
  if (!context.mounted) return;
  switch (action) {
    case _EventDetailAction.edit:
      onEdit?.call();
    case _EventDetailAction.delete:
      onDelete?.call();
    case null:
      break;
  }
}

class CalendarEventDetails extends StatelessWidget {
  const CalendarEventDetails({
    super.key,
    required this.event,
    required this.snapshot,
    this.canEdit = false,
    this.canDelete = false,
  });
  final EventItem event;
  final DaySnapshot snapshot;
  final bool canEdit;
  final bool canDelete;

  @override
  Widget build(BuildContext context) {
    final copy = AppLocalizations.of(context);
    final dateFormat = DateFormat.yMMMEd(copy.localeName);
    final axis = CalendarDayAxis(
      snapshot.date,
      snapshot.timezoneOffsetSeconds,
    );
    DateTime displayDateTime(DateTime value) => axis.usesLocalZone
        ? value.toLocal()
        : value.toUtc().add(Duration(seconds: snapshot.timezoneOffsetSeconds));
    String offsetLabel(int seconds) {
      final minutes = seconds ~/ 60;
      final sign = minutes < 0 ? '−' : '+';
      return 'UTC$sign${(minutes.abs() ~/ 60).toString().padLeft(2, '0')}:${(minutes.abs() % 60).toString().padLeft(2, '0')}';
    }

    final start = event.isAllDay
        ? event.startsAt
        : displayDateTime(event.startsAt);
    // All-day values are date carriers. Only the display converts exclusive end
    // to the final included civil date; the event itself remains untouched.
    final end = event.isAllDay
        ? event.endsAt.subtract(const Duration(days: 1))
        : displayDateTime(event.endsAt);
    final sameDay = DateUtils.isSameDay(start, end);
    String time(DateTime value) =>
        MaterialLocalizations.of(context).formatTimeOfDay(
          TimeOfDay.fromDateTime(value),
          alwaysUse24HourFormat: MediaQuery.alwaysUse24HourFormatOf(context),
        );
    String displayedTime(DateTime value) {
      final clock = time(value);
      if (start.timeZoneOffset == end.timeZoneOffset) return clock;
      if (start.timeZoneName != end.timeZoneName) {
        return '$clock ${value.timeZoneName}';
      }
      return '$clock ${offsetLabel(value.timeZoneOffset.inSeconds)}';
    }

    final editable = canEdit && event.actionTarget != null;
    final deletable = canDelete && event.actionTarget != null;
    final calendar =
        event.calendarLabel ??
        (event.source is CalendarDayItemSource
            ? copy.eventConnectedCalendar
            : copy.savedInFloe);
    Widget endpoint(String label, DateTime value) => Padding(
      padding: const EdgeInsets.only(bottom: 12),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            label,
            style: FloeType.caption.copyWith(color: FloeColor.textSecondary),
          ),
          const SizedBox(height: 4),
          Text(
            '${dateFormat.format(value)} · ${displayedTime(value)}',
            style: FloeType.title,
          ),
        ],
      ),
    );
    return FloeDetailDialog(
      title: event.title,
      children: [
        Text(
          calendar,
          style: FloeType.bodySmall.copyWith(color: FloeColor.textSecondary),
        ),
        const SizedBox(height: 24),
        if (event.isAllDay) ...[
          Text(
            copy.allDay,
            style: FloeType.caption.copyWith(color: FloeColor.textSecondary),
          ),
          const SizedBox(height: 6),
          Text(
            sameDay
                ? dateFormat.format(start)
                : '${dateFormat.format(start)} – ${dateFormat.format(end)}',
            style: FloeType.titleLarge,
          ),
        ] else if (sameDay) ...[
          Text(dateFormat.format(start), style: FloeType.body),
          const SizedBox(height: 6),
          Text(
            '${displayedTime(start)} – ${displayedTime(end)}',
            style: FloeType.titleLarge,
          ),
        ] else ...[
          endpoint(copy.actionStart, start),
          endpoint(copy.actionEnd, end),
        ],
        if (!event.isAllDay) ...[
          const SizedBox(height: 8),
          Text(
            axis.usesLocalZone
                ? copy.eventDeviceTime
                : offsetLabel(snapshot.timezoneOffsetSeconds),
            style: FloeType.caption.copyWith(color: FloeColor.textSecondary),
          ),
        ],
        if (event.timezone != null && !event.isAllDay) ...[
          const SizedBox(height: 16),
          ExpansionTile(
            tilePadding: EdgeInsets.zero,
            childrenPadding: EdgeInsets.zero,
            shape: const Border(),
            collapsedShape: const Border(),
            title: Text(copy.eventCalendarDetails, style: FloeType.bodySmall),
            children: [
              Align(
                alignment: Alignment.centerLeft,
                child: Padding(
                  padding: const EdgeInsets.only(bottom: 12),
                  child: Text(
                    '${copy.eventSourceTimezone}: ${event.timezone}',
                    style: FloeType.bodySmall,
                  ),
                ),
              ),
            ],
          ),
        ],
        if (editable || deletable) ...[
          const SizedBox(height: 24),
          Wrap(
            spacing: 12,
            runSpacing: 8,
            children: [
              if (editable)
                FloeButton.outlined(
                  onPressed: () =>
                      Navigator.pop(context, _EventDetailAction.edit),
                  icon: const Icon(LucideIcons.pencil, size: 16),
                  child: Text(copy.eventEdit),
                ),
              if (deletable)
                FloeButton.text(
                  onPressed: () =>
                      Navigator.pop(context, _EventDetailAction.delete),
                  child: Text(copy.delete),
                ),
            ],
          ),
        ],
      ],
    );
  }
}
