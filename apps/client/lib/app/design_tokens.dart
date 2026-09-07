import 'package:flutter/material.dart';

abstract final class FloePalette {
  static const primary50 = Color(0xFFF7F5FF);
  static const primary100 = Color(0xFFEFEBFF);
  static const primary200 = Color(0xFFDED6FF);
  static const primary300 = Color(0xFFC4B5FD);
  static const primary400 = Color(0xFF9B86F7);
  static const primary500 = Color(0xFF7C63EE);
  static const primary600 = Color(0xFF654BE0);
  static const primary700 = Color(0xFF5138BF);
  static const primary800 = Color(0xFF432F98);
  static const primary900 = Color(0xFF372878);

  static const neutral0 = Color(0xFFFFFFFF);
  static const neutral25 = Color(0xFFFBFAFF);
  static const neutral50 = Color(0xFFF8F7FB);
  static const neutral100 = Color(0xFFF1EFF6);
  static const neutral200 = Color(0xFFE8E6F0);
  static const neutral300 = Color(0xFFD2CEDC);
  static const neutral400 = Color(0xFFA8A2B4);
  static const neutral500 = Color(0xFF777184);
  static const neutral600 = Color(0xFF5B5668);
  static const neutral700 = Color(0xFF403C4A);
  static const neutral800 = Color(0xFF292632);
  static const neutral900 = Color(0xFF1B1922);
  static const neutral950 = Color(0xFF15182B);

  static const blue50 = Color(0xFFF1F6FF);
  static const blue100 = Color(0xFFE4EEFF);
  static const blue300 = Color(0xFFA9C8FA);
  static const blue500 = Color(0xFF3D86EC);
  static const blue700 = Color(0xFF225EBC);
  static const blue900 = Color(0xFF173B71);
  static const mint50 = Color(0xFFF0FAF7);
  static const mint100 = Color(0xFFDDF5ED);
  static const mint300 = Color(0xFF8DDCC2);
  static const mint500 = Color(0xFF43B590);
  static const mint700 = Color(0xFF267B62);
  static const mint900 = Color(0xFF194D3F);
  static const amber50 = Color(0xFFFFF8ED);
  static const amber100 = Color(0xFFFFEBCB);
  static const amber300 = Color(0xFFF8C66B);
  static const amber500 = Color(0xFFE99B20);
  static const amber700 = Color(0xFFA55E0D);
  static const amber900 = Color(0xFF663909);
  static const coral50 = Color(0xFFFFF4F2);
  static const coral100 = Color(0xFFFFE2DD);
  static const coral300 = Color(0xFFF6A69C);
  static const coral500 = Color(0xFFE7675D);
  static const coral700 = Color(0xFFA63D37);
  static const coral900 = Color(0xFF652A27);

  static const success600 = mint700;
  static const warning50 = amber50;
  static const warning600 = amber700;
  static const error50 = coral50;
  static const error600 = coral700;
  static const error700 = Color(0xFF8A322E);
  static const error800 = coral900;

  static const glacialField = LinearGradient(
    begin: Alignment.topLeft,
    end: Alignment.bottomRight,
    colors: [primary50, neutral25, mint50],
    stops: [0, 0.52, 1],
  );
}

abstract final class FloeColor {
  static const canvas = FloePalette.neutral25;
  static const surface = FloePalette.neutral0;
  static const surfaceSubtle = FloePalette.neutral50;
  static const textPrimary = FloePalette.neutral950;
  static const textSecondary = FloePalette.neutral600;
  static const textTertiary = FloePalette.neutral500;
  static const border = FloePalette.neutral200;
  static const borderStrong = FloePalette.neutral300;
  static const focus = FloePalette.primary600;

  static const neutralHover = FloePalette.neutral50;
  static const quietHover = FloePalette.neutral100;
  static const neutralPressed = FloePalette.neutral200;
  static const selectionHover = FloePalette.primary50;
  static const selectionPressed = FloePalette.primary100;
  static const disabledSurface = FloePalette.neutral50;
  static const disabledContent = FloePalette.neutral400;
}

abstract final class FloeSpace {
  static const xxs = 2.0;
  static const xs = 4.0;
  static const sm = 8.0;
  static const md = 12.0;
  static const base = 16.0;
  static const lg = 24.0;
  static const xl = 32.0;
  static const xxl = 48.0;
  static const xxxl = 64.0;
  static const xxxxl = 96.0;
}

abstract final class FloeRadius {
  static const xs = 8.0;
  static const sm = 12.0;
  static const md = 16.0;
  static const lg = 20.0;
  static const xl = 28.0;
  static const frame = 32.0;
}

abstract final class FloeControlSize {
  static const compact = 36.0;
  static const standard = 44.0;
  static const field = 48.0;
}

abstract final class FloeControlInsets {
  static const button = EdgeInsets.symmetric(
    horizontal: FloeSpace.base,
    vertical: FloeSpace.md,
  );
  static const field = EdgeInsets.symmetric(
    horizontal: FloeSpace.base,
    vertical: FloeSpace.md,
  );
  static const menu = EdgeInsets.symmetric(
    horizontal: FloeSpace.md,
    vertical: FloeSpace.sm,
  );
  static const card = EdgeInsets.all(20);
  static const dialog = EdgeInsets.all(28);
}

abstract final class FloeType {
  static const displayLarge = TextStyle(
    fontSize: 40,
    fontWeight: FontWeight.w600,
    height: 1.12,
    letterSpacing: -1.2,
    color: FloePalette.neutral950,
  );
  static const display = TextStyle(
    fontSize: 32,
    fontWeight: FontWeight.w600,
    height: 1.16,
    letterSpacing: -0.8,
    color: FloePalette.neutral950,
  );
  static const headlineLarge = TextStyle(
    fontSize: 24,
    fontWeight: FontWeight.w600,
    height: 1.25,
    letterSpacing: -0.43,
    color: FloePalette.neutral950,
  );
  static const headline = TextStyle(
    fontSize: 20,
    fontWeight: FontWeight.w600,
    height: 1.3,
    letterSpacing: -0.24,
    color: FloePalette.neutral950,
  );
  static const bodyLarge = TextStyle(
    fontSize: 16,
    height: 1.55,
    letterSpacing: -0.1,
    color: FloePalette.neutral950,
  );
  static const body = TextStyle(
    fontSize: 14,
    letterSpacing: 0,
    height: 1.5,
    color: FloePalette.neutral600,
  );
  static const label = TextStyle(
    fontSize: 12,
    fontWeight: FontWeight.w600,
    height: 1.35,
    letterSpacing: 0,
    color: FloePalette.neutral600,
  );
  static const numeric = TextStyle(
    fontSize: 13,
    letterSpacing: 0,
    fontWeight: FontWeight.w500,
    height: 1.35,
    color: FloePalette.neutral600,
    fontFeatures: [FontFeature.tabularFigures()],
  );
}
