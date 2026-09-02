import 'package:flutter/material.dart';

abstract final class FloeColors {
  static const canvas = Color(0xFFFAFAF8);
  static const surface = Color(0xFFFFFFFF);
  static const text = Color(0xFF1D1C1A);
  static const secondary = Color(0xFF77736B);
  static const divider = Color(0xFFE7E5DF);
  static const sage = Color(0xFF4D805A);
  static const sageSoft = Color(0xFFF2F7F3);
  static const overdue = Color(0xFFA66B16);
  static const error = Color(0xFFA64239);
}

abstract final class FloeTheme {
  static ThemeData get light => ThemeData(
    colorScheme: ColorScheme.fromSeed(
      seedColor: FloeColors.sage,
      surface: FloeColors.surface,
      error: FloeColors.error,
    ),
    scaffoldBackgroundColor: FloeColors.canvas,
    fontFamily: '.AppleSystemUIFont',
    dividerColor: FloeColors.divider,
    textTheme: const TextTheme(
      headlineMedium: TextStyle(
        fontSize: 28,
        height: 1.2,
        fontWeight: FontWeight.w600,
        color: FloeColors.text,
      ),
      titleLarge: TextStyle(
        fontSize: 20,
        height: 1.3,
        fontWeight: FontWeight.w600,
        color: FloeColors.text,
      ),
      bodyLarge: TextStyle(fontSize: 15, height: 1.45, color: FloeColors.text),
      bodyMedium: TextStyle(
        fontSize: 13,
        height: 1.4,
        color: FloeColors.secondary,
      ),
    ),
    inputDecorationTheme: InputDecorationTheme(
      filled: true,
      fillColor: FloeColors.surface,
      border: OutlineInputBorder(
        borderRadius: BorderRadius.circular(12),
        borderSide: const BorderSide(color: FloeColors.divider),
      ),
      enabledBorder: OutlineInputBorder(
        borderRadius: BorderRadius.circular(12),
        borderSide: const BorderSide(color: FloeColors.divider),
      ),
      focusedBorder: OutlineInputBorder(
        borderRadius: BorderRadius.circular(12),
        borderSide: const BorderSide(color: FloeColors.sage, width: 2),
      ),
    ),
  );
}
