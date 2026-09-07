import 'package:flutter/material.dart';

import 'design_tokens.dart';

abstract final class FloeField {
  static InputDecoration decoration({
    required String label,
    String? placeholder,
    String? description,
    String? errorText,
    bool enabled = true,
    Widget? prefixIcon,
    Widget? suffixIcon,
  }) => InputDecoration(
    labelText: label,
    hintText: placeholder,
    helperText: description,
    errorText: errorText,
    enabled: enabled,
    prefixIcon: prefixIcon,
    suffixIcon: suffixIcon,
  );

  static TextStyle textStyle(BuildContext context) =>
      Theme.of(context).textTheme.bodyMedium
          ?.copyWith(color: FloeColor.textPrimary) ??
      FloeType.body.copyWith(color: FloeColor.textPrimary);
}
