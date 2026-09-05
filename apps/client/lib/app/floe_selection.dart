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
  Widget build(BuildContext context) => _FloeChoice(
    selected: value,
    enabled: onChanged != null,
    semanticLabel: semanticLabel,
    onActivate: onChanged == null ? null : () => onChanged!(!value),
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
  Widget build(BuildContext context) => _FloeChoice(
    selected: value,
    enabled: onChanged != null,
    title: title,
    onActivate: onChanged == null ? null : () => onChanged!(!value),
  );
}

class FloeRadioTile<T> extends StatefulWidget {
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
  State<FloeRadioTile<T>> createState() => _FloeRadioTileState<T>();
}

class _FloeRadioTileState<T> extends State<FloeRadioTile<T>>
    with RadioClient<T> {
  @override
  final FocusNode focusNode = FocusNode();

  @override
  bool get enabled => widget.enabled;

  @override
  T get radioValue => widget.value;

  @override
  bool get tristate => false;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    registry = RadioGroup.maybeOf<T>(context);
    assert(registry != null, 'FloeRadioTile must be inside a RadioGroup<$T>.');
  }

  @override
  void dispose() {
    registry = null;
    focusNode.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => _FloeChoice(
    selected: registry?.groupValue == widget.value,
    enabled: widget.enabled,
    radio: true,
    title: widget.title,
    focusNode: focusNode,
    onActivate: widget.enabled ? () => registry?.onChanged(widget.value) : null,
  );
}

class _FloeChoice extends StatefulWidget {
  const _FloeChoice({
    required this.selected,
    required this.enabled,
    this.onActivate,
    this.title,
    this.semanticLabel,
    this.focusNode,
    this.radio = false,
  });

  final bool selected;
  final bool enabled;
  final VoidCallback? onActivate;
  final Widget? title;
  final String? semanticLabel;
  final FocusNode? focusNode;
  final bool radio;

  @override
  State<_FloeChoice> createState() => _FloeChoiceState();
}

class _FloeChoiceState extends State<_FloeChoice> {
  FocusNode? internalFocusNode;
  FocusNode get focusNode =>
      widget.focusNode ?? (internalFocusNode ??= FocusNode());

  bool hovered = false;
  bool focused = false;

  @override
  void dispose() {
    internalFocusNode?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final visual = _FloeChoiceVisual(
      selected: widget.selected,
      enabled: widget.enabled,
      hovered: hovered,
      focused: focused,
      radio: widget.radio,
    );
    final content = widget.title == null
        ? SizedBox.square(dimension: 44, child: Center(child: visual))
        : ConstrainedBox(
            constraints: const BoxConstraints(minHeight: 48),
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
              child: Row(
                children: [
                  visual,
                  const SizedBox(width: 12),
                  Expanded(
                    child: DefaultTextStyle.merge(
                      style: TextStyle(
                        color: widget.enabled
                            ? FloePalette.neutral950
                            : FloePalette.neutral500,
                        fontSize: 13,
                        fontWeight: FontWeight.w500,
                        height: 1.5,
                      ),
                      child: widget.title!,
                    ),
                  ),
                ],
              ),
            ),
          );
    return Semantics(
      checked: widget.selected,
      enabled: widget.enabled,
      inMutuallyExclusiveGroup: widget.radio,
      label: widget.semanticLabel,
      onTap: widget.onActivate,
      child: FocusableActionDetector(
        enabled: widget.enabled,
        focusNode: focusNode,
        mouseCursor: widget.enabled
            ? SystemMouseCursors.click
            : SystemMouseCursors.forbidden,
        shortcuts: const {
          SingleActivator(LogicalKeyboardKey.space): ActivateIntent(),
          SingleActivator(LogicalKeyboardKey.enter): ActivateIntent(),
        },
        actions: {
          ActivateIntent: CallbackAction<ActivateIntent>(
            onInvoke: (_) {
              widget.onActivate?.call();
              return null;
            },
          ),
        },
        onShowHoverHighlight: (value) => setState(() => hovered = value),
        onShowFocusHighlight: (value) => setState(() => focused = value),
        child: GestureDetector(
          behavior: HitTestBehavior.opaque,
          onTap: widget.onActivate,
          child: AnimatedContainer(
            duration: FloeMotion.reduceMotion(context)
                ? Duration.zero
                : FloeMotion.hoverDuration,
            decoration: BoxDecoration(
              color: hovered && widget.enabled
                  ? FloePalette.primary50
                  : Colors.transparent,
              borderRadius: BorderRadius.circular(14),
            ),
            child: content,
          ),
        ),
      ),
    );
  }
}

