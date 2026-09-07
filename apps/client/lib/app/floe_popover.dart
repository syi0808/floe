import 'dart:math' as math;

import 'package:flutter/material.dart';

import 'floe_motion.dart';
import 'floe_squircle.dart';

Rect floeAnchorRect(BuildContext context) {
  final box = context.findRenderObject()! as RenderBox;
  return box.localToGlobal(Offset.zero) & box.size;
}

enum FloePopoverHorizontalAnchor { start, center }

Future<T?> showFloePopover<T>({
  required BuildContext context,
  required Rect anchor,
  required WidgetBuilder builder,
  required double width,
  required double height,
  FloePopoverHorizontalAnchor horizontalAnchor =
      FloePopoverHorizontalAnchor.start,
}) => Navigator.of(context).push<T>(
  _FloePopoverRoute<T>(
    anchor: anchor,
    width: width,
    height: height,
    horizontalAnchor: horizontalAnchor,
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
    required this.horizontalAnchor,
    required this.builder,
    required this.themes,
    required this.reduceMotion,
    required this.dismissLabel,
  });
  final Rect anchor;
  final double width;
  final double height;
  final FloePopoverHorizontalAnchor horizontalAnchor;
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
        const gap = 6.0;
        final roomAbove = math.max(0.0, anchor.top - topEdge - gap);
        final roomBelow = math.max(0.0, bottomEdge - anchor.bottom - gap);
        final above = roomBelow < panelHeight && roomAbove > roomBelow;
        final anchorX = switch (horizontalAnchor) {
          FloePopoverHorizontalAnchor.start => anchor.left,
          FloePopoverHorizontalAnchor.center => anchor.center.dx,
        };
        final idealLeft = switch (horizontalAnchor) {
          FloePopoverHorizontalAnchor.start => anchorX,
          FloePopoverHorizontalAnchor.center => anchorX - panelWidth / 2,
        };
        final left = idealLeft.clamp(
          leftEdge,
          math.max(leftEdge, rightEdge - panelWidth),
        );
        final top =
            (above ? anchor.top - panelHeight - gap : anchor.bottom + gap)
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
                  ((anchorX - left) / math.max(1.0, panelWidth) * 2 - 1).clamp(
                    -1,
                    1,
                  ),
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
