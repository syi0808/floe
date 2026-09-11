import 'package:floe_client/features/agent/agent_calendar_sources.dart';
import 'package:floe_client/features/day_canvas/domain/day_models.dart';
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
}
