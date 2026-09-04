import 'package:flutter/material.dart';

import 'floe_motion.dart';

enum _ButtonKind { filled, outlined, text, icon }

final class FloeButton extends StatelessWidget {
  const FloeButton.filled({
    required this.onPressed,
    required this.child,
    this.icon,
    this.style,
    this.focusNode,
    super.key,
  }) : _kind = _ButtonKind.filled,
       tooltip = null,
       constraints = null,
       padding = null;

  const FloeButton.outlined({
    required this.onPressed,
    required this.child,
    this.icon,
    this.style,
    this.focusNode,
    super.key,
  }) : _kind = _ButtonKind.outlined,
       tooltip = null,
       constraints = null,
       padding = null;

  const FloeButton.text({
    required this.onPressed,
    required this.child,
    this.icon,
    this.style,
    this.focusNode,
    super.key,
  }) : _kind = _ButtonKind.text,
       tooltip = null,
       constraints = null,
       padding = null;

  const FloeButton.icon({
    required this.onPressed,
    required Widget icon,
    this.style,
    this.focusNode,
    this.tooltip,
    this.constraints,
    this.padding,
    super.key,
  }) : _kind = _ButtonKind.icon,
       child = icon,
       icon = null;

  final _ButtonKind _kind;
  final VoidCallback? onPressed;
  final Widget child;
  final Widget? icon;
  final ButtonStyle? style;
  final FocusNode? focusNode;
  final String? tooltip;
  final BoxConstraints? constraints;
  final EdgeInsetsGeometry? padding;

  @override
  Widget build(BuildContext context) => PressableScale(
    builder: (states) => switch (_kind) {
      _ButtonKind.filled =>
        icon == null
            ? FilledButton(
                onPressed: onPressed,
                style: style,
                focusNode: focusNode,
                statesController: states,
                child: child,
              )
            : FilledButton.icon(
                onPressed: onPressed,
                style: style,
                focusNode: focusNode,
                statesController: states,
                icon: icon,
                label: child,
              ),
      _ButtonKind.outlined =>
        icon == null
            ? OutlinedButton(
                onPressed: onPressed,
                style: style,
                focusNode: focusNode,
                statesController: states,
                child: child,
              )
            : OutlinedButton.icon(
                onPressed: onPressed,
                style: style,
                focusNode: focusNode,
                statesController: states,
                icon: icon,
                label: child,
              ),
      _ButtonKind.text =>
        icon == null
            ? TextButton(
                onPressed: onPressed,
                style: style,
                focusNode: focusNode,
                statesController: states,
                child: child,
              )
            : TextButton.icon(
                onPressed: onPressed,
                style: style,
                focusNode: focusNode,
                statesController: states,
                icon: icon,
                label: child,
              ),
      _ButtonKind.icon => IconButton(
        onPressed: onPressed,
        style: style,
        focusNode: focusNode,
        statesController: states,
        tooltip: tooltip,
        constraints: constraints,
        padding: padding,
        icon: child,
      ),
    },
  );
}
