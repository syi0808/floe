import 'package:flutter/material.dart';

import 'design_tokens.dart';
import 'floe_motion.dart';
import 'floe_squircle.dart';

final class FloeActionCard extends StatelessWidget {
  const FloeActionCard({
    super.key,
    required this.title,
    required this.description,
    required this.onPressed,
    this.leading,
    this.trailing,
    this.focusNode,
  });

  final Widget title;
  final Widget description;
  final VoidCallback? onPressed;
  final Widget? leading;
  final Widget? trailing;
  final FocusNode? focusNode;

  @override
  Widget build(BuildContext context) => PressableScale(
    scale: 0.985,
    builder: (states) => ListenableBuilder(
      listenable: states,
      builder: (context, _) {
        final enabled = onPressed != null;
        final hovered = states.value.contains(WidgetState.hovered);
        final pressed = states.value.contains(WidgetState.pressed);
        final fill = !enabled
            ? FloeColor.disabledSurface
            : pressed
            ? FloePalette.primary100
            : hovered
            ? FloePalette.primary50
            : FloeColor.surface;
        return FloeSquircle(
          size: FloeSquircleSize.lg,
          fill: fill,
          borderColor: hovered || pressed
              ? FloePalette.primary200
              : FloePalette.neutral200,
          child: InkWell(
            onTap: onPressed,
            focusNode: focusNode,
            statesController: states,
            child: Padding(
              padding: FloeControlInsets.card,
              child: Row(
                children: [
                  if (leading case final leading?) ...[
                    leading,
                    const SizedBox(width: FloeSpace.md),
                  ],
                  Expanded(
                    child: Column(
                      mainAxisSize: MainAxisSize.min,
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        DefaultTextStyle.merge(
                          style: FloeType.title.copyWith(
                            color: FloePalette.neutral900,
                            height: 1.3,
                          ),
                          child: title,
                        ),
                        const SizedBox(height: FloeSpace.xxs),
                        DefaultTextStyle.merge(
                          style: FloeType.bodySmall.copyWith(
                            color: FloePalette.neutral600,
                            fontSize: 12,
                            height: 1.4,
                          ),
                          child: description,
                        ),
                      ],
                    ),
                  ),
                  if (trailing case final trailing?) ...[
                    const SizedBox(width: FloeSpace.md),
                    IconTheme(
                      data: IconThemeData(
                        size: 18,
                        color: enabled
                            ? FloePalette.neutral500
                            : FloeColor.disabledContent,
                      ),
                      child: trailing,
                    ),
                  ],
                ],
              ),
            ),
          ),
        );
      },
    ),
  );
}
