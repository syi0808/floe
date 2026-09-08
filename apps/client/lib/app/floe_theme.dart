import 'package:flutter/material.dart';

import 'design_tokens.dart';
import 'floe_motion.dart';
import 'floe_squircle.dart';
import 'floe_states.dart';

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
      scaffoldBackgroundColor: FloeColor.canvas,
      tooltipTheme: const TooltipThemeData(waitDuration: Duration(seconds: 2)),
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
        mouseCursor: WidgetStateMouseCursor.clickable,
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
            labelLarge: FloeType.button,
            labelMedium: FloeType.label,
          ).apply(
            fontFamily: 'Pretendard',
            bodyColor: FloePalette.neutral950,
            displayColor: FloePalette.neutral950,
          ),
      inputDecorationTheme: InputDecorationTheme(
        filled: true,
        fillColor: FloeColor.surface,
        constraints: const BoxConstraints(minHeight: FloeControlSize.field),
        contentPadding: FloeControlInsets.field,
        border: _inputBorder(FloeColor.border),
        enabledBorder: _inputBorder(FloeColor.border),
        hoverColor: FloeColor.neutralHover,
        focusedBorder: _inputBorder(FloeColor.focus, width: 2),
        errorBorder: _inputBorder(FloePalette.error600),
        focusedErrorBorder: _inputBorder(FloePalette.error600, width: 2),
      ),
      filledButtonTheme: FilledButtonThemeData(
        style: ButtonStyle(
          mouseCursor: WidgetStateMouseCursor.clickable,
          tapTargetSize: MaterialTapTargetSize.shrinkWrap,
          textStyle: const WidgetStatePropertyAll(FloeType.button),
          animationDuration: FloeMotion.hoverDuration,
          backgroundColor: WidgetStateProperty.resolveWith(
            FloeStates.filledBackground,
          ),
          foregroundColor: WidgetStateProperty.resolveWith(
            FloeStates.filledForeground,
          ),
          overlayColor: const WidgetStatePropertyAll(Colors.transparent),
          padding: const WidgetStatePropertyAll(FloeControlInsets.button),
          minimumSize: const WidgetStatePropertyAll(
            Size(FloeControlSize.standard, FloeControlSize.standard),
          ),
          shape: WidgetStatePropertyAll(
            floeSquircleBorder(FloeSquircleSize.sm),
          ),
        ),
      ),
      textButtonTheme: TextButtonThemeData(
        style: ButtonStyle(
          mouseCursor: WidgetStateMouseCursor.clickable,
          textStyle: const WidgetStatePropertyAll(FloeType.button),
          animationDuration: FloeMotion.hoverDuration,
          foregroundColor: WidgetStateProperty.resolveWith(
            FloeStates.quietForeground,
          ),
          backgroundColor: WidgetStateProperty.resolveWith(
            FloeStates.quietBackground,
          ),
          overlayColor: const WidgetStatePropertyAll(Colors.transparent),
          padding: const WidgetStatePropertyAll(FloeControlInsets.button),
          minimumSize: const WidgetStatePropertyAll(
            Size(FloeControlSize.standard, FloeControlSize.standard),
          ),
          shape: WidgetStatePropertyAll(
            floeSquircleBorder(FloeSquircleSize.sm),
          ),
        ),
      ),
      iconButtonTheme: IconButtonThemeData(
        style: ButtonStyle(
          mouseCursor: WidgetStateMouseCursor.clickable,
          animationDuration: FloeMotion.hoverDuration,
          foregroundColor: WidgetStateProperty.resolveWith(
            FloeStates.quietForeground,
          ),
          backgroundColor: WidgetStateProperty.resolveWith(
            FloeStates.quietBackground,
          ),
          overlayColor: const WidgetStatePropertyAll(Colors.transparent),
          minimumSize: const WidgetStatePropertyAll(
            Size.square(FloeControlSize.standard),
          ),
          shape: WidgetStatePropertyAll(
            floeSquircleBorder(FloeSquircleSize.sm),
          ),
        ),
      ),
      outlinedButtonTheme: OutlinedButtonThemeData(
        style: ButtonStyle(
          mouseCursor: WidgetStateMouseCursor.clickable,
          tapTargetSize: MaterialTapTargetSize.shrinkWrap,
          textStyle: const WidgetStatePropertyAll(FloeType.button),
          animationDuration: FloeMotion.hoverDuration,
          foregroundColor: WidgetStateProperty.resolveWith(
            FloeStates.quietForeground,
          ),
          backgroundColor: WidgetStateProperty.resolveWith(
            FloeStates.outlinedBackground,
          ),
          overlayColor: const WidgetStatePropertyAll(Colors.transparent),
          padding: const WidgetStatePropertyAll(FloeControlInsets.button),
          side: WidgetStateProperty.resolveWith(FloeStates.outlinedSide),
          minimumSize: const WidgetStatePropertyAll(
            Size(FloeControlSize.standard, FloeControlSize.standard),
          ),
          shape: WidgetStatePropertyAll(
            floeSquircleBorder(FloeSquircleSize.sm),
          ),
        ),
      ),
      segmentedButtonTheme: SegmentedButtonThemeData(
        style: ButtonStyle(
          mouseCursor: WidgetStateMouseCursor.clickable,
          animationDuration: FloeMotion.hoverDuration,
          foregroundColor: WidgetStateProperty.resolveWith(
            FloeStates.segmentForeground,
          ),
          backgroundColor: WidgetStateProperty.resolveWith(
            FloeStates.segmentBackground,
          ),
          overlayColor: const WidgetStatePropertyAll(Colors.transparent),
          side: WidgetStateProperty.resolveWith(FloeStates.segmentSide),
          minimumSize: const WidgetStatePropertyAll(
            Size(FloeControlSize.standard, FloeControlSize.standard),
          ),
          shape: WidgetStatePropertyAll(
            floeSquircleBorder(FloeSquircleSize.sm),
          ),
        ),
      ),
      listTileTheme: const ListTileThemeData(
        mouseCursor: WidgetStateMouseCursor.clickable,
      ),
      popupMenuTheme: const PopupMenuThemeData(
        mouseCursor: WidgetStateMouseCursor.clickable,
      ),
      dialogTheme: DialogThemeData(
        constraints: BoxConstraints(minWidth: 280, maxWidth: 540),
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
}
