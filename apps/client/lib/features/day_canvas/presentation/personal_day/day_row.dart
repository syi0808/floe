part of '../personal_day_screen.dart';

class _DayRow extends StatelessWidget {
  const _DayRow({
    required this.item,
    required this.snapshot,
    required this.disabled,
    required this.complete,
    required this.delete,
    this.onOpen,
  });
  final DayItem item;
  final DaySnapshot snapshot;
  final bool disabled;
  final Future<void> Function(TaskItem, bool) complete;
  final Future<void> Function(DayItem) delete;
  final VoidCallback? onOpen;
  @override
  Widget build(BuildContext context) {
    final task = item is TaskItem ? item as TaskItem : null;
    final overdue =
        task != null &&
        !task.isCompleted &&
        task.deadline?.isBefore(snapshot.generatedAt) == true;
    final subtitle = switch (item) {
      EventItem(:final startsAt, :final endsAt) =>
        '${_time(context, startsAt)}–${_time(context, endsAt)}',
      TaskItem(:final deadline) =>
        deadline == null
            ? AppLocalizations.of(context).timeNotSet
            : AppLocalizations.of(context).dueAt(_time(context, deadline)),
      NoteItem() => AppLocalizations.of(context).todaySThought,
    };
    final label = switch (item) {
      EventItem() => AppLocalizations.of(context).event,
      TaskItem() => AppLocalizations.of(context).task,
      NoteItem() => AppLocalizations.of(context).note,
    };
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
        title: Text(
          item.title,
          maxLines: 3,
          overflow: TextOverflow.ellipsis,
          style: FloeType.bodyLarge.copyWith(
            decoration: task?.isCompleted == true
                ? TextDecoration.lineThrough
                : null,
          ),
        ),
        subtitle: Text(
          overdue
              ? AppLocalizations.of(context).overdueItem(label, subtitle)
              : '$label · $subtitle',
          style: FloeType.body.copyWith(
            color: overdue ? FloePalette.warning600 : FloePalette.neutral500,
          ),
        ),
        trailing: item is EventItem && (item as EventItem).externalId != null
            ? FloeTooltip(
                message: AppLocalizations.of(context)
                    .readOnlyEventManagedInItsOriginal,
                child: Icon(Icons.lock_outline, size: 18),
              )
            : FloeButton.icon(
                tooltip: AppLocalizations.of(context)
                    .deleteItemLabel(item.title),
                onPressed: disabled ? null : () => _confirmDelete(context),
                icon: Icon(Icons.more_horiz, size: 20),
              ),
      ),
    );
  }

  Future<void> _confirmDelete(BuildContext context) async {
    final confirmed = await showFloeDialog<bool>(
      context,
      (context) => FloeDialog(
        title: Text(AppLocalizations.of(context).deleteThisItem),
        content: Text(
          AppLocalizations.of(context).deleteItemMessage(item.title),
        ),
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
    if (confirmed == true) await delete(item);
  }
}
