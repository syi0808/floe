import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/features/day_canvas/application/calendar_gateway.dart';
import 'package:floe_client/features/day_canvas/domain/day_models.dart';
import 'package:floe_client/features/day_canvas/presentation/calendar_layout.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  test(
    'local DST axis preserves elapsed time and distinguishes repeated hours',
    () {
      final spring = CalendarDayAxis(DateTime(2026, 3, 8), -28800);
      final fall = CalendarDayAxis(DateTime(2026, 11, 1), -25200);
      if (DateTime(2026, 3, 8).timeZoneOffset.inSeconds != -28800) {
        expect(spring.minutes, 1440);
        return;
      }
      expect(spring.minutes, 1380);
      expect(fall.minutes, 1500);
      expect(spring.hourLabel(2), startsWith('03:00'));
      expect(fall.hourLabel(1), startsWith('01:00'));
      expect(fall.hourLabel(2), startsWith('01:00'));
      expect(fall.hourLabel(1), isNot(fall.hourLabel(2)));
      expect(fall.minute(DateTime.parse('2026-11-02T07:45:00Z')), 1485);
      expect(fall.hourLabel(25), '24:00');
    },
  );
  test(
    'EventKit reads civil-day endpoints, not a fixed 24-hour interval',
    () async {
      final calls = <MethodCall>[];
      const channel = MethodChannel('floe/calendar');
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .setMockMethodCallHandler(channel, (call) async {
            calls.add(call);
            return <Object>[];
          });
      addTearDown(
        () => TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
            .setMockMethodCallHandler(channel, null),
      );
      for (final entry in [
        (DateTime.utc(2026, 3, 8), -28800, -25200, 23),
        (DateTime.utc(2026, 11, 1), -25200, -28800, 25),
      ]) {
        final query = DayQuery(
          personId: 'test',
          date: entry.$1,
          now: entry.$1,
          timezoneOffsetSeconds: entry.$2,
          endTimezoneOffsetSeconds: entry.$3,
        );
        await const EventKitCalendarAdapter().read('test-calendar', query);
        final arguments = calls.last.arguments as Map;
        final start = DateTime.parse(arguments['starts_at'] as String);
        final end = DateTime.parse(arguments['ends_at'] as String);
        expect(end.difference(start).inHours, entry.$4);
      }
      await const EventKitCalendarAdapter().calendars(requestAccess: false);
      expect(calls.last.arguments, {'request_access': false});
    },
  );
  test('EventKit preserves the native provider identity', () async {
    final calls = <MethodCall>[];
    const channel = MethodChannel('floe/calendar');
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(channel, (call) async {
          calls.add(call);
          return <Object>[
            {'id': 'primary', 'name': 'iCloud · Home', 'provider': 'event_kit'},
          ];
        });
    addTearDown(
      () => TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .setMockMethodCallHandler(channel, null),
    );

    final calendars = await const EventKitCalendarAdapter(
      deviceId: 'local-device-1',
    ).calendars();

    expect(calendars.single.provider, 'event_kit');
    expect(calls.single.arguments, {
      'device_id': 'local-device-1',
      'request_access': true,
    });
  });
}
