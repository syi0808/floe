import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_selection.dart';
import 'package:floe_client/app/floe_theme.dart';

void main() {
  test('disabled selection uses the subdued prototype palette', () {
    final theme = FloeTheme.light.checkboxTheme;
    const disabled = {WidgetState.disabled, WidgetState.selected};
    expect(theme.fillColor!.resolve(disabled), FloePalette.neutral50);
    expect(theme.checkColor!.resolve(disabled), FloePalette.neutral300);
    expect(
      WidgetStateProperty.resolveAs<BorderSide?>(theme.side, disabled)!.color,
      FloePalette.neutral200,
    );
    expect(
      theme.fillColor!.resolve({WidgetState.selected}),
      FloePalette.primary600,
    );
  });

  testWidgets(
    'checkbox has native semantics, keyboard toggle and disabled guard',
    (tester) async {
      var checked = false;
      var enabled = true;
      late StateSetter update;
      await tester.pumpWidget(
        MaterialApp(
          theme: FloeTheme.light,
          home: Scaffold(
            body: StatefulBuilder(
              builder: (context, setState) {
                update = setState;
                return FloeCheckbox(
                  value: checked,
                  semanticLabel: 'Complete task',
                  onChanged: enabled
                      ? (value) => setState(() => checked = value!)
                      : null,
                );
              },
            ),
          ),
        ),
      );
      await tester.sendKeyEvent(LogicalKeyboardKey.tab);
      await tester.sendKeyEvent(LogicalKeyboardKey.space);
      await tester.pumpAndSettle();
      expect(checked, isTrue);
      expect(
        tester.widget<Checkbox>(find.byType(Checkbox)).semanticLabel,
        'Complete task',
      );
      update(() => enabled = false);
      await tester.pumpAndSettle();
      await tester.tap(find.byType(Checkbox));
      await tester.pumpAndSettle();
      expect(checked, isTrue);
    },
  );

  testWidgets('radio group uses arrows and maintains one selection', (
    tester,
  ) async {
    var all = true;
    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        home: Scaffold(
          body: StatefulBuilder(
            builder: (context, setState) => RadioGroup<bool>(
              groupValue: all,
              onChanged: (value) => setState(() => all = value!),
              child: const Column(
                children: [
                  FloeRadioTile(value: true, title: Text('All')),
                  FloeRadioTile(value: false, title: Text('Selected')),
                ],
              ),
            ),
          ),
        ),
      ),
    );
    await tester.sendKeyEvent(LogicalKeyboardKey.tab);
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await tester.pumpAndSettle();
    expect(all, isFalse);
    await tester.tap(find.text('All'));
    await tester.pumpAndSettle();
    expect(all, isTrue);
  });

  testWidgets('press feedback is suppressed with reduced motion', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        home: MediaQuery(
          data: const MediaQueryData(disableAnimations: true),
          child: Scaffold(body: FloeCheckbox(value: true, onChanged: (_) {})),
        ),
      ),
    );
    final gesture = await tester.startGesture(
      tester.getCenter(find.byType(Checkbox)),
    );
    await tester.pump();
    expect(tester.widget<AnimatedScale>(find.byType(AnimatedScale)).scale, 1);
    await gesture.up();
    await tester.pumpAndSettle();
  });
}
