import 'package:flutter/material.dart';
import 'package:intl/intl.dart';

import '../../../app/design_tokens.dart';
import '../../../app/floe_button.dart';
import '../../../app/floe_squircle.dart';
import '../../../app/floe_date_picker.dart';
import '../../../app/floe_popover.dart';
import '../../../app/floe_time_picker.dart';

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
              FloeTimePickerButton(
                semanticLabel: '$label time',
                value: TimeOfDay.fromDateTime(value),
                enabled: enabled,
                onChanged: (time) => changeTime(time.hour, time.minute),
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
