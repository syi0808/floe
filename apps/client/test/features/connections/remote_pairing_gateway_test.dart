import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/runtime/native_transport.dart';
import 'package:floe_client/features/connections/application/remote_pairing_gateway.dart';
import 'package:floe_client/features/connections/application/remote_owner_operation.dart';
import 'package:floe_client/features/connections/domain/remote_owner_models.dart';

const pairingId = '00000000-0000-4000-8000-000000000002';
const target = PairingTarget('http://127.0.0.1:8431');

Map<String, dynamic> report({String status = 'approved'}) => {
  'pairing_id': pairingId,
  'person_id': 'person',
  'device_id': 'device',
  'outcome': {
    'status': status,
    if (status == 'approved') ...{
      'client_id': pairingId,
      'token': 'secret-issued-token',
    },
  },
};

void main() {
  test(
    'approved report stays retained until explicit persistence acknowledgement',
    () async {
      final calls = <Map<String, dynamic>>[];
      final gateway = NativeRemotePairingGateway(
        (request) async {
          calls.add(request);
          final operation = request['operation'] as Map;
          return {
            'operation_id': operation['operation_id'] ?? request['request_id'],
            'done': true,
            'pairing': report(),
          };
        },
        expectedPersonId: 'person',
        expectedDeviceId: 'device',
      );
      final result = await gateway.remotePairingStatus(
        target: target,
        pairingId: pairingId,
        pollingProof: 'proof',
      );
      expect(result.token, 'secret-issued-token');
      expect(result.toString(), isNot(contains('secret-issued-token')));
      expect(calls, hasLength(1));
      expect(calls.single['schema_version'], 2);
      expect(calls.single['operation'], {
        'kind': 'status',
        'target': {'base_url': target.baseUrl},
        'pairing_id': pairingId,
        'polling_proof': 'proof',
      });
      final reread = await gateway.remotePairingStatus(
        target: target,
        pairingId: pairingId,
        pollingProof: 'proof',
      );
      expect(reread.operationId, result.operationId);
      expect(calls, hasLength(1));
      await gateway.releaseApprovedPairing(pairingId);
      expect((calls.last['operation'] as Map)['release'], true);
      expect(
        (calls.last['operation'] as Map)['operation_id'],
        result.operationId,
      );
      expect(calls.last['request_id'], isNot(result.operationId));
    },
  );

  test('lost submit acknowledgement observes original operation without resubmission', () async {
    var submits = 0;
    String? operationId;
    final gateway = NativeRemotePairingGateway(
      (request) async {
        final operation = request['operation'] as Map;
        if (operation['kind'] != 'read_result') {
          submits++;
          operationId = request['request_id'] as String;
          throw const NativeTransportException('timeout', 'lost');
        }
        expect(operation['operation_id'], operationId);
        return {
          'operation_id': operationId,
          'done': true,
          'pairing': report(status: 'pending'),
        };
      },
      expectedPersonId: 'person',
      expectedDeviceId: 'device',
      pollInterval: Duration.zero,
    );
    expect(
      (await gateway.remotePairingStatus(
        target: target,
        pairingId: pairingId,
        pollingProof: 'proof',
      )).status,
      'pending',
    );
    expect(submits, 1);
  });

  test('lost release retains operation identity and never creates another credential', () async {
    var submits = 0;
    var releases = 0;
    final gateway = NativeRemotePairingGateway(
      (request) async {
        final operation = request['operation'] as Map;
        if (operation['kind'] != 'read_result') submits++;
        if (operation['release'] == true && releases++ == 0) {
          throw const NativeTransportException('timeout', 'lost');
        }
        return {
          'operation_id': operation['operation_id'] ?? request['request_id'],
          'done': true,
          'pairing': report(),
        };
      },
      expectedPersonId: 'person',
      expectedDeviceId: 'device',
    );
    await gateway.remotePairingStatus(
      target: target,
      pairingId: pairingId,
      pollingProof: 'proof',
    );
    await expectLater(
      gateway.releaseApprovedPairing(pairingId),
      throwsA(isA<RemoteOperationPending>()),
    );
    await gateway.releaseApprovedPairing(pairingId);
    expect(submits, 1);
  });

  test(
    'strict reports reject flat wire, unknown fields and misplaced credentials',
    () {
      for (final invalid in [
        {
          'pairing_id': pairingId,
          'person_id': 'person',
          'device_id': 'device',
          'status': 'approved',
          'token': 'secret',
        },
        {...report(), 'unknown': true},
        {
          ...report(),
          'outcome': {'status': 'pending', 'token': 'secret'},
        },
        {
          ...report(),
          'outcome': {'status': 'approved', 'token': 'secret'},
        },
        {
          ...report(),
          'outcome': {
            'status': 'approved',
            'client_id': pairingId,
            'token': '',
          },
        },
        {
          ...report(),
          'producer': {'invalid': true},
        },
        {
          ...report(),
          'issuer': {'invalid': true},
        },
      ]) {
        expect(
          () => RemotePairingStatus.fromJson(invalid, 'operation'),
          throwsFormatException,
        );
      }
    },
  );

  for (final field in ['pairing_id', 'person_id', 'device_id']) {
    test('foreign $field is rejected before release', () async {
      final calls = <Map<String, dynamic>>[];
      final gateway = NativeRemotePairingGateway(
        (request) async {
          calls.add(request);
          return {
            'operation_id': request['request_id'],
            'done': true,
            'pairing': {...report(status: 'pending'), field: 'foreign'},
          };
        },
        expectedPersonId: 'person',
        expectedDeviceId: 'device',
      );
      await expectLater(
        gateway.remotePairingStatus(
          target: target,
          pairingId: pairingId,
          pollingProof: 'proof',
        ),
        throwsFormatException,
      );
      expect(calls, hasLength(1));
    });
  }
}
