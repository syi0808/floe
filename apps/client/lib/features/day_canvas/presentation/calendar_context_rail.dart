import 'package:floe_client/l10n/app_localizations.dart';
import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../../app/design_tokens.dart';
import '../../../app/floe_selection.dart';
import '../../../app/floe_feedback.dart';
import '../../../app/floe_squircle.dart';
import '../domain/day_models.dart';
import 'day_appearance.dart';

class CalendarContextRail extends StatelessWidget {
  const CalendarContextRail({
    super.key,
    required this.snapshot,
    required this.disabled,
    required this.complete,
    required this.onTasks,
    required this.onOpenTask,
  });
  final DaySnapshot snapshot;
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
