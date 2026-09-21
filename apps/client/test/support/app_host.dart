import 'package:floe_client/app/runtime/app_runtime.dart';
import 'package:floe_client/features/day/application/native_day_gateway.dart';
import 'package:floe_client/features/day/application/calendar_gateway.dart';
import 'package:floe_client/features/actions/application/calendar_action_facade.dart';

final class TestAppHost {
  TestAppHost._(
    this.runtime,
    CalendarAdapter adapter,
    DateTime Function()? clock,
  ) : day = NativeDayGateway(runtime, adapter, clock: clock),
      actions = CalendarActionFacade(runtime);

  final AppRuntime runtime;
  final NativeDayGateway day;
  final CalendarActionFacade actions;

  static Future<TestAppHost> open({
    required String libraryPath,
    required String databasePath,
    required String deviceId,
    CalendarAdapter calendarAdapter = const EventKitCalendarAdapter(),
    DateTime Function()? clock,
  }) async => TestAppHost._(
    await AppRuntime.open(
      libraryPath: libraryPath,
      databasePath: databasePath,
      deviceId: deviceId,
    ),
    calendarAdapter,
    clock,
  );

  Future<void> close() async {
    await day.drain();
    await runtime.close();
  }
}
