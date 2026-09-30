import 'dart:io';

import 'package:floe_client/app/runtime/app_runtime.dart';
import 'package:floe_client/features/connections/domain/source_connection.dart';
import 'package:floe_client/features/day/application/calendar_gateway.dart';
import 'package:floe_client/features/day/application/calendar_observation_publisher.dart';
import 'package:floe_client/features/day/application/native_day_gateway.dart';
import 'package:floe_client/features/day/domain/day_models.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import 'disposable_product_profile.dart';

void require(bool condition, String label) {
  if (!condition) throw StateError(label);
}

final class FixtureCalendarAdapter implements CalendarAdapter {
  List<CalendarChoice> inventory = const [CalendarChoice('home', 'Home')];
  List<Map<String, dynamic>> records = [
    {
      'external_id': 'event-1',
      'external_revision': '1',
      'can_modify': false,
      'title': 'Meeting',
      'schedule': {
        'kind': 'timed',
        'starts_at': '2026-09-03T15:00:00Z',
        'ends_at': '2026-09-03T16:00:00Z',
        'timezone': 'Asia/Seoul',
      },
    },
  ];
  String? deniedCalendarId;

  @override
  Future<List<CalendarChoice>> calendars({bool requestAccess = true}) async =>
      inventory;

  @override
  Future<List<Map<String, dynamic>>> read(
    String calendarId,
    DayQuery query,
  ) async {
    if (deniedCalendarId == calendarId) {
      throw PlatformException(code: 'permission_denied');
    }
    return records;
  }

  @override
  Future<void> openSettings() async {}
}

DayQuery queryFor(String personId) => DayQuery(
  personId: personId,
  date: DateTime.utc(2026, 9, 4),
  now: DateTime.utc(2026, 9, 4),
  timezoneOffsetSeconds: 32400,
);

Future<SourceConnection> establish(
  AppRuntime runtime,
  DayQuery query,
  FixtureCalendarAdapter adapter, {
  bool all = false,
}) async {
  final source = await runtime.calendarSource.establishNative(
    query.personId,
    resourceMode: all ? 'all_available' : 'selected',
    resources: [
      for (final calendar in adapter.inventory)
        SourceResource(handle: calendar.id, label: calendar.name),
    ],
  );
  require(source.state == 'ready', 'Reviewed native source must start Ready.');
  require(
    source.nativeSubjectFingerprint == 'a' * 64,
    'Native fixture subject missing.',
  );
  return source;
}

Future<void> mirrorContinuity(String library) async {
  final profile = await DisposableProductProfile.create(
    personId: localPersonId,
  );
  NativeDayGateway? day;
  try {
    final query = queryFor(profile.personId);
    final adapter = FixtureCalendarAdapter();
    var runtime = await profile.open(library);
    day = NativeDayGateway(runtime, adapter, clock: () => query.now);
    final source = await establish(runtime, query, adapter);
    var snapshot = await day.syncCalendar(query);
    require(
      snapshot.calendar!.sourceConnectionId == source.connectionId,
      'Mirror source identity.',
    );
    require(snapshot.calendar!.provider == 'event_kit', 'Mirror provider.');
    require(
      snapshot.items.whereType<EventItem>().length == 1,
      'First import event count.',
    );
    require(snapshot.calendarMirrorRevision == 1, 'First mirror revision.');
    var current = (await runtime.calendarSource.inspectNative(query.personId))!;
    require(
      current.revision == source.revision,
      'Sync changed source revision.',
    );
    require(
      current.sourceAuthority == source.sourceAuthority,
      'Sync changed source authority.',
    );
    adapter.records.first['can_modify'] = true;
    snapshot = await day.syncCalendar(query);
    require(
      snapshot.items.whereType<EventItem>().single.canModify,
      'Second import content.',
    );
    require(snapshot.calendarMirrorRevision == 2, 'Second mirror revision.');
    current = (await runtime.calendarSource.inspectNative(query.personId))!;
    require(
      current.revision == source.revision,
      'Second sync changed source revision.',
    );
    require(
      current.sourceAuthority == source.sourceAuthority,
      'Second sync changed source authority.',
    );
    await day.drain();
    await profile.closeRuntime();
    runtime = await profile.open(library);
    day = NativeDayGateway(runtime, adapter, clock: () => query.now);
    final restored = await day.loadDay(query);
    require(
      restored.calendar!.sourceConnectionId == source.connectionId,
      'Reopened mirror identity.',
    );
    require(
      restored.items.whereType<EventItem>().single.canModify,
      'Reopened mirror content.',
    );
    require(restored.calendarMirrorRevision == 2, 'Reopened mirror revision.');
    current = (await runtime.calendarSource.inspectNative(query.personId))!;
    require(
      current.resources.single.handle == 'home',
      'Reopened source resources.',
    );
    require(
      current.revision == source.revision &&
          current.sourceAuthority == source.sourceAuthority,
      'Reopened source authority.',
    );
    stdout.writeln('DAY_MIRROR_SOURCE_CONTINUITY_PASSED');
  } finally {
    await day?.drain();
    await profile.cleanup();
  }
}

