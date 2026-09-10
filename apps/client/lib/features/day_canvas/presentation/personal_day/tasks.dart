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
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Row(
          children: [
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    AppLocalizations.of(context).task,
                    style: FloeType.display,
                  ),
                  SizedBox(height: FloeSpace.xs),
                  Text(
                    AppLocalizations.of(context)
                        .taskSummary(remaining, tasks.length),
                    style: FloeType.body,
                  ),
                ],
              ),
            ),
            FloeButton.filled(
              onPressed: () => _showComingSoon(context),
              icon: Icon(Icons.add),
              child: Text(AppLocalizations.of(context).newTask),
            ),
          ],
        ),
        SizedBox(height: FloeSpace.xl),
        FloeSquircle(
          padding: EdgeInsets.symmetric(
            horizontal: FloeSpace.lg,
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
                      ),
                      if (index < tasks.length - 1)
                        FloeDivider(height: 1, indent: 56),
                    ],
                  ],
                ),
        ),
      ],
    );
  }
}

class _TaskDetailScreen extends StatefulWidget {
  const _TaskDetailScreen({
    required this.task,
    required this.snapshot,
    required this.narrow,
    required this.onComplete,
  });
  final TaskItem task;
  final DaySnapshot snapshot;
  final bool narrow;
  final Future<void> Function(TaskItem, bool) onComplete;
  @override
  State<_TaskDetailScreen> createState() => _TaskDetailScreenState();
}

class _TaskDetailScreenState extends State<_TaskDetailScreen> {
  final Map<String, bool> subtaskChecks = {};
  bool suggestionVisible = true;

