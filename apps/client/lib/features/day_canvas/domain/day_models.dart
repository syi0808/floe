enum DayItemKind { event, task, note }

enum TaskPriority { low, normal, high }

final class DayQuery {
  const DayQuery({
    required this.personId,
    required this.date,
    required this.now,
    required this.timezoneOffsetSeconds,
    this.endTimezoneOffsetSeconds,
  });

  final String personId;
  final DateTime date;
  final DateTime now;
  final int timezoneOffsetSeconds;
  final int? endTimezoneOffsetSeconds;

  factory DayQuery.local({
    required String personId,
    required DateTime date,
    required DateTime now,
  }) {
    final start = DateTime(date.year, date.month, date.day);
    final end = DateTime(date.year, date.month, date.day + 1);
    return DayQuery(
      personId: personId,
      date: start,
      now: now,
      timezoneOffsetSeconds: start.timeZoneOffset.inSeconds,
      endTimezoneOffsetSeconds: end.timeZoneOffset.inSeconds,
    );
  }

  DateTime get startsAt => DateTime.utc(
    date.year,
    date.month,
    date.day,
  ).subtract(Duration(seconds: timezoneOffsetSeconds));
  DateTime get endsAt =>
      DateTime.utc(date.year, date.month, date.day + 1).subtract(
        Duration(seconds: endTimezoneOffsetSeconds ?? timezoneOffsetSeconds),
      );
}

sealed class DayItem {
  const DayItem({
    required this.id,
    required this.title,
    required this.revision,
    required this.createdAt,
  });

  final String id;
  final String title;
  final int revision;
  final DateTime createdAt;
  DayItemKind get kind;
}

final class EventItem extends DayItem {
  const EventItem({
    required super.id,
    required super.title,
    required super.revision,
    required super.createdAt,
    required this.startsAt,
    required this.endsAt,
    this.isAllDay = false,
    this.calendarName,
    this.externalId,
    this.provider,
    this.timezone,
  });

  final DateTime startsAt;
  final DateTime endsAt;
  final bool isAllDay;
  final String? calendarName;
  final String? externalId;
  final String? provider;
  final String? timezone;
  String get sourceLabel => calendarName == null
      ? ''
      : '${provider == 'fixture' ? 'Fixture' : 'Calendar'} · $calendarName';
  @override
  DayItemKind get kind => DayItemKind.event;
}

final class TaskItem extends DayItem {
  const TaskItem({
    required super.id,
    required super.title,
    required super.revision,
    required super.createdAt,
    this.deadline,
    this.completedAt,
    this.priority = TaskPriority.normal,
  });

  final DateTime? deadline;
  final DateTime? completedAt;
  final TaskPriority priority;
  bool get isCompleted => completedAt != null;
  @override
  DayItemKind get kind => DayItemKind.task;
}

final class NoteItem extends DayItem {
  const NoteItem({
    required super.id,
    required super.title,
    required super.revision,
    required super.createdAt,
  });

  @override
  DayItemKind get kind => DayItemKind.note;
}

final class DaySnapshot {
  const DaySnapshot({
    required this.personId,
    required this.date,
    required this.generatedAt,
    required this.timezoneOffsetSeconds,
    required this.items,
    this.nowEventId,
    this.nextEventId,
    this.overdueTaskCount = 0,
    this.calendar,
  });

  final String personId;
  final DateTime date;
  final DateTime generatedAt;
  final int timezoneOffsetSeconds;
  final List<DayItem> items;
  final String? nowEventId;
  final String? nextEventId;
  final int overdueTaskCount;
  final CalendarConnection? calendar;
}

final class ConnectedCalendar {
  const ConnectedCalendar({
    required this.id,
    required this.name,
    this.error,
    this.lastSuccessAt,
  });

  final String id;
  final String name;
  final String? error;
  final DateTime? lastSuccessAt;

  String? get account {
    final separator = name.indexOf(' · ');
    return separator > 0 ? name.substring(0, separator) : null;
  }

  String get title {
    final separator = name.indexOf(' · ');
    return separator > 0 ? name.substring(separator + 3) : name;
  }
}

final class CalendarConnection {
  const CalendarConnection({
    required this.id,
    required this.name,
    required this.provider,
    required this.revision,
    this.lastSuccessAt,
    this.error,
    this.rangeStart,
    this.rangeEnd,
    this.calendarIds = const [],
    this.calendars = const [],
    this.includeAll = false,
  });
  final String id;
  final String name;
  final String provider;
  final int revision;
  final DateTime? lastSuccessAt;
  final String? error;
  final String? rangeStart;
  final String? rangeEnd;
  final List<String> calendarIds;
  final List<ConnectedCalendar> calendars;
  final bool includeAll;
  List<ConnectedCalendar> get connectedCalendars =>
      calendars.isEmpty ? [ConnectedCalendar(id: id, name: name)] : calendars;
  List<String> get selectedCalendarIds => calendars.isNotEmpty
      ? calendars.map((calendar) => calendar.id).toList()
      : calendarIds.isEmpty
      ? [id]
      : calendarIds;
}

final class CaptureReceipt {
  const CaptureReceipt({
    required this.id,
    required this.originalInput,
    required this.capturedAt,
    required this.revision,
  });

  final String id;
  final String originalInput;
  final DateTime capturedAt;
  final int revision;
}

sealed class ClassificationDraft {
  const ClassificationDraft();
}

final class EventDraft extends ClassificationDraft {
  const EventDraft({
    required this.title,
    required this.startsAt,
    required this.endsAt,
  });
  final String title;
  final DateTime startsAt;
  final DateTime endsAt;
}

final class TaskDraft extends ClassificationDraft {
  const TaskDraft({required this.title, this.deadline});
  final String title;
  final DateTime? deadline;
}

final class NoteDraft extends ClassificationDraft {
  const NoteDraft({required this.content});
  final String content;
}
