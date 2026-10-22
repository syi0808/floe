import 'package:floe_client/app/floe_primitives.dart';
import 'package:floe_client/features/day/presentation/calendar_layout.dart';
import 'package:intl/intl.dart';
import 'package:floe_client/l10n/app_localizations.dart';
import 'package:flutter/material.dart';

import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_selection.dart';
import 'package:floe_client/app/floe_button.dart';
import 'package:floe_client/features/day/domain/day_models.dart';

class CalendarContextRail extends StatelessWidget {
  const CalendarContextRail({
    super.key,
    required this.snapshot,
    required this.query,
    required this.disabled,
    required this.complete,
    required this.onTasks,
    required this.onOpenTask,
    this.showCoverage = true,
    this.observationCurrent = true,
  });
  final bool showCoverage;
  final bool observationCurrent;
  final DaySnapshot snapshot;
  final DayQuery query;
  final bool disabled;
  final Future<void> Function(TaskItem, bool) complete;
  final VoidCallback onTasks;
  final ValueChanged<TaskItem> onOpenTask;
  @override
  Widget build(BuildContext context) {
    final tasks = snapshot.items
        .whereType<TaskItem>()
        .where((task) => !task.isCompleted)
        .toList();
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 8),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              Expanded(
                child: Text(
                  AppLocalizations.of(context).tasks,
                  style: FloeType.title,
                ),
              ),
              FloeButton.text(
                onPressed: onTasks,
                child: Text(AppLocalizations.of(context).viewAll),
              ),
            ],
          ),
          if (tasks.isEmpty)
            Padding(
              padding: const EdgeInsets.symmetric(vertical: 12),
              child: Text(
                AppLocalizations.of(context).noTasksForSelectedDay,
                style: FloeType.bodySmall.copyWith(
                  color: FloeColor.textSecondary,
                ),
              ),
            ),
          for (final task in tasks.take(3))
            Row(
              children: [
                FloeCheckbox(
                  semanticLabel: task.title,
                  value: task.isCompleted,
                  onChanged: disabled
                      ? null
                      : (value) => complete(task, value!),
                ),
                Expanded(
                  child: FloeButton.text(
                    onPressed: () => onOpenTask(task),
                    style: const ButtonStyle(alignment: Alignment.centerLeft),
                    child: Text(task.title, style: FloeType.bodySmall),
                  ),
                ),
              ],
            ),
          if (showCoverage) ...[
            const SizedBox(height: FloeSpace.lg),
            CalendarCoverageDetails(
              snapshot: snapshot,
              query: query,
              observationCurrent: observationCurrent,
              hideEmptyStatus: true,
            ),
          ],
        ],
      ),
    );
  }
}

class CalendarCoverageDetails extends StatelessWidget {
  const CalendarCoverageDetails({
    super.key,
    required this.snapshot,
    required this.query,
    this.observationCurrent = true,
    this.hideEmptyStatus = false,
  });

  final bool observationCurrent;
  final bool hideEmptyStatus;
  final DaySnapshot snapshot;
  final DayQuery query;

  @override
  Widget build(BuildContext context) {
    final coverage = snapshot.calendarCoverage;
    final complete =
        observationCurrent && hasCompleteCalendarCoverage(snapshot, query);
    final hasEvents = snapshot.items.any((item) => item is EventItem);
    final title = !hasEvents && hideEmptyStatus
        ? AppLocalizations.of(context).eventCalendarDetails
        : !observationCurrent
        ? AppLocalizations.of(context).dayStatusNeedsRefresh
        : complete
        ? 'Calendars synced for this date'
        : coverage == null || coverage.sources.isEmpty
        ? AppLocalizations.of(context).dayCalendarStatusUnavailable
        : 'Some calendar data may be missing or out of date';
    final showCoverageStatus = hasEvents || !hideEmptyStatus;
    return ExpansionTile(
      tilePadding: const EdgeInsets.symmetric(horizontal: FloeSpace.md),
      childrenPadding: const EdgeInsets.all(FloeSpace.md),
      shape: const Border(),
      collapsedShape: const Border(),
      title: Text(
        title,
        style: FloeType.bodySmall.copyWith(
          color: !showCoverageStatus || complete
              ? FloePalette.neutral600
              : FloePalette.warning600,
        ),
      ),
      children: [
        Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            if (coverage == null)
              Text(
                'No Calendar coverage was returned for this date.',
                style: FloeType.bodySmall.copyWith(
                  color: FloePalette.neutral600,
                ),
              )
            else if (coverage.sources.isEmpty)
              Text(
                'No Calendar sources were returned for this date.',
                style: FloeType.bodySmall.copyWith(
                  color: FloePalette.neutral600,
                ),
              )
            else
              for (final (index, source) in coverage.sources.indexed) ...[
                _CoverageEntry(
                  kind: 'Source',
                  label: source.label,
                  state: source.state,
                  query: query,
                  lastSuccessAt: source.lastSuccessAt,
                  lastRange: source.lastRange,
                  failure: source.failure?.name,
                  failureAt: source.failureAt,
                ),
                for (final resource in source.resources)
                  Padding(
                    padding: EdgeInsets.only(left: FloeSpace.md),
                    child: _CoverageEntry(
                      kind: 'Resource',
                      label: resource.label,
                      state: resource.state,
                      query: query,
                      lastSuccessAt: resource.lastSuccessAt,
                      lastRange: resource.lastRange,
                      failure: resource.failure?.name,
                      failureAt: resource.failureAt,
                    ),
                  ),
                if (index < coverage.sources.length - 1)
                  Padding(
                    padding: EdgeInsets.symmetric(vertical: FloeSpace.sm),
                    child: FloeDivider(height: 1),
                  ),
              ],
          ],
        ),
      ],
    );
  }
}

