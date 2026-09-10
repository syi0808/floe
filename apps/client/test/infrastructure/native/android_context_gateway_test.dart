import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/infrastructure/native/android_context_gateway.dart';

void main() {
  test('Android Calendar projection accepts only the strict bounded View', () {
    final view = <String, dynamic>{
      'schema_version': 1,
      'view_id': 'calendar.timeline',
      'source_handle': 'calendar.timeline:source',
      'observed_at_unix_ms': 1000,
      'expires_at_unix_ms': 301000,
      'range_start_unix_ms': 0,
      'range_end_unix_ms': 86400000,
      'coverage_complete': true,
      'items': <Object?>[
        <String, dynamic>{
          'evidence_handle': 'calendar.event:first',
          'untrusted_title': 'Planning review',
          'starts_at_unix_ms': 10000,
          'ends_at_unix_ms': 20000,
          'all_day': false,
        },
      ],
    };
    expect(() => validateAndroidCalendarView(view), returnsNormally);

    final escalated = Map<String, dynamic>.from(view)..['authority'] = 'create';
    expect(() => validateAndroidCalendarView(escalated), throwsFormatException);
    final duplicate = Map<String, dynamic>.from(view)
      ..['items'] = <Object?>[
        ...(view['items']! as List<Object?>),
        ...(view['items']! as List<Object?>),
      ];
    expect(() => validateAndroidCalendarView(duplicate), throwsFormatException);
  });

  test('Android Contacts projection accepts only bounded identities', () {
    final view = <String, dynamic>{
      'schema_version': 1,
      'view_id': 'people.identity',
      'source_handle': 'people:android',
      'observed_at_unix_ms': 1000,
      'expires_at_unix_ms': 301000,
      'coverage_complete': true,
      'identities': <Object?>[
        <String, dynamic>{
          'identity_handle': 'person.identity:first',
          'display_name': 'Alex',
          'aliases': <String>[],
          'confidence_millis': 1000,
          'evidence_handles': <String>['contact.evidence:first'],
        },
      ],
    };
    expect(() => validateAndroidPeopleView(view), returnsNormally);

    final leaked = Map<String, dynamic>.from(view);
    leaked['identities'] = <Object?>[
      <String, dynamic>{
        ...((view['identities']! as List<Object?>).first!
            as Map<String, dynamic>),
        'note': 'private contact note',
      },
    ];
    expect(() => validateAndroidPeopleView(leaked), throwsFormatException);
  });
}
