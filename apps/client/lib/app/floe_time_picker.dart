import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'design_tokens.dart';
import 'floe_button.dart';
import 'floe_popover.dart';

Future<TimeOfDay?> showFloeTimePicker({
  required BuildContext context,
  required Rect anchor,
  required TimeOfDay initialTime,
}) => showFloePopover<TimeOfDay>(
  context: context,
  anchor: anchor,
  width: 280,
  height: 236,
  horizontalAnchor: FloePopoverHorizontalAnchor.center,
  builder: (_) => FloeTimePicker(initialTime: initialTime),
);

class FloeTimePicker extends StatefulWidget {
  const FloeTimePicker({super.key, required this.initialTime});

  final TimeOfDay initialTime;

  @override
  State<FloeTimePicker> createState() => _FloeTimePickerState();
}

class _FloeTimePickerState extends State<FloeTimePicker> {
  late TimeOfDay selected = widget.initialTime;

  @override
  Widget build(BuildContext context) {
    final use24HourFormat = MediaQuery.alwaysUse24HourFormatOf(context);
    return Column(
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(8, 6, 8, 4),
          child: Row(
            children: [
              FloeButton.text(
                onPressed: () => Navigator.of(context).pop(),
                child: const Text('Cancel'),
              ),
              Expanded(
                child: Semantics(
                  liveRegion: true,
                  child: Text(
                    MaterialLocalizations.of(context).formatTimeOfDay(
                      selected,
                      alwaysUse24HourFormat: use24HourFormat,
                    ),
                    textAlign: TextAlign.center,
                    style: const TextStyle(
                      fontSize: 13,
                      fontWeight: FontWeight.w600,
                      fontFeatures: [FontFeature.tabularFigures()],
                    ),
                  ),
                ),
              ),
              FloeButton.text(
                onPressed: () => Navigator.of(context).pop(selected),
                child: const Text('Done'),
              ),
            ],
          ),
        ),
        const Divider(height: 1, color: FloePalette.neutral100),
        Expanded(
          child: CupertinoTheme(
            data: CupertinoTheme.of(context).copyWith(
              textTheme: const CupertinoTextThemeData(
                dateTimePickerTextStyle: TextStyle(
                  color: FloePalette.neutral950,
                  fontSize: 21,
                  fontWeight: FontWeight.w500,
                  fontFeatures: [FontFeature.tabularFigures()],
                ),
              ),
            ),
            child: CupertinoDatePicker(
              mode: CupertinoDatePickerMode.time,
              initialDateTime: DateTime(
                2000,
                1,
                1,
                selected.hour,
                selected.minute,
              ),
              use24hFormat: use24HourFormat,
              showTimeSeparator: true,
              itemExtent: 34,
              backgroundColor: Colors.transparent,
              selectionOverlayBuilder:
                  (context, {required columnCount, required selectedIndex}) =>
                      CupertinoPickerDefaultSelectionOverlay(
                        background: FloeColor.selectionHover,
                        capStartEdge: selectedIndex == 0,
                        capEndEdge: selectedIndex == columnCount - 1,
                      ),
              onDateTimeChanged: (value) =>
                  setState(() => selected = TimeOfDay.fromDateTime(value)),
            ),
          ),
        ),
      ],
    );
  }
}

class FloeTimePickerButton extends StatelessWidget {
  const FloeTimePickerButton({
    super.key,
    required this.value,
    required this.onChanged,
    this.enabled = true,
    this.semanticLabel,
  });

  final TimeOfDay value;
  final ValueChanged<TimeOfDay> onChanged;
  final bool enabled;
  final String? semanticLabel;

  void step(int minutes) {
    final totalMinutes = (value.hour * 60 + value.minute + minutes) % 1440;
    onChanged(
      TimeOfDay(hour: totalMinutes ~/ 60, minute: totalMinutes.remainder(60)),
    );
  }

  @override
  Widget build(BuildContext context) {
    final formatted = MaterialLocalizations.of(context).formatTimeOfDay(
      value,
      alwaysUse24HourFormat: MediaQuery.alwaysUse24HourFormatOf(context),
    );
    return CallbackShortcuts(
      bindings: {
        const SingleActivator(LogicalKeyboardKey.arrowUp): () => step(1),
        const SingleActivator(LogicalKeyboardKey.arrowDown): () => step(-1),
      },
      child: Builder(
        builder: (anchorContext) => Semantics(
          label: semanticLabel,
          value: formatted,
          button: true,
          increasedValue: 'One minute later',
          decreasedValue: 'One minute earlier',
          onIncrease: enabled ? () => step(1) : null,
          onDecrease: enabled ? () => step(-1) : null,
          child: FloeButton.text(
            onPressed: !enabled
                ? null
                : () async {
                    final next = await showFloeTimePicker(
                      context: context,
                      anchor: floeAnchorRect(anchorContext),
                      initialTime: value,
                    );
                    if (next != null && context.mounted) onChanged(next);
                  },
            icon: const Icon(CupertinoIcons.clock, size: 17),
            child: Text(
              formatted,
              style: const TextStyle(
                fontFeatures: [FontFeature.tabularFigures()],
              ),
            ),
          ),
        ),
      ),
    );
  }
}
