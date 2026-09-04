import 'dart:math' as math;
import 'dart:ui';

import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import 'design_tokens.dart';
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
      padding: const WidgetStatePropertyAll(EdgeInsets.zero),
      minimumSize: const WidgetStatePropertyAll(Size(0, 32)),
      tapTargetSize: MaterialTapTargetSize.shrinkWrap,
      backgroundColor: const WidgetStatePropertyAll(Colors.transparent),
      foregroundColor: WidgetStatePropertyAll(color),
      textStyle: WidgetStateProperty.resolveWith(
        (states) => TextStyle(
          fontFamily: 'Pretendard',
          fontSize: 13,
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
        if (leading != null) ...[leading!, const SizedBox(width: 8)],
        if (icon != null) ...[Icon(icon, size: 16), const SizedBox(width: 8)],
        Flexible(child: Text(label)),
      ],
    ),
  );
}

class FloeDotSpinner extends StatefulWidget {
  const FloeDotSpinner({super.key});
  @override
  State<FloeDotSpinner> createState() => _FloeDotSpinnerState();
}

class _FloeDotSpinnerState extends State<FloeDotSpinner>
    with SingleTickerProviderStateMixin {
  late final AnimationController animation = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 1100),
  );
  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (FloeMotion.reduceMotion(context)) {
      animation.stop();
    } else {
      animation.repeat();
    }
  }

  @override
  void dispose() {
    animation.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => Semantics(
    label: 'Loading calendar',
    liveRegion: true,
    child: RotationTransition(
      turns: animation,
      child: SizedBox.square(
        dimension: 32,
        child: Stack(
          children: [
            for (var index = 0; index < 8; index++)
              Positioned(
                left: 13 + 11 * math.cos(index * math.pi / 4),
                top: 13 + 11 * math.sin(index * math.pi / 4),
                child: Container(
                  width: 6,
                  height: 6,
                  decoration: BoxDecoration(
                    shape: BoxShape.circle,
                    color: FloePalette.primary600.withValues(
                      alpha: .25 + index * .1,
                    ),
                  ),
                ),
              ),
          ],
        ),
      ),
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
      barrierLabel: 'Dismiss dialog',
      barrierColor: FloePalette.neutral950.withValues(alpha: .28),
      transitionDuration: reduced
          ? Duration.zero
          : const Duration(milliseconds: 240),
      reverseTransitionDuration: reduced
          ? Duration.zero
          : const Duration(milliseconds: 120),
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
  });
  final String title;
  final List<Widget> children;
  @override
  Widget build(BuildContext context) => Dialog(
    constraints: const BoxConstraints(maxWidth: 540),
    insetPadding: const EdgeInsets.all(24),
    child: SingleChildScrollView(
      padding: const EdgeInsets.all(32),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              Expanded(
                child: Text(
                  title,
                  style: const TextStyle(
                    fontSize: 26,
                    fontWeight: FontWeight.w700,
                    letterSpacing: -1,
                  ),
                ),
              ),
              IconButton(
                tooltip: 'Close',
                onPressed: () => Navigator.pop(context),
                icon: const Icon(LucideIcons.x, size: 20),
              ),
            ],
          ),
          const SizedBox(height: 24),
          ...children,
        ],
      ),
    ),
  );
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
    decoration: const BoxDecoration(
      border: Border(left: BorderSide(color: FloePalette.primary200, width: 2)),
    ),
    padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
    child: Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Icon(icon, size: 18, color: FloePalette.primary600),
        const SizedBox(width: 12),
        Expanded(
          child: Text(
            text,
            style: const TextStyle(
              fontSize: 13,
              height: 1.7,
              color: FloePalette.neutral600,
            ),
          ),
        ),
      ],
    ),
  );
}

class FloeReadOnlyPill extends StatelessWidget {
  const FloeReadOnlyPill({super.key});
  @override
  Widget build(BuildContext context) => const FloeSquircle(
    size: FloeSquircleSize.sm,
    fill: FloePalette.neutral50,
    borderWidth: 0,
    padding: EdgeInsets.symmetric(horizontal: 8, vertical: 4),
    child: Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Icon(LucideIcons.lockKeyhole, size: 12),
        SizedBox(width: 4),
        Text('Read-only', style: TextStyle(fontSize: 10)),
      ],
    ),
  );
}
