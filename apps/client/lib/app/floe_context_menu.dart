import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'design_tokens.dart';
import 'floe_motion.dart';
import 'floe_popover.dart';

class FloeMenuEntry<T> {
  const FloeMenuEntry({
    required this.value,
    required this.label,
    required this.icon,
    this.enabled = true,
    this.destructive = false,
    this.separator = false,
  });
  final T value;
  final String label;
  final IconData icon;
  final bool enabled;
  final bool destructive;
  final bool separator;
}

Future<T?> showFloeContextMenu<T>({
  required BuildContext context,
  required Rect anchor,
  required List<FloeMenuEntry<T>> entries,
  String? explanation,
}) => showFloePopover<T>(
  context: context,
  anchor: anchor,
  width: 248,
  height:
      12 +
      entries.length * 36 +
      entries.where((entry) => entry.separator).length * 9 +
      (explanation == null ? 0 : 42),
  builder: (_) =>
      FloeContextMenu<T>(entries: entries, explanation: explanation),
);

class FloeContextMenu<T> extends StatefulWidget {
  const FloeContextMenu({super.key, required this.entries, this.explanation});
  final List<FloeMenuEntry<T>> entries;
  final String? explanation;
  @override
  State<FloeContextMenu<T>> createState() => _FloeContextMenuState<T>();
}

class _FloeContextMenuState<T> extends State<FloeContextMenu<T>> {
  int active = -1;
  int pressed = -1;
  late final keys = List.generate(widget.entries.length, (_) => GlobalKey());

  void move(int step) {
    for (var offset = 1; offset <= widget.entries.length; offset++) {
      final index =
          ((active < 0 ? (step > 0 ? -1 : 0) : active) + offset * step) %
          widget.entries.length;
      if (!widget.entries[index].enabled) continue;
      setState(() => active = index);
      WidgetsBinding.instance.addPostFrameCallback((_) {
        final target = keys[index].currentContext;
        if (mounted && target != null) Scrollable.ensureVisible(target);
      });
      return;
    }
  }

  KeyEventResult navigate(FocusNode node, KeyEvent event) {
    if (event is! KeyDownEvent && event is! KeyRepeatEvent) {
      return KeyEventResult.ignored;
    }
    final key = event.logicalKey;
    if (key == LogicalKeyboardKey.arrowDown ||
        key == LogicalKeyboardKey.arrowUp) {
      move(key == LogicalKeyboardKey.arrowDown ? 1 : -1);
    } else if (key == LogicalKeyboardKey.home ||
        key == LogicalKeyboardKey.end) {
      active = -1;
      move(key == LogicalKeyboardKey.home ? 1 : -1);
    } else if (key == LogicalKeyboardKey.enter ||
        key == LogicalKeyboardKey.space) {
      if (active >= 0) Navigator.of(context).pop(widget.entries[active].value);
    } else if (key == LogicalKeyboardKey.escape ||
        key == LogicalKeyboardKey.tab) {
      Navigator.of(context).pop();
    } else {
      return KeyEventResult.ignored;
    }
    return KeyEventResult.handled;
  }

  @override
  Widget build(BuildContext context) => Focus(
    autofocus: true,
    onKeyEvent: navigate,
    child: SingleChildScrollView(
      padding: const EdgeInsets.all(6),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          for (var index = 0; index < widget.entries.length; index++) ...[
            if (widget.entries[index].separator)
              const Padding(
                padding: EdgeInsets.symmetric(vertical: 4, horizontal: 6),
                child: Divider(height: 1, color: FloePalette.neutral100),
              ),
            Builder(
              builder: (context) {
                final entry = widget.entries[index];
                final highlighted = entry.enabled && active == index;
                final foreground = !entry.enabled
                    ? FloePalette.neutral400
                    : entry.destructive
                    ? Theme.of(context).colorScheme.error
                    : highlighted
                    ? FloePalette.primary700
                    : FloePalette.neutral950;
                void choose() => Navigator.of(context).pop(entry.value);
                return Semantics(
                  excludeSemantics: true,
                  key: keys[index],
                  button: true,
                  enabled: entry.enabled,
                  selected: highlighted,
                  label: entry.label,
                  onTap: entry.enabled ? choose : null,
                  child: MouseRegion(
                    cursor: entry.enabled
                        ? SystemMouseCursors.click
                        : SystemMouseCursors.basic,
                    onEnter: (_) =>
                        setState(() => active = entry.enabled ? index : -1),
                    onExit: (_) {
                      if (active == index) setState(() => active = -1);
                    },
                    child: GestureDetector(
                      excludeFromSemantics: true,
                      behavior: HitTestBehavior.opaque,
                      onTap: entry.enabled ? choose : null,
                      onTapDown: entry.enabled
                          ? (_) => setState(() => pressed = index)
                          : null,
                      onTapCancel: () => setState(() => pressed = -1),
                      onTapUp: (_) => setState(() => pressed = -1),
                      child: AnimatedScale(
                        scale:
                            pressed == index &&
                                !FloeMotion.reduceMotion(context)
                            ? .98
                            : 1,
                        duration: FloeMotion.pressDuration,
                        curve: FloeMotion.easeOut,
                        child: Container(
                          height: 36,
                          padding: const EdgeInsets.symmetric(horizontal: 10),
                          decoration: BoxDecoration(
                            color: highlighted
                                ? FloePalette.primary50
                                : Colors.transparent,
                            borderRadius: BorderRadius.circular(8),
                          ),
                          child: Row(
                            children: [
                              Icon(entry.icon, size: 15, color: foreground),
                              const SizedBox(width: 10),
                              Expanded(
                                child: Text(
                                  entry.label,
                                  style: TextStyle(
                                    fontSize: 12,
                                    fontWeight: FontWeight.w500,
                                    color: foreground,
                                  ),
                                ),
                              ),
                            ],
                          ),
                        ),
                      ),
                    ),
                  ),
                );
              },
            ),
          ],
          if (widget.explanation != null)
            Padding(
              padding: const EdgeInsets.fromLTRB(10, 7, 10, 3),
              child: Text(
                widget.explanation!,
                style: const TextStyle(
                  fontSize: 10,
                  height: 1.4,
                  color: FloePalette.neutral500,
                ),
              ),
            ),
        ],
      ),
    ),
  );
}
