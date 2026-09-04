import 'package:flutter/material.dart';

import 'design_tokens.dart';
import 'floe_motion.dart';
import 'floe_squircle.dart';

abstract final class FloeTheme {
  static final destructiveButtonStyle = ButtonStyle(
    animationDuration: FloeMotion.hoverDuration,
    backgroundColor: WidgetStateProperty.resolveWith((states) {
      if (states.contains(WidgetState.pressed)) return FloePalette.error800;
      if (states.contains(WidgetState.hovered) ||
          states.contains(WidgetState.focused)) {
        return FloePalette.error700;
      }
      return FloePalette.error600;
    }),
    foregroundColor: const WidgetStatePropertyAll(FloePalette.neutral0),
    overlayColor: const WidgetStatePropertyAll(Colors.transparent),
  );

  static ThemeData get light {
    final colorScheme = ColorScheme.fromSeed(
      seedColor: FloePalette.primary600,
      primary: FloePalette.primary600,
      surface: FloePalette.neutral0,
      error: FloePalette.error600,
    );
    return ThemeData(
      useMaterial3: true,
      fontFamily: 'Pretendard',
      splashFactory: NoSplash.splashFactory,
      splashColor: Colors.transparent,
      highlightColor: Colors.transparent,
      hoverColor: Colors.transparent,
      focusColor: FloePalette.primary50,
      colorScheme: colorScheme,
      scaffoldBackgroundColor: FloePalette.neutral25,
      scrollbarTheme: ScrollbarThemeData(
        thickness: const WidgetStatePropertyAll(4),
        radius: const Radius.circular(8),
        thumbColor: WidgetStateProperty.resolveWith(
          (states) =>
              states.contains(WidgetState.hovered) ||
                  states.contains(WidgetState.dragged)
              ? FloePalette.primary400
              : FloePalette.primary200,
        ),
        trackVisibility: const WidgetStatePropertyAll(false),
      ),
      sliderTheme: const SliderThemeData(
        trackHeight: 4,
        activeTrackColor: FloePalette.primary200,
        inactiveTrackColor: FloePalette.primary100,
        activeTickMarkColor: Colors.transparent,
        inactiveTickMarkColor: Colors.transparent,
        thumbColor: FloePalette.primary600,
        thumbShape: RoundSliderThumbShape(enabledThumbRadius: 6),
        overlayShape: RoundSliderOverlayShape(overlayRadius: 12),
      ),
      dividerColor: FloePalette.neutral200,
      textTheme:
          const TextTheme(
            displayLarge: FloeType.displayLarge,
            headlineMedium: FloeType.display,
            headlineSmall: FloeType.headlineLarge,
            titleLarge: FloeType.headline,
            bodyLarge: FloeType.bodyLarge,
            bodyMedium: FloeType.body,
            labelLarge: TextStyle(
              fontSize: 14,
              height: 1.35,
              fontWeight: FontWeight.w600,
            ),
            labelMedium: FloeType.label,
          ).apply(
            fontFamily: 'Pretendard',
            bodyColor: FloePalette.neutral950,
            displayColor: FloePalette.neutral950,
          ),
      inputDecorationTheme: InputDecorationTheme(
        filled: true,
        fillColor: FloePalette.neutral0,
        contentPadding: const EdgeInsets.symmetric(
          horizontal: FloeSpace.base,
          vertical: FloeSpace.md,
        ),
        border: _inputBorder(FloePalette.neutral200),
        enabledBorder: _inputBorder(FloePalette.neutral200),
        hoverColor: FloePalette.neutral50,
        focusedBorder: _inputBorder(FloePalette.primary600, width: 2),
        errorBorder: _inputBorder(FloePalette.error600),
        focusedErrorBorder: _inputBorder(FloePalette.error600, width: 2),
      ),
      filledButtonTheme: FilledButtonThemeData(
        style: ButtonStyle(
          mouseCursor: WidgetStateMouseCursor.clickable,
          tapTargetSize: MaterialTapTargetSize.shrinkWrap,
          textStyle: const WidgetStatePropertyAll(
            TextStyle(
              fontFamily: 'Pretendard',
              fontSize: 16,
              fontWeight: FontWeight.w600,
            ),
          ),
          animationDuration: FloeMotion.hoverDuration,
          backgroundColor: WidgetStateProperty.resolveWith(_filledBackground),
          foregroundColor: WidgetStateProperty.resolveWith(_filledForeground),
          overlayColor: const WidgetStatePropertyAll(Colors.transparent),
          minimumSize: const WidgetStatePropertyAll(Size(44, 44)),
          shape: WidgetStatePropertyAll(
            floeSquircleBorder(FloeSquircleSize.md),
          ),
        ),
      ),
      textButtonTheme: TextButtonThemeData(
        style: ButtonStyle(
          mouseCursor: WidgetStateMouseCursor.clickable,
          textStyle: const WidgetStatePropertyAll(
            TextStyle(
              fontFamily: 'Pretendard',
              fontSize: 14,
              fontWeight: FontWeight.w400,
            ),
          ),
          animationDuration: FloeMotion.hoverDuration,
          foregroundColor: WidgetStateProperty.resolveWith(_quietForeground),
          backgroundColor: WidgetStateProperty.resolveWith(_quietBackground),
          overlayColor: const WidgetStatePropertyAll(Colors.transparent),
          minimumSize: const WidgetStatePropertyAll(Size(44, 44)),
          shape: WidgetStatePropertyAll(
            floeSquircleBorder(FloeSquircleSize.md),
          ),
        ),
      ),
      iconButtonTheme: IconButtonThemeData(
        style: ButtonStyle(
          mouseCursor: WidgetStateMouseCursor.clickable,
          animationDuration: FloeMotion.hoverDuration,
          foregroundColor: WidgetStateProperty.resolveWith(_quietForeground),
          backgroundColor: WidgetStateProperty.resolveWith(_quietBackground),
          overlayColor: const WidgetStatePropertyAll(Colors.transparent),
          minimumSize: const WidgetStatePropertyAll(Size.square(44)),
          shape: WidgetStatePropertyAll(
            floeSquircleBorder(FloeSquircleSize.md),
          ),
        ),
      ),
      outlinedButtonTheme: OutlinedButtonThemeData(
        style: ButtonStyle(
          mouseCursor: WidgetStateMouseCursor.clickable,
          tapTargetSize: MaterialTapTargetSize.shrinkWrap,
          textStyle: const WidgetStatePropertyAll(
            TextStyle(
              fontFamily: 'Pretendard',
              fontSize: 16,
              fontWeight: FontWeight.w500,
            ),
          ),
          animationDuration: FloeMotion.hoverDuration,
          foregroundColor: WidgetStateProperty.resolveWith(_quietForeground),
          backgroundColor: WidgetStateProperty.resolveWith(_outlinedBackground),
          overlayColor: const WidgetStatePropertyAll(Colors.transparent),
          side: WidgetStateProperty.resolveWith(_outlinedSide),
          minimumSize: const WidgetStatePropertyAll(Size(44, 44)),
          shape: WidgetStatePropertyAll(
            floeSquircleBorder(FloeSquircleSize.md),
          ),
        ),
      ),
      segmentedButtonTheme: SegmentedButtonThemeData(
        style: ButtonStyle(
          mouseCursor: WidgetStateMouseCursor.clickable,
          animationDuration: FloeMotion.hoverDuration,
          foregroundColor: WidgetStateProperty.resolveWith(_segmentForeground),
          backgroundColor: WidgetStateProperty.resolveWith(_segmentBackground),
          overlayColor: const WidgetStatePropertyAll(Colors.transparent),
          side: WidgetStateProperty.resolveWith(_segmentSide),
          minimumSize: const WidgetStatePropertyAll(Size(44, 44)),
          shape: WidgetStatePropertyAll(
            floeSquircleBorder(FloeSquircleSize.md),
          ),
        ),
      ),
      checkboxTheme: CheckboxThemeData(
        shape: floeSquircleBorder(FloeSquircleSize.xs),
        side: const BorderSide(color: FloePalette.neutral300, width: 1.5),
        overlayColor: const WidgetStatePropertyAll(Colors.transparent),
      ),
      dialogTheme: DialogThemeData(
        backgroundColor: FloePalette.neutral0,
        surfaceTintColor: Colors.transparent,
        elevation: 0,
        shape: floeSquircleBorder(
          FloeSquircleSize.xl,
          borderColor: FloePalette.neutral200,
          borderWidth: 1,
        ),
      ),
    );
  }

