import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'design_tokens.dart';
import 'floe_motion.dart';

class FloeCheckbox extends StatelessWidget {
  const FloeCheckbox({
    required this.value,
    required this.onChanged,
    this.semanticLabel,
    super.key,
  });

  final bool value;
  final ValueChanged<bool?>? onChanged;
  final String? semanticLabel;

  @override
  Widget build(BuildContext context) => _SelectionFeedback(
    enabled: onChanged != null,
    child: Checkbox(
      value: value,
      onChanged: onChanged,
      semanticLabel: semanticLabel,
    ),
  );
}

class FloeCheckboxTile extends StatelessWidget {
  const FloeCheckboxTile({
    required this.value,
    required this.title,
    required this.onChanged,
    super.key,
  });

  final bool value;
  final Widget title;
  final ValueChanged<bool?>? onChanged;

  @override
  Widget build(BuildContext context) => _SelectionFeedback(
    enabled: onChanged != null,
    child: CheckboxListTile(
      value: value,
      title: DefaultTextStyle.merge(
        style: TextStyle(
          color: onChanged == null
              ? FloePalette.neutral500
              : FloePalette.neutral950,
          fontSize: 13,
        ),
        child: title,
      ),
      onChanged: onChanged,
      controlAffinity: ListTileControlAffinity.leading,
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(14)),
      hoverColor: FloePalette.primary50,
      contentPadding: const EdgeInsets.symmetric(horizontal: 12),
    ),
  );
}

class FloeRadioTile<T> extends StatelessWidget {
  const FloeRadioTile({
    required this.value,
    required this.title,
    this.enabled = true,
    super.key,
  });

  final T value;
  final Widget title;
  final bool enabled;

  @override
  Widget build(BuildContext context) => _SelectionFeedback(
    enabled: enabled,
    child: RadioListTile<T>(
      value: value,
      enabled: enabled,
      title: DefaultTextStyle.merge(
        style: TextStyle(
          color: enabled ? FloePalette.neutral950 : FloePalette.neutral500,
          fontSize: 13,
        ),
        child: title,
      ),
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(14)),
      hoverColor: FloePalette.primary50,
      contentPadding: const EdgeInsets.symmetric(horizontal: 12),
    ),
  );
}

class _SelectionFeedback extends StatefulWidget {
  const _SelectionFeedback({required this.enabled, required this.child});

  final bool enabled;
  final Widget child;

  @override
  State<_SelectionFeedback> createState() => _SelectionFeedbackState();
}

class _SelectionFeedbackState extends State<_SelectionFeedback> {
  bool pressed = false;
  bool focused = false;

  void press(bool value) {
    if (pressed != value) setState(() => pressed = value);
  }

  @override
  void didUpdateWidget(_SelectionFeedback oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!widget.enabled) pressed = false;
  }

  @override
  Widget build(BuildContext context) => Focus(
    canRequestFocus: false,
    onFocusChange: (value) => setState(() {
      focused = value;
      if (!value) pressed = false;
    }),
    onKeyEvent: (node, event) {
      if (event.logicalKey == LogicalKeyboardKey.space ||
          event.logicalKey == LogicalKeyboardKey.enter) {
        press(widget.enabled && event is! KeyUpEvent);
      }
      return KeyEventResult.ignored;
    },
    child: MouseRegion(
      cursor: widget.enabled
          ? SystemMouseCursors.click
          : SystemMouseCursors.forbidden,
      child: Listener(
        onPointerDown: (_) => press(widget.enabled),
        onPointerUp: (_) => press(false),
        onPointerCancel: (_) => press(false),
        child: AnimatedScale(
          scale: pressed && widget.enabled && !FloeMotion.reduceMotion(context)
              ? .97
              : 1,
          duration: FloeMotion.reduceMotion(context)
              ? Duration.zero
              : const Duration(milliseconds: 120),
          curve: FloeMotion.easeOut,
          child: DecoratedBox(
            decoration: BoxDecoration(
              borderRadius: BorderRadius.circular(14),
              border: Border.all(
                color: focused && widget.enabled
                    ? FloePalette.primary600
                    : Colors.transparent,
                width: 2,
              ),
            ),
            child: widget.child,
          ),
        ),
      ),
    ),
  );
}
