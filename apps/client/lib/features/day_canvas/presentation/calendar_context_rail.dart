import 'package:floe_client/l10n/app_localizations.dart';
import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../../app/design_tokens.dart';
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
        FloeSquircle(
          padding: EdgeInsets.all(24),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                AppLocalizations.of(context).yourOwnRhythm,
                style: TextStyle(fontSize: 16, fontWeight: FontWeight.w600),
              ),
              SizedBox(height: 20),
              Text(
                AppLocalizations.of(context).aFewThingsThatBelongToYou,
                style: TextStyle(
                  fontSize: 14,
                  height: 1.8,
                  color: FloePalette.neutral600,
                ),
              ),
              SizedBox(height: 20),
              if (tasks.isEmpty)
                Text(
                  AppLocalizations.of(context).noTasksForToday,
                  style: TextStyle(fontSize: 13, color: FloePalette.neutral600),
                ),
              for (final task in tasks.take(3))
                Row(
                  children: [
                    Checkbox(
                      value: task.isCompleted,
                      onChanged: disabled
                          ? null
                          : (value) => complete(task, value!),
                    ),
                    Expanded(
                      child: GestureDetector(
                        onTap: () => onOpenTask(task),
                        child: Text(
                          task.title,
                          style: TextStyle(
                            fontSize: 13,
                            decoration: task.isCompleted
                                ? TextDecoration.lineThrough
                                : null,
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
        SizedBox(height: 24),
        FloeSquircle(
          padding: EdgeInsets.all(24),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                AppLocalizations.of(context).aNoteToSelf,
                style: TextStyle(fontSize: 16, fontWeight: FontWeight.w600),
              ),
              SizedBox(height: 20),
              Text(
                DayAppearance.of(context)?.dailyNote ??
                    (notes.isEmpty
                        ? AppLocalizations.of(context)
                              .leaveALittleRoomBetweenThingsNot
                        : notes.first.title),
                style: TextStyle(
                  fontSize: 14,
                  height: 1.9,
                  color: FloePalette.neutral600,
                ),
              ),
              SizedBox(height: 24),
              Text(
                AppLocalizations.of(context).savedInFloeStaysWhenYouDisconnect,
                style: TextStyle(
                  fontSize: 11,
                  height: 1.7,
                  color: FloePalette.neutral600,
                ),
              ),
            ],
          ),
        ),
        SizedBox(height: 24),
        Padding(
          padding: EdgeInsets.symmetric(horizontal: 12),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Icon(LucideIcons.link, size: 15, color: FloePalette.neutral500),
              SizedBox(width: 10),
              Expanded(
                child: Text(
                  AppLocalizations.of(context)
                      .wonderingWhereAnEventCameFromOpen,
                  style: TextStyle(
                    fontSize: 11,
                    height: 1.8,
                    color: FloePalette.neutral600,
                  ),
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }
}
