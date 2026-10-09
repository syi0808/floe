enum DayItemKind { event, task, note }

enum TaskPriority { low, normal, high }

enum ManualCalendarOperationStatus {
  pending,
  executing,
  blocked,
  notApplied,
  unknown,
  succeeded,
}

final class ManualCalendarDestination {
  const ManualCalendarDestination({
    required this.destinationRef,
    required this.label,
  });

  final String destinationRef;
  final String label;
}

final class ManualCalendarOperationReceipt {
  const ManualCalendarOperationReceipt({
    required this.operationRef,
    required this.revision,
    required this.status,
    required this.collectionPending,
  });

  factory ManualCalendarOperationReceipt.fromJson(Map<String, dynamic> json) {
    const expected = {
      'operation_ref',
      'revision',
      'status',
      'collection_pending',
    };
    if (json.keys.toSet().difference(expected).isNotEmpty ||
        !json.keys.toSet().containsAll(expected) ||
        json['operation_ref'] is! String ||
        json['revision'] is! int ||
        (json['revision'] as int) <= 0 ||
        json['collection_pending'] is! bool) {
      throw const FormatException('Invalid Calendar operation receipt.');
    }
    final status = switch (json['status']) {
      'pending' => ManualCalendarOperationStatus.pending,
      'executing' => ManualCalendarOperationStatus.executing,
      'blocked' => ManualCalendarOperationStatus.blocked,
      'not_applied' => ManualCalendarOperationStatus.notApplied,
      'unknown' => ManualCalendarOperationStatus.unknown,
      'succeeded' => ManualCalendarOperationStatus.succeeded,
      _ => throw const FormatException('Invalid Calendar operation status.'),
    };
    return ManualCalendarOperationReceipt(
      operationRef: json['operation_ref'] as String,
      revision: json['revision'] as int,
      status: status,
      collectionPending: json['collection_pending'] as bool,
    );
  }

  final String operationRef;
  final int revision;
  final ManualCalendarOperationStatus status;
  final bool collectionPending;
}

final class ManualCalendarOperationPage {
  const ManualCalendarOperationPage({
    required this.operations,
    required this.nextCursor,
  });

  final List<ManualCalendarOperationReceipt> operations;
  final String? nextCursor;
}

sealed class ManualCalendarOperationIntent {
  const ManualCalendarOperationIntent();
  Map<String, dynamic> toJson(String Function(DateTime) timestamp);
}

final class CreateManualCalendarEvent extends ManualCalendarOperationIntent {
  const CreateManualCalendarEvent({
    required this.destinationRef,
    required this.title,
    required this.startsAt,
    required this.endsAt,
    required this.timezone,
  });

  final String destinationRef;
  final String title;
  final DateTime startsAt;
  final DateTime endsAt;
  final String timezone;

  @override
  Map<String, dynamic> toJson(String Function(DateTime) timestamp) => {
    'kind': 'create',
    'destination_ref': destinationRef,
    'title': title,
    'schedule': {
      'starts_at': timestamp(startsAt.toUtc()),
      'ends_at': timestamp(endsAt.toUtc()),
      'timezone': timezone,
    },
  };
}

final class UpdateManualCalendarEvent extends ManualCalendarOperationIntent {
  const UpdateManualCalendarEvent({
    required this.eventRef,
    required this.expectedRevision,
    required this.title,
    required this.startsAt,
    required this.endsAt,
    required this.timezone,
  });

  final String eventRef;
  final int expectedRevision;
  final String title;
  final DateTime startsAt;
  final DateTime endsAt;
  final String timezone;

  @override
  Map<String, dynamic> toJson(String Function(DateTime) timestamp) => {
    'kind': 'update',
    'event_ref': eventRef,
    'expected_revision': expectedRevision,
    'title': title,
    'schedule': {
      'starts_at': timestamp(startsAt.toUtc()),
      'ends_at': timestamp(endsAt.toUtc()),
      'timezone': timezone,
    },
  };
}

final class DeleteManualCalendarEvent extends ManualCalendarOperationIntent {
  const DeleteManualCalendarEvent({
    required this.eventRef,
    required this.expectedRevision,
  });

