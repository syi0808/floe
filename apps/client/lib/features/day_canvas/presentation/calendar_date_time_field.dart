import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:intl/intl.dart';

import '../../../app/design_tokens.dart';
import '../../../app/floe_button.dart';
import '../../../app/floe_squircle.dart';
import '../../../app/floe_date_picker.dart';
import '../../../app/floe_popover.dart';

class CalendarDateTimeField extends StatelessWidget {
  const CalendarDateTimeField({
    super.key,
    required this.label,
    required this.value,
    required this.onChanged,
    this.enabled = true,
    this.validator,
  });

  final String label;
  final DateTime value;
  final ValueChanged<DateTime> onChanged;
  final bool enabled;
  final FormFieldValidator<DateTime>? validator;

  void changeTime(int hour, int minute) {
    final next = DateTime(value.year, value.month, value.day, hour, minute);
    if (next.hour == hour && next.minute == minute) onChanged(next);
  }

  @override
  Widget build(BuildContext context) => FormField<DateTime>(
    validator: (_) => validator?.call(value),
    builder: (field) => Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          label,
          style: const TextStyle(fontSize: 12, color: FloePalette.neutral600),
        ),
        const SizedBox(height: 6),
        FloeSquircle(
          padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 4),
          fill: FloePalette.neutral50,
          child: Row(
            children: [
              Expanded(
                child: Builder(
                  builder: (anchorContext) => FloeButton.text(
                    onPressed: !enabled
                        ? null
                        : () async {
                            final date = await showFloeDatePicker(
                              context: context,
                              anchor: floeAnchorRect(anchorContext),
                              initialDate: value,
                            );
                            if (date != null && context.mounted) {
                              final next = DateTime(
                                date.year,
                                date.month,
                                date.day,
                                value.hour,
                                value.minute,
                              );
                              if (next.hour == value.hour &&
                                  next.minute == value.minute) {
                                onChanged(next);
                              }
                            }
                          },
                    icon: const Icon(Icons.calendar_today_outlined, size: 16),
                    child: Text(DateFormat.yMMMd().format(value)),
                  ),
                ),
              ),
              _TimeSegment(
                label: '$label hour',
                value: value.hour,
                limit: 24,
                enabled: enabled,
                onChanged: (hour) => changeTime(hour, value.minute),
              ),
              const Text(':', style: TextStyle(fontWeight: FontWeight.w600)),
              _TimeSegment(
                label: '$label minute',
                value: value.minute,
                limit: 60,
                enabled: enabled,
                onChanged: (minute) => changeTime(value.hour, minute),
              ),
            ],
          ),
        ),
        if (field.hasError)
          Padding(
            padding: const EdgeInsets.only(top: 6),
            child: Text(
              field.errorText!,
              style: TextStyle(
                color: Theme.of(context).colorScheme.error,
                fontSize: 12,
              ),
            ),
          ),
      ],
    ),
  );
}

class _TimeSegment extends StatefulWidget {
  const _TimeSegment({
    required this.label,
    required this.value,
    required this.limit,
    required this.enabled,
    required this.onChanged,
  });
  final String label;
  final int value;
  final int limit;
  final bool enabled;
  final ValueChanged<int> onChanged;
  @override
  State<_TimeSegment> createState() => _TimeSegmentState();
}

class _TimeSegmentState extends State<_TimeSegment> {
  late final controller = TextEditingController(text: padded);
  final focus = FocusNode();
  String get padded => widget.value.toString().padLeft(2, '0');

  @override
  void initState() {
    super.initState();
    focus.addListener(() {
      if (focus.hasFocus) {
        controller.selection = TextSelection(
          baseOffset: 0,
          extentOffset: controller.text.length,
        );
      } else {
        controller.text = padded;
      }
    });
  }

  @override
  void didUpdateWidget(_TimeSegment oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.value != widget.value &&
        int.tryParse(controller.text) != widget.value) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (!mounted) return;
        controller.text = padded;
        if (focus.hasFocus) {
          controller.selection = const TextSelection(
            baseOffset: 0,
            extentOffset: 2,
          );
        }
      });
    }
  }

  void step(int amount) =>
      widget.onChanged((widget.value + amount) % widget.limit);

  @override
  void dispose() {
    controller.dispose();
    focus.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => SizedBox(
    width: 58,
    child: CallbackShortcuts(
      bindings: {
        const SingleActivator(LogicalKeyboardKey.arrowUp): () => step(1),
        const SingleActivator(LogicalKeyboardKey.arrowDown): () => step(-1),
      },
      child: Row(
        children: [
          Expanded(
            child: Semantics(
              label: widget.label,
              child: TextFormField(
                controller: controller,
                focusNode: focus,
                enabled: widget.enabled,
                textAlign: TextAlign.center,
                keyboardType: TextInputType.number,
                inputFormatters: [
                  FilteringTextInputFormatter.digitsOnly,
                  LengthLimitingTextInputFormatter(2),
                ],
                style: const TextStyle(
                  fontSize: 16,
                  fontWeight: FontWeight.w600,
                  fontFeatures: [FontFeature.tabularFigures()],
                ),
                decoration: InputDecoration(
                  isDense: true,
                  border: InputBorder.none,
                  contentPadding: const EdgeInsets.symmetric(
                    vertical: FloeSpace.md,
                  ),
                  hintText: '00',
                ),
                validator: (text) {
                  final number = int.tryParse(text ?? '');
                  return number == null ||
                          number >= widget.limit ||
                          number != widget.value
                      ? '—'
                      : null;
                },
                onChanged: (text) {
                  final number = int.tryParse(text);
                  if (number != null && number < widget.limit) {
                    widget.onChanged(number);
                  }
                },
              ),
            ),
          ),
          SizedBox(
            width: 18,
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                FloeButton.icon(
                  tooltip: 'Increase ${widget.label}',
                  onPressed: widget.enabled ? () => step(1) : null,
                  padding: EdgeInsets.zero,
                  constraints: const BoxConstraints.tightFor(
                    width: 18,
                    height: 18,
                  ),
                  icon: const Icon(Icons.keyboard_arrow_up, size: 16),
                ),
                FloeButton.icon(
                  tooltip: 'Decrease ${widget.label}',
                  onPressed: widget.enabled ? () => step(-1) : null,
                  padding: EdgeInsets.zero,
                  constraints: const BoxConstraints.tightFor(
                    width: 18,
                    height: 18,
                  ),
                  icon: const Icon(Icons.keyboard_arrow_down, size: 16),
                ),
              ],
            ),
          ),
        ],
      ),
    ),
  );
}
