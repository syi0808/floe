import 'package:floe_client/features/experts/domain/agent_calendar_sources.dart';
import 'package:floe_client/features/day/domain/day_models.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  for (final provider in const [
    'event_kit',
    'google_calendar',
    'microsoft_calendar',
    'android',
  ]) {
    test('$provider connection is usable by Schedule', () {
      final sources = AgentCalendarSources(
        personId: '00000000-0000-4000-8000-000000000001',
        connection: CalendarConnection(
          connectionId: '00000000-0000-4000-8000-000000000010',
          deviceId: provider == 'android' ? 'android-device' : 'paired-device',
          provider: provider,
          revision: 7,
          sourceAuthority: const CalendarSourceAuthority(
            incarnation: '00000000-0000-4000-8000-000000000009',
            epoch: 1,
          ),
          calendars: const [ConnectedCalendar(id: 'primary', name: 'Primary')],
        ),
      );

      expect(sources.usable, isTrue);
      expect(sources.connectionId, '00000000-0000-4000-8000-000000000010');
      expect(
        sources.deviceId,
        provider == 'android' ? 'android-device' : 'paired-device',
      );
      expect(sources.containsScope(provider, const ['primary']), isTrue);
    });
  }

  test('cosmetic mirror changes preserve consent fingerprint', () {
    AgentCalendarSources build({
      required int revision,
      required String name,
      String? error,
      CalendarSourceAuthority? authority,
      String deviceId = 'paired-device',
      String provider = 'event_kit',
      String connectionId = '00000000-0000-4000-8000-000000000010',
      List<String> ids = const ['primary', 'secondary'],
    }) => AgentCalendarSources(
      personId: '00000000-0000-4000-8000-000000000001',
      connection: CalendarConnection(
        connectionId: connectionId,
        deviceId: deviceId,
        provider: provider,
        revision: revision,
        sourceAuthority:
            authority ??
            const CalendarSourceAuthority(
              incarnation: '00000000-0000-4000-8000-000000000009',
              epoch: 1,
            ),
        calendars: [
          for (final id in ids)
            ConnectedCalendar(id: id, name: name, error: error),
        ],
      ),
    );

    final baseline = build(revision: 1, name: 'Primary');
    for (var index = 0; index < 100; index++) {
      expect(
        build(
          revision: index + 2,
          name: 'Renamed $index',
          error: index.isEven ? 'stale' : null,
        ).fingerprint,
        baseline.fingerprint,
      );
    }
    expect(
      build(
        revision: 1,
        name: 'Primary',
        authority: const CalendarSourceAuthority(
          incarnation: '00000000-0000-4000-8000-000000000011',
          epoch: 1,
        ),
      ).fingerprint,
      isNot(baseline.fingerprint),
    );
    expect(
      build(revision: 1, name: 'Primary', deviceId: 'other-device').fingerprint,
      isNot(baseline.fingerprint),
    );
    expect(
      build(revision: 1, name: 'Primary', ids: const ['primary']).fingerprint,
      isNot(baseline.fingerprint),
    );
  });

  test('missing source authority disables consent review', () {
    final sources = AgentCalendarSources(
      personId: '00000000-0000-4000-8000-000000000001',
      connection: const CalendarConnection(
        connectionId: '00000000-0000-4000-8000-000000000010',
        deviceId: 'paired-device',
        provider: 'event_kit',
        revision: 1,
        calendars: [ConnectedCalendar(id: 'primary', name: 'Primary')],
      ),
    );
    expect(sources.usable, isFalse);
  });
}
