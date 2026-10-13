import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/features/day/domain/day_models.dart';
import 'package:floe_client/features/day/infrastructure/app_wire_day_gateway.dart';

const _person = '11111111-1111-4111-8111-111111111111';
const _event = '22222222-2222-4222-8222-222222222222';
const _source = '33333333-3333-4333-8333-333333333333';
const _resource = '44444444-4444-4444-8444-444444444444';

void main() {
  final labels = ['${'a' * 256} · ${'b' * 256}', '${'가' * 341}a'];
  for (final label in labels) {
    test(
      'Day preserves ${utf8.encode(label).length}-byte qualified labels',
      () async {
        final result = await AppWireDayGateway(_Transport(_snapshot(label)))
            .loadDay(_query());
        expect((result.items.single as EventItem).calendarLabel, label);
        expect(result.calendarCoverage!.sources.single.label, label);
        expect(
          result.calendarCoverage!.sources.single.resources.single.label,
          label,
        );
      },
    );
  }
  for (final location in ['event', 'source', 'resource']) {
    test('Day rejects over-capacity $location labels', () async {
      final snapshot = _snapshot('Calendar');
      final oversized = '${'가' * 341}ab';
      if (location == 'event') {
        ((snapshot['items'] as List).single['source']
                as Map)['calendar_label'] =
            oversized;
      } else {
        final source =
            ((snapshot['calendar'] as Map)['sources'] as List).single as Map;
        final target = location == 'source'
            ? source
            : (source['resources'] as List).single as Map;
        target['label'] = oversized;
      }
      await expectLater(
        AppWireDayGateway(_Transport(snapshot)).loadDay(_query()),
        throwsFormatException,
      );
    });
  }
}

DayQuery _query() => DayQuery(
  personId: _person,
  date: DateTime(2026, 10, 13),
  now: DateTime.utc(2026, 10, 13),
  timezoneOffsetSeconds: 0,
);

Map<String, dynamic> _snapshot(String label) => {
  'schema_version': 1,
  'person_id': _person,
  'date': '2026-10-13',
  'generated_at': '2026-10-13T00:00:00Z',
  'timezone_offset_seconds': 0,
  'now_event_id': null,
  'next_event_id': _event,
  'overdue_task_count': 0,
  'calendar_mirror_revision': 1,
  'items': [
    {
      'kind': 'event',
      'id': _event,
      'person_id': _person,
      'title': 'Synthetic event',
      'schedule': {
        'kind': 'timed',
        'starts_at': '2026-10-13T01:00:00Z',
        'ends_at': '2026-10-13T02:00:00Z',
        'timezone': 'UTC',
      },
      'source': {
        'kind': 'calendar',
        'source_ref': _source,
        'calendar_ref': _resource,
        'calendar_label': label,
      },
      'created_at': '2026-10-13T00:00:00Z',
      'updated_at': '2026-10-13T00:00:00Z',
      'revision': 1,
      'deleted_at': null,
      'action_target': {'event_id': _event, 'expected_revision': 1},
    },
  ],
  'calendar': {
    'sources': [
      {
        'source_ref': _source,
        'label': label,
        'state': 'pending',
        'last_success_at': null,
        'last_range': null,
        'failure': null,
        'failure_at': null,
        'resources': [
          {
            'resource_ref': _resource,
            'label': label,
            'state': 'pending',
            'last_success_at': null,
            'last_range': null,
            'failure': null,
            'failure_at': null,
          },
        ],
      },
    ],
  },
};

final class _Transport implements AppWireTransport {
  _Transport(this.snapshot);
  final Map<String, dynamic> snapshot;
  @override
  Future<Map<String, dynamic>> queryV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) async {
    expect((request['query'] as Map)['kind'], 'day.snapshot');
    return {'kind': 'day_snapshot', 'snapshot': snapshot};
  }

  @override
  Future<Map<String, dynamic>> commandV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) => throw StateError('Unexpected command');
  @override
  Future<Map<String, dynamic>> eventsV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) => throw StateError('Unexpected events');
  @override
  Future<void> close() async {}
}
