import 'package:floe_client/app/runtime/local_context_gateway.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../support/app_wire_transport.dart';

void main() {
  test('native commands derive identity in App while retaining request and epoch evidence', () async {
    final requests = <Map<String, dynamic>>[];
    final transport = CallbackAppWireTransport((request) async {
      requests.add(request);
      return {
        'kind': 'context_applied',
        'command_id': request['command_id'],
        'context': {'person_id': 'person'},
      };
    });
    final gateway = NativeLocalContextGateway(transport, deviceId: 'device');
    await gateway.registerAttentionHost(personId: 'person', hostEpoch: 'epoch');
    final result = <String, dynamic>{
      'person_id': 'person',
      'device_id': 'device',
      'request_id': 'acquisition',
      'host_epoch': 'epoch',
      'native_subject_fingerprint_before': 'a' * 64,
      'native_subject_fingerprint_after': 'a' * 64,
    };
    await gateway.completeAttentionAcquisition(
      personId: 'person',
      hostEpoch: 'epoch',
      result: result,
    );
    await gateway.disposeAttentionHost(personId: 'person', hostEpoch: 'epoch');
    for (final request in requests) {
      expect(request.keys.toSet(), {
        'schema_version',
        'request_id',
        'command_id',
        'command',
      });
      expect((request['command'] as Map)['kind'], 'context.apply');
    }
    final completion =
        ((requests[1]['command'] as Map)['command'] as Map)['result'] as Map;
    expect(completion.containsKey('person_id'), isFalse);
    expect(completion.containsKey('device_id'), isFalse);
    expect(completion['request_id'], 'acquisition');
    expect(completion['host_epoch'], 'epoch');
    expect(completion['native_subject_fingerprint_after'], 'a' * 64);
    expect(result['device_id'], 'device');
    await expectLater(
      gateway.completeAttentionAcquisition(
        personId: 'person',
        hostEpoch: 'epoch',
        result: {...result, 'device_id': 'foreign'},
      ),
      throwsFormatException,
    );
    expect(requests, hasLength(3));
  });

  test('native polls use queries and reject stale Person or wrong command correlation', () async {
    var foreign = false;
    final transport = CallbackAppWireTransport((request) async {
      if (request['query'] case final Map query) {
        expect(query['kind'], 'context.read');
        expect((query['query'] as Map)['kind'], 'poll_acquisitions');
        return {
          'kind': 'context_read',
          'context': {
            'person_id': foreign ? 'other' : 'person',
            'acquisitions': <Object>[],
          },
        };
      }
      return {
        'kind': 'context_applied',
        'command_id': 'wrong',
        'context': {'person_id': 'person'},
      };
    });
    final gateway = NativeLocalContextGateway(transport, deviceId: 'device');
    expect(
      await gateway.pollAcquisitions(personId: 'person', hostEpoch: 'epoch'),
      isEmpty,
    );
    foreign = true;
    await expectLater(
      gateway.pollAcquisitions(personId: 'person', hostEpoch: 'epoch'),
      throwsFormatException,
    );
    await expectLater(
      gateway.registerAcquisitionHost(personId: 'person', hostEpoch: 'epoch'),
      throwsFormatException,
    );
  });
}
