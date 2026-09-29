import 'package:floe_client/features/connections/application/native_calendar_access_gateway.dart';
import 'package:floe_client/features/connections/domain/native_calendar_access.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../support/app_wire_transport.dart';

void main() {
  Map<String, Object?> overviewJson() => {
    'schema_version': 1,
    'person_id': 'person',
    'provider': 'event_kit',
    'connection_id': 'connection',
    'selected_resources': ['home'],
    'granted_resources': <String>[],
    'source_authority': {
      'incarnation': '00000000-0000-4000-8000-000000000009',
      'epoch': 1,
    },
    'state': 'needs_review',
    'review_required': true,
  };

  test('overview parses a grant-less review state', () {
    final overview = NativeCalendarAccessOverview.fromJson(overviewJson());
    expect(overview.state, 'needs_review');
    expect(overview.hasGrant, isFalse);
  });

  test('overview parses a granted state with an all-or-none expectation', () {
    final overview = NativeCalendarAccessOverview.fromJson({
      ...overviewJson(),
      'granted_resources': ['home'],
      'grant_id': '00000000-0000-4000-8000-000000000010',
      'grant_authority': {'incarnation': 'fixture', 'access_epoch': 1},
      'state': 'active',
      'review_required': false,
    });
    expect(overview.state, 'active');
    expect(overview.hasGrant, isTrue);
  });

  test('overview rejects partial grant expectations and unknown states', () {
    expect(
      () => NativeCalendarAccessOverview.fromJson({
        ...overviewJson(),
        'grant_id': '00000000-0000-4000-8000-000000000010',
        'state': 'active',
        'review_required': false,
      }),
      throwsFormatException,
    );
    expect(
      () => NativeCalendarAccessOverview.fromJson({
        ...overviewJson(),
        'state': 'observing',
      }),
      throwsFormatException,
    );
    expect(
      () => NativeCalendarAccessOverview.fromJson({
        ...overviewJson(),
        'grant_id': '00000000-0000-4000-8000-000000000010',
        'grant_authority': {'incarnation': 'fixture', 'access_epoch': 1},
      }),
      throwsFormatException,
    );
  });

  test('overview rejects obsolete policy fields', () {
    expect(
      () => NativeCalendarAccessOverview.fromJson({
        ...overviewJson(),
        'consumer_policy': {'incarnation': 'old', 'epoch': 1},
      }),
      throwsFormatException,
    );
  });

  test('subject preview parses the reviewed fingerprint', () {
    final preview = NativeCalendarSubjectPreview.fromJson({
      'provider': 'event_kit',
      'device_id': 'device',
      'calendar_ids': ['home'],
      'connection_scope': 'selected',
      'connection_id': 'connection',
      'connection_revision': 1,
      'source_authority': {
        'incarnation': '00000000-0000-4000-8000-000000000009',
        'epoch': 1,
      },
      'native_subject_fingerprint': 'f' * 64,
    });
    expect(preview.nativeSubjectFingerprint, 'f' * 64);
    expect(
      () => NativeCalendarSubjectPreview.fromJson({'provider': 'event_kit'}),
      throwsFormatException,
    );
  });

  test('subject preview sends more than four selected calendars', () async {
    final calendarIds = List.generate(11, (index) => 'calendar-$index');
    final requests = <Map<String, dynamic>>[];
    final transport = CallbackAppWireTransport((request) async {
      requests.add(request);
      return {
        'kind': 'local_access_operation',
        'operation_id':
            request['query'] is Map &&
                (request['query'] as Map)['kind'] == 'access.calendar.preview'
            ? request['request_id']
            : (request['query'] as Map)['operation_id'],
        'done': true,
        'state': 'ready',
        'calendar_subject_preview': {
          'provider': 'event_kit',
          'device_id': 'device',
          'calendar_ids': calendarIds,
          'connection_scope': 'all',
          'connection_id': 'connection',
          'connection_revision': 1,
          'source_authority': {
            'incarnation': '00000000-0000-4000-8000-000000000009',
            'epoch': 1,
          },
          'native_subject_fingerprint': 'f' * 64,
        },
      };
    });
    final gateway = AppWireNativeCalendarAccessGateway(
      transport,
      deviceId: 'device',
    );

    final preview = await gateway.previewCalendarSubject(
      personId: 'person',
      provider: 'event_kit',
      connectionId: 'connection',
      calendarIds: calendarIds,
      connectionScope: 'all',
      connectionRevision: 1,
      sourceAuthority: const {
        'incarnation': '00000000-0000-4000-8000-000000000009',
        'epoch': 1,
      },
    );

    expect(preview.calendarIds, calendarIds);
    expect(
      (requests.first['query'] as Map)['request']['calendar_ids'],
      calendarIds,
    );
  });
}
