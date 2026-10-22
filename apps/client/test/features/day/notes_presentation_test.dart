import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/floe_app.dart';
import 'package:floe_client/app/runtime/local_owner_gateways_scope.dart';
import 'package:floe_client/features/day/application/fake_day_gateway.dart';
import 'package:floe_client/preview/calendar_fixture.dart';
import 'package:floe_client/preview/product_fixture.dart';

Future<void> openNotes(
  WidgetTester tester, {
  bool empty = false,
  double width = 1200,
}) async {
  tester.view.physicalSize = Size(width, 1000);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
  await tester.pumpWidget(
    previewAppearance(
      FloeApp(
        personId: calendarPreviewQuery.personId,
        gateway: empty ? FakeDayGateway() : calendarPreviewGateway(),
        query: calendarPreviewQuery,
        ownerGateways: const LocalOwnerGateways(),
      ),
    ),
  );
  await tester.pumpAndSettle();
  await tester.tap(find.byTooltip('Notes'));
  await tester.pumpAndSettle();
}

void main() {
  testWidgets(
    'empty collection does not ask users to clear nonexistent filters',
    (tester) async {
      await openNotes(tester, empty: true);
      expect(find.text('A place for your thoughts'), findsOneWidget);
      expect(find.text('Clear filters'), findsNothing);
      expect(find.text('New note'), findsOneWidget);
      await tester.tap(find.text('New note'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Cancel'));
      await tester.pumpAndSettle();
      expect(find.text('A place for your thoughts'), findsOneWidget);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox.shrink());
    },
  );
  for (final width in [390.0, 1200.0]) {
    testWidgets('personal filter is explicit and reversible at $width', (
      tester,
    ) async {
      await openNotes(tester, width: width);
      expect(find.text('All notes · 1'), findsOneWidget);
      await tester.tap(find.text('Personal only'));
      await tester.pumpAndSettle();
      expect(find.text('Matching notes · 0'), findsOneWidget);
      expect(find.text('No notes found'), findsOneWidget);
      expect(find.text('Clear filters'), findsOneWidget);
      await tester.tap(find.text('Clear filters'));
      await tester.pumpAndSettle();
      expect(find.text('All notes · 1'), findsOneWidget);
      expect(find.text('Launch plan'), findsOneWidget);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox.shrink());
    });
  }
}
