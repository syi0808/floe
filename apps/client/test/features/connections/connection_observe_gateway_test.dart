import 'package:floe_client/features/connections/application/connection_observe_gateway.dart';
import 'package:floe_client/features/connections/domain/connection_observe.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../support/app_wire_transport.dart';

const connectorId = 'calendar.google';
const connectionId = '00000000-0000-4000-8000-000000000011';

Map<String, Object?> overview(String status) => {
  'connector_id': connectorId,
  'connection_id': connectionId,
  'status': status,
  'enabled': status == 'active',
  'source_resources': ['provider-calendar-a'],
  'members': [
    {
      'view_id': 'calendar.events',
      'state': status == 'active' ? 'active' : 'paused',
      'review_required': false,
    },
  ],
};

Map<String, Object?> expectation() => {
  'connector_id': connectorId,
  'connection_id': connectionId,
  'source_authority': {
    'incarnation': '00000000-0000-4000-8000-000000000022',
    'epoch': 1,
  },
  'connection_revision': 2,
  'native_subject': null,
  'producer_fingerprint': 'a' * 64,
  'members': [
    {
      'view_id': 'calendar.events',
      'policy_digest': 'b' * 64,
      'resource': 'connection/$connectionId/view/calendar.events',
      'expected_grant_id': null,
      'expected_grant_authority': null,
    },
  ],
};

void main() {
  test(
    'inspect, review and enable use one connection-level owner wire',
    () async {
      final starts = <Map<String, dynamic>>[];
      final responses = <String, Map<String, dynamic>>{};
      final transport = CallbackAppWireTransport((request) async {
        final intent = (request['command'] ?? request['query']) as Map;
        if (intent['kind'] == 'connection_observe.read_result') {
          final operationId = intent['operation_id'] as String;
          return responses[operationId]!;
        }
        starts.add(Map<String, dynamic>.from(intent));
        final operationId = ownerOperationId(request);
        final kind = intent['kind'] as String;
        final response = <String, dynamic>{
          'kind': 'connection_observe',
          'operation_id': operationId,
          'done': true,
          'state': 'ready',
          'overview': kind == 'connection_observe.review'
              ? null
              : overview(
                  kind == 'connection_observe.set_enabled'
                      ? 'active'
                      : 'paused',
                ),
          'reviewed': kind == 'connection_observe.review'
              ? expectation()
              : null,
          'failure': null,
        };
        responses[operationId] = response;
        return response;
      });
      final gateway = AppWireConnectionObserveGateway(transport);
      final before = await gateway.inspect(
        connectorId: connectorId,
        connectionId: connectionId,
      );
      expect(before.sourceResources, ['provider-calendar-a']);
      final reviewed = await gateway.review(
        connectorId: connectorId,
        connectionId: connectionId,
      );
      final enabled = await gateway.setEnabled(
        connectorId: connectorId,
        connectionId: connectionId,
        enabled: true,
        expected: reviewed,
      );
      expect(enabled.enabled, isTrue);
      expect(starts.map((intent) => intent['kind']), [
        'connection_observe.inspect',
        'connection_observe.review',
        'connection_observe.set_enabled',
      ]);
      final mutation = starts.last['mutation'] as Map;
      expect(mutation['expected'], expectation());
      expect(mutation.containsKey('source_resources'), isFalse);
      expect(mutation.containsKey('selected_handles'), isFalse);
    },
  );

  test('review rejects remote routing authority in the product snapshot', () {
    final raw = expectation()..['provider_identity'] = 'provider';
    expect(() => ConnectionObserveReview.fromJson(raw), throwsFormatException);
  });
}
