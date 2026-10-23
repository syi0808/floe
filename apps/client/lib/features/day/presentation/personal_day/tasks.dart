part of '../personal_day_screen.dart';

class _TasksScreen extends StatelessWidget {
  const _TasksScreen({
    required this.snapshot,
    required this.disabled,
    required this.onComplete,
    required this.onOpen,
    required this.rowFocusNode,
    required this.headingFocusNode,
  });

  final DaySnapshot snapshot;
  final bool disabled;
  final Future<void> Function(TaskItem, bool) onComplete;
  final ValueChanged<TaskItem> onOpen;
  final FocusNode Function(String) rowFocusNode;
  final FocusNode headingFocusNode;

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
                      Focus(
                        focusNode: headingFocusNode,
                        skipTraversal: true,
                        child: Semantics(
                          header: true,
                          child: Text(
                            AppLocalizations.of(context).tasks,
                            key: const Key('tasks-page-title'),
                            style: FloeType.pageTitle,
                          ),
                        ),
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
                            onOpen: () => onOpen(task),
                            taskCollection: true,
                            focusNode: rowFocusNode(task.id),
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
    required this.onDelete,
  });
  final TaskItem task;
  final DaySnapshot snapshot;
  final bool narrow;
  final bool disabled;
  final Future<void> Function(TaskItem, bool) onComplete;
  final Future<bool> Function(TaskItem) onDelete;
  @override
  State<_TaskDetailScreen> createState() => _TaskDetailScreenState();
}

class _TaskDetailScreenState extends State<_TaskDetailScreen> {
  final FocusNode _deleteActionFocusNode = FocusNode(
    debugLabel: 'task-detail-delete',
  );
  bool _confirmingDelete = false;
  bool _deleting = false;

  @override
  void dispose() {
    _deleteActionFocusNode.dispose();
    super.dispose();
  }

  void _restoreDeleteFocus() {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _deleteActionFocusNode.requestFocus();
    });
  }

  Future<void> _deleteTask() async {
    if (widget.disabled || _confirmingDelete || _deleting) return;
    setState(() => _confirmingDelete = true);
    final confirmed = await _confirmDeleteItem(context, widget.task);
    if (!mounted) return;
    if (!confirmed) {
      setState(() => _confirmingDelete = false);
      _restoreDeleteFocus();
      return;
    }
    setState(() {
      _confirmingDelete = false;
      _deleting = true;
    });
    try {
      final deleted = await widget.onDelete(widget.task);
      if (!deleted && mounted) _restoreDeleteFocus();
    } finally {
      if (mounted) setState(() => _deleting = false);
    }
  }

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
              Wrap(
                alignment: WrapAlignment.start,
                crossAxisAlignment: WrapCrossAlignment.center,
                spacing: FloeSpace.sm,
                runSpacing: FloeSpace.sm,
                children: [
                  FloeButton.outlined(
                    key: const Key('task-detail-complete'),
                    loading: widget.disabled && !_deleting,
                    loadingLabel: copy.updatingTask,
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
                  FloeButton.text(
                    key: const Key('task-detail-delete'),
                    focusNode: _deleteActionFocusNode,
                    loading: _deleting,
                    loadingLabel: copy.deletingTask,
                    onPressed: widget.disabled || _confirmingDelete || _deleting
                        ? null
                        : _deleteTask,
                    style: ButtonStyle(
                      foregroundColor: WidgetStateProperty.resolveWith<Color?>((
                        states,
                      ) {
                        if (states.contains(WidgetState.disabled)) {
                          return FloeColor.disabledContent;
                        }
                        if (states.contains(WidgetState.pressed)) {
                          return FloePalette.error800;
                        }
                        if (states.contains(WidgetState.hovered) ||
                            states.contains(WidgetState.focused)) {
                          return FloePalette.error700;
                        }
                        return FloePalette.error600;
                      }),
                    ),
                    icon: const Icon(LucideIcons.trash2, size: 18),
                    child: Text(copy.deleteTask),
                  ),
                ],
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

Future<bool> _confirmDeleteItem(BuildContext context, TaskItem task) async {
  final confirmed = await showFloeDialog<bool>(
    context,
    (context) => FloeDialog(
      title: Text(AppLocalizations.of(context).deleteThisItem),
      content: Text(AppLocalizations.of(context).deleteItemMessage(task.title)),
      actions: [
        FloeButton.text(
          onPressed: () => Navigator.pop(context, false),
          child: Text(AppLocalizations.of(context).cancel),
        ),
        FloeButton.filled(
          onPressed: () => Navigator.pop(context, true),
          style: FloeTheme.destructiveButtonStyle,
          child: Text(AppLocalizations.of(context).delete),
        ),
      ],
    ),
  );
  return confirmed == true;
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
