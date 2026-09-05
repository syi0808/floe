import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import 'design_tokens.dart';
import 'floe_motion.dart';
import 'floe_squircle.dart';

@immutable
class FloeSelectOption<T> {
  const FloeSelectOption({
    required this.value,
    required this.label,
    this.description,
    this.enabled = true,
  });

  final T value;
  final String label;
  final String? description;
  final bool enabled;
}

class FloeSelect<T> extends StatelessWidget {
  const FloeSelect({
    required this.label,
    required this.value,
    required this.options,
    required this.onChanged,
    this.enabled = true,
    this.placeholder = 'Choose an option',
    this.description,
    this.validator,
    this.autovalidateMode,
    super.key,
  });

  final String label;
  final T? value;
  final List<FloeSelectOption<T>> options;
  final ValueChanged<T?> onChanged;
  final bool enabled;
  final String placeholder;
  final String? description;
  final FormFieldValidator<T>? validator;
  final AutovalidateMode? autovalidateMode;

  @override
  Widget build(BuildContext context) => FormField<T>(
    key: ValueKey(value),
    initialValue: value,
    enabled: enabled,
    validator: validator,
    autovalidateMode: autovalidateMode,
    builder: (field) => _FloeSelectionAnchor<T>(
      label: label,
      value: field.value,
      options: options,
      enabled: enabled,
      placeholder: placeholder,
      description: description,
      errorText: field.errorText,
      onSelected: (nextValue) {
        field.didChange(nextValue);
        onChanged(nextValue);
      },
    ),
  );
}

class FloeDropdown<T> extends StatelessWidget {
  const FloeDropdown({
    required this.label,
    required this.items,
    required this.onSelected,
    this.enabled = true,
    this.icon,
    super.key,
  });

  final String label;
  final List<FloeSelectOption<T>> items;
  final ValueChanged<T> onSelected;
  final bool enabled;
  final Widget? icon;

  @override
  Widget build(BuildContext context) => _FloeSelectionAnchor<T>(
    label: label,
    options: items,
    enabled: enabled,
    menu: true,
    icon: icon,
    onSelected: onSelected,
  );
}

class _FloeSelectionAnchor<T> extends StatefulWidget {
  const _FloeSelectionAnchor({
    required this.label,
    required this.options,
    required this.enabled,
    required this.onSelected,
    this.value,
    this.placeholder,
    this.description,
    this.errorText,
    this.menu = false,
    this.icon,
  });

  final String label;
  final T? value;
  final List<FloeSelectOption<T>> options;
  final bool enabled;
  final ValueChanged<T> onSelected;
  final String? placeholder;
  final String? description;
  final String? errorText;
  final bool menu;
  final Widget? icon;

  @override
  State<_FloeSelectionAnchor<T>> createState() =>
      _FloeSelectionAnchorState<T>();
}

class _FloeSelectionAnchorState<T> extends State<_FloeSelectionAnchor<T>> {
  final controller = MenuController();
  final focusNode = FocusNode();
  bool open = false;

  FloeSelectOption<T>? get selected {
    for (final option in widget.options) {
      if (option.value == widget.value) return option;
    }
    return null;
  }

