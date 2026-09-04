import 'package:floe_client/app/floe_theme.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('tooltips wait two seconds before appearing on hover', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        home: Scaffold(
          body: Center(
            child: IconButton(
              tooltip: 'Settings',
              onPressed: () {},
              icon: const Icon(Icons.settings),
            ),
          ),
        ),
      ),
    );

    final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await mouse.addPointer(location: Offset.zero);
    addTearDown(mouse.removePointer);
    await mouse.moveTo(tester.getCenter(find.byType(IconButton)));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 1999));
    expect(find.text('Settings'), findsNothing);

    await tester.pump(const Duration(milliseconds: 1));
    await tester.pumpAndSettle();
    expect(find.text('Settings'), findsOneWidget);
  });
}
