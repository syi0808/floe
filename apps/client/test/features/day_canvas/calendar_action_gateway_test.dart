import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/features/day_canvas/application/ffi_day_gateway.dart';
import 'package:floe_client/features/day_canvas/domain/calendar_action.dart';

import 'calendar_gateway_test.dart' show FixtureCalendarAdapter, query;

void main() {
  test(
    'action decisions persist through the Dart JSON C ABI without writes',
    () async {
      final library = File('../../target/debug/libfloe_ffi.dylib').absolute;
      expect(
        library.existsSync(),
        isTrue,
        reason: 'cargo build -p floe-ffi required',
      );
      final directory = await Directory.systemTemp.createTemp('floe-actions-');
      final adapter = FixtureCalendarAdapter()..records = [];
      Future<FfiDayGateway> open() => FfiDayGateway.open(
        libraryPath: library.path,
        databasePath: '${directory.path}/actions.db',
        calendarAdapter: adapter,
        clock: () => DateTime.utc(2000),
      );
      var gateway = await open();
      try {
        await gateway.selectCalendars(adapter.inventory, query);
        expect(await gateway.loadCalendarActions(query.personId), isEmpty);
        final now = DateTime.now().toUtc();
        Future<CalendarAction> propose() => gateway.proposeCalendarAction(
          personId: query.personId,
          calendarId: 'fixture',
          title: 'Focus',
          startsAt: now.add(const Duration(hours: 1)),
          endsAt: now.add(const Duration(hours: 2)),
          timezone: 'Asia/Seoul',
        );
        final pending = await propose();
        expect(pending.status, CalendarActionStatus.pending);
        expect(pending.createdAt.year, now.year);
        expect(pending.calendarName, 'Test calendar');
        expect(
          pending.expiresAt.difference(pending.createdAt),
          const Duration(minutes: 15),
        );
        final approved = await gateway.decideCalendarAction(
          personId: query.personId,
          actionId: pending.id,
          decision: CalendarActionDecision.approve,
        );
        expect(approved.status, CalendarActionStatus.approved);
        expect(approved.approvedAt, isNotNull);
        expect(approved.executionId, pending.executionId);
        await expectLater(
          gateway.decideCalendarAction(
            personId: query.personId,
            actionId: pending.id,
            decision: CalendarActionDecision.approve,
          ),
          throwsA(isA<FfiDayGatewayException>()),
        );
        final second = await propose();
        final rejected = await gateway.decideCalendarAction(
          personId: query.personId,
          actionId: second.id,
          decision: CalendarActionDecision.reject,
        );
        expect(rejected.status, CalendarActionStatus.rejected);
        await gateway.close();
        gateway = await open();
        final restored = await gateway.loadCalendarAction(
          query.personId,
          pending.id,
        );
        expect(restored.status, CalendarActionStatus.approved);
        expect(restored.executionId, approved.executionId);
        expect(await gateway.loadCalendarActions(query.personId), hasLength(2));
        expect((await gateway.loadDay(query)).items, isEmpty);
        expect(adapter.records, isEmpty);
      } finally {
        await gateway.close();
        await directory.delete(recursive: true);
      }
    },
  );
}
