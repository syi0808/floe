import 'package:flutter/material.dart';

abstract final class FloePalette {
  static const primary50 = Color(0xFFF3F6FF);
  static const primary100 = Color(0xFFE7ECFF);
  static const primary200 = Color(0xFFCED8FF);
  static const primary300 = Color(0xFFAAB9FF);
  static const primary400 = Color(0xFF7E94F4);
  static const primary500 = Color(0xFF6078E8);
  static const primary600 = Color(0xFF4B64D8);
  static const primary700 = Color(0xFF3F50B5);
  static const primary800 = Color(0xFF354493);
  static const primary900 = Color(0xFF303C75);

  static const neutral0 = Color(0xFFFFFFFF);
  static const neutral50 = Color(0xFFF8F9FB);
  static const neutral100 = Color(0xFFF1F3F6);
  static const neutral200 = Color(0xFFE3E6EB);
  static const neutral300 = Color(0xFFCCD1D9);
  static const neutral400 = Color(0xFF9DA4AF);
  static const neutral500 = Color(0xFF707884);
  static const neutral600 = Color(0xFF505761);
  static const neutral800 = Color(0xFF272B31);
  static const neutral950 = Color(0xFF111317);

  static const aqua100 = Color(0xFFD5F5F2);
  static const aqua300 = Color(0xFF78D8D2);
  static const violet50 = Color(0xFFF8F4FF);
  static const violet300 = Color(0xFFC4A8FF);
  static const success600 = Color(0xFF2D7D49);
  static const warning50 = Color(0xFFFFF9EB);
  static const warning600 = Color(0xFF946215);
  static const error50 = Color(0xFFFFF4F2);
  static const error600 = Color(0xFFB8463A);

  static const glacialField = LinearGradient(
    begin: Alignment.topLeft,
    end: Alignment.bottomRight,
    colors: [primary50, aqua100, violet50],
    stops: [0, 0.52, 1],
  );
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
}

abstract final class FloeRadius {
  static const xs = 4.0;
  static const sm = 6.0;
  static const md = 10.0;
  static const lg = 14.0;
  static const xl = 20.0;
}

abstract final class FloeType {
  static const display = TextStyle(
    fontSize: 32,
    fontWeight: FontWeight.w600,
    height: 1.16,
    letterSpacing: -0.8,
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
    height: 1.5,
    color: FloePalette.neutral600,
  );
  static const label = TextStyle(
    fontSize: 12,
    fontWeight: FontWeight.w600,
    height: 1.35,
    letterSpacing: 0.12,
    color: FloePalette.neutral600,
  );
}
