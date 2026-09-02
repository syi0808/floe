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
    await tester.tap(find.byTooltip('캡처 저장'));
    await tester.pumpAndSettle();
    expect(find.text('어디에 담을까요?'), findsOneWidget);

    await tester.tap(find.text('할 일'));
    await tester.pump();
    await tester.tap(find.text('분류하여 추가'));
    await tester.pumpAndSettle();
    expect(find.widgetWithText(ListTile, '우유 사기'), findsOneWidget);
    expect(find.byType(Checkbox), findsOneWidget);
  });
}
