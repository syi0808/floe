import 'package:flutter/material.dart';

import 'design_tokens.dart';

final class FloeMascot extends StatelessWidget {
  const FloeMascot({this.size = 44, super.key});

  final double size;

  @override
  Widget build(BuildContext context) => Semantics(
    label: 'Floe',
    image: true,
    child: ExcludeSemantics(
      child: CustomPaint(size: Size.square(size), painter: _MascotPainter()),
    ),
  );
}

final class _MascotPainter extends CustomPainter {
  @override
  void paint(Canvas canvas, Size size) {
    final bounds = Offset.zero & size;
    final body = Path()
      ..moveTo(size.width * 0.5, size.height * 0.05)
      ..cubicTo(
        size.width * 0.78,
        size.height * 0.04,
        size.width * 0.96,
        size.height * 0.25,
        size.width * 0.91,
        size.height * 0.54,
      )
      ..cubicTo(
        size.width * 0.88,
        size.height * 0.77,
        size.width * 0.72,
        size.height * 0.94,
        size.width * 0.52,
        size.height * 0.89,
      )
      ..cubicTo(
        size.width * 0.34,
        size.height * 0.84,
        size.width * 0.25,
        size.height * 0.98,
        size.width * 0.11,
        size.height * 0.9,
      )
      ..cubicTo(
        size.width * 0.19,
        size.height * 0.72,
        size.width * 0.04,
        size.height * 0.62,
        size.width * 0.09,
        size.height * 0.38,
      )
      ..cubicTo(
        size.width * 0.14,
        size.height * 0.15,
        size.width * 0.3,
        size.height * 0.06,
        size.width * 0.5,
        size.height * 0.05,
      )
      ..close();
    canvas.drawPath(
      body,
      Paint()
        ..shader = const LinearGradient(
          begin: Alignment.topLeft,
          end: Alignment.bottomRight,
          colors: [
            FloePalette.primary300,
            FloePalette.primary500,
            Color(0xFF69CFC0),
          ],
        ).createShader(bounds),
    );
    final eyePaint = Paint()..color = FloePalette.neutral950;
    canvas.drawOval(
      Rect.fromCenter(
        center: Offset(size.width * 0.39, size.height * 0.46),
        width: size.width * 0.075,
        height: size.height * 0.12,
      ),
      eyePaint,
    );
    canvas.drawOval(
      Rect.fromCenter(
        center: Offset(size.width * 0.63, size.height * 0.46),
        width: size.width * 0.075,
        height: size.height * 0.12,
      ),
      eyePaint,
    );
  }

  @override
  bool shouldRepaint(covariant CustomPainter oldDelegate) => false;
}
