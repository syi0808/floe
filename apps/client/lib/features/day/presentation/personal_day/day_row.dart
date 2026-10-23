part of '../personal_day_screen.dart';

class _DayRow extends StatelessWidget {
  const _DayRow({
    required this.item,
    required this.snapshot,
    required this.disabled,
    required this.complete,
    this.onOpen,
    this.taskCollection = false,
    this.focusNode,
  });
  final DayItem item;
  final DaySnapshot snapshot;
  final bool disabled;
  final Future<void> Function(TaskItem, bool) complete;
  final VoidCallback? onOpen;
  final bool taskCollection;
  final FocusNode? focusNode;
  @override
  Widget build(BuildContext context) {
    final task = item is TaskItem ? item as TaskItem : null;
    final copy = AppLocalizations.of(context);
    final overdue =
        task != null &&
        !task.isCompleted &&
        task.deadline?.isBefore(snapshot.generatedAt) == true;
    final subtitle = switch (item) {
      EventItem(:final startsAt, :final endsAt) =>
        '${_time(context, startsAt)}–${_time(context, endsAt)}',
      TaskItem(:final deadline) =>
        deadline == null
            ? copy.timeNotSet
            : copy.dueAt(_deadline(context, deadline)),
      NoteItem() => copy.todaySThought,
    };
    final label = switch (item) {
      EventItem() => copy.event,
      TaskItem() => copy.task,
      NoteItem() => copy.note,
    };
    final titleText = Text(
      item.title,
      maxLines: 3,
      overflow: TextOverflow.ellipsis,
      style: FloeType.bodyLarge.copyWith(
        fontWeight: taskCollection ? FontWeight.w500 : null,
        decoration: task?.isCompleted == true
            ? TextDecoration.lineThrough
            : null,
      ),
    );
    final title = taskCollection && task?.deadline == null
        ? Semantics(
            label: '${item.title}. ${copy.timeNotSet}',
            child: ExcludeSemantics(child: titleText),
          )
        : titleText;
    final taskCollectionSubtitle = task == null || task.deadline == null
        ? null
        : Text(
            overdue ? '${copy.overdue} · $subtitle' : subtitle,
            semanticsLabel: overdue ? '${copy.overdue} · $subtitle' : null,
            style: FloeType.bodySmall.copyWith(
              color: overdue ? FloePalette.warning600 : FloePalette.neutral500,
            ),
          );
    return ConstrainedBox(
      constraints: const BoxConstraints(minHeight: 72),
      child: FloeListRow(
        onPressed: onOpen,
        leading: SizedBox.square(
          dimension: 44,
          child: Center(
            child: task != null
                ? FloeCheckbox(
                    semanticLabel: task.title,
                    value: task.isCompleted,
                    onChanged: disabled
                        ? null
                        : (value) => complete(task, value ?? false),
                  )
                : Icon(
                    item is EventItem
                        ? Icons.calendar_today_outlined
                        : Icons.notes_outlined,
                    color: item is EventItem
                        ? FloePalette.blue500
                        : FloePalette.mint700,
                    size: 20,
                  ),
          ),
        ),
        title: title,
        focusNode: focusNode,
        subtitle: taskCollection
            ? taskCollectionSubtitle
            : Text(
                overdue
                    ? copy.overdueItem(label, subtitle)
                    : '$label · $subtitle',
                style: FloeType.body.copyWith(
                  color: overdue
                      ? FloePalette.warning600
                      : FloePalette.neutral500,
                ),
              ),
      ),
    );
  }
}
