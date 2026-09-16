import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';

enum FloeMascotMotion {
  idle,
  listening,
  talking,
  thinking,
  working,
  success,
  error,
  acknowledgement,
}

final class FloeMascot extends StatefulWidget {
  const FloeMascot({
    this.size = 44,
    this.motion = FloeMascotMotion.idle,
    this.semanticLabel = 'Floe',
    super.key,
  });

  static const assetPath = 'assets/floe-mascot.svg';
  static const bodyAssetPath = 'assets/floe-mascot-body.svg';
  static const eyesAssetPath = 'assets/floe-mascot-eyes.svg';

  final double size;
  final FloeMascotMotion motion;
  final String semanticLabel;

  @override
  State<FloeMascot> createState() => _FloeMascotState();
}

final class _FloeMascotState extends State<FloeMascot>
    with SingleTickerProviderStateMixin {
  late final AnimationController _controller = AnimationController(vsync: this);

  bool _reducedMotion = false;
  bool _configured = false;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final reducedMotion =
        MediaQuery.maybeOf(context)?.disableAnimations ?? false;
    if (!_configured || reducedMotion != _reducedMotion) {
      _reducedMotion = reducedMotion;
      _configured = true;
      _configureMotion();
    }
  }

  @override
  void didUpdateWidget(covariant FloeMascot oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.motion != widget.motion) {
      _configureMotion();
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  void _configureMotion() {
    _controller.stop();

    if (_reducedMotion || widget.motion == FloeMascotMotion.idle) {
      _controller.value = 0;
      return;
    }

    _controller.duration = _durationFor(widget.motion);
    if (_repeats(widget.motion)) {
      _controller.repeat();
    } else {
      _controller.forward(from: 0);
    }
  }

  @override
  Widget build(BuildContext context) => Semantics(
    label: widget.semanticLabel,
    image: true,
    child: ExcludeSemantics(
      child: RepaintBoundary(
        child: SizedBox.square(
          dimension: widget.size,
          child: _reducedMotion || widget.motion == FloeMascotMotion.idle
              ? SvgPicture.asset(
                  FloeMascot.assetPath,
                  fit: BoxFit.contain,
                )
              : AnimatedBuilder(
                  animation: _controller,
                  builder: (context, child) {
                    final frame = _FloeMotionFrame.sample(
                      widget.motion,
                      _controller.value,
                    );
                    return _AnimatedMascotLayers(
                      frame: frame,
                      size: widget.size,
                    );
                  },
                ),
        ),
      ),
    ),
  );
}

final class _AnimatedMascotLayers extends StatelessWidget {
  const _AnimatedMascotLayers({required this.frame, required this.size});

  static const _eyeAlignment = Alignment(0, 0.073);

  final _FloeMotionFrame frame;
  final double size;

  @override
  Widget build(BuildContext context) {
    final layers = Stack(
      fit: StackFit.expand,
      clipBehavior: Clip.none,
      children: [
        SvgPicture.asset(
          FloeMascot.bodyAssetPath,
          fit: BoxFit.contain,
        ),
        Transform.translate(
          offset: Offset(frame.eyeDx * size, frame.eyeDy * size),
          child: Transform.scale(
            scaleX: frame.eyeScaleX,
            scaleY: frame.eyeScaleY,
            alignment: _eyeAlignment,
            child: SvgPicture.asset(
              FloeMascot.eyesAssetPath,
              fit: BoxFit.contain,
            ),
          ),
        ),
      ],
    );

    return Transform.translate(
      offset: Offset(frame.dx * size, frame.dy * size),
      child: Transform.rotate(
        angle: frame.rotation,
        alignment: Alignment.center,
        child: Transform.scale(
          scaleX: frame.scaleX,
          scaleY: frame.scaleY,
          alignment: Alignment.center,
          child: layers,
        ),
      ),
    );
  }
}

final class _FloeMotionFrame {
  const _FloeMotionFrame({
    this.dx = 0,
    this.dy = 0,
    this.rotation = 0,
    this.scaleX = 1,
    this.scaleY = 1,
    this.eyeDx = 0,
    this.eyeDy = 0,
    this.eyeScaleX = 1,
    this.eyeScaleY = 1,
  });

