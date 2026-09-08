import 'package:flutter/material.dart';
import 'package:intl/intl.dart';

import '../../../app/design_tokens.dart';
import '../../../app/floe_button.dart';
import '../../../app/floe_date_picker.dart';
import '../../../app/floe_field.dart';
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

  static final fieldButtonStyle = TextButton.styleFrom(
    alignment: Alignment.centerLeft,
    padding: EdgeInsets.zero,
    minimumSize: const Size(0, 32),
    tapTargetSize: MaterialTapTargetSize.shrinkWrap,
  );

  void changeTime(int hour, int minute) {
    final next = DateTime(value.year, value.month, value.day, hour, minute);
    if (next.hour == hour && next.minute == minute) onChanged(next);
  }

  @override
  Widget build(BuildContext context) => FormField<DateTime>(
    validator: (_) => validator?.call(value),
    builder: (field) => InputDecorator(
      key: ValueKey('$label-${value.toIso8601String()}'),
      decoration: FloeField.decoration(
        label: label,
        errorText: field.errorText,
        enabled: enabled,
      ),
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
                            field.didChange(next);
                            onChanged(next);
                          }
                        }
                      },
                style: fieldButtonStyle,
                icon: const Icon(Icons.calendar_today_outlined, size: 16),
                child: Text(DateFormat.yMMMd().format(value)),
              ),
            ),
          ),
          const SizedBox(
            height: 24,
            child: VerticalDivider(width: FloeSpace.lg),
          ),
          FloeTimePickerButton(
            semanticLabel: '$label time',
            value: TimeOfDay.fromDateTime(value),
            enabled: enabled,
            style: fieldButtonStyle,
            onChanged: (time) {
              changeTime(time.hour, time.minute);
              field.didChange(
                DateTime(
                  value.year,
                  value.month,
                  value.day,
                  time.hour,
                  time.minute,
                ),
              );
            },
          ),
        ],
      ),
    ),
  );
}
