import 'package:flutter/material.dart';

import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_button.dart';
import 'package:floe_client/app/floe_loading.dart';
import 'package:floe_client/features/day/domain/day_models.dart';
import 'package:floe_client/features/day/presentation/calendar_event_actions.dart';
import 'package:floe_client/features/day/presentation/calendar_event_details.dart';
import 'package:floe_client/features/day/presentation/calendar_layout.dart';
import 'package:floe_client/l10n/app_localizations.dart';

/// A view projection of the selected calendar day, not a claim of availability.
final class DayScheduleProjection {
  DayScheduleProjection(
    DaySnapshot snapshot,
    this.query, {
    bool observationCurrent = true,
  }) {
    // All-day UTC midnights carry civil dates, not instants. The owner has
    // already selected their membership in this day's snapshot.
    events =
        snapshot.items
            .whereType<EventItem>()
            .where(
              (event) =>
                  event.endsAt.isAfter(event.startsAt) &&
                  (event.isAllDay ||
                      event.startsAt.isBefore(query.endsAt) &&
                          event.endsAt.isAfter(query.startsAt)),
            )
            .toList()
          ..sort((a, b) {
            final start = a.startsAt.compareTo(b.startsAt);
            return start != 0 ? start : a.id.compareTo(b.id);
          });
    allDay = events.where((event) => event.isAllDay).toList();
    timed = events.where((event) => !event.isAllDay).toList();
    final at = snapshot.generatedAt;
    isToday =
        observationCurrent &&
        !at.isBefore(query.startsAt) &&
        at.isBefore(query.endsAt) &&
        !query.now.isBefore(query.startsAt) &&
        query.now.isBefore(query.endsAt);
    // The owner chooses Now and Next. Other overlapping/tied events remain
    // visible in Schedule rather than being silently hidden or reprioritized.
    current = isToday
        ? timed
              .where(
                (event) =>
                    event.id == snapshot.nowEventId &&
                    !event.startsAt.isAfter(at) &&
                    event.endsAt.isAfter(at),
              )
              .toList()
        : [];
    next = isToday
        ? timed
              .where(
                (event) =>
                    event.id == snapshot.nextEventId &&
                    event.startsAt.isAfter(at),
              )
              .toList()
        : [];
    final focused = {
      ...current.map((event) => event.id),
      ...next.map((event) => event.id),
    };
    remaining = isToday
        ? timed
              .where(
                (event) =>
                    !focused.contains(event.id) && event.endsAt.isAfter(at),
              )
              .toList()
        : [];
    earlier = isToday
        ? timed.where((event) => !event.endsAt.isAfter(at)).toList()
        : [];
  }
  final DayQuery query;
  late final List<EventItem> events,
      allDay,
      timed,
      current,
      next,
      remaining,
      earlier;
  late final bool isToday;
}

/// Narrow screens use one readable scroll surface instead of a miniature grid.
class DayOverview extends StatelessWidget {
  const DayOverview({
    super.key,
    required this.snapshot,
    required this.query,
    this.loading = false,
    this.observationCurrent = true,
    this.focusOnly = false,
    this.onEdit,
    this.onDelete,
  });
  final DaySnapshot snapshot;
  final DayQuery query;
  final bool loading;
  final bool observationCurrent;
  final bool focusOnly;
  final ValueChanged<EventItem>? onEdit;
  final ValueChanged<EventItem>? onDelete;