  @override
  void didUpdateWidget(_FloeSelectionAnchor<T> oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!widget.enabled && controller.isOpen) controller.close();
  }

  @override
  void dispose() {
    focusNode.dispose();
    super.dispose();
  }

  void toggle() {
    if (!widget.enabled) return;
    controller.isOpen ? controller.close() : controller.open();
  }

  @override
  Widget build(BuildContext context) => LayoutBuilder(
    builder: (context, constraints) {
      final availableWidth = MediaQuery.sizeOf(context).width - 24;
      final anchorWidth = constraints.hasBoundedWidth
          ? constraints.maxWidth
          : 240.0;
      final menuWidth = math.min(math.max(anchorWidth, 240.0), availableWidth);
      final trigger = _FloeSelectionTrigger(
        label: widget.label,
        value: widget.menu
            ? widget.label
            : selected?.label ?? widget.placeholder ?? '',
        enabled: widget.enabled,
        open: open,
        focusNode: focusNode,
        icon: widget.icon,
        onPressed: toggle,
      );
      return MenuAnchor(
        controller: controller,
        childFocusNode: focusNode,
        animated: !FloeMotion.reduceMotion(context),
        alignmentOffset: const Offset(0, 6),
        style: MenuStyle(
          backgroundColor: const WidgetStatePropertyAll(FloePalette.neutral0),
          surfaceTintColor: const WidgetStatePropertyAll(Colors.transparent),
          shadowColor: WidgetStatePropertyAll(
            FloePalette.neutral950.withValues(alpha: .08),
          ),
          elevation: const WidgetStatePropertyAll(8),
          padding: const WidgetStatePropertyAll(EdgeInsets.all(5)),
          fixedSize: WidgetStatePropertyAll(Size.fromWidth(menuWidth)),
          maximumSize: const WidgetStatePropertyAll(Size.fromHeight(320)),
          side: const WidgetStatePropertyAll(
            BorderSide(color: FloePalette.neutral200),
          ),
          shape: WidgetStatePropertyAll(
            floeSquircleBorder(FloeSquircleSize.md),
          ),
          alignment: AlignmentDirectional.topStart,
        ),
        onOpen: () => setState(() => open = true),
        onClose: () => setState(() => open = false),
        menuChildren: [
          if (widget.options.isEmpty)
            const Padding(
              padding: EdgeInsets.all(12),
              child: Text(
                'No options available',
                style: TextStyle(color: FloePalette.neutral600),
              ),
            ),
          for (final option in widget.options)
            MenuItemButton(
              semanticsLabel: option.label,
              style: _optionStyle(option.enabled),
              trailingIcon: !widget.menu && option.value == widget.value
                  ? const Icon(
                      LucideIcons.check,
                      size: 16,
                      color: FloePalette.primary600,
                    )
                  : null,
              onPressed: option.enabled
                  ? () => widget.onSelected(option.value)
                  : null,
              child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    option.label,
                    style: const TextStyle(fontSize: 14, height: 1.35),
                  ),
                  if (option.description != null) ...[
                    const SizedBox(height: 3),
                    Text(
                      option.description!,
                      style: const TextStyle(
                        color: FloePalette.neutral600,
                        fontSize: 12,
                        height: 1.4,
                      ),
                    ),
                  ],
                ],
              ),
            ),
        ],
        builder: (context, controller, child) => widget.menu
            ? widget.icon == null
                  ? trigger
                  : Tooltip(message: widget.label, child: trigger)
            : Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Text(
                    widget.label,
                    style: const TextStyle(
                      color: FloePalette.neutral950,
                      fontSize: 13,
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                  const SizedBox(height: 8),
                  trigger,
                  if (widget.description != null) ...[
                    const SizedBox(height: 8),
                    Text(
                      widget.description!,
                      style: const TextStyle(
                        color: FloePalette.neutral600,
                        fontSize: 12,
                        height: 1.5,
                      ),
                    ),
                  ],
                  if (widget.errorText != null) ...[
                    const SizedBox(height: 8),
                    Text(
                      widget.errorText!,
                      style: const TextStyle(
                        color: FloePalette.error600,
                        fontSize: 12,
                        height: 1.5,
                      ),
                    ),
                  ],
                ],
              ),
      );
    },
  );

  ButtonStyle _optionStyle(bool enabled) => ButtonStyle(
    animationDuration: FloeMotion.hoverDuration,
    foregroundColor: WidgetStateProperty.resolveWith(
      (states) => enabled ? FloePalette.neutral950 : FloePalette.neutral400,
    ),
    backgroundColor: WidgetStateProperty.resolveWith(
      (states) =>
          states.contains(WidgetState.hovered) ||
              states.contains(WidgetState.focused)
          ? FloePalette.primary50
          : Colors.transparent,
    ),
    overlayColor: const WidgetStatePropertyAll(Colors.transparent),
    side: WidgetStateProperty.resolveWith(
      (states) =>
          states.contains(WidgetState.focused) ||
              states.contains(WidgetState.hovered)
          ? const BorderSide(color: FloePalette.primary600, width: 2)
          : const BorderSide(color: Colors.transparent, width: 2),
    ),
    shape: WidgetStatePropertyAll(floeSquircleBorder(FloeSquircleSize.sm)),
    padding: const WidgetStatePropertyAll(EdgeInsets.all(10)),
    minimumSize: const WidgetStatePropertyAll(Size(44, 44)),
    mouseCursor: WidgetStatePropertyAll(
      enabled ? SystemMouseCursors.click : SystemMouseCursors.forbidden,
    ),
  );
}

