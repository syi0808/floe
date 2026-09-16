import 'package:floe_client/app/floe_mascot.dart';
import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('idle mascot keeps the canonical static asset', (tester) async {
    await tester.pumpWidget(
      const MaterialApp(home: Center(child: FloeMascot())),
    );

    expect(find.byType(SvgPicture), findsOneWidget);
    expect(
      find.byWidgetPredicate(
        (widget) => widget is Semantics && widget.properties.label == 'Floe',
      ),
      findsOneWidget,
    );
  });

  testWidgets('active motion uses independent body and eye layers', (
    tester,
  ) async {
    await tester.pumpWidget(
      const MaterialApp(
        home: Center(
          child: FloeMascot(motion: FloeMascotMotion.talking),
        ),
      ),
    );
    await tester.pump(const Duration(milliseconds: 120));

    expect(find.byType(SvgPicture), findsNWidgets(2));
    expect(tester.takeException(), isNull);
  });

  testWidgets('reduced motion falls back to the canonical static asset', (
    tester,
  ) async {
    await tester.pumpWidget(
      const MaterialApp(
        home: MediaQuery(
          data: MediaQueryData(disableAnimations: true),
          child: Center(
            child: FloeMascot(motion: FloeMascotMotion.working),
          ),
        ),
      ),
    );
    await tester.pump(const Duration(seconds: 2));

    expect(find.byType(SvgPicture), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
}
