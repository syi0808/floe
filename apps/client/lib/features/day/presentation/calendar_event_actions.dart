import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter/gestures.dart';
import 'package:floe_client/l10n/app_localizations.dart';

import 'package:floe_client/app/floe_context_menu.dart';
import 'package:floe_client/app/floe_button.dart';
import 'package:floe_client/app/floe_popover.dart';

import 'package:floe_client/features/day/domain/day_models.dart';
import 'package:floe_client/features/day/presentation/calendar_event_details.dart';

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

  bool get canEdit => event.actionTarget != null && onEdit != null;
  bool get canDelete => event.actionTarget != null && onDelete != null;

  Future<void> menu(BuildContext context, [Offset? position]) async {
    final copy = AppLocalizations.of(context);
    final rect = floeAnchorRect(context);
    final anchor = position == null
        ? Rect.fromLTWH(rect.right - 24, rect.top, 24, 24)
        : Rect.fromLTWH(position.dx, position.dy, 0, 0);
    final selected = await showFloeContextMenu<String>(
      context: context,
      anchor: anchor,
      entries: [
        FloeMenuEntry(
          value: 'open',
          label: copy.eventDetails,
          icon: Icons.open_in_new,
        ),
        if (canEdit)
          FloeMenuEntry(
            value: 'edit',
            label: copy.eventEditAction,
            icon: Icons.edit_outlined,
          ),
        if (canDelete)
          FloeMenuEntry(
            value: 'delete',
            label: copy.eventDeleteAction,
            icon: Icons.delete_outline,
            destructive: true,
            separator: true,
          ),
      ],
    );
    if (!context.mounted) return;
    switch (selected) {
      case 'open':
        openCalendarEvent(
          context,
          event,
          snapshot,
          onEdit: canEdit ? onEdit : null,
          onDelete: canDelete ? onDelete : null,
        );
      case 'edit':
        if (canEdit) onEdit?.call();
      case 'delete':
        if (canDelete) onDelete?.call();
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
            if (canEdit || canDelete)
              Positioned(
                top: 0,
                right: 0,
                bottom: 0,
                child: Center(
                  child: FloeButton.icon(
                    tooltip: AppLocalizations.of(context).eventActions,
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
  );
}
