import 'package:flutter/material.dart';
import 'package:intl/intl.dart';
import 'package:floe_client/l10n/app_localizations.dart';

import '../domain/day_models.dart';

class CalendarDayAxis {
  CalendarDayAxis(DateTime date, int offsetSeconds) {
    final local = DateTime(date.year, date.month, date.day);
    usesLocalZone = local.timeZoneOffset.inSeconds == offsetSeconds;
    start = usesLocalZone
        ? local.toUtc()
        : DateTime.utc(
            date.year,
            date.month,
            date.day,
          ).subtract(Duration(seconds: offsetSeconds));
    end = usesLocalZone
        ? DateTime(date.year, date.month, date.day + 1).toUtc()
        : start.add(Duration(days: 1));
    offset = offsetSeconds;
  }

  late final DateTime start;
  late final DateTime end;
  late final bool usesLocalZone;
  late final int offset;
  double get minutes => end.difference(start).inSeconds / 60;
  double minute(DateTime value) =>
      value.toUtc().difference(start).inSeconds / 60;
  String time(DateTime value) {
    final local = usesLocalZone
        ? value.toLocal()
        : value.toUtc().add(Duration(seconds: offset));
    final clock =
        '${local.hour.toString().padLeft(2, '0')}:${local.minute.toString().padLeft(2, '0')}';
    return minutes == 1440 ? clock : '$clock ${local.timeZoneName}';
  }

  String hourLabel(int hour) =>
      hour * 60 == minutes ? '24:00' : time(start.add(Duration(hours: hour)));
}

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
  final axis = CalendarDayAxis(date, offsetSeconds);
  double minute(DateTime value) => axis.minute(value);
  final sorted =
      events
          .where(
            (event) =>
                !event.isAllDay &&
                minute(event.endsAt) > 0 &&
                minute(event.startsAt) < axis.minutes &&
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
    final start = minute(event.startsAt).clamp(0.0, axis.minutes);
    final end = minute(event.endsAt).clamp(0.0, axis.minutes);
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

String calendarRange(
  BuildContext context,
  EventItem event,
  int offset, {
  DateTime? date,
}) => event.isAllDay
    ? AppLocalizations.of(context).allDay
    : date == null
    ? '${calendarTime(event.startsAt, offset)} – ${calendarTime(event.endsAt, offset)}'
    : '${CalendarDayAxis(date, offset).time(event.startsAt)} – ${CalendarDayAxis(date, offset).time(event.endsAt)}';

String formatTimestamp(BuildContext context, DateTime value) =>
    DateFormat.yMMMd(AppLocalizations.of(context).localeName)
        .add_jm()
        .format(value.toLocal());
