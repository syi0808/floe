import 'package:floe_client/app/floe_app.dart';
import 'package:floe_client/app/floe_squircle.dart';
import 'package:floe_client/features/day_canvas/application/fake_day_gateway.dart';
import 'package:floe_client/features/day_canvas/domain/day_models.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  for (final width in [390.0, 1440.0, 1920.0]) {
    testWidgets('workspace fills the window without a frame at $width', (
      tester,
    ) async {
      tester.view.physicalSize = Size(width, 1000);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final date = DateTime.utc(2026, 9, 4);
      await tester.pumpWidget(
        FloeApp(
          gateway: FakeDayGateway(),
          query: DayQuery(
            personId: 'test',
            date: date,
            now: date,
            timezoneOffsetSeconds: 0,
          ),
        ),
      );
      await tester.pumpAndSettle();
      final scaffold = tester.widget<Scaffold>(find.byType(Scaffold));
      final safeArea = scaffold.body! as SafeArea;
      expect(safeArea.child, isA<Stack>());
      expect(
        tester.getRect(find.byWidget(safeArea.child)),
        Rect.fromLTWH(0, 0, width, 1000),
      );
      expect(
        find.byWidgetPredicate(
          (widget) =>
              widget is FloeSquircle && widget.size == FloeSquircleSize.frame,
        ),
        findsNothing,
      );
      expect(find.byTooltip('Settings'), findsOneWidget);
      await tester.tap(find.byTooltip('Settings'));
      await tester.pumpAndSettle();
      expect(find.text('Connections'), findsWidgets);
      expect(tester.takeException(), isNull);
    });
  }
}
