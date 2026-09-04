import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';

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

final class FloeScreenEntrance extends StatefulWidget {
  const FloeScreenEntrance({
    required this.identity,
    required this.child,
    super.key,
  });

  final Object identity;
  final Widget child;

  @override
  State<FloeScreenEntrance> createState() => _FloeScreenEntranceState();
}

final class _FloeScreenEntranceState extends State<FloeScreenEntrance>
    with SingleTickerProviderStateMixin {
  late final AnimationController animation = AnimationController(
    vsync: this,
    duration: FloeMotion.selectionDuration,
  )..forward();

  @override
  void didUpdateWidget(FloeScreenEntrance oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.identity != oldWidget.identity) animation.forward(from: 0);
  }

  @override
  void dispose() {
    animation.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    if (FloeMotion.reduceMotion(context)) return widget.child;
    return AnimatedBuilder(
      animation: animation,
      child: widget.child,
      builder: (context, child) {
        final progress = FloeMotion.easeOut.transform(animation.value);
        return Opacity(
          opacity: progress,
          child: Transform.translate(
            offset: Offset(0, 3 * (1 - progress)),
            child: child,
          ),
        );
      },
    );
  }
}

final class PressableScale extends StatefulWidget {
  const PressableScale({
    required this.builder,
    this.scale = 0.97,
    this.alignment = Alignment.center,
    super.key,
  }) : assert(scale > 0 && scale <= 1);

  final Widget Function(WidgetStatesController states) builder;
  final double scale;
  final Alignment alignment;

  @override
  State<PressableScale> createState() => _PressableScaleState();
}

final class _PressableScaleState extends State<PressableScale>
    with SingleTickerProviderStateMixin {
  late final AnimationController animation = AnimationController(
    vsync: this,
    value: 1,
    lowerBound: 0,
    upperBound: 1,
  );
  late final WidgetStatesController states = WidgetStatesController()
    ..addListener(_onStatesChanged);
  bool reduceMotion = false;
  bool updateScheduled = false;

  void _onStatesChanged() {
    if (SchedulerBinding.instance.schedulerPhase !=
        SchedulerPhase.persistentCallbacks) {
      _updateScale();
      return;
    }
    if (updateScheduled) return;
    updateScheduled = true;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      updateScheduled = false;
      if (mounted) _updateScale();
    });
  }

  void _updateScale() {
    final pressed =
        states.value.contains(WidgetState.pressed) &&
        !states.value.contains(WidgetState.disabled);
    final target = pressed && !reduceMotion ? widget.scale : 1.0;
    if (animation.value == target && !animation.isAnimating) return;
    if (reduceMotion || states.value.contains(WidgetState.disabled)) {
      animation.value = target;
    } else {
      animation.animateTo(
        target,
        duration: FloeMotion.pressDuration,
        curve: FloeMotion.easeOut,
      );
    }
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    reduceMotion = FloeMotion.reduceMotion(context);
    _updateScale();
  }

  @override
  void didUpdateWidget(PressableScale oldWidget) {
    super.didUpdateWidget(oldWidget);
    _updateScale();
  }

  @override
  void dispose() {
    states.dispose();
    animation.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return ScaleTransition(
      scale: animation,
      alignment: widget.alignment,
      child: widget.builder(states),
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
