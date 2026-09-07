import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:intl/intl.dart';

import 'design_tokens.dart';
import 'floe_button.dart';
import 'floe_motion.dart';
import 'floe_popover.dart';

Future<DateTime?> showFloeDatePicker({
  required BuildContext context,
  required Rect anchor,
  required DateTime initialDate,
}) => showFloePopover<DateTime>(
  context: context,
  anchor: anchor,
  width: 300,
  height: 386,
  builder: (_) => FloeDatePicker(initialDate: initialDate),
);

class FloeDatePicker extends StatefulWidget {
  const FloeDatePicker({super.key, required this.initialDate});
  final DateTime initialDate;
  @override
  State<FloeDatePicker> createState() => _FloeDatePickerState();
}

class _FloeDatePickerState extends State<FloeDatePicker> {
  late DateTime active = DateUtils.dateOnly(widget.initialDate);
  late DateTime month = DateTime(active.year, active.month);
  final gridFocus = FocusNode();
  int direction = 1;

  @override
  void dispose() {
    gridFocus.dispose();
    super.dispose();
  }

  void setActive(DateTime next) {
    if (next.year < 1900 || next.year > 2200) return;
    setState(() {
      direction = next.isBefore(active) ? -1 : 1;
      active = next;
      month = DateTime(next.year, next.month);
    });
  }

  void stepMonth(int step) {
    final target = DateTime(month.year, month.month + step);
    final days = DateUtils.getDaysInMonth(target.year, target.month);
    setActive(DateTime(target.year, target.month, active.day.clamp(1, days)));
  }

  KeyEventResult navigate(FocusNode node, KeyEvent event) {
    if (event is! KeyDownEvent && event is! KeyRepeatEvent) {
      return KeyEventResult.ignored;
    }
    final key = event.logicalKey;
    final step = switch (key) {
      LogicalKeyboardKey.arrowLeft => -1,
      LogicalKeyboardKey.arrowRight => 1,
      LogicalKeyboardKey.arrowUp => -7,
      LogicalKeyboardKey.arrowDown => 7,
      _ => 0,
    };
    if (step != 0) {
      setActive(DateTime(active.year, active.month, active.day + step));
    } else if (key == LogicalKeyboardKey.pageUp ||
        key == LogicalKeyboardKey.pageDown) {
      stepMonth(key == LogicalKeyboardKey.pageUp ? -1 : 1);
    } else if (key == LogicalKeyboardKey.enter ||
        key == LogicalKeyboardKey.space) {
      Navigator.of(context).pop(active);
    } else {
      return KeyEventResult.ignored;
    }
    return KeyEventResult.handled;
  }

