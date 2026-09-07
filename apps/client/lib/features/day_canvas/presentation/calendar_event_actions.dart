import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter/gestures.dart';

import '../../../app/floe_context_menu.dart';
import '../../../app/floe_button.dart';
import '../../../app/floe_popover.dart';

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
    final rect = floeAnchorRect(context);
    final anchor = position == null
        ? Rect.fromLTWH(rect.right - 24, rect.top, 24, 24)
        : Rect.fromLTWH(position.dx, position.dy, 0, 0);
    final selected = await showFloeContextMenu<String>(
      context: context,
      anchor: anchor,
      explanation: onEdit == null
          ? 'Editing unavailable. Refresh your calendar or check its permissions.'
          : null,
      entries: [
        const FloeMenuEntry(
          value: 'open',
          label: 'Open details',
          icon: Icons.open_in_new,
        ),
        FloeMenuEntry(
          value: 'edit',
          enabled: onEdit != null,
          label: 'Edit event…',
          icon: Icons.edit_outlined,
        ),
        FloeMenuEntry(
          value: 'delete',
          enabled: onDelete != null,
          label: 'Delete event…',
          icon: Icons.delete_outline,
          destructive: true,
          separator: true,
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
        child: GestureDetector(
          supportedDevices: const {
            PointerDeviceKind.touch,
            PointerDeviceKind.stylus,
          },
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
                  child: FloeButton.icon(
                    tooltip: 'Event actions',
                    size: FloeButtonSize.compact,
                    onPressed: () => menu(context),
                    icon: const Icon(Icons.more_horiz, size: 16),
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    ),
  );
}
