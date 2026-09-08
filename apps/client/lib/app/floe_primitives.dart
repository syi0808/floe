import 'package:flutter/material.dart';

import 'design_tokens.dart';
import 'floe_button.dart';
import 'floe_squircle.dart';
import 'floe_switch.dart';

Future<T?> showFloeSheet<T>(BuildContext context, WidgetBuilder builder) =>
    showModalBottomSheet<T>(
      context: context,
      isScrollControlled: true,
      useSafeArea: true,
      backgroundColor: Colors.transparent,
      builder: builder,
    );

final class FloeDivider extends StatelessWidget {
  const FloeDivider({
    this.height = 1,
    this.indent,
    this.endIndent,
    this.color = FloeColor.border,
    super.key,
  });

  final double height;
  final double? indent;
  final double? endIndent;
  final Color color;

  @override
  Widget build(BuildContext context) => Divider(
    height: height,
    indent: indent,
    endIndent: endIndent,
    color: color,
  );
}

final class FloeTooltip extends StatelessWidget {
  const FloeTooltip({
    required this.message,
    required this.child,
    this.triggerMode,
    super.key,
  });

  final String message;
  final Widget child;
  final TooltipTriggerMode? triggerMode;

  @override
  Widget build(BuildContext context) =>
      Tooltip(message: message, triggerMode: triggerMode, child: child);
}

final class FloeSlider extends StatelessWidget {
  const FloeSlider({
    required this.value,
    required this.onChanged,
    this.min = 0,
    this.max = 1,
    this.divisions,
    this.semanticLabel,
    this.semanticFormatterCallback,
    super.key,
  });

  final double value;
  final ValueChanged<double>? onChanged;
  final double min;
  final double max;
  final int? divisions;
  final String? semanticLabel;
  final SemanticFormatterCallback? semanticFormatterCallback;

  @override
  Widget build(BuildContext context) => Semantics(
    label: semanticLabel,
    slider: true,
    child: Slider(
      value: value,
      onChanged: onChanged,
      min: min,
      max: max,
      divisions: divisions,
      semanticFormatterCallback: semanticFormatterCallback,
    ),
  );
}

final class FloeScaffold extends StatelessWidget {
  const FloeScaffold({
    required this.body,
    this.backgroundColor = FloeColor.canvas,
    super.key,
  });

  final Widget body;
  final Color backgroundColor;

  @override
  Widget build(BuildContext context) =>
      Scaffold(backgroundColor: backgroundColor, body: body);
}

final class FloeDialog extends StatelessWidget {
  const FloeDialog({
    required this.title,
    required this.content,
    this.actions = const [],
    this.maxWidth = 540,
    super.key,
  });

  final Widget title;
  final Widget content;
  final List<Widget> actions;
  final double maxWidth;

  @override
  Widget build(BuildContext context) => Dialog(
    constraints: BoxConstraints(maxWidth: maxWidth),
    insetPadding: const EdgeInsets.all(FloeSpace.lg),
    child: SingleChildScrollView(
      padding: FloeControlInsets.dialog,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          DefaultTextStyle.merge(style: FloeType.headline, child: title),
          const SizedBox(height: FloeSpace.base),
          DefaultTextStyle.merge(style: FloeType.body, child: content),
          if (actions.isNotEmpty) ...[
            const SizedBox(height: FloeSpace.lg),
            Wrap(
              alignment: WrapAlignment.end,
              spacing: FloeSpace.sm,
              runSpacing: FloeSpace.sm,
              children: actions,
            ),
          ],
        ],
      ),
    ),
  );
}

final class FloeDialogSurface extends StatelessWidget {
  const FloeDialogSurface({
    required this.child,
    this.maxWidth = 540,
    super.key,
  });

  final Widget child;
  final double maxWidth;

  @override
  Widget build(BuildContext context) => Dialog(
    constraints: BoxConstraints(maxWidth: maxWidth),
    insetPadding: const EdgeInsets.all(FloeSpace.lg),
    child: child,
  );
}

final class FloeSwitchTile extends StatelessWidget {
  const FloeSwitchTile({
    required this.value,
    required this.onChanged,
    required this.title,
    this.subtitle,
    super.key,
  });

  final bool value;
  final ValueChanged<bool>? onChanged;
  final String title;
  final String? subtitle;

  @override
  Widget build(BuildContext context) => FloeSwitch(
    value: value,
    onChanged: onChanged,
    label: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(title, style: FloeType.controlLabel),
        if (subtitle != null) ...[
          const SizedBox(height: FloeSpace.xxs),
          Text(subtitle!, style: FloeType.bodySmall),
        ],
      ],
    ),
  );
}

final class FloeListRow extends StatelessWidget {
  const FloeListRow({
    required this.title,
    this.subtitle,
    this.leading,
    this.trailing,
    this.onPressed,
    super.key,
  });

  final Widget title;
  final Widget? subtitle;
  final Widget? leading;
  final Widget? trailing;
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) => FloeSquircle(
    size: FloeSquircleSize.sm,
    fill: Colors.transparent,
    borderWidth: 0,
    child: FloeButton.text(
      onPressed: onPressed,
      style: const ButtonStyle(
        alignment: Alignment.centerLeft,
        padding: WidgetStatePropertyAll(EdgeInsets.all(FloeSpace.md)),
      ),
      child: Row(
        children: [
          if (leading != null) ...[
            leading!,
            const SizedBox(width: FloeSpace.md),
          ],
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                DefaultTextStyle.merge(
                  style: FloeType.controlLabel,
                  child: title,
                ),
                if (subtitle != null) ...[
                  const SizedBox(height: FloeSpace.xxs),
                  DefaultTextStyle.merge(
                    style: FloeType.bodySmall,
                    child: subtitle!,
                  ),
                ],
              ],
            ),
          ),
          if (trailing != null) ...[
            const SizedBox(width: FloeSpace.md),
            trailing!,
          ],
        ],
      ),
    ),
  );
}

final class FloePressable extends StatelessWidget {
  const FloePressable({
    required this.child,
    required this.onPressed,
    this.onDoubleTap,
    this.onHover,
    this.onFocusChange,
    this.fill = Colors.transparent,
    this.hoverFill = FloeColor.neutralHover,
    this.size = FloeSquircleSize.md,
    this.borderColor = Colors.transparent,
    this.borderWidth = 0,
    this.statesController,
    super.key,
  });

  final Widget child;
  final VoidCallback? onPressed;
  final VoidCallback? onDoubleTap;
  final ValueChanged<bool>? onHover;
  final ValueChanged<bool>? onFocusChange;
  final Color fill;
  final Color hoverFill;
  final FloeSquircleSize size;
  final Color borderColor;
  final double borderWidth;
  final WidgetStatesController? statesController;

  @override
  Widget build(BuildContext context) => FloeSquircle(
    size: size,
    fill: fill,
    borderColor: borderColor,
    borderWidth: borderWidth,
    child: InkWell(
      statesController: statesController,
      mouseCursor: onPressed == null
          ? SystemMouseCursors.basic
          : SystemMouseCursors.click,
      onTap: onPressed,
      onDoubleTap: onDoubleTap,
      onHover: onHover,
      onFocusChange: onFocusChange,
      hoverColor: hoverFill,
      customBorder: floeSquircleBorder(size),
      child: child,
    ),
  );
}
