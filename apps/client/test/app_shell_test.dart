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
      final height = width == 390 ? 700.0 : 768.0;
      tester.view.physicalSize = Size(width, height);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final date = DateTime.utc(2026, 9, 4);
      await tester.pumpWidget(
        FloeApp(
          gateway: FakeDayGateway(
            initialItems: width == 1920
                ? []
                : [
                    for (var index = 0; index < 6; index++)
                      EventItem(
                        id: 'all-day-$index',
                        title: 'All-day event $index',
                        revision: 0,
                        createdAt: date,
                        startsAt: date,
                        endsAt: date.add(const Duration(days: 1)),
                        isAllDay: true,
                      ),
                    EventItem(
                      id: 'timed',
                      title: 'Timed event',
                      revision: 0,
                      createdAt: date,
                      startsAt: date.add(const Duration(hours: 9)),
                      endsAt: date.add(const Duration(hours: 10)),
                    ),
                  ],
          ),
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
        Rect.fromLTWH(0, 0, width, height),
      );
      expect(
        find.byWidgetPredicate(
          (widget) =>
              widget is FloeSquircle && widget.size == FloeSquircleSize.frame,
        ),
        findsNothing,
      );
      expect(find.byTooltip('Settings'), findsOneWidget);
      final timeline = find.byKey(const Key('timeline-card'));
      expect(tester.getRect(timeline).bottom, lessThan(height - 60));
      expect(tester.getRect(timeline).top, lessThan(140));
      expect(
        find.ancestor(
          of: timeline,
          matching: find.byType(SingleChildScrollView),
        ),
        findsNothing,
      );
      final settingsRect = tester.getRect(find.byTooltip('Settings'));
      if (width != 1920) {
        final scroll = find.byKey(const Key('calendar-scroll'));
        final controller = tester
            .widget<SingleChildScrollView>(scroll)
            .controller!;
        final previousOffset = controller.offset;
        final previousBounds = tester.getRect(timeline);
        await tester.drag(scroll, const Offset(0, -80));
        await tester.pumpAndSettle();
        expect(controller.offset, greaterThan(previousOffset));
        expect(tester.getRect(timeline), previousBounds);
      }
      if (width > 780) {
        expect(settingsRect.right, lessThan(100));
      } else {
        expect(settingsRect.top, greaterThan(height - 100));
      }
      await tester.tap(find.byTooltip('Settings'));
      await tester.pumpAndSettle();
      expect(find.text('Connections'), findsWidgets);
      expect(tester.takeException(), isNull);
    });
  }
}
