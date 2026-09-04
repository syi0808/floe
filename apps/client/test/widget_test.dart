import 'package:floe_client/app/floe_app.dart';
import 'package:floe_client/app/floe_mascot.dart';
import 'package:floe_client/features/day_canvas/application/fake_day_gateway.dart';
import 'package:floe_client/features/day_canvas/domain/day_models.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flutter_svg/flutter_svg.dart';

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
    await tester.ensureVisible(find.byKey(const Key('capture-field')));
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
    expect(find.text('우유 사기'), findsOneWidget);
    expect(find.byType(Checkbox), findsAtLeastNWidgets(1));
    expect(find.text('Captured “우유 사기”'), findsOneWidget);
    await tester.ensureVisible(find.byTooltip('Dismiss capture'));
    await tester.tap(find.byTooltip('Dismiss capture'));
    await tester.pumpAndSettle();
    expect(find.text('Captured “우유 사기”'), findsNothing);
    expect(
      tester
          .widget<TextField>(find.byKey(const Key('capture-field')))
          .controller!
          .text,
      isEmpty,
    );
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
    expect(find.text('Thu, Sep 3'), findsOneWidget);

    await tester.tap(find.text('Today').first);
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

    await tester.tap(find.text('Tasks'));
    await tester.pumpAndSettle();
    final deleteButton = find.byTooltip('$title 삭제').last;
    await tester.ensureVisible(deleteButton);
    await tester.pumpAndSettle();
    await tester.tap(deleteButton);
    await tester.pumpAndSettle();
    expect(find.text('항목을 삭제할까요?'), findsOneWidget);
    await tester.tap(find.text('취소'));
    await tester.pumpAndSettle();
    expect(find.text(title), findsOneWidget);

    await tester.ensureVisible(deleteButton);
    await tester.pumpAndSettle();
    await tester.tap(deleteButton);
    await tester.pumpAndSettle();
    await tester.tap(find.text('삭제'));
    await tester.pumpAndSettle();
    expect(find.text(title), findsNothing);
    expect(find.text('아직 할 일이 없어요.'), findsOneWidget);
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

    expect(find.byKey(const Key('timeline-card')), findsOneWidget);
    expect(find.text('오늘은 아직 비어 있어요'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('adapts the timeline and Floe suggestion to a phone', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(390, 844);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);

    final now = DateTime.utc(2026, 9, 2, 9, 15);
    await tester.pumpWidget(
      FloeApp(
        gateway: FakeDayGateway(
          initialItems: [
            EventItem(
              id: 'event-1',
              title: '디자인 리뷰',
              revision: 0,
              createdAt: now,
              startsAt: DateTime.utc(2026, 9, 2, 9),
              endsAt: DateTime.utc(2026, 9, 2, 10),
            ),
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

    expect(find.text('Today'), findsNWidgets(2));
    expect(find.text('Tasks'), findsOneWidget);
    expect(find.text('Notes'), findsOneWidget);
    expect(find.byType(NavigationBar), findsNothing);
    expect(find.byTooltip('Floe 제안 열기'), findsOneWidget);
    expect(tester.takeException(), isNull);

    await tester.tap(find.byTooltip('Floe 제안 열기'));
    await tester.pumpAndSettle();
    expect(find.text('Reserve a 20-minute break?'), findsOneWidget);
    expect(find.text('Add break'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('navigates collections and opens semantic task detail', (
    tester,
  ) async {
    final now = DateTime.utc(2026, 9, 2, 9);
    await tester.pumpWidget(
      FloeApp(
        gateway: FakeDayGateway(
          initialItems: [
            TaskItem(
              id: 'task-1',
              title: '출시 체크리스트 검토',
              revision: 0,
              createdAt: now,
            ),
            NoteItem(
              id: 'note-1',
              title: '출시 회고 메모',
              revision: 0,
              createdAt: now,
            ),
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

    final mascot = tester.widget<SvgPicture>(find.byType(SvgPicture).first);
    expect(
      (mascot.bytesLoader as SvgAssetLoader).assetName,
      FloeMascot.assetPath,
    );

    await tester.tap(find.text('Notes'));
    await tester.pumpAndSettle();
    expect(find.text('All notes · 1'), findsOneWidget);
    expect(find.text('출시 회고 메모'), findsOneWidget);
    expect(find.text('일'), findsNothing);

    await tester.tap(find.text('Tasks'));
    await tester.pumpAndSettle();
    await tester.tap(find.widgetWithText(ListTile, '출시 체크리스트 검토'));
    await tester.pumpAndSettle();
    expect(find.text('Back to today'), findsOneWidget);
    expect(find.text('Subtasks'), findsOneWidget);
    expect(find.text('Floe suggests'), findsOneWidget);
  });
}

String _dateLabel(DateTime value) =>
    '${['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'][value.weekday - 1]}, ${['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'][value.month - 1]} ${value.day}';
