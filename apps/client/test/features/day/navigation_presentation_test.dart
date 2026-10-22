import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/floe_app.dart';
import 'package:floe_client/app/floe_selection.dart';
import 'package:floe_client/app/runtime/local_owner_gateways_scope.dart';
import 'package:floe_client/preview/calendar_fixture.dart';
import 'package:floe_client/preview/product_fixture.dart';

void main() {
  testWidgets(
    'desktop navigation stays reachable in a short window with large text',
    (tester) async {
      tester.view.physicalSize = const Size(1200, 1000);
      tester.view.devicePixelRatio = 1;
      tester.platformDispatcher.textScaleFactorTestValue = 2;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      addTearDown(tester.platformDispatcher.clearTextScaleFactorTestValue);
      await tester.pumpWidget(
        previewAppearance(
          FloeApp(
            personId: calendarPreviewQuery.personId,
            gateway: calendarPreviewGateway(),
            query: calendarPreviewQuery,
            ownerGateways: const LocalOwnerGateways(),
          ),
        ),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.byTooltip('Notes'));
      await tester.pumpAndSettle();
      tester.view.physicalSize = const Size(1200, 400);
      await tester.pumpAndSettle();
      await tester.ensureVisible(find.byTooltip('More'));
      await tester.pumpAndSettle();
      expect(
        tester.getRect(find.byTooltip('More')).bottom,
        lessThanOrEqualTo(400),
      );
      await tester.tap(find.byTooltip('More'));
      await tester.pumpAndSettle();
      expect(find.text('Activity'), findsOneWidget);
      expect(find.text('Settings'), findsOneWidget);
      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox.shrink());
    },
  );

  for (final width in [320.0, 390.0, 1200.0]) {
    for (final largeText in [false, true]) {
      testWidgets(
        'three destinations retain filters and utility access at $width, large text $largeText',
        (tester) async {
          tester.view.physicalSize = Size(width, 1000);
          tester.view.devicePixelRatio = 1;
          addTearDown(tester.view.resetPhysicalSize);
          addTearDown(tester.view.resetDevicePixelRatio);
          await tester.pumpWidget(
            previewAppearance(
              FloeApp(
                personId: calendarPreviewQuery.personId,
                gateway: calendarPreviewGateway(),
                query: calendarPreviewQuery,
                ownerGateways: const LocalOwnerGateways(),
                builder: (context, child) => MediaQuery(
                  data: MediaQuery.of(
                    context,
                  ).copyWith(textScaler: TextScaler.linear(largeText ? 2 : 1)),
                  child: child!,
                ),
              ),
            ),
          );
          await tester.pumpAndSettle();
          expect(find.byTooltip('Today'), findsOneWidget);
          expect(find.byTooltip('Tasks'), findsOneWidget);
          expect(find.byTooltip('Notes'), findsOneWidget);
          expect(find.byTooltip('Voice'), findsNothing);
          expect(find.text('Activity'), findsNothing);
          if (width <= 780) {
            expect(find.byKey(const Key('timeline-card')), findsNothing);
            await tester.tap(find.text('Timeline'));
            await tester.pumpAndSettle();
            expect(find.byKey(const Key('timeline-card')), findsOneWidget);
            await tester.tap(find.byKey(const Key('day-status-details')));
            await tester.pumpAndSettle();
            expect(find.text('Calendar status unavailable'), findsNWidgets(2));
            await tester.tap(find.byType(FloeCheckbox));
            await tester.pumpAndSettle();
            expect(find.text('No tasks for this day.'), findsOneWidget);
            await tester.tap(find.byKey(const Key('day-details-close')));
            await tester.pumpAndSettle();
            await tester.tap(find.byTooltip('Close')); // Completion toast.
            await tester.pumpAndSettle();
            await tester.tap(find.text('Schedule'));
            await tester.pumpAndSettle();
            expect(find.byKey(const Key('timeline-card')), findsNothing);
          }

          await tester.tap(find.byTooltip('Notes'));
          await tester.pumpAndSettle();
          await tester.enterText(
            find.byType(TextField).first,
            'no matching note',
          );
          await tester.pumpAndSettle();
          expect(find.text('Matching notes · 0'), findsOneWidget);
          await tester.tap(find.byTooltip('Tasks'));
          await tester.pumpAndSettle();
          await tester.tap(find.text('Prepare launch brief'));
          await tester.pumpAndSettle();
          expect(find.text('Review now'), findsNothing);
          await tester.tap(find.text('Back'));
          await tester.pumpAndSettle();
          expect(find.text('Prepare launch brief'), findsOneWidget);
          await tester.tap(find.byTooltip('Notes'));
          await tester.pumpAndSettle();
          expect(find.text('Matching notes · 0'), findsOneWidget);
          // Floe is an invocation, not another destination. Closing keeps context.
          await tester.tap(find.text('Floe').last);
          await tester.pumpAndSettle();
          expect(
            find.text('Conversation is unavailable in this session.'),
            findsOneWidget,
          );
          if (width > 960) {
            await tester.sendKeyEvent(LogicalKeyboardKey.escape);
          } else {
            await tester.tap(find.byTooltip('Close'));
          }
          await tester.pumpAndSettle();
          expect(find.text('Matching notes · 0'), findsOneWidget);
          await tester.tap(find.byTooltip('More'));
          await tester.pumpAndSettle();
          expect(find.text('Activity'), findsOneWidget);
          expect(find.text('Connected services'), findsOneWidget);
          expect(find.text('Settings'), findsOneWidget);
          await tester.sendKeyEvent(LogicalKeyboardKey.escape);
          await tester.pumpAndSettle();
          expect(tester.takeException(), isNull);
          await tester.pumpWidget(const SizedBox.shrink());
        },
      );
    }
  }
}
