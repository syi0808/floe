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
          padding: const EdgeInsets.all(24),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              const Text(
                'Your own rhythm',
                style: TextStyle(fontSize: 16, fontWeight: FontWeight.w600),
              ),
              const SizedBox(height: 20),
              const Text(
                'A few things that belong to you, not your calendar.',
                style: TextStyle(
                  fontSize: 14,
                  height: 1.8,
                  color: FloePalette.neutral600,
                ),
              ),
              const SizedBox(height: 20),
              if (tasks.isEmpty)
                const Text(
                  'No tasks for today.',
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
              const SizedBox(height: 20),
              FloeTextLink(
                label: 'See your tasks',
                icon: LucideIcons.arrowRight,
                onPressed: onTasks,
              ),
            ],
          ),
        ),
        const SizedBox(height: 24),
        FloeSquircle(
          padding: const EdgeInsets.all(24),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              const Text(
                'A note to self',
                style: TextStyle(fontSize: 16, fontWeight: FontWeight.w600),
              ),
              const SizedBox(height: 20),
              Text(
                DayAppearance.of(context)?.dailyNote ??
                    (notes.isEmpty
                        ? 'Leave a little room between things. Not every empty space needs filling.'
                        : notes.first.title),
                style: const TextStyle(
                  fontSize: 14,
                  height: 1.9,
                  color: FloePalette.neutral600,
                ),
              ),
              const SizedBox(height: 24),
              const Text(
                'Saved in Floe · stays when you disconnect',
                style: TextStyle(
                  fontSize: 11,
                  height: 1.7,
                  color: FloePalette.neutral600,
                ),
              ),
            ],
          ),
        ),
        const SizedBox(height: 24),
        const Padding(
          padding: EdgeInsets.symmetric(horizontal: 12),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Icon(LucideIcons.link, size: 15, color: FloePalette.neutral500),
              SizedBox(width: 10),
              Expanded(
                child: Text(
                  'Wondering where an event came from?\nOpen it to see its source and time zone.',
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