  @override
  Widget build(BuildContext context) {
    final appearance = DayAppearance.of(context)?.tasks[widget.task.id];
    final primary = FloeSquircle(
      padding: widget.narrow
          ? EdgeInsets.symmetric(
              horizontal: MediaQuery.sizeOf(context).width <= 430 ? 21 : 23,
              vertical: MediaQuery.sizeOf(context).width <= 430 ? 25 : 27,
            )
          : EdgeInsets.all(43),
      child: ConstrainedBox(
        constraints: BoxConstraints(minHeight: widget.narrow ? 0 : 634),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Row(
              children: [
                _ToneDot(color: FloePalette.blue500),
                SizedBox(width: FloeSpace.md),
                Text(
                  AppLocalizations.of(context).task,
                  style: FloeType.body.copyWith(height: 1.15),
                ),
              ],
            ),
            SizedBox(height: 18),
            Text(
              widget.task.title,
              style: FloeType.hero.copyWith(
                fontSize: widget.narrow ? 39 : 48,
                height: 1.2,
              ),
            ),
            SizedBox(height: FloeSpace.lg),
            Text(
              appearance?.description ??
                  AppLocalizations.of(context).noDescriptionYet,
              style: FloeType.body.copyWith(fontSize: 16, height: 1.65),
            ),
            SizedBox(height: 34),
            _LabeledValue(
              label: AppLocalizations.of(context).due,
              value: widget.task.deadline == null
                  ? AppLocalizations.of(context).noDueDate
                  : AppLocalizations.of(context).today,
              color: FloePalette.primary600,
            ),
            SizedBox(height: FloeSpace.base),
            _LabeledValue(
              label: AppLocalizations.of(context).timeContext,
              value:
                  appearance?.timeContext ??
                  AppLocalizations.of(context).notScheduled,
            ),
            SizedBox(height: FloeSpace.base),
            _LabeledValue(
              label: AppLocalizations.of(context).calendar,
              value:
                  appearance?.project ?? AppLocalizations.of(context).personal,
              color: FloePalette.mint700,
            ),
            SizedBox(height: FloeSpace.xl),
            FloeDivider(),
            SizedBox(height: 26),
            Text(
              AppLocalizations.of(context).subtasks,
              style: FloeType.title.copyWith(fontSize: 17, height: 1.2),
            ),
            SizedBox(height: 10),
            if (appearance == null || appearance.subtasks.isEmpty)
              Padding(
                padding: EdgeInsets.symmetric(vertical: 20),
                child: Text(
                  AppLocalizations.of(context).noSubtasksYet,
                  style: FloeType.body,
                ),
              ),
            for (final subtask
                in appearance?.subtasks ??
                    <({String title, String duration, bool done})>[])
              Container(
                constraints: BoxConstraints(minHeight: 62),
                decoration: BoxDecoration(
                  border: Border(
                    bottom: BorderSide(color: FloePalette.neutral200),
                  ),
                ),
                child: Row(
                  children: [
                    SizedBox(
                      width: 44,
                      child: FloeCheckbox(
                        semanticLabel: subtask.title,
                        value: subtaskChecks[subtask.title] ?? subtask.done,
                        onChanged: (value) => setState(
                          () => subtaskChecks[subtask.title] = value ?? false,
                        ),
                      ),
                    ),
                    Expanded(
                      child: Text(
                        subtask.title,
                        style: FloeType.body.copyWith(
                          color: (subtaskChecks[subtask.title] ?? subtask.done)
                              ? FloePalette.neutral500
                              : FloePalette.neutral950,
                          decoration:
                              (subtaskChecks[subtask.title] ?? subtask.done)
                              ? TextDecoration.lineThrough
                              : null,
                        ),
                      ),
                    ),
                    SizedBox(width: FloeSpace.sm),
                    Text(
                      subtask.duration,
                      style: FloeType.bodySmall.copyWith(
                        color: FloePalette.neutral600,
                      ),
                    ),
                  ],
                ),
              ),
            SizedBox(height: FloeSpace.sm),
            Align(
              alignment: Alignment.centerLeft,
              child: FloeButton.text(
                onPressed: () => _showComingSoon(context),
                style: const ButtonStyle(
                  padding: WidgetStatePropertyAll(EdgeInsets.zero),
                  foregroundColor: WidgetStatePropertyAll(
                    FloePalette.primary600,
                  ),
                ),
                icon: Icon(LucideIcons.plus, size: 18),
                child: Text(AppLocalizations.of(context).addASubtask),
              ),
            ),
          ],
        ),
      ),
    );
    final rail = Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (suggestionVisible) ...[
          FloeSquircle(
            padding: EdgeInsets.all(widget.narrow ? 23 : 27),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Row(
                  children: [
                    FloeMascot(size: 38),
                    SizedBox(width: 10),
                    Expanded(
                      child: Text(
                        AppLocalizations.of(context).floeSuggests,
                        style: FloeType.controlLabel.copyWith(
                          color: FloePalette.primary600,
                        ),
                      ),
                    ),
                    FloeSquircle(
                      size: FloeSquircleSize.md,
                      child: FloeButton.icon(
                        tooltip: AppLocalizations.of(context).dismissSuggestion,
                        size: FloeButtonSize.compact,
                        onPressed: () =>
                            setState(() => suggestionVisible = false),
                        icon: Icon(LucideIcons.x, size: 18),
                      ),
                    ),
                  ],
                ),
                SizedBox(height: 18),
                Text(
                  appearance?.suggestion ??
                      AppLocalizations.of(context)
                          .reviewTheContextBeforeStartingThisTask,
                  style: FloeType.body.copyWith(height: 1.55),
                ),
                SizedBox(height: 18),
                Wrap(
                  alignment: WrapAlignment.end,
                  spacing: 12,
                  children: [
                    FloeButton.filled(
                      onPressed: () =>
                          setState(() => suggestionVisible = false),
                      child: Text(AppLocalizations.of(context).reviewNow),
                    ),
                    FloeButton.text(
                      style: const ButtonStyle(
                        minimumSize: WidgetStatePropertyAll(Size(0, 40)),
                        padding: WidgetStatePropertyAll(
                          EdgeInsets.symmetric(horizontal: FloeSpace.md),
                        ),
                        tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                        textStyle: WidgetStatePropertyAll(FloeType.bodySmall),
                      ),
                      onPressed: () =>
                          setState(() => suggestionVisible = false),
                      child: Text(AppLocalizations.of(context).snooze),
                    ),
                  ],
                ),
              ],
            ),
          ),
          SizedBox(height: widget.narrow ? 16 : 20),
        ],
        FloeSquircle(
          fill: FloePalette.primary50,
          borderColor: FloePalette.primary100,
          padding: EdgeInsets.all(widget.narrow ? 23 : 27),
          child: ConstrainedBox(
            constraints: BoxConstraints(),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  AppLocalizations.of(context).notes,
                  style: FloeType.title.copyWith(fontSize: 17),
                ),
                SizedBox(height: 18),
                Text(
                  appearance?.note ??
                      AppLocalizations.of(context).noLinkedNotes,
                  style: FloeType.body.copyWith(height: 1.7),
                ),
                SizedBox(height: 20),
                Text(
                  AppLocalizations.of(context).updatedThisMorning,
                  style: FloeType.bodySmall.copyWith(
                    fontSize: 12,
                    color: FloePalette.neutral500,
                  ),
                ),
              ],
            ),
          ),
        ),
      ],
    );
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: EdgeInsets.symmetric(vertical: FloeSpace.md),
          child: Row(
            children: [
              Spacer(),
              FloeDropdown<bool>(
                label: AppLocalizations.of(context).taskOptions,
                icon: Icon(LucideIcons.ellipsis, size: 21),
                onSelected: (completed) =>
                    widget.onComplete(widget.task, completed),
                items: [
                  FloeSelectOption(
                    value: !widget.task.isCompleted,
                    label: widget.task.isCompleted
                        ? AppLocalizations.of(context).markIncomplete
                        : AppLocalizations.of(context).completeTask,
                  ),
                ],
              ),
            ],
          ),
        ),
        LayoutBuilder(
          builder: (context, constraints) {
            if (MediaQuery.sizeOf(context).width <= 1080) {
              return Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  primary,
                  SizedBox(height: widget.narrow ? 16 : 24),
                  rail,
                ],
              );
            }
            return Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Expanded(child: primary),
                SizedBox(width: FloeSpace.lg),
                SizedBox(
                  width: ((constraints.maxWidth - 24) * .3).clamp(
                    288,
                    double.infinity,
                  ),
                  child: rail,
                ),
              ],
            );
          },
        ),
      ],
    );
  }
}

class _LabeledValue extends StatelessWidget {
  const _LabeledValue({
    required this.label,
    required this.value,
    this.color = FloePalette.neutral950,
  });
  final String label;
  final String value;
  final Color color;
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
          style: FloeType.body.copyWith(color: color, height: 1.15),
        ),
      ),
    ],
  );
}

Future<void> _openNote(BuildContext context, NoteItem note, bool narrow) async {
  final detail = _NoteDetail(note: note);
  if (narrow) {
    await showFloeSheet<void>(context, (context) => detail);
  } else {
    await showFloeDialog<void>(
      context,
      (context) => FloeDialogSurface(maxWidth: 640, child: detail),
    );
  }
}
