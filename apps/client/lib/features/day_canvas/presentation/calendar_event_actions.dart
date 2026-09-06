import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../domain/day_models.dart';
import 'calendar_event_details.dart';

class CalendarEventActions extends StatelessWidget {
  const CalendarEventActions({
    super.key,
    required this.event,
    required this.snapshot,
    required this.child,
    this.onEdit,
    this.onDelete,
  });
  final EventItem event;
  final DaySnapshot snapshot;
  final Widget child;
  final VoidCallback? onEdit;
  final VoidCallback? onDelete;

  Future<void> menu(BuildContext context, [Offset? position]) async {
    final overlay =
        Overlay.of(context).context.findRenderObject()! as RenderBox;
    final box = context.findRenderObject()! as RenderBox;
    final anchor = overlay.globalToLocal(
      position ?? box.localToGlobal(Offset(box.size.width, 0)),
    );
    final selected = await showMenu<String>(
      context: context,
      position: RelativeRect.fromSize(
        Rect.fromLTWH(anchor.dx, anchor.dy, 0, 0),
        overlay.size,
      ),
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(16)),
      items: [
        const PopupMenuItem(value: 'open', child: Text('Open details')),
        PopupMenuItem(
          value: 'edit',
          enabled: onEdit != null,
          child: const Text('Edit event…'),
        ),
        const PopupMenuDivider(),
        PopupMenuItem(
          value: 'delete',
          enabled: onDelete != null,
          child: Text(
            'Delete event…',
            style: TextStyle(
              color: onDelete == null
                  ? null
                  : Theme.of(context).colorScheme.error,
            ),
          ),
        ),
        if (onEdit == null)
          const PopupMenuItem(
            enabled: false,
            child: Text(
              'Editing unavailable for this event',
              style: TextStyle(fontSize: 11),
            ),
          ),
      ],
    );
    if (!context.mounted) return;
    switch (selected) {
      case 'open':
        openCalendarEvent(context, event, snapshot);
      case 'edit':
        onEdit?.call();
      case 'delete':
        onDelete?.call();
    }
  }

  @override
  Widget build(BuildContext context) => CallbackShortcuts(
    bindings: {
      const SingleActivator(LogicalKeyboardKey.f10, shift: true): () =>
          menu(context),
      const SingleActivator(LogicalKeyboardKey.contextMenu): () =>
          menu(context),
    },
    child: Focus(
      child: GestureDetector(
        onSecondaryTapDown: (details) => menu(context, details.globalPosition),
        onLongPressStart: (details) => menu(context, details.globalPosition),
        child: Stack(
          fit: StackFit.passthrough,
          children: [
            child,
            Positioned(
              top: 0,
              right: 0,
              bottom: 0,
              child: Center(
                child: IconButton(
                  tooltip: 'Event actions',
                  visualDensity: VisualDensity.compact,
                  constraints: const BoxConstraints(
                    minWidth: 24,
                    minHeight: 24,
                  ),
                  padding: const EdgeInsets.all(3),
                  iconSize: 16,
                  onPressed: () => menu(context),
                  icon: const Icon(Icons.more_horiz),
                ),
              ),
            ),
          ],
        ),
      ),
    ),
  );
}