  final String eventRef;
  final int expectedRevision;

  @override
  Map<String, dynamic> toJson(String Function(DateTime) timestamp) => {
    'kind': 'delete',
    'event_ref': eventRef,
    'expected_revision': expectedRevision,
  };
}

final class DayQuery {
  const DayQuery({
    required this.personId,
    required this.date,
    required this.now,
    required this.timezoneOffsetSeconds,
    this.endTimezoneOffsetSeconds,
  });

  /// The local expected owner identity. It is not part of the wire query.
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

sealed class DayItemSource {
  const DayItemSource();
}

final class ManualDayItemSource extends DayItemSource {
  const ManualDayItemSource();
}

final class CaptureDayItemSource extends DayItemSource {
  const CaptureDayItemSource({required this.captureId});

  final String captureId;
}

final class CalendarDayItemSource extends DayItemSource {
  const CalendarDayItemSource({
    required this.sourceRef,
    required this.calendarRef,
    required this.calendarLabel,
  });

  final String sourceRef;
  final String calendarRef;
  final String calendarLabel;
}

final class DayEventTarget {
  const DayEventTarget({required this.eventId, required this.expectedRevision});

  final String eventId;
  final int expectedRevision;
}

sealed class DayItem {
  const DayItem({
    required this.id,
    required this.title,
    required this.revision,
    required this.createdAt,
    this.source = const ManualDayItemSource(),
  });

  final String id;
  final String title;
  final int revision;
  final DateTime createdAt;
  final DayItemSource source;
  DayItemKind get kind;
}

final class EventItem extends DayItem {
  const EventItem({
    required super.id,
    required super.title,
    required super.revision,
    required super.createdAt,
    super.source,
    required this.startsAt,
    required this.endsAt,
    this.isAllDay = false,
    this.timezone,
    this.actionTarget,
  });

  final DateTime startsAt;
  final DateTime endsAt;
  final bool isAllDay;
  final String? timezone;
  final DayEventTarget? actionTarget;

  String? get calendarLabel => switch (source) {
    CalendarDayItemSource(:final calendarLabel) => calendarLabel,
    _ => null,
  };

  @override
  DayItemKind get kind => DayItemKind.event;
}

final class TaskItem extends DayItem {
  const TaskItem({
    required super.id,
    required super.title,
    required super.revision,
    required super.createdAt,
    super.source,
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
    super.source,
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
    this.calendarCoverage,
  });

  final String personId;
  final DateTime date;
  final DateTime generatedAt;
  final int timezoneOffsetSeconds;
  final List<DayItem> items;
  final String? nowEventId;
  final String? nextEventId;
  final int overdueTaskCount;
  final DayCalendarCoverage? calendarCoverage;
}

enum DayCoverageState { current, stale, partial, unavailable, pending }

enum DayCalendarFailure {
  permission_denied,
  calendar_unavailable,
  provider_unavailable,
  source_changed,
  source_fenced,
  vault_locked,
  budget_exceeded,
  deadline_exceeded,
  cancelled,
}

final class DayCalendarRange {
  const DayCalendarRange({
    required this.startDate,
    required this.endDateExclusive,
    required this.timezoneOffsetSeconds,
    this.endTimezoneOffsetSeconds,
  });

  final DateTime startDate;
  final DateTime endDateExclusive;
  final int timezoneOffsetSeconds;
  final int? endTimezoneOffsetSeconds;

  DateTime get startsAt => DateTime.utc(
    startDate.year,
    startDate.month,
    startDate.day,
  ).subtract(Duration(seconds: timezoneOffsetSeconds));

  DateTime get endsAt =>
      DateTime.utc(
        endDateExclusive.year,
        endDateExclusive.month,
        endDateExclusive.day,
      ).subtract(
        Duration(seconds: endTimezoneOffsetSeconds ?? timezoneOffsetSeconds),
      );

