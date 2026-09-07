import 'package:flutter/material.dart';

import 'design_tokens.dart';
import 'floe_motion.dart';
import 'floe_loading.dart';
import 'floe_squircle.dart';

enum _ButtonKind { filled, outlined, text, icon }

enum FloeButtonSize { compact, standard }

final class FloeButton extends StatelessWidget {
  const FloeButton.filled({
    required this.onPressed,
    required this.child,
    this.icon,
    this.style,
    this.focusNode,
    this.loading = false,
    super.key,
  }) : _kind = _ButtonKind.filled,
       tooltip = null,
       constraints = null,
       padding = null,
       size = FloeButtonSize.standard;

  const FloeButton.outlined({
    required this.onPressed,
    required this.child,
    this.icon,
    this.style,
    this.focusNode,
    this.loading = false,
    super.key,
  }) : _kind = _ButtonKind.outlined,
       tooltip = null,
       constraints = null,
       padding = null,
       size = FloeButtonSize.standard;

  const FloeButton.text({
    required this.onPressed,
    required this.child,
    this.icon,
    this.style,
    this.focusNode,
    this.loading = false,
    super.key,
  }) : _kind = _ButtonKind.text,
       tooltip = null,
       constraints = null,
       padding = null,
       size = FloeButtonSize.standard;

  const FloeButton.icon({
    required this.onPressed,
    required Widget icon,
    this.style,
    this.focusNode,
    this.tooltip,
    this.constraints,
    this.padding,
    this.size = FloeButtonSize.standard,
    this.loading = false,
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
  final FloeButtonSize size;
  final bool loading;

  Widget get _content => Stack(
    alignment: Alignment.center,
    children: [
      ExcludeSemantics(
        excluding: loading,
        child: Opacity(opacity: loading ? 0 : 1, child: child),
      ),
      if (loading) const FloeSpinner(size: 18),
    ],
  );

  BoxConstraints? get _iconConstraints =>
      constraints ??
      (size == FloeButtonSize.compact
          ? const BoxConstraints.tightFor(
              width: FloeControlSize.compact,
              height: FloeControlSize.compact,
            )
          : null);

  EdgeInsetsGeometry? get _iconPadding =>
      padding ?? (size == FloeButtonSize.compact ? EdgeInsets.zero : null);

  ButtonStyle? get _iconStyle {
    if (size == FloeButtonSize.standard) return style;
    return IconButton.styleFrom(
      shape: floeSquircleBorder(FloeSquircleSize.xs),
      tapTargetSize: MaterialTapTargetSize.shrinkWrap,
    ).merge(style);
  }

  @override
  Widget build(BuildContext context) => PressableScale(
    builder: (states) => switch (_kind) {
      _ButtonKind.filled =>
        icon == null
            ? FilledButton(
                onPressed: loading ? null : onPressed,
                style: style,
                focusNode: focusNode,
                statesController: states,
                child: _content,
              )
            : FilledButton.icon(
                onPressed: loading ? null : onPressed,
                style: style,
                focusNode: focusNode,
                statesController: states,
                icon: Opacity(opacity: loading ? 0 : 1, child: icon),
                label: _content,
              ),
      _ButtonKind.outlined =>
        icon == null
            ? OutlinedButton(
                onPressed: loading ? null : onPressed,
                style: style,
                focusNode: focusNode,
                statesController: states,
                child: _content,
              )
            : OutlinedButton.icon(
                onPressed: loading ? null : onPressed,
                style: style,
                focusNode: focusNode,
                statesController: states,
                icon: Opacity(opacity: loading ? 0 : 1, child: icon),
                label: _content,
              ),
      _ButtonKind.text =>
        icon == null
            ? TextButton(
                onPressed: loading ? null : onPressed,
                style: style,
                focusNode: focusNode,
                statesController: states,
                child: _content,
              )
            : TextButton.icon(
                onPressed: loading ? null : onPressed,
                style: style,
                focusNode: focusNode,
                statesController: states,
                icon: Opacity(opacity: loading ? 0 : 1, child: icon),
                label: _content,
              ),
      _ButtonKind.icon => IconButton(
        onPressed: loading ? null : onPressed,
        style: _iconStyle,
        focusNode: focusNode,
        statesController: states,
        tooltip: tooltip,
        constraints: _iconConstraints,
        padding: _iconPadding,
        icon: _content,
      ),
    },
  );
}
