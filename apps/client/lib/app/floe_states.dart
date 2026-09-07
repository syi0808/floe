import 'package:flutter/material.dart';

import 'design_tokens.dart';

abstract final class FloeStates {
  static Color filledBackground(Set<WidgetState> states) {
    if (states.contains(WidgetState.disabled)) return FloePalette.primary200;
    if (states.contains(WidgetState.pressed)) return FloePalette.primary800;
    if (states.contains(WidgetState.hovered) ||
        states.contains(WidgetState.focused)) {
      return FloePalette.primary700;
    }
    return FloePalette.primary600;
  }

  static Color filledForeground(Set<WidgetState> states) =>
      states.contains(WidgetState.disabled)
      ? FloeColor.textSecondary
      : FloeColor.surface;

  static Color quietForeground(Set<WidgetState> states) {
    if (states.contains(WidgetState.disabled)) {
      return FloeColor.disabledContent;
    }
    if (states.contains(WidgetState.hovered) ||
        states.contains(WidgetState.pressed) ||
        states.contains(WidgetState.focused)) {
      return FloeColor.textPrimary;
    }
    return FloeColor.textSecondary;
  }

  static Color quietBackground(Set<WidgetState> states) {
    if (states.contains(WidgetState.pressed)) {
      return FloeColor.neutralPressed;
    }
    if (states.contains(WidgetState.hovered)) return FloeColor.quietHover;
    if (states.contains(WidgetState.focused)) return FloeColor.selectionHover;
    return Colors.transparent;
  }

  static Color outlinedBackground(Set<WidgetState> states) {
    if (states.contains(WidgetState.pressed)) return FloeColor.quietHover;
    if (states.contains(WidgetState.hovered)) return FloeColor.neutralHover;
    if (states.contains(WidgetState.focused)) return FloeColor.selectionHover;
    return FloeColor.surface;
  }

  static BorderSide outlinedSide(Set<WidgetState> states) => BorderSide(
    color: states.contains(WidgetState.focused)
        ? FloeColor.focus
        : states.contains(WidgetState.hovered)
        ? FloeColor.borderStrong
        : FloeColor.border,
  );

  static Color segmentForeground(Set<WidgetState> states) =>
      states.contains(WidgetState.selected)
      ? FloePalette.primary800
      : quietForeground(states);

  static Color segmentBackground(Set<WidgetState> states) {
    if (states.contains(WidgetState.selected)) {
      return states.contains(WidgetState.pressed)
          ? FloeColor.selectionPressed
          : FloePalette.primary100;
    }
    return quietBackground(states);
  }

  static BorderSide segmentSide(Set<WidgetState> states) => BorderSide(
    color: states.contains(WidgetState.focused)
        ? FloeColor.focus
        : FloeColor.borderStrong,
  );
}
