import 'dart:io';

import 'package:floe_client/features/day_canvas/application/ffi_day_gateway.dart';
import 'package:floe_client/features/day_canvas/application/focus_gateway.dart';
import 'package:floe_client/features/day_canvas/domain/day_models.dart';
import 'package:floe_client/features/day_canvas/domain/focus_models.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('actual JSON/C ABI persists scoped preference CRUD and rejects stale writes', () async {
    final directory = await Directory.systemTemp.createTemp('floe-focus-ffi-');
    final now = DateTime.utc(2026, 9, 5);
    final query = DayQuery(
      personId: localPersonId,
      date: now,
      now: now,
      timezoneOffsetSeconds: 32400,
    );
    const value = FocusPreferenceValue(
      startMinute: 600,
      endMinute: 900,
      durationMinutes: 45,
    );
    Future<FfiDayGateway> open() => FfiDayGateway.open(
      libraryPath: File('../../target/debug/libfloe_ffi.dylib').absolute.path,
      databasePath: '${directory.path}/focus.db',
      clock: () => now,
    );
    var gateway = await open();
    addTearDown(() async {
      await gateway.close();
      await directory.delete(recursive: true);
    });
    expect(await gateway.loadFocusPreference(query), isNull);
    final saved = await gateway.saveFocusPreference(query, 0, value);
    expect(saved.personId, localPersonId);
    expect(saved.source, 'user_entered');
    expect(saved.value?.durationMinutes, 45);
    final other = DayQuery(
      personId: '00000000-0000-4000-8000-000000000002',
      date: now,
      now: now,
      timezoneOffsetSeconds: 32400,
    );
    expect(await gateway.loadFocusPreference(other), isNull);
    await expectLater(
      gateway.saveFocusPreference(query, 0, null),
      throwsA(
        isA<FocusGatewayException>().having(
          (error) => error.code,
          'code',
          'conflict',
        ),
      ),
    );
    await gateway.close();
    gateway = await open();
    expect((await gateway.loadFocusPreference(query))?.revision, 1);
    final edited = await gateway.saveFocusPreference(
      query,
      1,
      const FocusPreferenceValue(
        startMinute: 660,
        endMinute: 960,
        durationMinutes: 60,
      ),
    );
    expect(edited.revision, 2);
    final deleted = await gateway.saveFocusPreference(query, 2, null);
    expect(deleted.value, isNull);
    await gateway.close();
    gateway = await open();
    final reopened = await gateway.loadFocusPreference(query);
    expect(reopened?.revision, 3);
    expect(reopened?.value, isNull);
    expect((await gateway.loadDay(query)).items, isEmpty);
    final yesterday = DayQuery(
      personId: localPersonId,
      date: now.subtract(const Duration(days: 1)),
      now: now,
      timezoneOffsetSeconds: 0,
    );
    await expectLater(
      gateway.suggestFocus(yesterday, 'fixture-not-called'),
      throwsA(
        isA<FocusGatewayException>().having(
          (error) => error.code,
          'code',
          'no_focus_slot',
        ),
      ),
    );
    await expectLater(
      gateway.saveFocusPreference(
        query,
        3,
        const FocusPreferenceValue(
          startMinute: 900,
          endMinute: 600,
          durationMinutes: 60,
        ),
      ),
      throwsA(
        isA<FocusGatewayException>().having(
          (error) => error.code,
          'code',
          'validation',
        ),
      ),
    );
    expect((await gateway.loadFocusPreference(query))?.revision, 3);
  });
}
