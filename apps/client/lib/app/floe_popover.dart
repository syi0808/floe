import 'dart:math' as math;

import 'package:flutter/material.dart';

import 'floe_motion.dart';
import 'floe_squircle.dart';

Rect floeAnchorRect(BuildContext context) {
  final box = context.findRenderObject()! as RenderBox;
  return box.localToGlobal(Offset.zero) & box.size;
}

Future<T?> showFloePopover<T>({
  required BuildContext context,
  required Rect anchor,
  required WidgetBuilder builder,
  required double width,
  required double height,
}) => Navigator.of(context).push<T>(
  _FloePopoverRoute<T>(
    anchor: anchor,
    width: width,
    height: height,
    builder: builder,
    themes: InheritedTheme.capture(
      from: context,
      to: Navigator.of(context).context,
    ),
    reduceMotion: FloeMotion.reduceMotion(context),
    dismissLabel: MaterialLocalizations.of(context).modalBarrierDismissLabel,
  ),
);

class _FloePopoverRoute<T> extends PopupRoute<T> {
  _FloePopoverRoute({
    required this.anchor,
    required this.width,
    required this.height,
    required this.builder,
    required this.themes,
    required this.reduceMotion,
    required this.dismissLabel,
  });
  final Rect anchor;
  final double width;
  final double height;
  final WidgetBuilder builder;
  final CapturedThemes themes;
  final bool reduceMotion;
  final String dismissLabel;
  @override
  Color get barrierColor => Colors.transparent;
  @override
  bool get barrierDismissible => true;
  @override
  String get barrierLabel => dismissLabel;
  @override
  Duration get transitionDuration =>
      reduceMotion ? Duration.zero : FloeMotion.popoverDuration;
  @override
  Duration get reverseTransitionDuration =>
      reduceMotion ? Duration.zero : FloeMotion.pressDuration;

  @override
  Widget buildPage(
    BuildContext context,
    Animation<double> animation,
    Animation<double> secondaryAnimation,
  ) => themes.wrap(
    LayoutBuilder(
      builder: (context, constraints) {
        final media = MediaQuery.of(context);
        final leftEdge = media.padding.left + 12;
        final topEdge = media.padding.top + 12;
        final rightEdge = constraints.maxWidth - media.padding.right - 12;
        final bottomEdge =
            constraints.maxHeight -
            media.viewInsets.bottom -
            media.padding.bottom -
            12;
        final panelWidth = math.min(width, math.max(0.0, rightEdge - leftEdge));
        final panelHeight = math.min(
          height,
          math.max(0.0, bottomEdge - topEdge),
        );
        final above =
            anchor.bottom + 6 + panelHeight > bottomEdge &&
            anchor.top > constraints.maxHeight / 2;
        final left = anchor.left.clamp(
          leftEdge,
          math.max(leftEdge, rightEdge - panelWidth),
        );
        final top = (above ? anchor.top - panelHeight - 6 : anchor.bottom + 6)
            .clamp(topEdge, math.max(topEdge, bottomEdge - panelHeight));
        return Stack(
          children: [
            Positioned(
              left: left.toDouble(),
              top: top.toDouble(),
              width: panelWidth,
              height: panelHeight,
              child: FloeFadeScaleTransition(
                animation: animation,
                beginScale: .97,
                alignment: Alignment(
                  ((anchor.left - left) / math.max(1.0, panelWidth) * 2 - 1)
                      .clamp(-1, 1),
                  above ? 1 : -1,
                ),
                child: FloeSquircle(
                  size: FloeSquircleSize.md,
                  elevation: 12,
                  child: FocusScope(child: Builder(builder: builder)),
                ),
              ),
            ),
          ],
        );
      },
    ),
  );
}
