import 'package:floe_client/app/floe_app.dart';
import 'package:floe_client/features/day_canvas/domain/day_models.dart';
import 'package:floe_client/preview/prototype_fixture.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('anchored suggestion reserves a break through the gateway', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(1440, 1100);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    final gateway = prototypeGateway();
    await tester.pumpWidget(
      prototypeAppearance(FloeApp(gateway: gateway, query: prototypeQuery)),
    );
    await tester.pumpAndSettle();
    final anchor = tester.getRect(find.byTooltip('Floe 제안 열기'));
    await tester.tap(find.byTooltip('Floe 제안 열기'));
    await tester.pumpAndSettle();
    expect(
      tester.getTopLeft(find.text('Floe suggestion')).dx,
      greaterThan(anchor.left),
    );
    await tester.tap(find.text('Add break'));
    await tester.pumpAndSettle();
    final snapshot = await gateway.loadDay(prototypeQuery);
    final reserved = snapshot.items.whereType<EventItem>().singleWhere(
      (event) => event.title == 'Reserved break',
    );
    expect(reserved.startsAt, DateTime.utc(2026, 9, 3, 15));
    expect(reserved.endsAt, DateTime.utc(2026, 9, 3, 15, 20));
    expect(find.byTooltip('Floe 제안 열기'), findsNothing);
    expect(tester.takeException(), isNull);
  });

  testWidgets('notes filter and search use the displayed content', (
    tester,
  ) async {
    await tester.pumpWidget(
      prototypeAppearance(
        FloeApp(gateway: prototypeGateway(), query: prototypeQuery),
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Notes'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Filter'));
    await tester.pumpAndSettle();
    expect(find.text('Gratitude'), findsOneWidget);
    expect(find.text('Launch plan'), findsNothing);
    await tester.enterText(find.byType(TextField), 'Slow down');
    await tester.pumpAndSettle();
    expect(find.text('Evening reset'), findsOneWidget);
    expect(find.text('Gratitude'), findsNothing);
    expect(tester.takeException(), isNull);
  });
}