  static OutlineInputBorder _inputBorder(Color color, {double width = 1}) =>
      OutlineInputBorder(
        borderRadius: BorderRadius.circular(FloeRadius.md),
        borderSide: BorderSide(color: color, width: width),
      );

  static Color _filledBackground(Set<WidgetState> states) {
    if (states.contains(WidgetState.disabled)) return FloePalette.primary200;
    if (states.contains(WidgetState.pressed)) return FloePalette.primary800;
    if (states.contains(WidgetState.hovered) ||
        states.contains(WidgetState.focused)) {
      return FloePalette.primary700;
    }
    return FloePalette.primary600;
  }

  static Color _filledForeground(Set<WidgetState> states) =>
      states.contains(WidgetState.disabled)
      ? FloePalette.neutral600
      : FloePalette.neutral0;

  static Color _quietForeground(Set<WidgetState> states) {
    if (states.contains(WidgetState.disabled)) return FloePalette.neutral400;
    if (states.contains(WidgetState.hovered) ||
        states.contains(WidgetState.pressed) ||
        states.contains(WidgetState.focused)) {
      return FloePalette.neutral950;
    }
    return FloePalette.neutral600;
  }

  static Color _quietBackground(Set<WidgetState> states) {
    if (states.contains(WidgetState.pressed)) return FloePalette.neutral200;
    if (states.contains(WidgetState.hovered)) return FloePalette.neutral100;
    if (states.contains(WidgetState.focused)) return FloePalette.primary50;
    return Colors.transparent;
  }

  static Color _outlinedBackground(Set<WidgetState> states) {
    if (states.contains(WidgetState.pressed)) return FloePalette.neutral100;
    if (states.contains(WidgetState.hovered)) return FloePalette.neutral50;
    if (states.contains(WidgetState.focused)) return FloePalette.primary50;
    return FloePalette.neutral0;
  }

  static BorderSide _outlinedSide(Set<WidgetState> states) => BorderSide(
    color: states.contains(WidgetState.focused)
        ? FloePalette.primary600
        : states.contains(WidgetState.hovered)
        ? FloePalette.neutral400
        : FloePalette.neutral300,
  );

  static Color _segmentForeground(Set<WidgetState> states) =>
      states.contains(WidgetState.selected)
      ? FloePalette.primary800
      : _quietForeground(states);

  static Color _segmentBackground(Set<WidgetState> states) {
    if (states.contains(WidgetState.selected)) {
      return states.contains(WidgetState.pressed)
          ? FloePalette.primary200
          : FloePalette.primary100;
    }
    return _quietBackground(states);
  }

  static BorderSide _segmentSide(Set<WidgetState> states) => BorderSide(
    color: states.contains(WidgetState.focused)
        ? FloePalette.primary600
        : FloePalette.neutral300,
  );
}