Future<void> inventoryReconciliation(String library) async {
  final profile = await DisposableProductProfile.create(
    personId: localPersonId,
  );
  NativeDayGateway? day;
  try {
    final query = queryFor(profile.personId);
    final adapter = FixtureCalendarAdapter();
    final runtime = await profile.open(library);
    day = NativeDayGateway(runtime, adapter, clock: () => query.now);
    final original = await establish(runtime, query, adapter, all: true);
    await day.syncCalendar(query);
    adapter.inventory = const [
      CalendarChoice('home', 'Home'),
      CalendarChoice('work', 'Work'),
    ];
    final snapshot = await day.syncCalendar(query);
    final expanded = (await runtime.calendarSource.inspectNative(
      query.personId,
    ))!;
    require(
      expanded.selectedCalendarIds.join(',') == 'home,work',
      'Expanded inventory.',
    );
    require(
      expanded.revision == original.revision + 1,
      'Inventory source revision.',
    );
    require(
      expanded.sourceAuthority != original.sourceAuthority,
      'Inventory source authority.',
    );
    require(
      snapshot.items.whereType<EventItem>().length == 2,
      'Expanded mirror events.',
    );
    adapter.deniedCalendarId = 'work';
    final failed = await day.syncCalendar(query);
    require(
      failed.calendar!.sourceStatuses['work']!.error == 'permission_denied',
      'Read failure status.',
    );
    final current = (await runtime.calendarSource.inspectNative(
      query.personId,
    ))!;
    require(
      current.revision == expanded.revision,
      'Read failure changed source revision.',
    );
    require(
      current.sourceAuthority == expanded.sourceAuthority,
      'Read failure changed source authority.',
    );
    require(
      current.selectedCalendarIds.join(',') == 'home,work',
      'Read failure changed resources.',
    );
    stdout.writeln('ALL_AVAILABLE_SOURCE_AUTHORITY_PASSED');
  } finally {
    await day?.drain();
    await profile.cleanup();
  }
}

Future<void> wideSource(String library) async {
  final profile = await DisposableProductProfile.create(
    personId: localPersonId,
  );
  NativeDayGateway? day;
  try {
    final query = queryFor(profile.personId);
    final adapter = FixtureCalendarAdapter()
      ..inventory = List.generate(
        CalendarObservationPublisher.maxCalendarCount + 1,
        (index) => CalendarChoice('calendar-$index', 'Calendar $index'),
      );
    final runtime = await profile.open(library);
    day = NativeDayGateway(runtime, adapter, clock: () => query.now);
    final source = await establish(runtime, query, adapter);
    require(
      source.resources.length == adapter.inventory.length,
      'Wide source lost resources.',
    );
    final snapshot = await day.syncCalendar(query);
    require(
      snapshot.items.whereType<EventItem>().length == adapter.inventory.length,
      'Wide import lost events.',
    );
    require(
      snapshot.calendar!.error == null,
      'Bounded publication failed Day import.',
    );
    stdout.writeln('WIDE_SOURCE_BOUNDED_PUBLICATION_PASSED');
  } finally {
    await day?.drain();
    await profile.cleanup();
  }
}

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  try {
    final library = Platform.environment['FLOE_VALIDATION_FFI']!;
    await mirrorContinuity(library);
    await inventoryReconciliation(library);
    await wideSource(library);
    exit(0);
  } on Object catch (error, stack) {
    stderr.writeln('$error\n$stack');
    exit(1);
  }
}
