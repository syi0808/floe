import 'package:flutter/material.dart';

final class FloeMascot extends StatelessWidget {
  const FloeMascot({this.size = 44, super.key});

  static const assetPath = 'assets/floe-mascot.png';

  final double size;

  @override
  Widget build(BuildContext context) => Semantics(
    label: 'Floe',
    image: true,
    child: ExcludeSemantics(
      child: Image.asset(
        assetPath,
        width: size,
        height: size,
        fit: BoxFit.contain,
        filterQuality: FilterQuality.high,
      ),
    ),
  );
}