class _FloeChoiceVisual extends StatelessWidget {
  const _FloeChoiceVisual({
    required this.selected,
    required this.enabled,
    required this.hovered,
    required this.focused,
    required this.radio,
  });

  final bool selected;
  final bool enabled;
  final bool hovered;
  final bool focused;
  final bool radio;

  @override
  Widget build(BuildContext context) {
    final reduced = FloeMotion.reduceMotion(context);
    final selectedColor = enabled
        ? FloePalette.primary600
        : FloePalette.neutral50;
    final borderColor = !enabled
        ? FloePalette.neutral200
        : selected || hovered
        ? FloePalette.primary600
        : FloePalette.neutral500;
    final markColor = enabled ? FloePalette.neutral0 : FloePalette.neutral300;
    final duration = reduced ? Duration.zero : FloeMotion.hoverDuration;
    return SizedBox.square(
      dimension: 20,
      child: Stack(
        clipBehavior: Clip.none,
        alignment: Alignment.center,
        children: [
          Positioned(
            left: -5,
            top: -5,
            width: 30,
            height: 30,
            child: AnimatedOpacity(
              opacity: focused && enabled ? 1 : 0,
              duration: duration,
              child: DecoratedBox(
                decoration: ShapeDecoration(
                  color: Colors.transparent,
                  shape: radio
                      ? const CircleBorder(
                          side: BorderSide(
                            color: FloePalette.primary600,
                            width: 2,
                          ),
                        )
                      : floeSquircleBorder(
                          FloeSquircleSize.sm,
                          borderColor: FloePalette.primary600,
                          borderWidth: 2,
                        ),
                ),
              ),
            ),
          ),
          AnimatedContainer(
            key: const ValueKey('floe-choice-visual'),
            width: 20,
            height: 20,
            duration: duration,
            curve: FloeMotion.easeOut,
            decoration: ShapeDecoration(
              color: selected ? selectedColor : FloePalette.neutral0,
              shadows: enabled
                  ? [
                      BoxShadow(
                        color: selected
                            ? FloePalette.primary600.withValues(alpha: .15)
                            : FloePalette.neutral950.withValues(alpha: .05),
                        offset: const Offset(0, 1),
                        blurRadius: 2,
                      ),
                    ]
                  : const [],
              shape: radio
                  ? CircleBorder(
                      side: BorderSide(color: borderColor, width: 1.5),
                    )
                  : floeSquircleBorder(
                      FloeSquircleSize.sm,
                      borderColor: borderColor,
                      borderWidth: 1.5,
                    ),
            ),
            child: AnimatedOpacity(
              opacity: selected ? 1 : 0,
              duration: reduced
                  ? Duration.zero
                  : const Duration(milliseconds: 120),
              child: Center(
                child: radio
                    ? Container(
                        width: 8,
                        height: 8,
                        decoration: BoxDecoration(
                          color: markColor,
                          shape: BoxShape.circle,
                        ),
                      )
                    : CustomPaint(
                        size: const Size.square(20),
                        painter: _FloeCheckPainter(markColor),
                      ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _FloeCheckPainter extends CustomPainter {
  const _FloeCheckPainter(this.color);

  final Color color;

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()
      ..color = color
      ..style = PaintingStyle.stroke
      ..strokeWidth = 2.2
      ..strokeCap = StrokeCap.round
      ..strokeJoin = StrokeJoin.round;
    final path = Path()
      ..moveTo(5, 10)
      ..lineTo(8.2, 13.2)
      ..lineTo(15, 6.5);
    canvas.drawPath(path, paint);
  }

  @override
  bool shouldRepaint(_FloeCheckPainter oldDelegate) =>
      color != oldDelegate.color;
}
