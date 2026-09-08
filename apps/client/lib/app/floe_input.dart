import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'floe_field.dart';

class FloeInput extends StatelessWidget {
  const FloeInput({
    required this.label,
    this.controller,
    this.focusNode,
    this.enabled = true,
    this.readOnly = false,
    this.autofocus = false,
    this.placeholder,
    this.description,
    this.errorText,
    this.keyboardType,
    this.textInputAction,
    this.textCapitalization = TextCapitalization.none,
    this.textAlign = TextAlign.start,
    this.autocorrect = true,
    this.enableSuggestions = true,
    this.obscureText = false,
    this.minLines,
    this.maxLines = 1,
    this.inputFormatters,
    this.onChanged,
    this.onSubmitted,
    this.validator,
    this.autovalidateMode,
    this.compact = false,
    super.key,
  });

  final String label;
  final TextEditingController? controller;
  final FocusNode? focusNode;
  final bool enabled;
  final bool readOnly;
  final bool autofocus;
  final String? placeholder;
  final String? description;
  final String? errorText;
  final TextInputType? keyboardType;
  final TextInputAction? textInputAction;
  final TextCapitalization textCapitalization;
  final TextAlign textAlign;
  final bool autocorrect;
  final bool enableSuggestions;
  final bool obscureText;
  final int? minLines;
  final int? maxLines;
  final List<TextInputFormatter>? inputFormatters;
  final ValueChanged<String>? onChanged;
  final ValueChanged<String>? onSubmitted;
  final FormFieldValidator<String>? validator;
  final AutovalidateMode? autovalidateMode;
  final bool compact;

  @override
  Widget build(BuildContext context) => TextFormField(
    controller: controller,
    focusNode: focusNode,
    enabled: enabled,
    readOnly: readOnly,
    autofocus: autofocus,
    keyboardType: keyboardType,
    textInputAction: textInputAction,
    textCapitalization: textCapitalization,
    textAlign: textAlign,
    autocorrect: autocorrect,
    enableSuggestions: enableSuggestions,
    obscureText: obscureText,
    minLines: minLines,
    maxLines: maxLines,
    inputFormatters: inputFormatters,
    onChanged: onChanged,
    onFieldSubmitted: onSubmitted,
    validator: validator,
    autovalidateMode: autovalidateMode,
    style: FloeField.textStyle(context),
    decoration: FloeField.decoration(
      label: label,
      placeholder: placeholder,
      description: description,
      errorText: errorText,
      enabled: enabled,
      isDense: compact,
      contentPadding: compact
          ? const EdgeInsets.symmetric(horizontal: 12, vertical: 10)
          : null,
    ),
  );
}