  /// Presentation-only coverage test against the selected Day query.
  bool covers(DayQuery query) {
    final queryDate = DateTime(
      query.date.year,
      query.date.month,
      query.date.day,
    );
    final queryEndDate = DateTime(
      query.date.year,
      query.date.month,
      query.date.day + 1,
    );
    final rangeStartsNoLater = !DateTime(
      startDate.year,
      startDate.month,
      startDate.day,
    ).isAfter(queryDate);
    final rangeEndsNoEarlier = !DateTime(
      endDateExclusive.year,
      endDateExclusive.month,
      endDateExclusive.day,
    ).isBefore(queryEndDate);
    return rangeStartsNoLater &&
        rangeEndsNoEarlier &&
        !startsAt.isAfter(query.startsAt) &&
        !endsAt.isBefore(query.endsAt);
  }
}

final class DayCalendarResourceCoverage {
  const DayCalendarResourceCoverage({
    required this.resourceRef,
    required this.label,
    required this.state,
    this.lastSuccessAt,
    this.lastRange,
    this.failure,
    this.failureAt,
  });

  final String resourceRef;
  final String label;
  final DayCoverageState state;
  final DateTime? lastSuccessAt;
  final DayCalendarRange? lastRange;
  final DayCalendarFailure? failure;
  final DateTime? failureAt;
}

final class DayCalendarSourceCoverage {
  const DayCalendarSourceCoverage({
    required this.sourceRef,
    required this.label,
    required this.state,
    required this.resources,
    this.lastSuccessAt,
    this.lastRange,
    this.failure,
    this.failureAt,
  });

  final String sourceRef;
  final String label;
  final DayCoverageState state;
  final DateTime? lastSuccessAt;
  final DayCalendarRange? lastRange;
  final DayCalendarFailure? failure;
  final DateTime? failureAt;
  final List<DayCalendarResourceCoverage> resources;
}

final class DayCalendarCoverage {
  const DayCalendarCoverage({required this.sources});

  final List<DayCalendarSourceCoverage> sources;
}

/// True only when the owner returned current complete coverage for this query.
/// This derives display wording from the immutable owner snapshot; it does not
/// cache or revise the returned coverage.
bool hasCompleteCalendarCoverage(DaySnapshot snapshot, DayQuery query) {
  final coverage = snapshot.calendarCoverage;
  if (coverage == null || coverage.sources.isEmpty) return false;
  for (final source in coverage.sources) {
    if (source.state != DayCoverageState.current ||
        source.lastRange?.covers(query) != true ||
        source.resources.isEmpty) {
      return false;
    }
    for (final resource in source.resources) {
      if (resource.state != DayCoverageState.current ||
          resource.failure != null ||
          resource.lastRange?.covers(query) != true) {
        return false;
      }
    }
  }
  return true;
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

enum DayRefreshFailure {
  source_changed,
  permission_denied,
  unavailable,
  vault_locked,
  budget_exceeded,
  deadline_exceeded,
  cancelled,
  host_interrupted,
  storage_unavailable,
  invalid_acquisition,
}

sealed class DayRefreshSnapshot {
  const DayRefreshSnapshot({
    required this.operationRef,
    required this.revision,
  });
  final String operationRef;
  final int revision;
  bool get terminal =>
      this is CompletedDayRefresh ||
      this is FailedDayRefresh ||
      this is InterruptedDayRefresh;
}

final class PendingDayRefresh extends DayRefreshSnapshot {
  const PendingDayRefresh({
    required super.operationRef,
    required super.revision,
  });
}

final class RunningDayRefresh extends DayRefreshSnapshot {
  const RunningDayRefresh({
    required super.operationRef,
    required super.revision,
  });
}

final class CompletedDayRefresh extends DayRefreshSnapshot {
  const CompletedDayRefresh({
    required super.operationRef,
    required super.revision,
    required this.day,
  });
  final DaySnapshot day;
}

final class FailedDayRefresh extends DayRefreshSnapshot {
  const FailedDayRefresh({
    required super.operationRef,
    required super.revision,
    required this.failure,
  });
  final DayRefreshFailure failure;
}

final class InterruptedDayRefresh extends DayRefreshSnapshot {
  const InterruptedDayRefresh({
    required super.operationRef,
    required super.revision,
    required this.failure,
  });
  final DayRefreshFailure failure;
}
