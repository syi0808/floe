import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_motion.dart';
import 'package:floe_client/app/floe_squircle.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:figma_squircle/figma_squircle.dart';

void main() {
  test('theme removes ripple and resolves quiet hover states', () {
    final theme = FloeTheme.light;
    final textStyle = theme.textButtonTheme.style!;

    expect(theme.splashFactory, NoSplash.splashFactory);
    expect(theme.splashColor, Colors.transparent);
    expect(theme.highlightColor, Colors.transparent);
    expect(
      textStyle.backgroundColor!.resolve({WidgetState.hovered}),
      FloePalette.neutral100,
    );
    expect(
      textStyle.backgroundColor!.resolve({WidgetState.pressed}),
      FloePalette.neutral200,
    );
    expect(
      textStyle.overlayColor!.resolve({WidgetState.pressed}),
      Colors.transparent,
    );
    expect(
      theme.filledButtonTheme.style!.minimumSize!.resolve({}),
      const Size(44, 44),
    );
    expect(
      theme.filledButtonTheme.style!.shape!.resolve({}),
      isA<SmoothRectangleBorder>(),
    );
  });

  testWidgets('shared squircle owns continuous fill and clipping', (
    tester,
  ) async {
    await tester.pumpWidget(
      const MaterialApp(
        home: FloeSquircle(
          size: FloeSquircleSize.lg,
          child: SizedBox.square(dimension: 80),
        ),
      ),
    );

    final material = tester.widget<Material>(
      find.descendant(
        of: find.byType(FloeSquircle),
        matching: find.byType(Material),
      ),
    );
    expect(material.shape, isA<SmoothRectangleBorder>());
    final shape = material.shape! as SmoothRectangleBorder;
    expect(shape.borderRadius.topLeft.cornerSmoothing, .82);
    expect(floeSquircleBorder(FloeSquircleSize.md).side, BorderSide.none);
    expect(material.clipBehavior, Clip.antiAlias);
  });

  testWidgets('pointer press scales and releases with no lasting transform', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        home: Center(
          child: PressableScale(
            child: FilledButton(onPressed: () {}, child: const Text('담기')),
          ),
        ),
      ),
    );

    final gesture = await tester.startGesture(
      tester.getCenter(find.text('담기')),
      kind: PointerDeviceKind.mouse,
    );
    await tester.pump();
    expect(
      tester.widget<AnimatedScale>(find.byType(AnimatedScale)).scale,
      0.97,
    );

    await gesture.up();
    await tester.pump();
    expect(tester.widget<AnimatedScale>(find.byType(AnimatedScale)).scale, 1);
  });

  testWidgets('reduced motion keeps press feedback stationary', (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        home: MediaQuery(
          data: const MediaQueryData(disableAnimations: true),
          child: Center(
            child: PressableScale(
              child: FilledButton(onPressed: () {}, child: const Text('담기')),
            ),
          ),
        ),
      ),
    );

    final gesture = await tester.startGesture(
      tester.getCenter(find.text('담기')),
      kind: PointerDeviceKind.mouse,
    );
    await tester.pump();
    expect(tester.widget<AnimatedScale>(find.byType(AnimatedScale)).scale, 1);
    await gesture.up();
  });
}
