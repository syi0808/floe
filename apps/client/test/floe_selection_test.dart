import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_selection.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

void main() {
  testWidgets(
    'custom checkbox has semantics, keyboard toggle and disabled guard',
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
      expect(find.byType(Checkbox), findsNothing);
      expect(
        tester.getSemantics(find.byType(FloeCheckbox)).label,
        'Complete task',
      );
      update(() => enabled = false);
      await tester.pumpAndSettle();
      await tester.tap(find.byType(FloeCheckbox));
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
    final visual = find.byKey(const ValueKey('floe-choice-visual'));
    final originalSize = tester.getSize(visual);
    final gesture = await tester.startGesture(
      tester.getCenter(find.byType(FloeCheckbox)),
    );
    await tester.pump();
    expect(tester.getSize(visual), originalSize);
    await gesture.up();
    await tester.pumpAndSettle();
    expect(tester.getSize(visual), originalSize);
  });

  testWidgets('pointer focus and hover move off the previous choice', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        home: Scaffold(
          body: Column(
            children: [
              FloeCheckboxTile(
                value: false,
                title: const Text('First'),
                onChanged: (_) {},
              ),
              FloeCheckboxTile(
                value: false,
                title: const Text('Second'),
                onChanged: (_) {},
              ),
            ],
          ),
        ),
      ),
    );

    List<double> focusOpacities() => tester
        .widgetList<AnimatedOpacity>(
          find.byKey(const ValueKey('floe-choice-focus-ring')),
        )
        .map((widget) => widget.opacity)
        .toList();
    List<Color?> hoverColors() => tester
        .widgetList<Container>(
          find.byKey(const ValueKey('floe-choice-hover-surface')),
        )
        .map((widget) => (widget.decoration! as BoxDecoration).color)
        .toList();

    await tester.sendKeyEvent(LogicalKeyboardKey.tab);
    await tester.pumpAndSettle();
    expect(focusOpacities(), [1, 0]);
    await tester.tap(find.text('Second'));
    await tester.pumpAndSettle();
    expect(focusOpacities(), [0, 0]);

    final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await mouse.addPointer(location: Offset.zero);
    addTearDown(mouse.removePointer);
    await mouse.moveTo(tester.getCenter(find.text('First')));
    await tester.pump();
    expect(hoverColors(), [FloePalette.primary50, Colors.transparent]);
    await mouse.moveTo(tester.getCenter(find.text('Second')));
    await tester.pump();
    expect(hoverColors(), [Colors.transparent, FloePalette.primary50]);
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
    expect(find.byType(AnimatedScale), findsNothing);
    await tester.sendKeyEvent(LogicalKeyboardKey.tab);
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 80));
    final scale = tester.widget<ScaleTransition>(
      find.byKey(const ValueKey('floe-selection-scale')),
    );
    expect(scale.alignment, Alignment.topCenter);
    expect(scale.scale.value, greaterThan(.97));
    expect(scale.scale.value, lessThan(1));
    await tester.pumpAndSettle();
    expect(
      find.byKey(const ValueKey('floe-selection-option')),
      findsNWidgets(3),
    );
    expect(
      tester.getSize(find.byKey(const ValueKey('floe-selection-popup'))).width,
      greaterThan(280),
    );
    List<Color?> optionColors() => tester
        .widgetList<Container>(
          find.byKey(const ValueKey('floe-selection-option')),
        )
        .map((widget) => (widget.decoration! as ShapeDecoration).color)
        .toList();
    final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await mouse.addPointer(location: Offset.zero);
    addTearDown(mouse.removePointer);
    await mouse.moveTo(tester.getCenter(find.text('Home').last));
    await tester.pump();
    expect(optionColors(), [
      FloePalette.primary50,
      Colors.transparent,
      Colors.transparent,
    ]);
    await mouse.moveTo(tester.getCenter(find.text('Work')));
    await tester.pump();
    expect(optionColors(), [
      Colors.transparent,
      FloePalette.primary50,
      Colors.transparent,
    ]);
    await tester.tap(find.text('Team'));
    await tester.pumpAndSettle();
    expect(selected, 'home');
    await tester.tap(find.text('Work'));
    await tester.pump();
    expect(find.byKey(const ValueKey('floe-selection-popup')), findsOneWidget);
    await tester.pump(const Duration(milliseconds: 40));
    final fadingOut = tester.widget<FadeTransition>(
      find.byKey(const ValueKey('floe-selection-fade')),
    );
    expect(fadingOut.opacity.value, greaterThan(0));
    expect(fadingOut.opacity.value, lessThan(1));
    await tester.pumpAndSettle();
    expect(selected, 'work');
    expect(find.byKey(const ValueKey('floe-selection-popup')), findsNothing);
  });

  testWidgets('disabled select ignores pointer hover', (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        home: const Scaffold(
          body: SizedBox(
            width: 320,
            child: FloeSelect<String>(
              label: 'Target calendar',
              value: 'home',
              options: [FloeSelectOption(value: 'home', label: 'Home')],
              enabled: false,
              onChanged: _ignoreSelection,
            ),
          ),
        ),
      ),
    );

    ShapeDecoration triggerDecoration() =>
        tester
                .widget<AnimatedContainer>(find.byType(AnimatedContainer))
                .decoration!
            as ShapeDecoration;

    final initialDecoration = triggerDecoration();
    final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await mouse.addPointer(location: Offset.zero);
    addTearDown(mouse.removePointer);
    await mouse.moveTo(tester.getCenter(find.text('Home')));
    await tester.pumpAndSettle();

    expect(triggerDecoration().color, initialDecoration.color);
    expect(triggerDecoration().shape, initialDecoration.shape);
  });

  testWidgets('pointer hover does not scroll an open select', (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        home: Scaffold(
          body: SizedBox(
            width: 320,
            child: FloeSelect<int>(
              label: 'Long list',
              value: 0,
              options: [
                for (var index = 0; index < 12; index++)
                  FloeSelectOption(value: index, label: 'Option $index'),
              ],
              onChanged: (_) {},
            ),
          ),
        ),
      ),
    );

    await tester.tap(find.text('Option 0'));
    await tester.pumpAndSettle();
    final scrollable = find.descendant(
      of: find.byKey(const ValueKey('floe-selection-popup')),
      matching: find.byType(Scrollable),
    );
    await tester.drag(scrollable, const Offset(0, -100));
    await tester.pumpAndSettle();
    final position = tester.state<ScrollableState>(scrollable).position;
    final beforeHover = position.pixels;
    final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await mouse.addPointer(location: Offset.zero);
    addTearDown(mouse.removePointer);
    await mouse.moveTo(tester.getCenter(find.text('Option 5')));
    await tester.pumpAndSettle();
    expect(position.pixels, beforeHover);
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

void _ignoreSelection(String? _) {}
