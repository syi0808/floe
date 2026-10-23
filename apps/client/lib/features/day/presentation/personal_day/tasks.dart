part of '../personal_day_screen.dart';

class _TasksScreen extends StatelessWidget {
  const _TasksScreen({
    required this.snapshot,
    required this.disabled,
    required this.onComplete,
    required this.onOpen,
    required this.onDelete,
  });

  final DaySnapshot snapshot;
  final bool disabled;
  final Future<void> Function(TaskItem, bool) onComplete;
  final ValueChanged<TaskItem> onOpen;
  final Future<void> Function(DayItem) onDelete;

  @override
  Widget build(BuildContext context) {
    final tasks = snapshot.items.whereType<TaskItem>().toList();
    final remaining = tasks.where((task) => !task.isCompleted).length;
    return Align(
      alignment: Alignment.topLeft,
      child: ConstrainedBox(
        key: const Key('tasks-collection'),
        constraints: const BoxConstraints(maxWidth: 840),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Row(
              children: [
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        AppLocalizations.of(context).tasks,
                        key: const Key('tasks-page-title'),
                        style: FloeType.pageTitle,
                      ),
                      SizedBox(height: FloeSpace.xs),
                      Text(
                        AppLocalizations.of(context)
                            .taskSummary(remaining, tasks.length),
                        key: const Key('tasks-page-summary'),
                        style: FloeType.bodySmall,
                      ),
                    ],
                  ),
                ),
              ],
            ),
            SizedBox(height: FloeSpace.base),
            Padding(
              padding: EdgeInsets.symmetric(
                horizontal: 0,
                vertical: FloeSpace.sm,
              ),
              child: tasks.isEmpty
                  ? Padding(
                      padding: EdgeInsets.symmetric(vertical: FloeSpace.xxxl),
                      child: Center(
                        child: Text(
                          AppLocalizations.of(context).noTasksYet,
                          style: FloeType.body,
                        ),
                      ),
                    )
                  : Column(
                      children: [
                        for (final (index, task) in tasks.indexed) ...[
                          _DayRow(
                            item: task,
                            snapshot: snapshot,
                            disabled: disabled,
                            complete: onComplete,
                            delete: onDelete,
                            onOpen: () => onOpen(task),
                            taskCollection: true,
                          ),
                          if (index < tasks.length - 1)
                            FloeDivider(height: 1, indent: 56),
                        ],
                      ],
                    ),
            ),
          ],
        ),
      ),
    );
  }
}

class _TaskDetailScreen extends StatefulWidget {
  const _TaskDetailScreen({
    required this.task,
    required this.snapshot,
    required this.narrow,
    required this.disabled,
    required this.onComplete,
  });
  final TaskItem task;
  final DaySnapshot snapshot;
  final bool narrow;
  final bool disabled;
  final Future<void> Function(TaskItem, bool) onComplete;
  @override
  State<_TaskDetailScreen> createState() => _TaskDetailScreenState();
}

class _TaskDetailScreenState extends State<_TaskDetailScreen> {
  @override
  Widget build(BuildContext context) {
    final appearance = DayAppearance.of(context)?.tasks[widget.task.id];
    final copy = AppLocalizations.of(context);
    return Align(
      alignment: Alignment.topLeft,
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 760),
        child: Padding(
          padding: EdgeInsets.symmetric(vertical: widget.narrow ? 16 : 32),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Text(
                widget.task.title,
                style: FloeType.display.copyWith(
                  fontSize: widget.narrow ? 28 : 36,
                ),
              ),
              const SizedBox(height: FloeSpace.lg),
              if (appearance?.description case final description?) ...[
                Text(description, style: FloeType.body),
                const SizedBox(height: FloeSpace.lg),
              ],
              _LabeledValue(
                label: copy.due,
                value: widget.task.deadline == null
                    ? copy.noDueDate
                    : _deadline(context, widget.task.deadline!),
              ),
              const SizedBox(height: FloeSpace.lg),
              Align(
                alignment: Alignment.centerLeft,
                child: FloeButton.outlined(
                  loading: widget.disabled,
                  onPressed: widget.disabled
                      ? null
                      : () => widget.onComplete(
                          widget.task,
                          !widget.task.isCompleted,
                        ),
                  icon: Icon(
                    widget.task.isCompleted
                        ? LucideIcons.rotateCcw
                        : LucideIcons.check,
                    size: 18,
                  ),
                  child: Text(
                    widget.task.isCompleted
                        ? copy.markIncomplete
                        : copy.completeTask,
                  ),
                ),
              ),
              if (appearance != null && appearance.subtasks.isNotEmpty) ...[
                const SizedBox(height: 32),
                Text(copy.subtasks, style: FloeType.title),
                const SizedBox(height: 12),
                // Preview context has no mutation owner. Do not offer pretend edits.
                for (final subtask in appearance.subtasks)
                  Padding(
                    padding: const EdgeInsets.symmetric(vertical: 8),
                    child: Row(
                      children: [
                        Icon(
                          subtask.done ? LucideIcons.check : LucideIcons.minus,
                          size: 16,
                        ),
                        const SizedBox(width: 12),
                        Expanded(child: Text(subtask.title)),
                      ],
                    ),
                  ),
              ],
              if (appearance?.note case final note?) ...[
                const SizedBox(height: 32),
                Text(copy.notes, style: FloeType.title),
                const SizedBox(height: 12),
                Text(note, style: FloeType.body),
              ],
            ],
          ),
        ),
      ),
    );
  }
}

class _LabeledValue extends StatelessWidget {
  const _LabeledValue({required this.label, required this.value});
  final String label;
  final String value;
  @override
  Widget build(BuildContext context) => Row(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      SizedBox(
        width: MediaQuery.sizeOf(context).width <= 430
            ? 92
            : MediaQuery.sizeOf(context).width <= 780
            ? 110
            : 150,
        child: Text(label, style: FloeType.body.copyWith(height: 1.15)),
      ),
      Expanded(
        child: Text(
          value,
          style: FloeType.body.copyWith(
            color: FloeColor.textPrimary,
            height: 1.15,
          ),
        ),
      ),
    ],
  );
}