  final double dx;
  final double dy;
  final double rotation;
  final double scaleX;
  final double scaleY;
  final double eyeDx;
  final double eyeDy;
  final double eyeScaleX;
  final double eyeScaleY;

  static _FloeMotionFrame sample(FloeMascotMotion motion, double t) {
    final cycle = math.pi * 2 * t;
    final pulse = (1 - math.cos(cycle)) / 2;

    return switch (motion) {
      FloeMascotMotion.idle => const _FloeMotionFrame(),
      FloeMascotMotion.listening => _FloeMotionFrame(
        dy: -0.003 * pulse,
        scaleX: 1 - (0.006 * pulse),
        scaleY: 1 + (0.014 * pulse),
        eyeScaleY: 1 + (0.08 * pulse),
      ),
      FloeMascotMotion.talking => _FloeMotionFrame(
        dy: -0.004 * pulse,
        scaleX: 1 + (0.012 * math.sin(cycle)),
        scaleY: 1 - (0.010 * math.sin(cycle)),
        eyeScaleY: 1 + (0.03 * pulse),
      ),
      FloeMascotMotion.thinking => _FloeMotionFrame(
        dx: 0.004 * math.sin(cycle),
        rotation: 0.014 * math.sin(cycle),
        eyeDx: 0.018 * math.sin(cycle),
        eyeDy: -0.004 * pulse,
      ),
      FloeMascotMotion.working => _FloeMotionFrame(
        dy: -0.004 * pulse,
        rotation: 0.008 * math.sin(cycle),
        scaleX: 1 + (0.008 * pulse),
        scaleY: 1 + (0.012 * pulse),
        eyeScaleY: 1 + (0.04 * pulse),
      ),
      FloeMascotMotion.success => _FloeMotionFrame(
        dy: -0.018 * math.sin(math.pi * t),
        scaleX: 1 + (0.018 * math.sin(math.pi * t)),
        scaleY: 1 + (0.028 * math.sin(math.pi * t)),
        eyeScaleY: 1 + (0.12 * math.sin(math.pi * t)),
      ),
      FloeMascotMotion.error => _errorFrame(t),
      FloeMascotMotion.acknowledgement => _acknowledgementFrame(t),
    };
  }

  static _FloeMotionFrame _errorFrame(double t) {
    final shake = math.sin(6 * math.pi * t) * (1 - t);
    return _FloeMotionFrame(
      dx: 0.018 * shake,
      rotation: 0.010 * shake,
      eyeScaleY: 1 - (0.10 * math.sin(math.pi * t)),
    );
  }

  static _FloeMotionFrame _acknowledgementFrame(double t) {
    final normalizedBlink = (t - 0.45) / 0.10;
    final blink = math.exp(-(normalizedBlink * normalizedBlink));
    return _FloeMotionFrame(
      dy: -0.006 * math.sin(math.pi * t),
      rotation: -0.010 * math.sin(math.pi * t),
      eyeScaleY: 1 - (0.88 * blink),
    );
  }
}

Duration _durationFor(FloeMascotMotion motion) => switch (motion) {
  FloeMascotMotion.idle => Duration.zero,
  FloeMascotMotion.listening => const Duration(milliseconds: 1500),
  FloeMascotMotion.talking => const Duration(milliseconds: 720),
  FloeMascotMotion.thinking => const Duration(milliseconds: 1800),
  FloeMascotMotion.working => const Duration(milliseconds: 1300),
  FloeMascotMotion.success => const Duration(milliseconds: 300),
  FloeMascotMotion.error => const Duration(milliseconds: 320),
  FloeMascotMotion.acknowledgement => const Duration(milliseconds: 260),
};

bool _repeats(FloeMascotMotion motion) => switch (motion) {
  FloeMascotMotion.listening ||
  FloeMascotMotion.talking ||
  FloeMascotMotion.thinking ||
  FloeMascotMotion.working => true,
  FloeMascotMotion.idle ||
  FloeMascotMotion.success ||
  FloeMascotMotion.error ||
  FloeMascotMotion.acknowledgement => false,
};