  @override
  Widget build(BuildContext context) {
    final schedule = DayScheduleProjection(
      snapshot,
      query,
      observationCurrent: observationCurrent,
    );
    final copy = AppLocalizations.of(context);
    if (focusOnly && (!schedule.isToday || loading)) {
      return const SizedBox.shrink();
    }
    Widget group(String label, List<EventItem> items, {bool current = false}) =>
        items.isEmpty
        ? const SizedBox.shrink()
        : Padding(
            padding: const EdgeInsets.only(bottom: 20),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                if (label.isNotEmpty)
                  Text(
                    label,
                    style: FloeType.title.copyWith(
                      color: current
                          ? FloePalette.primary700
                          : FloeColor.textSecondary,
                    ),
                  ),
                if (label.isNotEmpty) const SizedBox(height: 8),
                for (final event in items)
                  _ScheduleEventRow(
                    event: event,
                    snapshot: snapshot,
                    query: query,
                    current: current,
                    onEdit: onEdit,
                    onDelete: onDelete,
                  ),
              ],
            ),
          );
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (loading)
          const Padding(
            padding: EdgeInsets.symmetric(vertical: 16),
            child: Center(child: FloeSpinner()),
          ),
        if (schedule.isToday && !loading) ...[
          group(copy.dayNow, schedule.current, current: true),
          group(copy.dayNext, schedule.next),
        ],
        if (!focusOnly) ...[
          group(copy.allDay, schedule.allDay),
          group(
            schedule.isToday &&
                    (schedule.current.isNotEmpty || schedule.next.isNotEmpty)
                ? copy.dayOtherEvents
                : '',
            schedule.isToday && !loading ? schedule.remaining : schedule.timed,
          ),
          if (schedule.isToday && !loading && schedule.earlier.isNotEmpty)
            ExpansionTile(
              key: PageStorageKey('earlier-${query.date.toIso8601String()}'),
              tilePadding: EdgeInsets.zero,
              childrenPadding: EdgeInsets.zero,
              shape: const Border(),
              collapsedShape: const Border(),
              title: Text(
                copy.dayEarlierCount(schedule.earlier.length),
                style: FloeType.bodySmall,
              ),
              children: [
                for (final event in schedule.earlier)
                  _ScheduleEventRow(
                    event: event,
                    snapshot: snapshot,
                    query: query,
                    onEdit: onEdit,
                    onDelete: onDelete,
                  ),
              ],
            ),
          if (!loading && schedule.events.isEmpty)
            Padding(
              padding: const EdgeInsets.symmetric(vertical: 24),
              child: Text(
                observationCurrent &&
                        hasCompleteCalendarCoverage(snapshot, query)
                    ? copy.dayNoEvents
                    : copy.dayNoEventsLoaded,
                style: FloeType.body.copyWith(color: FloeColor.textSecondary),
              ),
            ),
        ],
      ],
    );
  }
}

class _ScheduleEventRow extends StatelessWidget {
  const _ScheduleEventRow({
    required this.event,
    required this.snapshot,
    required this.query,
    this.current = false,
    this.onEdit,
    this.onDelete,
  });
  final EventItem event;
  final DaySnapshot snapshot;
  final DayQuery query;
  final bool current;
  final ValueChanged<EventItem>? onEdit;
  final ValueChanged<EventItem>? onDelete;

  @override
  Widget build(BuildContext context) {
    final copy = AppLocalizations.of(context);
    final axis = CalendarDayAxis(
      query.date,
      query.timezoneOffsetSeconds,
      endOffsetSeconds: query.endTimezoneOffsetSeconds,
    );
    final time = event.isAllDay
        ? copy.allDay
        : '${axis.time(event.startsAt)} – ${axis.time(event.endsAt)}';
    final writable = event.actionTarget != null;
    return Padding(
      padding: const EdgeInsets.only(bottom: 4),
      child: CalendarEventActions(
        event: event,
        snapshot: snapshot,
        onEdit: writable && onEdit != null ? () => onEdit!(event) : null,
        onDelete: writable && onDelete != null ? () => onDelete!(event) : null,
        child: FloeButton.text(
          onPressed: () => openCalendarEvent(context, event, snapshot),
          style: ButtonStyle(
            alignment: Alignment.centerLeft,
            backgroundColor: WidgetStateProperty.resolveWith(
              (states) => states.contains(WidgetState.hovered)
                  ? FloePalette.neutral100
                  : current
                  ? FloePalette.primary50
                  : Colors.transparent,
            ),
            padding: const WidgetStatePropertyAll(
              EdgeInsets.fromLTRB(12, 12, 40, 12),
            ),
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                time,
                style: FloeType.caption.copyWith(
                  color: FloeColor.textSecondary,
                ),
              ),
              const SizedBox(height: 4),
              Text(
                event.title,
                style: FloeType.title.copyWith(color: FloeColor.textPrimary),
              ),
              if (event.calendarLabel case final label?) ...[
                const SizedBox(height: 4),
                Text(
                  label,
                  style: FloeType.caption.copyWith(
                    color: FloeColor.textSecondary,
                  ),
                ),
              ],
            ],
          ),
        ),
      ),
    );
  }
}
