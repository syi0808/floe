import 'package:floe_client/infrastructure/native/apple_context_gateway.dart';
import 'package:floe_client/infrastructure/native/android_context_gateway.dart';
import 'package:floe_client/features/day_canvas/domain/day_models.dart';
import 'package:floe_client/infrastructure/native/local_context_publication.dart';
import 'package:floe_client/infrastructure/native/macos_context_gateway.dart';
import 'package:floe_client/infrastructure/native/native_transport.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  const person = '00000000-0000-4000-8000-000000000001';
  const device = 'local-device';
  final now = DateTime.fromMillisecondsSinceEpoch(1000, isUtc: true);

  test('publishes only validated Apple View payloads', () async {
    final transport = _RecordingTransport();
    final native = _FakeAppleContext();
    final gateway = PublishingAppleContextGateway(
      gateway: native,
      transport: transport,
      personId: person,
      deviceId: device,
      clock: () => now,
    );

    await gateway.readContacts();
    await gateway.readFeasibility(
      AppleFeasibilityQuery(
        eventHandle: 'event:one',
        evidenceHandles: const ['calendar:event:one'],
        latitude: 37.0,
        longitude: 127.0,
        eventStart: now,
        eventEnd: now.add(const Duration(hours: 1)),
        travelMode: AppleTravelMode.transit,
      ),
    );
    await gateway.readWellbeing();

    expect(transport.published.map((entry) => entry.view['view_id']), [
      'people.identity',
      'schedule.feasibility',
      'wellbeing.derived',
    ]);
    expect(
      transport.published.every((entry) => entry.personId == person),
      true,
    );
    expect(
      transport.published.every((entry) => entry.deviceId == device),
      true,
    );
    expect(
      transport.published[1].view.keys,
      isNot(contains('weather_attribution')),
    );
  });

  test('rejects invalid native output and revokes its cached View', () async {
    final transport = _RecordingTransport();
    final native = _FakeAppleContext()
      ..people = {..._peopleView, 'raw_phone_number': '+82-10-0000-0000'};
    final gateway = PublishingAppleContextGateway(
      gateway: native,
      transport: transport,
      personId: person,
      deviceId: device,
      clock: () => now,
    );

    await expectLater(gateway.readContacts(), throwsFormatException);

    expect(transport.published, isEmpty);
    expect(transport.revoked.single.viewId, 'people.identity');
  });

  test(
    'revokes unknown, stale, denied, logout, and former Person data',
    () async {
      final transport = _RecordingTransport();
      final native = _FakeAppleContext()
        ..wellbeing = {
          ..._wellbeingView,
          'capacity': 'unknown',
          'recovery': 'unknown',
          'confidence_millis': 0,
        }
        ..permissionGranted = false;
      final gateway = PublishingAppleContextGateway(
        gateway: native,
        transport: transport,
        personId: person,
        deviceId: device,
        clock: () => now,
      );

      await gateway.readWellbeing();
      await gateway.requestPermission(AppleContextSource.contacts);
      native.people = {
        ..._peopleView,
        'observed_at_unix_ms': 0,
        'expires_at_unix_ms': 1,
      };
      await expectLater(gateway.readContacts(), throwsFormatException);
      await gateway.bindPerson('00000000-0000-4000-8000-000000000002');
      await gateway.logout();

      expect(transport.revoked.map((entry) => entry.viewId), [
        'wellbeing.derived',
        'people.identity',
        'people.identity',
        null,
        null,
      ]);
      expect(transport.revoked[3].personId, person);
      expect(
        transport.revoked[4].personId,
        '00000000-0000-4000-8000-000000000002',
      );
    },
  );

  test(
    'publishes macOS coarse attention and clears unknown attention',
    () async {
      final transport = _RecordingTransport();
      final native = _FakeMacOSContext();
      final gateway = PublishingMacOSContextGateway(
        gateway: native,
        transport: transport,
        personId: person,
        deviceId: device,
        clock: () => now,
      );

      await gateway.readAttention();
      native.view = {
        ..._attentionView,
        'state': 'unknown',
        'confidence_millis': 0,
        'evidence_handles': <String>[],
      };
      await gateway.readAttention();

      expect(transport.published.single.view, _attentionView);
      expect(transport.revoked.single.viewId, 'attention.coarse');
    },
  );

  test('publishes Android Contacts and Health Connect projections', () async {
    final transport = _RecordingTransport();
    final native = _FakeAndroidContext();
    final gateway = PublishingAndroidContextGateway(
      gateway: native,
      transport: transport,
      personId: person,
      deviceId: device,
      clock: () => now,
    );

    await gateway.readContacts();
    await gateway.readWellbeing();

    expect(transport.published.map((entry) => entry.view['view_id']), [
      'people.identity',
      'wellbeing.derived',
    ]);
    expect(
      transport.published.expand((entry) => entry.view.keys),
      isNot(contains('steps')),
    );
  });

  test('Android denial and unknown health clear cached projections', () async {
    final transport = _RecordingTransport();
    final native = _FakeAndroidContext()
      ..permissionGranted = false
      ..wellbeing = {
        ..._androidWellbeingView,
        'capacity': 'unknown',
        'recovery': 'unknown',
        'confidence_millis': 0,
        'evidence_handles': <String>[],
      };
    final gateway = PublishingAndroidContextGateway(
      gateway: native,
      transport: transport,
      personId: person,
      deviceId: device,
      clock: () => now,
    );

    await gateway.requestPermission(AndroidContextSource.contacts);
    await gateway.readWellbeing();

    expect(transport.published, isEmpty);
    expect(transport.revoked.map((entry) => entry.viewId), [
      'people.identity',
      'wellbeing.derived',
    ]);
  });

  test(
    'Android selected calendars cross the durable Calendar adapter',
    () async {
      final native = _FakeAndroidContext();
      final adapter = AndroidCalendarAdapter(native);
      final query = DayQuery(
        personId: person,
        date: DateTime.utc(1970, 1, 1),
        now: now,
        timezoneOffsetSeconds: 0,
      );

      final calendars = await adapter.calendars();
      final records = await adapter.read(calendars.single.id, query);

      expect(calendars.single.provider, 'android');
      expect(records.single['external_id'], 'calendar.event:one');
      expect(records.single['can_modify'], false);
      expect(records.single['schedule'], {
        'kind': 'timed',
        'starts_at': '1970-01-01T00:00:02.000Z',
        'ends_at': '1970-01-01T00:00:03.000Z',
        'timezone': 'UTC',
      });
    },
  );
}

