import 'package:floe_client/l10n/app_localizations.dart';

import 'dart:ui';

import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import 'design_tokens.dart';
import 'floe_loading.dart';
import 'floe_motion.dart';
import 'floe_squircle.dart';

class FloeTextLink extends StatelessWidget {
  const FloeTextLink({
    super.key,
    required this.label,
    required this.onPressed,
    this.icon,
    this.leading,
    this.color = FloePalette.primary600,
  });
  final String label;
  final VoidCallback? onPressed;
  final IconData? icon;
  final Widget? leading;
  final Color color;

  @override
  Widget build(BuildContext context) => TextButton(
    onPressed: onPressed,
    style: ButtonStyle(
      alignment: Alignment.centerLeft,
      padding: WidgetStatePropertyAll(EdgeInsets.zero),
      minimumSize: WidgetStatePropertyAll(Size(0, 32)),
      tapTargetSize: MaterialTapTargetSize.shrinkWrap,
      backgroundColor: WidgetStatePropertyAll(Colors.transparent),
      foregroundColor: WidgetStatePropertyAll(color),
      textStyle: WidgetStateProperty.resolveWith(
        (states) => FloeType.bodySmall.copyWith(
          decoration:
              states.contains(WidgetState.hovered) ||
                  states.contains(WidgetState.focused)
              ? TextDecoration.underline
              : TextDecoration.none,
        ),
      ),
    ),
    child: Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        if (leading != null) ...[leading!, SizedBox(width: FloeSpace.sm)],
        if (icon != null) ...[
          Icon(icon, size: 16),
          SizedBox(width: FloeSpace.sm),
        ],
        Flexible(child: Text(label)),
      ],
    ),
  );
}

Future<T?> showFloeDialog<T>(
  BuildContext context,
  WidgetBuilder builder, {
  bool barrierDismissible = true,
}) {
  final reduced = FloeMotion.reduceMotion(context);
  return Navigator.of(context).push<T>(
    PageRouteBuilder<T>(
      opaque: false,
      barrierDismissible: barrierDismissible,
      barrierLabel: AppLocalizations.of(context).dismissDialog,
      barrierColor: FloePalette.neutral950.withValues(alpha: .28),
      transitionDuration: reduced ? Duration.zero : FloeMotion.dialogDuration,
      reverseTransitionDuration: reduced
          ? Duration.zero
          : FloeMotion.pressDuration,
      pageBuilder: (context, animation, secondaryAnimation) => builder(context),
      transitionsBuilder: (context, animation, secondaryAnimation, child) =>
          AnimatedBuilder(
            animation: animation,
            builder: (context, _) {
              final progress = FloeMotion.easeOut.transform(animation.value);
              return BackdropFilter(
                filter: ImageFilter.blur(
                  sigmaX: 5 * progress,
                  sigmaY: 5 * progress,
                ),
                child: Opacity(
                  opacity: progress,
                  child: Transform.translate(
                    offset: Offset(0, 8 * (1 - progress)),
                    child: Transform.scale(
                      scale: .96 + .04 * progress,
                      child: child,
                    ),
                  ),
                ),
              );
            },
          ),
    ),
  );
}

class FloeDetailDialog extends StatelessWidget {
  const FloeDetailDialog({
    super.key,
    required this.title,
    required this.children,
    this.loading = false,
    this.loadingLabel,
  });
  final String title;
  final List<Widget> children;
  final bool loading;
  final String? loadingLabel;
  @override
  Widget build(BuildContext context) => Dialog(
    constraints: BoxConstraints(maxWidth: 540),
    insetPadding: EdgeInsets.all(FloeSpace.lg),
    child: SingleChildScrollView(
      padding: FloeControlInsets.dialog,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          LayoutBuilder(
            builder: (context, constraints) {
              final heading = Text(
                title,
                style: FloeType.headlineLarge.copyWith(
                  fontSize: 26,
                  fontWeight: FontWeight.w700,
                  letterSpacing: -1,
                ),
              );
              final close = IconButton(
                tooltip: AppLocalizations.of(context).close,
                onPressed: () => Navigator.pop(context),
                icon: Icon(LucideIcons.x, size: 20),
              );
              if (constraints.maxWidth < 320 &&
                  MediaQuery.textScalerOf(context).scale(26) > 39) {
                return Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    Align(
                      alignment: AlignmentDirectional.centerEnd,
                      child: close,
                    ),
                    heading,
                  ],
                );
              }
              return Row(
                children: [
                  Expanded(child: heading),
                  close,
                ],
              );
            },
          ),
          SizedBox(height: FloeSpace.lg),
          FloeLoadingOverlay(
            loading: loading,
            label: loadingLabel,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: children,
            ),
          ),
        ],
      ),
    ),
  );
}

class FloeIconText extends StatelessWidget {
  const FloeIconText({
    super.key,
    required this.icon,
    required this.text,
    required this.style,
    this.gap = 12,
  });

  final Widget icon;
  final String text;
  final TextStyle style;
  final double gap;

  @override
  Widget build(BuildContext context) {
    final effectiveStyle = DefaultTextStyle.of(context).style.merge(style);
    final painter = TextPainter(
      text: TextSpan(text: text, style: effectiveStyle),
      textDirection: Directionality.of(context),
      textScaler: MediaQuery.textScalerOf(context),
      locale: Localizations.maybeLocaleOf(context),
    )..layout();
    final lineHeight = painter.preferredLineHeight;
    painter.dispose();
    return Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        SizedBox(
          height: lineHeight,
          child: Center(child: icon),
        ),
        SizedBox(width: gap),
        Expanded(child: Text(text, style: style)),
      ],
    );
  }
}

class FloeInfoNote extends StatelessWidget {
  const FloeInfoNote({
    super.key,
    required this.text,
    this.icon = LucideIcons.info,
  });
  final String text;
  final IconData icon;
  @override
  Widget build(BuildContext context) => Container(
    decoration: BoxDecoration(
      border: Border(left: BorderSide(color: FloePalette.primary200, width: 2)),
    ),
    padding: EdgeInsets.symmetric(horizontal: 16, vertical: 8),
    child: FloeIconText(
      icon: Icon(icon, size: 18, color: FloePalette.primary600),
      text: text,
      style: FloeType.bodySmall.copyWith(
        height: 1.7,
        color: FloePalette.neutral600,
      ),
    ),
  );
}

class FloeReadOnlyPill extends StatelessWidget {
  const FloeReadOnlyPill({super.key});
  @override
  Widget build(BuildContext context) => FloeSquircle(
    size: FloeSquircleSize.sm,
    fill: FloePalette.neutral50,
    borderWidth: 0,
    padding: EdgeInsets.symmetric(horizontal: 8, vertical: 4),
    child: Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Icon(LucideIcons.lockKeyhole, size: 12),
        SizedBox(width: FloeSpace.xs),
        Text(AppLocalizations.of(context).readOnly, style: FloeType.micro),
      ],
    ),
  );
}
