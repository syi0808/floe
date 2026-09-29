import 'package:floe_client/app/runtime/local_owner_gateways.dart';
import 'package:floe_client/features/settings/domain/feasibility_access.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../support/app_wire_transport.dart';

const personId = '00000000-0000-4000-8000-000000000011';
const deviceId = 'mac-local';

Map<String, Object?> overview() => {
  'schema_version': 1,
  'person_id': personId,
  'connector': 'feasibility.apple',
  'device_id': deviceId,
  'connection_id': 'feasibility.apple.local',
  'source_authority': null,
  'grant_id': null,
  'grant_authority': null,
  'state': 'needs_review',
  'review_required': true,
  'presence_available': true,
  'consumers': <String>[],
  'native_subject_fingerprint': 'a' * 64,
  'process_incarnation': null,
};

void main() {
  test('Feasibility uses contextual wire and never standing Observe', () async {
    final starts = <Map<String, dynamic>>[];
    final transport = CallbackAppWireTransport((request) async {
      final intent = (request['query'] ?? request['command']) as Map;
      if (intent['kind'] != 'access.local.read_result') {
        starts.add(Map<String, dynamic>.from(intent));
      }
      return {
        'kind': 'local_access_operation',
        'operation_id': intent['kind'] == 'access.local.read_result'
            ? intent['operation_id']
            : ownerOperationId(request),
        'done': true,
        'state': 'ready',
        'feasibility_access': overview(),
        'failure': null,
      };
    });
    final gateway = NativeFeasibilityAccessGateway(
      transport,
      deviceId: deviceId,
    );
    final inspected = await gateway.inspectFeasibility(personId);
    await gateway.reviewFeasibility(
      personId,
      reviewedPreview: inspected,
      query: const FeasibilityQuery(
        eventHandle: 'event',
        evidenceHandles: ['evidence'],
        destinationLatitude: 0,
        destinationLongitude: 0,
        eventStartUnixMs: 1000,
        eventEndUnixMs: 2000,
        travelMode: 'automobile',
      ),
    );
    expect(starts.map((intent) => intent['kind']), [
      'access.feasibility.inspect',
      'access.feasibility.configure',
    ]);
    final change = starts.last['change'] as Map;
    expect(change['kind'], 'review');
    expect(change['feasibility_query'], isA<Map>());
    expect(starts.last.containsKey('connector'), isFalse);
  });

  test('Feasibility decoder rejects standing source identity', () {
    expect(
      () => FeasibilityAccessOverview.fromJson({
        ...overview(),
        'connector': 'contacts.apple',
      }),
      throwsFormatException,
    );
  });
}