const _peopleView = <String, dynamic>{
  'schema_version': 1,
  'view_id': 'people.identity',
  'source_handle': 'people:apple:source',
  'observed_at_unix_ms': 1000,
  'expires_at_unix_ms': 301000,
  'coverage_complete': true,
  'identities': <Map<String, dynamic>>[
    {
      'identity_handle': 'person.identity:one',
      'display_name': 'Ada',
      'aliases': <String>['email:ada@example.test'],
      'confidence_millis': 1000,
      'evidence_handles': <String>['contact.evidence:one'],
    },
  ],
};

const _wellbeingView = <String, dynamic>{
  'schema_version': 1,
  'view_id': 'wellbeing.derived',
  'source_handle': 'wellbeing:apple-health',
  'observed_at_unix_ms': 1000,
  'expires_at_unix_ms': 1801000,
  'capacity': 'typical',
  'recovery': 'recovered',
  'confidence_millis': 600,
  'evidence_handles': <String>['health.sleep.window:one'],
};

const _attentionView = <String, dynamic>{
  'schema_version': 1,
  'view_id': 'attention.coarse',
  'source_handle': 'attention:macos_local',
  'observed_at_unix_ms': 1000,
  'expires_at_unix_ms': 61000,
  'state': 'focused',
  'confidence_millis': 750,
  'evidence_handles': <String>[
    'attention.macos:stable_activity',
    'attention.macos:recent_input',
  ],
};

const _androidWellbeingView = <String, dynamic>{
  'schema_version': 1,
  'view_id': 'wellbeing.derived',
  'source_handle': 'wellbeing:android-health',
  'observed_at_unix_ms': 1000,
  'expires_at_unix_ms': 301000,
  'capacity': 'typical',
  'recovery': 'recovered',
  'confidence_millis': 600,
  'evidence_handles': <String>['health.sleep.window:one'],
};

final class _FakeAppleContext implements AppleContextApi {
  Map<String, dynamic> people = Map.of(_peopleView);
  Map<String, dynamic> wellbeing = Map.of(_wellbeingView);
  bool permissionGranted = true;

  @override
  Future<List<Map<String, dynamic>>> connections() async => [];

