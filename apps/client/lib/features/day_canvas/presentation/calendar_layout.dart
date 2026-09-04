import 'package:flutter/material.dart';
import 'package:intl/intl.dart';
import 'package:floe_client/l10n/app_localizations.dart';

import '../domain/day_models.dart';

class CalendarPlacement {
  CalendarPlacement(this.event, this.start, this.end, this.column);
  final EventItem event;
  final double start;
  final double end;
  final int column;
  int columns = 1;
}

List<CalendarPlacement> layoutCalendarEvents(
  List<EventItem> events,
  DateTime date,
  int offsetSeconds,
) {
  final midnight = DateTime.utc(date.year, date.month, date.day);
  double minute(DateTime value) =>
      value
          .toUtc()
          .add(Duration(seconds: offsetSeconds))
          .difference(midnight)
          .inSeconds /
      60;
  final sorted =
      events
          .where(
            (event) =>
                !event.isAllDay &&
                minute(event.endsAt) > 0 &&
                minute(event.startsAt) < 1440 &&
                event.endsAt.isAfter(event.startsAt),
          )
          .toList()
        ..sort((first, second) {
          final start = first.startsAt.compareTo(second.startsAt);
          if (start != 0) return start;
          final end = second.endsAt.compareTo(first.endsAt);
          return end != 0 ? end : first.id.compareTo(second.id);
        });
  final result = <CalendarPlacement>[];
  final group = <CalendarPlacement>[];
  final columnEnds = <double>[];
  double groupEnd = -1;
  void finishGroup() {
    for (final placement in group) {
      placement.columns = columnEnds.length;
    }
    group.clear();
    columnEnds.clear();
  }

  for (final event in sorted) {
    final start = minute(event.startsAt).clamp(0.0, 1440.0);
    final end = minute(event.endsAt).clamp(0.0, 1440.0);
    if (start >= groupEnd) finishGroup();
    var column = columnEnds.indexWhere((end) => end <= start);
    if (column == -1) {
      column = columnEnds.length;
      columnEnds.add(end);
    } else {
      columnEnds[column] = end;
    }
    final placement = CalendarPlacement(event, start, end, column);
    result.add(placement);
    group.add(placement);
    if (end > groupEnd) groupEnd = end;
  }
  finishGroup();
  return result;
}

String calendarTime(DateTime value, int offset) {
  final time = value.toUtc().add(Duration(seconds: offset));
  return '${time.hour.toString().padLeft(2, '0')}:${time.minute.toString().padLeft(2, '0')}';
}

String calendarRange(BuildContext context, EventItem event, int offset) =>
    event.isAllDay
    ? AppLocalizations.of(context).allDay
    : '${calendarTime(event.startsAt, offset)} – ${calendarTime(event.endsAt, offset)}';

String formatTimestamp(BuildContext context, DateTime value) =>
    DateFormat.yMMMd(AppLocalizations.of(context).localeName)
        .add_jm()
        .format(value.toLocal());
