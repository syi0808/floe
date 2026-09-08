import 'package:flutter/material.dart';

import 'design_tokens.dart';
import 'floe_squircle.dart';

enum FloeBadgeTone { neutral, info, success, warning, danger }

final class FloeBadge extends StatelessWidget {
  const FloeBadge({
    required this.label,
    this.tone = FloeBadgeTone.neutral,
    this.icon,
    this.compact = false,
    super.key,
  });

  final String label;
  final FloeBadgeTone tone;
  final IconData? icon;
  final bool compact;

  @override
  Widget build(BuildContext context) {
    final (background, foreground, border) = switch (tone) {
      FloeBadgeTone.neutral => (
        FloePalette.neutral50,
        FloePalette.neutral600,
        FloePalette.neutral200,
      ),
      FloeBadgeTone.info => (
        FloePalette.primary50,
        FloePalette.primary700,
        FloePalette.primary200,
      ),
      FloeBadgeTone.success => (
        FloePalette.mint50,
        FloePalette.mint700,
        FloePalette.mint300,
      ),
      FloeBadgeTone.warning => (
        FloePalette.amber50,
        FloePalette.amber700,
        FloePalette.amber300,
      ),
      FloeBadgeTone.danger => (
        FloePalette.coral50,
        FloePalette.coral700,
        FloePalette.coral300,
      ),
    };
    return Semantics(
      label: label,
      child: ExcludeSemantics(
        child: FloeSquircle(
          size: FloeSquircleSize.xs,
          fill: background,
          borderColor: border,
          padding: EdgeInsets.symmetric(
            horizontal: compact ? FloeSpace.sm : 10,
            vertical: compact ? FloeSpace.xxs : FloeSpace.xs,
          ),
          child: icon == null
              ? Text(
                  label,
                  softWrap: true,
                  style: (compact ? FloeType.micro : FloeType.label).copyWith(
                    color: foreground,
                  ),
                )
              : Wrap(
                  crossAxisAlignment: WrapCrossAlignment.center,
                  spacing: FloeSpace.xs,
                  children: [
                    Icon(icon, size: compact ? 11 : 13, color: foreground),
                    Text(
                      label,
                      style: (compact ? FloeType.micro : FloeType.label)
                          .copyWith(color: foreground),
                    ),
                  ],
                ),
        ),
      ),
    );
  }
}
