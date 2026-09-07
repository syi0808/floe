import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'design_tokens.dart';
import 'floe_motion.dart';

class FloeSwitch extends StatefulWidget {
  const FloeSwitch({
    super.key,
    required this.value,
    required this.onChanged,
    this.label,
  });

  final bool value;
  final ValueChanged<bool>? onChanged;
  final Widget? label;

  @override
  State<FloeSwitch> createState() => _FloeSwitchState();
}

class _FloeSwitchState extends State<FloeSwitch> {
  final focusNode = FocusNode();
  bool hovered = false;
  bool focused = false;
  bool pressed = false;

  bool get enabled => widget.onChanged != null;

  void toggle() {
    if (enabled) widget.onChanged!(!widget.value);
  }

  @override
  void dispose() {
    focusNode.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final reduceMotion = FloeMotion.reduceMotion(context);
    final duration = reduceMotion
        ? Duration.zero
        : FloeMotion.selectionDuration;
    final active = widget.value;
    final track = !enabled
        ? FloeColor.disabledSurface
        : active
        ? hovered
              ? FloePalette.primary700
              : FloePalette.primary600
        : hovered
        ? FloePalette.neutral300
        : FloePalette.neutral200;
    final control = AnimatedContainer(
      duration: duration,
      curve: FloeMotion.easeOut,
      width: 42,
      height: 24,
      padding: const EdgeInsets.all(3),
      decoration: BoxDecoration(
        color: track,
        borderRadius: BorderRadius.circular(999),
        boxShadow: focused && enabled
            ? const [BoxShadow(color: FloePalette.primary300, spreadRadius: 2)]
            : null,
      ),
      child: AnimatedAlign(
        duration: duration,
        curve: FloeMotion.easeOut,
        alignment: active ? Alignment.centerRight : Alignment.centerLeft,
        child: AnimatedContainer(
          duration: reduceMotion ? Duration.zero : FloeMotion.pressDuration,
          curve: FloeMotion.easeOut,
          width: pressed && enabled ? 20 : 18,
          height: pressed && enabled ? 16 : 18,
          decoration: BoxDecoration(
            color: enabled ? Colors.white : FloePalette.neutral300,
            shape: BoxShape.circle,
            boxShadow: enabled
                ? const [
                    BoxShadow(
                      color: Color(0x26000000),
                      blurRadius: 3,
                      offset: Offset(0, 1),
                    ),
                  ]
                : null,
          ),
        ),
      ),
    );
    return Semantics(
      toggled: widget.value,
      enabled: enabled,
      button: true,
      onTap: enabled ? toggle : null,
      child: FocusableActionDetector(
        focusNode: focusNode,
        enabled: enabled,
        mouseCursor: enabled
            ? SystemMouseCursors.click
            : SystemMouseCursors.basic,
        shortcuts: const {
          SingleActivator(LogicalKeyboardKey.space): ActivateIntent(),
          SingleActivator(LogicalKeyboardKey.enter): ActivateIntent(),
        },
        actions: {ActivateIntent: CallbackAction(onInvoke: (_) => toggle())},
        onShowFocusHighlight: (value) => setState(() => focused = value),
        onShowHoverHighlight: (value) => setState(() => hovered = value),
        child: GestureDetector(
          behavior: HitTestBehavior.opaque,
          onTap: enabled ? toggle : null,
          onTapDown: enabled
              ? (_) {
                  focusNode.requestFocus();
                  setState(() => pressed = true);
                }
              : null,
          onTapUp: enabled ? (_) => setState(() => pressed = false) : null,
          onTapCancel: enabled ? () => setState(() => pressed = false) : null,
          child: ConstrainedBox(
            constraints: const BoxConstraints(
              minHeight: FloeControlSize.standard,
            ),
            child: Row(
              children: [
                if (widget.label != null) Expanded(child: widget.label!),
                if (widget.label != null) const SizedBox(width: FloeSpace.md),
                control,
              ],
            ),
          ),
        ),
      ),
    );
  }
}