class _CoverageEntry extends StatelessWidget {
  const _CoverageEntry({
    required this.kind,
    required this.label,
    required this.state,
    required this.query,
    required this.lastSuccessAt,
    required this.lastRange,
    required this.failure,
    required this.failureAt,
  });

  final String kind;
  final String label;
  final DayCoverageState state;
  final DayQuery query;
  final DateTime? lastSuccessAt;
  final DayCalendarRange? lastRange;
  final String? failure;
  final DateTime? failureAt;

  @override
  Widget build(BuildContext context) {
    final rangeCovers = lastRange?.covers(query) == true;
    final stateLabel = _stateLabel(state, rangeCovers);
    final locale = AppLocalizations.of(context).localeName;
    String date(DateTime value) => DateFormat.yMMMd(locale).format(value);
    final rangeLabel = lastRange == null
        ? null
        : '${date(lastRange!.startDate)} – ${date(lastRange!.endDateExclusive)} (end exclusive)';
    final failureLabel = failure == null ? null : _humanize(failure!);
    return Padding(
      padding: EdgeInsets.symmetric(vertical: FloeSpace.xs),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Expanded(
                child: Text('$kind · $label', style: FloeType.bodySmall),
              ),
              SizedBox(width: FloeSpace.sm),
              Text(
                stateLabel,
                style: FloeType.caption.copyWith(
                  color: state == DayCoverageState.current && rangeCovers
                      ? FloePalette.mint700
                      : FloePalette.warning600,
                ),
              ),
            ],
          ),
          if (rangeLabel != null) ...[
            SizedBox(height: FloeSpace.xxs),
            Text(
              'Last complete range: $rangeLabel',
              style: FloeType.caption.copyWith(color: FloePalette.neutral600),
            ),
          ],
          if (lastSuccessAt != null) ...[
            SizedBox(height: FloeSpace.xxs),
            Text(
              'Last complete at: ${formatTimestamp(context, lastSuccessAt!)}',
              style: FloeType.caption.copyWith(color: FloePalette.neutral600),
            ),
          ],
          if (failureLabel != null) ...[
            SizedBox(height: FloeSpace.xxs),
            Text(
              failureAt == null
                  ? 'Failure: $failureLabel'
                  : 'Failure: $failureLabel · ${formatTimestamp(context, failureAt!)}',
              style: FloeType.caption.copyWith(color: FloePalette.warning600),
            ),
          ],
        ],
      ),
    );
  }
}

String _stateLabel(DayCoverageState state, bool covers) {
  if (covers) return _humanize(state.name);
  return switch (state) {
    DayCoverageState.pending => 'Pending · uncovered',
    DayCoverageState.unavailable => 'Unavailable · uncovered/stale',
    DayCoverageState.partial => 'Partial · uncovered/stale',
    DayCoverageState.current || DayCoverageState.stale => 'Uncovered · stale',
  };
}

String _humanize(String value) => value
    .split('_')
    .map(
      (part) =>
          part.isEmpty ? part : '${part[0].toUpperCase()}${part.substring(1)}',
    )
    .join(' ');
