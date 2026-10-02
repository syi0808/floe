import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/preview/design_system_catalog.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('catalog exposes the reusable component sections', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(1024, 1200);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);

    await tester.pumpWidget(
      MaterialApp(
        debugShowCheckedModeBanner: false,
        theme: FloeTheme.light,
        home: const DesignSystemCatalog(),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('Interaction colors'), findsOneWidget);
    expect(find.text('Typography'), findsOneWidget);
    expect(find.text('Status badges'), findsOneWidget);
    expect(find.text('Buttons'), findsOneWidget);
    expect(find.text('Fields'), findsOneWidget);
    expect(find.text('Selection'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
}
