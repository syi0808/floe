import 'package:floe_client/features/connections/domain/native_calendar_access.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  Map<String, Object?> overviewJson() => {
    'schema_version': 1,
    'person_id': 'person',
    'provider': 'event_kit',
    'connection_id': 'connection',
    'selected_resources': ['home'],
    'granted_resources': <String>[],
    'source_authority': {'incarnation': '00000000-0000-4000-8000-000000000009', 'epoch': 1},
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
      'consumer_policy': {'incarnation': 'policy', 'epoch': 1},
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
        'consumer_policy': {'incarnation': 'policy', 'epoch': 1},
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
      'source_authority': {'incarnation': '00000000-0000-4000-8000-000000000009', 'epoch': 1},
      'native_subject_fingerprint': 'f' * 64,
    });
    expect(preview.nativeSubjectFingerprint, 'f' * 64);
    expect(
      () => NativeCalendarSubjectPreview.fromJson({
        'provider': 'event_kit',
      }),
      throwsFormatException,
    );
  });
}
