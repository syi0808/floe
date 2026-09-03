import 'package:flutter/material.dart';

import 'design_tokens.dart';

enum FloeSquircleSize { xs, sm, md, lg, xl, frame }

extension on FloeSquircleSize {
  double get extent => switch (this) {
    FloeSquircleSize.xs => FloeRadius.xs,
    FloeSquircleSize.sm => FloeRadius.sm,
    FloeSquircleSize.md => FloeRadius.md,
    FloeSquircleSize.lg => FloeRadius.lg,
    FloeSquircleSize.xl => FloeRadius.xl,
    FloeSquircleSize.frame => FloeRadius.frame,
  };
}

ContinuousRectangleBorder floeSquircleBorder(
  FloeSquircleSize size, {
  Color borderColor = Colors.transparent,
  double borderWidth = 0,
}) => ContinuousRectangleBorder(
  borderRadius: BorderRadius.circular(size.extent),
  side: BorderSide(color: borderColor, width: borderWidth),
);

final class FloeSquircle extends StatelessWidget {
  const FloeSquircle({
    required this.child,
    this.size = FloeSquircleSize.lg,
    this.fill = FloePalette.neutral0,
    this.borderColor = FloePalette.neutral200,
    this.borderWidth = 1,
    this.padding,
    this.elevation = 0,
    this.clipBehavior = Clip.antiAlias,
    super.key,
  });

  final Widget child;
  final FloeSquircleSize size;
  final Color fill;
  final Color borderColor;
  final double borderWidth;
  final EdgeInsetsGeometry? padding;
  final double elevation;
  final Clip clipBehavior;

  @override
  Widget build(BuildContext context) => Material(
    color: fill,
    elevation: elevation,
    shadowColor: const Color(0x1415182B),
    shape: floeSquircleBorder(
      size,
      borderColor: borderColor,
      borderWidth: borderWidth,
    ),
    clipBehavior: clipBehavior,
    child: padding == null ? child : Padding(padding: padding!, child: child),
  );
}
