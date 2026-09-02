import 'package:floe_client/app/floe_app.dart';
import 'package:floe_client/features/day_canvas/application/fake_day_gateway.dart';
import 'package:floe_client/features/day_canvas/domain/day_models.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('renders empty day and classifies a typed capture', (
    tester,
  ) async {
    final now = DateTime.utc(2026, 9, 2, 9);
    await tester.pumpWidget(
      FloeApp(
        gateway: FakeDayGateway(),
        query: DayQuery(
          personId: 'person-1',
          date: now,
          now: now,
          timezoneOffsetSeconds: 0,
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('오늘은 아직 비어 있어요'), findsOneWidget);
    await tester.enterText(find.byKey(const Key('capture-field')), '우유 사기');
    await tester.pump();
    await tester.tap(find.byTooltip('캡처 저장'));
    await tester.pumpAndSettle();
    expect(find.text('어디에 담을까요?'), findsOneWidget);

    await tester.tap(find.text('할 일'));
    await tester.pump();
    expect(find.text('마감 설정'), findsOneWidget);
    await tester.tap(find.text('분류하여 추가'));
    await tester.pumpAndSettle();
    expect(find.widgetWithText(ListTile, '우유 사기'), findsOneWidget);
    expect(find.byType(Checkbox), findsOneWidget);
  });

  testWidgets('today navigation resets a moved date', (tester) async {
    final initial = DateTime.utc(2026, 9, 2, 9);
    await tester.pumpWidget(
      FloeApp(
        gateway: FakeDayGateway(),
        query: DayQuery(
          personId: 'person-1',
          date: initial,
          now: initial,
          timezoneOffsetSeconds: 0,
        ),
      ),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.byTooltip('다음 날'));
    await tester.pumpAndSettle();
    expect(find.text('9월 3일 목요일'), findsOneWidget);

    await tester.tap(find.text('오늘'));
    await tester.pumpAndSettle();
    final today = DateTime.now();
    expect(find.text(_dateLabel(today)), findsOneWidget);
  });

  testWidgets('requires confirmation before deleting an item', (tester) async {
    final now = DateTime.utc(2026, 9, 2, 9);
    const title = '오후 집중 작업';
    await tester.pumpWidget(
      FloeApp(
        gateway: FakeDayGateway(
          initialItems: [
            TaskItem(id: 'task-1', title: title, revision: 0, createdAt: now),
          ],
        ),
        query: DayQuery(
          personId: 'person-1',
          date: now,
          now: now,
          timezoneOffsetSeconds: 0,
        ),
      ),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.byTooltip('$title 삭제'));
    await tester.pumpAndSettle();
    expect(find.text('항목을 삭제할까요?'), findsOneWidget);
    await tester.tap(find.text('취소'));
    await tester.pumpAndSettle();
    expect(find.text(title), findsOneWidget);

    await tester.tap(find.byTooltip('$title 삭제'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('삭제'));
    await tester.pumpAndSettle();
    expect(find.text(title), findsNothing);
    expect(find.text('오늘은 아직 비어 있어요'), findsOneWidget);
  });

  testWidgets('supports a narrow viewport and enlarged text', (tester) async {
    tester.view.physicalSize = const Size(390, 844);
    tester.view.devicePixelRatio = 1;
    tester.platformDispatcher.textScaleFactorTestValue = 2;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    addTearDown(tester.platformDispatcher.clearTextScaleFactorTestValue);

    final now = DateTime.utc(2026, 9, 2, 9);
    await tester.pumpWidget(
      FloeApp(
        gateway: FakeDayGateway(),
        query: DayQuery(
          personId: 'person-1',
          date: now,
          now: now,
          timezoneOffsetSeconds: 0,
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('지금'), findsOneWidget);
    expect(find.text('다음'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
}

String _dateLabel(DateTime value) =>
    '${value.month}월 ${value.day}일 ${['월', '화', '수', '목', '금', '토', '일'][value.weekday - 1]}요일';
