import 'package:flutter/material.dart';

abstract final class FloeMotion {
  static const easeOut = Cubic(0.23, 1, 0.32, 1);
  static const easeInOut = Cubic(0.77, 0, 0.175, 1);
  static const ease = Cubic(0.25, 0.1, 0.25, 1);
  static const sheetCurve = Cubic(0.32, 0.72, 0, 1);

  static const pressDuration = Duration(milliseconds: 120);
  static const hoverDuration = Duration(milliseconds: 140);
  static const popoverDuration = Duration(milliseconds: 160);
  static const selectionDuration = Duration(milliseconds: 180);
  static const dialogDuration = Duration(milliseconds: 240);
  static const reducedMotionDuration = Duration(milliseconds: 160);

  static bool reduceMotion(BuildContext context) =>
      MediaQuery.maybeOf(context)?.disableAnimations ?? false;
}

final class PressableScale extends StatefulWidget {
  const PressableScale({
    required this.child,
    this.enabled = true,
    this.scale = 0.97,
    this.alignment = Alignment.center,
    super.key,
  });

  final Widget child;
  final bool enabled;
  final double scale;
  final Alignment alignment;

  @override
  State<PressableScale> createState() => _PressableScaleState();
}

final class _PressableScaleState extends State<PressableScale> {
  bool _pointerDown = false;

  @override
  void didUpdateWidget(PressableScale oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!widget.enabled && _pointerDown) {
      _pointerDown = false;
    }
  }

  void _setPointerDown(bool value) {
    if (!widget.enabled || _pointerDown == value) return;
    setState(() => _pointerDown = value);
  }

  @override
  Widget build(BuildContext context) {
    final reduceMotion = FloeMotion.reduceMotion(context);
    return Listener(
      behavior: HitTestBehavior.translucent,
      onPointerDown: (_) => _setPointerDown(true),
      onPointerUp: (_) => _setPointerDown(false),
      onPointerCancel: (_) => _setPointerDown(false),
      child: AnimatedScale(
        scale: _pointerDown && !reduceMotion ? widget.scale : 1,
        alignment: widget.alignment,
        duration: reduceMotion ? Duration.zero : FloeMotion.pressDuration,
        curve: FloeMotion.easeOut,
        child: widget.child,
      ),
    );
  }
}

final class FloeFadeScaleTransition extends StatelessWidget {
  const FloeFadeScaleTransition({
    required this.animation,
    required this.child,
    this.beginScale = 0.96,
    this.alignment = Alignment.center,
    super.key,
  });

  final Animation<double> animation;
  final Widget child;
  final double beginScale;
  final Alignment alignment;

  @override
  Widget build(BuildContext context) {
    final curvedAnimation = CurvedAnimation(
      parent: animation,
      curve: FloeMotion.easeOut,
      reverseCurve: FloeMotion.easeOut,
    );
    final faded = FadeTransition(opacity: curvedAnimation, child: child);
    if (FloeMotion.reduceMotion(context)) return faded;
    return ScaleTransition(
      scale: Tween<double>(begin: beginScale, end: 1).animate(curvedAnimation),
      alignment: alignment,
      child: faded,
    );
  }
}

final class FloeAnimatedSwap extends StatelessWidget {
  const FloeAnimatedSwap({
    required this.child,
    this.duration = FloeMotion.selectionDuration,
    this.beginScale = 0.96,
    this.alignment = Alignment.center,
    this.layoutBuilder = AnimatedSwitcher.defaultLayoutBuilder,
    super.key,
  });

  final Widget child;
  final Duration duration;
  final double beginScale;
  final Alignment alignment;
  final AnimatedSwitcherLayoutBuilder layoutBuilder;

  @override
  Widget build(BuildContext context) {
    final reduceMotion = FloeMotion.reduceMotion(context);
    return AnimatedSwitcher(
      duration: reduceMotion ? FloeMotion.reducedMotionDuration : duration,
      switchInCurve: FloeMotion.easeOut,
      switchOutCurve: FloeMotion.easeOut,
      layoutBuilder: layoutBuilder,
      transitionBuilder: (child, animation) {
        final faded = FadeTransition(opacity: animation, child: child);
        if (reduceMotion) return faded;
        return ScaleTransition(
          scale: Tween<double>(begin: beginScale, end: 1).animate(animation),
          alignment: alignment,
          child: faded,
        );
      },
      child: child,
    );
  }
}