class _FloeSelectionTrigger extends StatefulWidget {
  const _FloeSelectionTrigger({
    required this.value,
    required this.enabled,
    required this.open,
    required this.focusNode,
    required this.onPressed,
    this.label,
    this.icon,
  });

  final String? label;
  final String value;
  final bool enabled;
  final bool open;
  final FocusNode focusNode;
  final VoidCallback onPressed;
  final Widget? icon;

  @override
  State<_FloeSelectionTrigger> createState() => _FloeSelectionTriggerState();
}

class _FloeSelectionTriggerState extends State<_FloeSelectionTrigger> {
  bool hovered = false;
  bool focused = false;
  bool pressed = false;

  @override
  Widget build(BuildContext context) {
    final reduced = FloeMotion.reduceMotion(context);
    final highlighted = widget.open || hovered;
    final iconOnly = widget.icon != null;
    return Semantics(
      button: true,
      enabled: widget.enabled,
      expanded: widget.open,
      label: widget.label,
      value: widget.value,
      child: MouseRegion(
        cursor: widget.enabled
            ? SystemMouseCursors.click
            : SystemMouseCursors.forbidden,
        onEnter: (_) => setState(() => hovered = true),
        onExit: (_) => setState(() => hovered = false),
        child: Focus(
          focusNode: widget.focusNode,
          onFocusChange: (value) => setState(() => focused = value),
          onKeyEvent: (node, event) {
            if (event is KeyDownEvent &&
                (event.logicalKey == LogicalKeyboardKey.enter ||
                    event.logicalKey == LogicalKeyboardKey.space ||
                    event.logicalKey == LogicalKeyboardKey.arrowDown ||
                    event.logicalKey == LogicalKeyboardKey.arrowUp)) {
              widget.onPressed();
              return KeyEventResult.handled;
            }
            return KeyEventResult.ignored;
          },
          child: Listener(
            onPointerDown: (_) => setState(() => pressed = widget.enabled),
            onPointerUp: (_) => setState(() => pressed = false),
            onPointerCancel: (_) => setState(() => pressed = false),
            child: AnimatedScale(
              scale: pressed && !reduced ? .985 : 1,
              duration: reduced
                  ? Duration.zero
                  : const Duration(milliseconds: 120),
              curve: FloeMotion.easeOut,
              child: AnimatedContainer(
                duration: reduced ? Duration.zero : FloeMotion.hoverDuration,
                constraints: BoxConstraints(
                  minWidth: iconOnly ? 44 : 0,
                  minHeight: 44,
                ),
                decoration: ShapeDecoration(
                  color: highlighted
                      ? FloePalette.neutral50
                      : FloePalette.neutral0,
                  shape: floeSquircleBorder(
                    FloeSquircleSize.md,
                    borderColor: focused
                        ? FloePalette.primary600
                        : highlighted
                        ? FloePalette.primary500
                        : FloePalette.neutral300,
                    borderWidth: focused ? 2 : 1,
                  ),
                ),
                child: Material(
                  color: Colors.transparent,
                  shape: floeSquircleBorder(FloeSquircleSize.md),
                  clipBehavior: Clip.antiAlias,
                  child: InkWell(
                    onTap: widget.enabled ? widget.onPressed : null,
                    overlayColor: const WidgetStatePropertyAll(
                      Colors.transparent,
                    ),
                    child: Padding(
                      padding: iconOnly
                          ? const EdgeInsets.all(11)
                          : const EdgeInsets.symmetric(
                              horizontal: 14,
                              vertical: 10,
                            ),
                      child: iconOnly
                          ? widget.icon
                          : Row(
                              children: [
                                Expanded(
                                  child: Text(
                                    widget.value,
                                    maxLines: 1,
                                    overflow: TextOverflow.ellipsis,
                                    style: TextStyle(
                                      color: widget.enabled
                                          ? FloePalette.neutral950
                                          : FloePalette.neutral500,
                                      fontSize: 14,
                                    ),
                                  ),
                                ),
                                const SizedBox(width: 16),
                                const Icon(
                                  LucideIcons.chevronDown,
                                  size: 16,
                                  color: FloePalette.neutral600,
                                ),
                              ],
                            ),
                    ),
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

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
  bool focused = false;

  @override
  Widget build(BuildContext context) => Focus(
    canRequestFocus: false,
    onFocusChange: (value) => setState(() => focused = value),
    child: MouseRegion(
      cursor: widget.enabled
          ? SystemMouseCursors.click
          : SystemMouseCursors.forbidden,
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
  );
}
