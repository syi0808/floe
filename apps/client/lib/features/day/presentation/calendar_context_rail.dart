import 'package:floe_client/app/floe_primitives.dart';
import 'package:floe_client/features/day/presentation/calendar_layout.dart';
import 'package:intl/intl.dart';
import 'package:floe_client/l10n/app_localizations.dart';
import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_selection.dart';
import 'package:floe_client/app/floe_feedback.dart';
import 'package:floe_client/app/floe_squircle.dart';
import 'package:floe_client/features/day/domain/day_models.dart';
import 'package:floe_client/features/day/presentation/day_appearance.dart';

class CalendarContextRail extends StatelessWidget {
  const CalendarContextRail({
    super.key,
    required this.snapshot,
    required this.query,
    required this.disabled,
    required this.complete,
    required this.onTasks,
    required this.onOpenTask,
  });
  final DaySnapshot snapshot;
  final DayQuery query;
  final bool disabled;
  final Future<void> Function(TaskItem, bool) complete;
  final VoidCallback onTasks;
  final ValueChanged<TaskItem> onOpenTask;
  @override
  Widget build(BuildContext context) {
    final tasks = snapshot.items.whereType<TaskItem>().toList();
    final notes = snapshot.items.whereType<NoteItem>().toList();
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        FloeCard(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                AppLocalizations.of(context).yourOwnRhythm,
                style: FloeType.title,
              ),
              SizedBox(height: 20),
              if (tasks.isEmpty)
                Text(
                  AppLocalizations.of(context).noTasksForSelectedDay,
                  style: FloeType.bodySmall.copyWith(
                    color: FloePalette.neutral600,
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
                      child: MouseRegion(
                        cursor: SystemMouseCursors.click,
                        child: GestureDetector(
                          onTap: () => onOpenTask(task),
                          child: Text(
                            task.title,
                            style: FloeType.bodySmall.copyWith(
                              decoration: task.isCompleted
                                  ? TextDecoration.lineThrough
                                  : null,
                            ),
                          ),
                        ),
                      ),
                    ),
                  ],
                ),
              SizedBox(height: 20),
              FloeTextLink(
                label: AppLocalizations.of(context).seeYourTasks,
                icon: LucideIcons.arrowRight,
                onPressed: onTasks,
              ),
            ],
          ),
        ),
        SizedBox(height: FloeSpace.lg),
        FloeCard(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                AppLocalizations.of(context).aNoteToSelf,
                style: FloeType.title,
              ),
              SizedBox(height: 20),
              Text(
                DayAppearance.of(context)?.dailyNote ??
                    (notes.isEmpty
                        ? AppLocalizations.of(context)
                              .leaveALittleRoomBetweenThingsNot
                        : notes.first.title),
                style: FloeType.body.copyWith(
                  height: 1.9,
                  color: FloePalette.neutral600,
                ),
              ),
            ],
          ),
        ),
        SizedBox(height: FloeSpace.lg),
        _CalendarCoverageCard(snapshot: snapshot, query: query),
        SizedBox(height: FloeSpace.lg),
        Padding(
          padding: EdgeInsets.symmetric(horizontal: FloeSpace.md),
          child: FloeIconText(
            icon: Icon(
              LucideIcons.link,
              size: 15,
              color: FloePalette.neutral500,
            ),
            gap: 10,
            text: AppLocalizations.of(context)
                .wonderingWhereAnEventCameFromOpen,
            style: FloeType.caption.copyWith(
              height: 1.8,
              color: FloePalette.neutral600,
            ),
          ),
        ),
      ],
    );
  }
}

class _CalendarCoverageCard extends StatelessWidget {
  const _CalendarCoverageCard({required this.snapshot, required this.query});

  final DaySnapshot snapshot;
  final DayQuery query;

  @override
  Widget build(BuildContext context) {
    final coverage = snapshot.calendarCoverage;
    final complete = hasCompleteCalendarCoverage(snapshot, query);
    return ExpansionTile(
      tilePadding: const EdgeInsets.symmetric(horizontal: FloeSpace.md),
      childrenPadding: const EdgeInsets.all(FloeSpace.md),
      shape: const Border(),
      collapsedShape: const Border(),
      title: Text(
        complete
            ? 'Calendars synced for this date'
            : coverage == null
            ? 'Calendar status unavailable'
            : coverage.sources.isEmpty
            ? 'No calendar data loaded'
            : 'Some calendar data may be missing or out of date',
        style: FloeType.bodySmall.copyWith(
          color: complete ? FloePalette.neutral600 : FloePalette.warning600,
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
