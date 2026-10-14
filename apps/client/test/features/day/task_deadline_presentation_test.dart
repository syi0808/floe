import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:intl/intl.dart';
import 'package:floe_client/app/runtime/local_owner_gateways_scope.dart';
import 'package:floe_client/features/day/application/day_gateway.dart';
import 'package:floe_client/features/day/domain/day_models.dart';
import 'package:floe_client/features/day/presentation/personal_day_screen.dart';
import 'package:floe_client/l10n/app_localizations.dart';

void main() {
  // Run this suite in separate TZ=Asia/Seoul and TZ=America/New_York
  // processes as well as the default zone: the fixtures cross local midnight
  // and the year boundary. UTC transport/storage values must remain unchanged.
  for (final deadline in [
    DateTime.utc(2026, 10, 14, 16, 30),
    DateTime.utc(2027, 1, 1, 0, 15),
  ]) {
    for (final width in [390.0, 1200.0]) {
      for (final detail in [false, true]) {
        testWidgets(
          '${detail ? 'detail' : 'list'} shows local deadline $deadline at width $width',
          (tester) async {
            final task = TaskItem(
              id: 'task',
              title: 'Deadline fixture',
              revision: 1,
              createdAt: DateTime.utc(2026, 10, 1),
              deadline: deadline,
            );
            await _openTasks(tester, task, width: width);
            final local = deadline.toLocal();
            final date = DateFormat.yMMMEd('en').format(local);
            final time =
                '${local.hour.toString().padLeft(2, '0')}:${local.minute.toString().padLeft(2, '0')}';
            final expected = '$date · $time';
            if (detail) {
              await tester.tap(find.text('Deadline fixture'));
              await tester.pumpAndSettle();
              expect(find.text(expected), findsOneWidget);
            } else {
              expect(find.textContaining('Due $expected'), findsOneWidget);
            }
            expect(task.deadline, deadline);
            expect(task.deadline!.isUtc, isTrue);
            expect(tester.takeException(), isNull);
            await tester.pumpWidget(const SizedBox.shrink());
          },
        );
      }
    }
  }

  testWidgets('missing deadline remains explicitly unscheduled', (
    tester,
  ) async {
    await _openTasks(
      tester,
      TaskItem(
        id: 'task',
        title: 'No deadline fixture',
        revision: 1,
        createdAt: DateTime.utc(2026, 10, 1),
      ),
    );
    await tester.tap(find.text('No deadline fixture'));
    await tester.pumpAndSettle();
    expect(find.text('No due date'), findsOneWidget);
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox.shrink());
  });
}

Future<void> _openTasks(
  WidgetTester tester,
  TaskItem task, {
  double width = 1200,
}) async {
  tester.view.physicalSize = Size(width, 1000);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
  final now = DateTime(2026, 10, 14, 12);
  await tester.pumpWidget(
    MaterialApp(
      locale: const Locale('en'),
      localizationsDelegates: AppLocalizations.localizationsDelegates,
      supportedLocales: AppLocalizations.supportedLocales,
      builder: (context, child) => MediaQuery(
        data: MediaQuery.of(context).copyWith(alwaysUse24HourFormat: true),
        child: child!,
      ),
      home: PersonalDayScreen(
        gateway: _Gateway(task),
        query: DayQuery.local(personId: 'person', date: now, now: now),
        ownerGateways: const LocalOwnerGateways(),
      ),
    ),
  );
  await tester.pumpAndSettle();
  await tester.tap(find.byTooltip('Tasks'));
  await tester.pumpAndSettle();
}

final class _Gateway implements DayGateway {
  _Gateway(this.task);
  final TaskItem task;
  @override
  Future<DaySnapshot> loadDay(DayQuery query) async => DaySnapshot(
    personId: query.personId,
    date: query.date,
    generatedAt: query.now,
    timezoneOffsetSeconds: query.timezoneOffsetSeconds,
    items: [task],
  );
  @override
  dynamic noSuchMethod(Invocation invocation) =>
      throw StateError('Unexpected gateway call: ${invocation.memberName}');
}