  @override
  Widget build(BuildContext context) {
    final today = DateUtils.dateOnly(DateTime.now());
    final locale = Localizations.localeOf(context).toLanguageTag();
    final localizations = MaterialLocalizations.of(context);
    final firstWeekday = localizations.firstDayOfWeekIndex;
    final leading = (month.weekday % 7 - firstWeekday + 7) % 7;
    return SingleChildScrollView(
      child: Padding(
        padding: const EdgeInsets.all(FloeSpace.md),
        child: Column(
          children: [
            Row(
              children: [
                Expanded(
                  child: Semantics(
                    liveRegion: true,
                    child: Text(
                      DateFormat.yMMMM(locale).format(month),
                      style: const TextStyle(
                        fontSize: 14,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                  ),
                ),
                FloeButton.icon(
                  tooltip: 'Previous month',
                  onPressed: month.year == 1900 && month.month == 1
                      ? null
                      : () => stepMonth(-1),
                  icon: const Icon(Icons.chevron_left, size: 18),
                  size: FloeButtonSize.compact,
                ),
                FloeButton.icon(
                  tooltip: 'Next month',
                  onPressed: month.year == 2200 && month.month == 12
                      ? null
                      : () => stepMonth(1),
                  icon: const Icon(Icons.chevron_right, size: 18),
                  size: FloeButtonSize.compact,
                ),
              ],
            ),
            const SizedBox(height: FloeSpace.sm),
            Row(
              children: [
                for (var weekday = 0; weekday < 7; weekday++)
                  Expanded(
                    child: Center(
                      child: Text(
                        localizations.narrowWeekdays[(weekday + firstWeekday) %
                            7],
                        style: const TextStyle(
                          fontSize: 11,
                          color: FloePalette.neutral500,
                        ),
                      ),
                    ),
                  ),
              ],
            ),
            const SizedBox(height: 6),
            Focus(
              focusNode: gridFocus,
              autofocus: true,
              onKeyEvent: navigate,
              child: AnimatedSwitcher(
                duration: FloeMotion.reduceMotion(context)
                    ? Duration.zero
                    : FloeMotion.selectionDuration,
                transitionBuilder: (child, animation) => FadeTransition(
                  opacity: animation,
                  child: FloeMotion.reduceMotion(context)
                      ? child
                      : SlideTransition(
                          position:
                              Tween<Offset>(
                                begin: Offset(.035 * direction, 0),
                                end: Offset.zero,
                              ).animate(
                                CurvedAnimation(
                                  parent: animation,
                                  curve: FloeMotion.easeOut,
                                ),
                              ),
                          child: child,
                        ),
                ),
                child: Column(
                  key: ValueKey(month),
                  children: [
                    for (var week = 0; week < 6; week++)
                      Row(
                        children: [
                          for (var weekday = 0; weekday < 7; weekday++)
                            Expanded(
                              child: Builder(
                                builder: (context) {
                                  final date = DateTime(
                                    month.year,
                                    month.month,
                                    1 - leading + week * 7 + weekday,
                                  );
                                  return _DateCell(
                                    date: date,
                                    label: DateFormat.yMMMMEEEEd(locale)
                                        .format(date),
                                    active: DateUtils.isSameDay(date, active),
                                    today: date == today,
                                    inMonth: date.month == month.month,
                                    enabled:
                                        date.year >= 1900 && date.year <= 2200,
                                    onSelected: () =>
                                        Navigator.of(context).pop(date),
                                  );
                                },
                              ),
                            ),
                        ],
                      ),
                  ],
                ),
              ),
            ),
            const SizedBox(height: FloeSpace.sm),
            const Divider(height: 1, color: FloePalette.neutral100),
            const SizedBox(height: 6),
            Row(
              children: [
                const Expanded(
                  child: Text(
                    '↑↓←→ to navigate',
                    style: TextStyle(
                      fontSize: 10,
                      color: FloePalette.neutral500,
                    ),
                  ),
                ),
                FloeButton.text(
                  onPressed: () => Navigator.of(context).pop(today),
                  child: const Text('Today'),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

class _DateCell extends StatefulWidget {
  const _DateCell({
    required this.date,
    required this.label,
    required this.active,
    required this.today,
    required this.inMonth,
    required this.enabled,
    required this.onSelected,
  });
  final DateTime date;
  final String label;
  final bool active;
  final bool today;
  final bool inMonth;
  final bool enabled;
  final VoidCallback onSelected;
  @override
  State<_DateCell> createState() => _DateCellState();
}

class _DateCellState extends State<_DateCell> {
  bool hovered = false;
  @override
  Widget build(BuildContext context) => Semantics(
    excludeSemantics: true,
    label: widget.label,
    button: true,
    selected: widget.active,
    enabled: widget.enabled,
    onTap: widget.enabled ? widget.onSelected : null,
    child: MouseRegion(
      cursor: widget.enabled
          ? SystemMouseCursors.click
          : SystemMouseCursors.basic,
      onEnter: (_) => setState(() => hovered = true),
      onExit: (_) => setState(() => hovered = false),
      child: PressableScale(
        builder: (states) => InkWell(
          statesController: states,
          excludeFromSemantics: true,
          onTap: widget.enabled ? widget.onSelected : null,
          overlayColor: const WidgetStatePropertyAll(Colors.transparent),
          child: AnimatedContainer(
            duration: FloeMotion.reduceMotion(context)
                ? Duration.zero
                : FloeMotion.hoverDuration,
            curve: FloeMotion.easeOut,
            height: FloeControlSize.compact,
            margin: const EdgeInsets.all(FloeSpace.xxs),
            decoration: BoxDecoration(
              borderRadius: BorderRadius.circular(FloeRadius.xs),
              color: widget.active
                  ? FloePalette.primary500
                  : hovered && widget.enabled
                  ? FloeColor.selectionHover
                  : Colors.transparent,
              border: widget.today
                  ? Border.all(color: FloePalette.primary400)
                  : null,
            ),
            child: Center(
              child: Text(
                '${widget.date.day}',
                style: TextStyle(
                  fontSize: 12,
                  fontWeight: widget.active || widget.today
                      ? FontWeight.w600
                      : FontWeight.w400,
                  color: widget.active
                      ? Colors.white
                      : widget.inMonth && widget.enabled
                      ? FloePalette.neutral950
                      : FloePalette.neutral400,
                ),
              ),
            ),
          ),
        ),
      ),
    ),
  );
}
