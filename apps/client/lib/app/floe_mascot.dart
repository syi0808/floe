import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';

final class FloeMascot extends StatelessWidget {
  const FloeMascot({this.size = 44, super.key});

  static const assetPath = 'assets/floe-mascot.svg';

  final double size;

  @override
  Widget build(BuildContext context) => Semantics(
    label: 'Floe',
    image: true,
    child: ExcludeSemantics(
      child: SvgPicture.asset(
        assetPath,
        width: size,
        height: size,
        fit: BoxFit.contain,
      ),
    ),
  );
}
