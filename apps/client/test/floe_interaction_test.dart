import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_motion.dart';
import 'package:floe_client/app/floe_button.dart';
import 'package:floe_client/app/floe_squircle.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:figma_squircle/figma_squircle.dart';

void main() {
  test('button themes use pointer only for enabled buttons', () {
    final theme = FloeTheme.light;
    for (final style in [
      theme.filledButtonTheme.style!,
      theme.outlinedButtonTheme.style!,
      theme.textButtonTheme.style!,
      theme.iconButtonTheme.style!,
      theme.segmentedButtonTheme.style!,
    ]) {
      expect(
        style.mouseCursor!.resolve({WidgetState.hovered}),
        SystemMouseCursors.click,
      );
      expect(
        style.mouseCursor!.resolve({WidgetState.disabled}),
        SystemMouseCursors.basic,
      );
    }
  });

  for (final variant in ['filled', 'outlined', 'text', 'icon']) {
    testWidgets('$variant shares scale, disabled and cancellation behavior', (
      tester,
    ) async {
      var clicks = 0;
      Widget button(bool enabled) {
        final VoidCallback? action = enabled ? () => clicks++ : null;
        return MaterialApp(
          theme: FloeTheme.light,
          home: Center(
            child: switch (variant) {
              'filled' => FloeButton.filled(
                onPressed: action,
                child: const Text('Press'),
              ),
              'outlined' => FloeButton.outlined(
                onPressed: action,
                child: const Text('Press'),
              ),
              'text' => FloeButton.text(
                onPressed: action,
                child: const Text('Press'),
              ),
              _ => FloeButton.icon(
                onPressed: action,
                icon: const Icon(Icons.add),
              ),
            },
          ),
        );
      }

      double scale() => tester
          .widget<ScaleTransition>(find.byType(ScaleTransition))
          .scale
          .value;
      await tester.pumpWidget(button(true));
      final size = tester.getSize(find.byType(FloeButton));
      final gesture = await tester.startGesture(
        tester.getCenter(find.byType(FloeButton)),
      );
      await tester.pumpAndSettle();
      expect(scale(), .97);
      expect(tester.getSize(find.byType(FloeButton)), size);
      await gesture.cancel();
      await tester.pumpAndSettle();
      expect(scale(), 1);
      expect(clicks, 0);
      await tester.pumpWidget(button(false));
      await tester.tap(find.byType(FloeButton));
      await tester.pumpAndSettle();
      expect(scale(), 1);
      expect(clicks, 0);
      await tester.pumpWidget(button(true));
      final held = await tester.startGesture(
        tester.getCenter(find.byType(FloeButton)),
      );
      await tester.pumpAndSettle();
      expect(scale(), .97);
      await tester.pumpWidget(button(false));
      await tester.pumpAndSettle();
      expect(scale(), 1);
      await held.up();
      await tester.pumpAndSettle();
      expect(clicks, 0);
      await tester.pumpWidget(button(true));
      await tester.tap(find.byType(FloeButton));
      await tester.pumpAndSettle();
      expect(clicks, 1);
    });
  }

  testWidgets('keyboard activation keeps native focus and fires once', (
    tester,
  ) async {
    final focus = FocusNode();
    addTearDown(focus.dispose);
    var clicks = 0;
    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        home: Center(
          child: FloeButton.filled(
            focusNode: focus,
            onPressed: () => clicks++,
            child: const Text('Press'),
          ),
        ),
      ),
    );
    focus.requestFocus();
    await tester.pump();
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 40));
    expect(
      tester.widget<ScaleTransition>(find.byType(ScaleTransition)).scale.value,
      lessThan(1),
    );
    await tester.pumpAndSettle();
    expect(clicks, 1);
    expect(focus.hasFocus, isTrue);
    expect(
      tester.widget<ScaleTransition>(find.byType(ScaleTransition)).scale.value,
      1,
    );
  });

  testWidgets('scroll gesture cancels press without activating button', (
    tester,
  ) async {
    var clicks = 0;
    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        home: SingleChildScrollView(
          child: Column(
            children: [
              FloeButton.filled(
                onPressed: () => clicks++,
                child: const Text('Press'),
              ),
              const SizedBox(height: 1500),
            ],
          ),
        ),
      ),
    );
    final gesture = await tester.startGesture(
      tester.getCenter(find.byType(FloeButton)),
    );
    await tester.pumpAndSettle();
    await gesture.moveBy(const Offset(0, -100));
    await tester.pumpAndSettle();
    expect(
      tester.widget<ScaleTransition>(find.byType(ScaleTransition)).scale.value,
      1,
    );
    await gesture.up();
    await tester.pumpAndSettle();
    expect(clicks, 0);
  });

  testWidgets(
    'screen entrance replays only for navigation and respects reduced motion',
    (tester) async {
      Widget screen(String identity, {bool reduced = false}) => MaterialApp(
        home: MediaQuery(
          data: MediaQueryData(disableAnimations: reduced),
          child: FloeScreenEntrance(
            identity: identity,
            child: const Text('Content'),
          ),
        ),
      );
      await tester.pumpWidget(screen('today'));
      await tester.pumpAndSettle();
      final opacity = find.descendant(
        of: find.byType(FloeScreenEntrance),
        matching: find.byType(Opacity),
      );
      expect(tester.widget<Opacity>(opacity).opacity, 1);
      await tester.pumpWidget(screen('today'));
      expect(tester.widget<Opacity>(opacity).opacity, 1);
      await tester.pumpWidget(screen('notes'));
      expect(tester.widget<Opacity>(opacity).opacity, 0);
      await tester.pumpAndSettle();
      expect(tester.widget<Opacity>(opacity).opacity, 1);
      await tester.pumpWidget(screen('tasks', reduced: true));
      expect(opacity, findsNothing);
      expect(find.text('Content'), findsOneWidget);
      await tester.pumpAndSettle();
    },
  );

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
          child: FloeButton.filled(onPressed: () {}, child: const Text('담기')),
        ),
      ),
    );

    final gesture = await tester.startGesture(
      tester.getCenter(find.text('담기')),
      kind: PointerDeviceKind.mouse,
    );
    await tester.pumpAndSettle();
    expect(
      tester.widget<ScaleTransition>(find.byType(ScaleTransition)).scale.value,
      0.97,
    );

    await gesture.up();
    await tester.pumpAndSettle();
    expect(
      tester.widget<ScaleTransition>(find.byType(ScaleTransition)).scale.value,
      1,
    );
  });

  testWidgets('reduced motion keeps press feedback stationary', (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        home: MediaQuery(
          data: const MediaQueryData(disableAnimations: true),
          child: Center(
            child: FloeButton.filled(onPressed: () {}, child: const Text('담기')),
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
      tester.widget<ScaleTransition>(find.byType(ScaleTransition)).scale.value,
      1,
    );
    await gesture.up();
  });
}
