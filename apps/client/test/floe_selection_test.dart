import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_selection.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

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

  testWidgets('selection controls never scale on pointer down', (tester) async {
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
    expect(find.byType(AnimatedScale), findsNothing);
    await gesture.up();
    await tester.pumpAndSettle();
  });

  testWidgets('custom select commits only enabled options', (tester) async {
    String? selected = 'home';
    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        home: Scaffold(
          body: SizedBox(
            width: 320,
            child: StatefulBuilder(
              builder: (context, setState) => FloeSelect<String>(
                label: 'Target calendar',
                value: selected,
                options: const [
                  FloeSelectOption(value: 'home', label: 'Home'),
                  FloeSelectOption(value: 'work', label: 'Work'),
                  FloeSelectOption(
                    value: 'team',
                    label: 'Team',
                    enabled: false,
                  ),
                ],
                onChanged: (value) => setState(() => selected = value),
              ),
            ),
          ),
        ),
      ),
    );

    expect(find.byType(DropdownButton<String>), findsNothing);
    await tester.sendKeyEvent(LogicalKeyboardKey.tab);
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await tester.pumpAndSettle();
    expect(find.byType(MenuItemButton), findsNWidgets(3));
    await tester.tap(find.text('Team'));
    await tester.pumpAndSettle();
    expect(selected, 'home');
    await tester.tap(find.text('Work'));
    await tester.pumpAndSettle();
    expect(selected, 'work');
    expect(find.byType(MenuItemButton), findsNothing);
  });

  testWidgets('custom dropdown invokes its selected action', (tester) async {
    var calls = 0;
    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        home: Scaffold(
          body: FloeDropdown<bool>(
            label: 'Task options',
            icon: const Icon(LucideIcons.ellipsis),
            items: const [FloeSelectOption(value: true, label: 'Complete')],
            onSelected: (_) => calls++,
          ),
        ),
      ),
    );

    expect(find.byType(PopupMenuButton<bool>), findsNothing);
    await tester.tap(find.byIcon(LucideIcons.ellipsis));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Complete'));
    await tester.pumpAndSettle();
    expect(calls, 1);
  });
}
