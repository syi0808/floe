import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:floe_client/l10n/app_localizations.dart';

import 'design_tokens.dart';
import 'floe_motion.dart';

abstract final class FloeLoading {
  static const minimumDuration = Duration(milliseconds: 500);

  static Future<void> minimumVisibility() =>
      Future<void>.delayed(minimumDuration);

  static Future<T> run<T>(Future<T> Function() operation) async {
    final minimum = minimumVisibility();
    try {
      return await operation();
    } finally {
      await minimum;
    }
  }
}

final class FloeSpinner extends StatefulWidget {
  const FloeSpinner({this.size = 32, this.label, super.key});

  final double size;
  final String? label;

  @override
  State<FloeSpinner> createState() => _FloeSpinnerState();
}

final class _FloeSpinnerState extends State<FloeSpinner>
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
  Widget build(BuildContext context) {
    final dotSize = widget.size * 3 / 16;
    final radius = widget.size * 11 / 32;
    final center = (widget.size - dotSize) / 2;
    return Semantics(
      label: widget.label ?? AppLocalizations.of(context).loadingCalendar,
      liveRegion: true,
      child: RotationTransition(
        turns: animation,
        child: SizedBox.square(
          dimension: widget.size,
          child: Stack(
            children: [
              for (var index = 0; index < 8; index++)
                Positioned(
                  left: center + radius * math.cos(index * math.pi / 4),
                  top: center + radius * math.sin(index * math.pi / 4),
                  child: DecoratedBox(
                    decoration: BoxDecoration(
                      shape: BoxShape.circle,
                      color: FloePalette.primary600.withValues(
                        alpha: .25 + index * .1,
                      ),
                    ),
                    child: SizedBox.square(dimension: dotSize),
                  ),
                ),
            ],
          ),
        ),
      ),
    );
  }
}

final class FloeLoadingOverlay extends StatelessWidget {
  const FloeLoadingOverlay({
    required this.loading,
    required this.child,
    this.label,
    this.dimBackground = true,
    this.blockInteraction = true,
    super.key,
  });

  final bool loading;
  final Widget child;
  final String? label;
  final bool dimBackground;
  final bool blockInteraction;

  @override
  Widget build(BuildContext context) => Stack(
    children: [
      ExcludeFocus(
        excluding: loading && blockInteraction,
        child: IgnorePointer(
          ignoring: loading && blockInteraction,
          child: child,
        ),
      ),
      if (loading)
        Positioned.fill(
          child: IgnorePointer(
            ignoring: !blockInteraction,
            child: ColoredBox(
              color: dimBackground
                  ? FloePalette.neutral0.withValues(alpha: .68)
                  : Colors.transparent,
              child: Center(child: FloeSpinner(label: label)),
            ),
          ),
        ),
    ],
  );
}