  @override
  Future<Map<String, dynamic>> readContacts({int limit = 64}) async => people;

  @override
  Future<Map<String, dynamic>> readFeasibility(
    AppleFeasibilityQuery query,
  ) async => {
    'view': {
      'schema_version': 1,
      'view_id': 'schedule.feasibility',
      'source_handle': 'feasibility:apple',
      'observed_at_unix_ms': 1000,
      'expires_at_unix_ms': 301000,
      'items': [
        {
          'event_handle': query.eventHandle,
          'evidence_handles': query.evidenceHandles,
          'travel_duration_seconds': 900,
          'leave_by_unix_ms': 2000,
          'weather_impact': 'minor',
          'confidence_millis': 800,
        },
      ],
    },
    'weather_attribution': {
      'legal_page_url': 'https://weather.example/legal',
      'combined_mark_light_url': 'https://weather.example/light.svg',
      'combined_mark_dark_url': 'https://weather.example/dark.svg',
    },
  };

  @override
  Future<Map<String, dynamic>> readWellbeing() async => wellbeing;

  @override
  Future<bool> requestPermission(AppleContextSource source) async =>
      permissionGranted;

  @override
  Future<Map<String, dynamic>> screenTimeCapability() async => {
    'schema_version': 1,
    'source_handle': 'attention:apple-device-activity',
    'outcome': 'entitlement_unavailable',
    'authorization': 'not_determined',
    'region_availability': 'unknown',
    'observed_at_unix_ms': 1000,
  };
}

final class _FakeMacOSContext implements MacOSContextApi {
  Map<String, dynamic> view = Map.of(_attentionView);

  @override
  Future<Map<String, dynamic>> readAttention() async => view;
}

final class _FakeAndroidContext implements AndroidContextApi {
  bool permissionGranted = true;
  Map<String, dynamic> wellbeing = Map.of(_androidWellbeingView);

  @override
  Future<List<Map<String, dynamic>>> connections() async => [];

  @override
  Future<List<AndroidCalendarOption>> listCalendars() async => [];

  @override
  Future<List<String>> selectedCalendars() async => ['calendar-one'];

  @override
  Future<List<String>> setSelectedCalendars(List<String> calendarIds) async =>
      calendarIds;

  @override
  Future<bool> requestPermission(AndroidContextSource source) async =>
      permissionGranted;

  @override
  Future<Map<String, dynamic>> readContacts({int limit = 64}) async =>
      Map.of(_peopleView)..['source_handle'] = 'people:android:source';

  @override
  Future<Map<String, dynamic>> readWellbeing() async => wellbeing;

  @override
  Future<Map<String, dynamic>> readCalendar({
    required DateTime rangeStart,
    required DateTime rangeEnd,
    String cursor = '',
    int limit = 128,
  }) async => {
    'schema_version': 1,
    'view_id': 'calendar.timeline',
    'source_handle': 'calendar.timeline:android',
    'observed_at_unix_ms': 1000,
    'expires_at_unix_ms': 301000,
    'range_start_unix_ms': rangeStart.millisecondsSinceEpoch,
    'range_end_unix_ms': rangeEnd.millisecondsSinceEpoch,
    'coverage_complete': true,
    'items': [
      {
        'evidence_handle': 'calendar.event:one',
        'untrusted_title': 'Planning review',
        'starts_at_unix_ms': 2000,
        'ends_at_unix_ms': 3000,
        'all_day': false,
      },
    ],
  };
}

final class _RecordingTransport implements LocalContextTransport {
  final List<_Publication> published = [];
  final List<_Revocation> revoked = [];

  @override
  Future<void> publishLocalContext({
    required String personId,
    required String deviceId,
    required Map<String, dynamic> view,
  }) async {
    published.add(_Publication(personId, deviceId, Map.of(view)));
  }

  @override
  Future<int> revokeLocalContext({
    required String personId,
    required String deviceId,
    String? viewId,
  }) async {
    revoked.add(_Revocation(personId, deviceId, viewId));
    return 1;
  }
}

final class _Publication {
  const _Publication(this.personId, this.deviceId, this.view);
  final String personId;
  final String deviceId;
  final Map<String, dynamic> view;
}

final class _Revocation {
  const _Revocation(this.personId, this.deviceId, this.viewId);
  final String personId;
  final String deviceId;
  final String? viewId;
}
