import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_input.dart';
import 'package:floe_client/app/floe_selection.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('input floats its label and uses the shared hover treatment', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        home: const Scaffold(
          body: SizedBox(width: 320, child: FloeInput(label: 'Name')),
        ),
      ),
    );

    final label = find.text('Name');
    final restingTop = tester.getTopLeft(label).dy;
    final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await mouse.addPointer(location: Offset.zero);
    addTearDown(mouse.removePointer);
    await mouse.moveTo(tester.getCenter(find.byType(TextFormField)));
    await tester.pumpAndSettle();

    final decorator = tester.widget<InputDecorator>(
      find.byType(InputDecorator),
    );
    expect(decorator.isHovering, isTrue);
    expect(decorator.decoration.hoverColor, FloePalette.neutral50);

    await tester.tap(find.byType(TextFormField));
    await tester.pumpAndSettle();
    expect(tester.getTopLeft(label).dy, lessThan(restingTop));
  });

  testWidgets('select shares input size and floating-label interaction', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        home: Scaffold(
          body: SizedBox(
            width: 320,
            child: Column(
              children: [
                const FloeInput(label: 'Name'),
                FloeSelect<String>(
                  label: 'Calendar',
                  value: null,
                  options: const [
                    FloeSelectOption(value: 'home', label: 'Home'),
                  ],
                  onChanged: (_) {},
                ),
              ],
            ),
          ),
        ),
      ),
    );

    final inputHeight = tester.getSize(find.byType(TextFormField)).height;
    final select = find.byKey(const ValueKey('floe-selection-input-decorator'));
    expect(tester.getSize(select).height, inputHeight);

    final label = find.text('Calendar');
    final restingTop = tester.getTopLeft(label).dy;
    expect(find.text('Choose an option'), findsNothing);
    await tester.tap(select);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 200));

    final decorator = tester.widget<InputDecorator>(select);
    expect(decorator.isFocused, isTrue);
    expect(tester.getTopLeft(label).dy, lessThan(restingTop));
    expect(find.text('Choose an option'), findsOneWidget);
  });
}
