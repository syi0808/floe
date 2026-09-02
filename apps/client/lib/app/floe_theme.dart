import 'package:flutter/material.dart';

import 'design_tokens.dart';

abstract final class FloeTheme {
  static ThemeData get light {
    final colorScheme = ColorScheme.fromSeed(
      seedColor: FloePalette.primary600,
      primary: FloePalette.primary600,
      surface: FloePalette.neutral0,
      error: FloePalette.error600,
    );
    return ThemeData(
      useMaterial3: true,
      colorScheme: colorScheme,
      scaffoldBackgroundColor: FloePalette.neutral50,
      dividerColor: FloePalette.neutral200,
      textTheme: const TextTheme(
        headlineMedium: FloeType.display,
        titleLarge: FloeType.headline,
        bodyLarge: FloeType.bodyLarge,
        bodyMedium: FloeType.body,
        labelLarge: TextStyle(
          fontSize: 14,
          height: 1.35,
          fontWeight: FontWeight.w600,
        ),
        labelMedium: FloeType.label,
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
        style: FilledButton.styleFrom(
          backgroundColor: FloePalette.primary600,
          foregroundColor: FloePalette.neutral0,
          minimumSize: const Size(40, 40),
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(FloeRadius.sm),
          ),
        ),
      ),
      textButtonTheme: TextButtonThemeData(
        style: TextButton.styleFrom(
          foregroundColor: FloePalette.neutral800,
          minimumSize: const Size(40, 40),
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(FloeRadius.sm),
          ),
        ),
      ),
      iconButtonTheme: IconButtonThemeData(
        style: IconButton.styleFrom(
          foregroundColor: FloePalette.neutral600,
          minimumSize: const Size.square(40),
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(FloeRadius.sm),
          ),
        ),
      ),
      checkboxTheme: CheckboxThemeData(
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(FloeRadius.xs),
        ),
        side: const BorderSide(color: FloePalette.neutral400, width: 1.5),
      ),
      dialogTheme: DialogThemeData(
        backgroundColor: FloePalette.neutral0,
        surfaceTintColor: Colors.transparent,
        elevation: 0,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(FloeRadius.lg),
          side: const BorderSide(color: FloePalette.neutral200),
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
