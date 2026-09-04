import 'package:floe_client/app/floe_app.dart';
import 'package:floe_client/app/floe_feedback.dart';
import 'package:floe_client/features/day_canvas/application/fake_day_gateway.dart';
import 'package:floe_client/features/day_canvas/domain/day_models.dart';
import 'package:floe_client/features/day_canvas/presentation/calendar_agenda.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final date = DateTime.utc(2026, 9, 4);
  final query = DayQuery(
    personId: 'test',
    date: date,
    now: date,
    timezoneOffsetSeconds: 0,
  );
  final items = [
    for (final entry in [
      ('review', 660, 720),
      ('planning', 675, 735),
      ('call', 690, 695),
    ])
      EventItem(
        id: entry.$1,
        title: entry.$1,
        revision: 1,
        createdAt: date,
        startsAt: date.add(Duration(minutes: entry.$2)),
        endsAt: date.add(Duration(minutes: entry.$3)),
        externalId: entry.$1,
        calendarName: 'Work',
      ),
  ];

  testWidgets('exact geometry, overlap, zoom anchor and short event dialog', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(1200, 1000);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    await tester.pumpWidget(
      FloeApp(
        gateway: FakeDayGateway(initialItems: items),
        query: query,
      ),
    );
    await tester.pumpAndSettle();
    final cards = find.byType(CalendarEventCard);
    final first = tester.getRect(cards.at(0));
    final second = tester.getRect(cards.at(1));
    final short = tester.getRect(cards.at(2));
    expect(first.height, 60);
    expect(short.height, 5);
    expect(second.top - first.top, 15);
    expect(first.right, lessThan(second.left));
    final scroll = tester
        .widget<SingleChildScrollView>(find.byKey(const Key('calendar-scroll')))
        .controller!;
    final before = scroll.offset;
    await tester.tap(find.byTooltip('Zoom in'));
    await tester.pumpAndSettle();
    expect(scroll.offset, closeTo(before * 2, .1));
    expect(tester.getSize(cards.at(0)).height, 120);
    await tester.ensureVisible(cards.at(2));
    await tester.tap(cards.at(2));
    await tester.pumpAndSettle();
    expect(find.byType(FloeDetailDialog), findsOneWidget);
    expect(find.text('11:30 – 11:35'), findsOneWidget);
    await tester.tap(find.byTooltip('Close'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 140));
    expect(find.byType(FloeDetailDialog), findsNothing);
    await tester.tap(find.byTooltip('Connect'));
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Today'));
    await tester.pumpAndSettle();
    expect(tester.widget<Slider>(find.byType(Slider)).value, 2);
    expect(tester.takeException(), isNull);
  });
  testWidgets('empty state is centered across the whole calendar', (
    tester,
  ) async {
    await tester.pumpWidget(FloeApp(gateway: FakeDayGateway(), query: query));
    await tester.pumpAndSettle();
    expect(find.text('A little breathing room.'), findsOneWidget);
    await tester.tap(find.text('View connected calendars'));
    await tester.pumpAndSettle();
    expect(find.text('Connections'), findsOneWidget);
    expect(find.byType(FloeDotSpinner), findsNothing);
  });
}
