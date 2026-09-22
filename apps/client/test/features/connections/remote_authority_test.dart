import 'package:floe_client/features/connections/application/remote_access_gateway.dart';
import 'package:floe_client/features/connections/application/local_server_client.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/features/settings/presentation/settings_screen.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flutter/material.dart';

void main() {
  const producer = {
    'schema_version': 1,
    'instance_id': '00000000-0000-4000-8000-000000000001',
    'execution_owner': '00000000-0000-4000-8000-000000000002',
    'audience': 'floe.server:00000000-0000-4000-8000-000000000001',
    'key_id': '00000000-0000-4000-8000-000000000003',
    'public_key': 'producer-public-key',
    'fingerprint': 'producer-fingerprint',
  };

  const owner = {
    'key_id': '00000000-0000-4000-8000-000000000004',
    'public_key': 'owner-public-key',
    'fingerprint': 'owner-fingerprint',
  };

  const pending = {
    'enrollment_id': '00000000-0000-4000-8000-000000000005',
    'key_id': '00000000-0000-4000-8000-000000000004',
    'fingerprint': 'owner-fingerprint',
    'local_confirmed': true,
    'admin_approved': false,
    'active': false,
  };

  const approved = {
    'enrollment_id': '00000000-0000-4000-8000-000000000005',
    'key_id': '00000000-0000-4000-8000-000000000004',
    'fingerprint': 'owner-fingerprint',
    'local_confirmed': true,
    'admin_approved': true,
    'active': true,
  };

  test(
    'inspection is read-only and review/status preserve explicit approval',
    () async {
      final calls = <Map<String, Object?>>[];
      final gateway = NativeRemoteAccessGateway((request) async {
        final operation = Map<String, Object?>.from(
          request['operation']! as Map,
        );
        if (operation['kind'] == 'read_result') {
          return {
            'operation_id':
                (request['operation'] as Map)['operation_id'] ??
                request['request_id'],
            'done': true,
          };
        }
        final action = operation;
        calls.add(action);
        Map<String, dynamic> success(Map<String, Object?> payload) => {
          'operation_id':
              (request['operation'] as Map)['operation_id'] ??
              request['request_id'],
          'done': true,
          ...payload,
        };
        switch (action['kind']) {
          case 'inspect_producer':
            return success({'producer': producer, 'owner': owner});
          case 'review_and_enroll':
            return success({'enrollment': pending});
          case 'enrollment_status':
            return success({'enrollment': approved});
          default:
            throw StateError('unexpected action ${action['kind']}');
        }
      });

      final inspection = await gateway.inspectRemoteProducer();
      expect(inspection.producer.fingerprint, 'producer-fingerprint');
      expect(inspection.ownerFingerprint, 'owner-fingerprint');
      expect(calls, hasLength(1));

      final status = await gateway.reviewAndEnrollRemoteProducer(
        producer: inspection.producer,
      );
      expect(status.localConfirmed, isTrue);
      expect(status.adminApproved, isFalse);
      expect(status.active, isFalse);

      final refreshed = await gateway.remoteEnrollmentStatus(
        enrollmentId: status.enrollmentId,
      );
      expect(refreshed.adminApproved, isTrue);
      expect(refreshed.active, isTrue);
      expect(calls.map((request) => request['kind']), [
        'inspect_producer',
        'review_and_enroll',
        'enrollment_status',
      ]);
    },
  );

  test(
    'saved pairing remains storage-only and mismatched identity fails',
    () async {
      final client = LocalServerClient(
        personId: 'person-1',
        deviceId: 'device-1',
        store: _MemoryCredentialStore(),
      );
      final connection = ServerConnection(
        address: 'http://127.0.0.1:8080',
        token: 'saved-pairing-token-with-at-least-32-characters',
        clientId: 'saved-client',
        personId: 'person-1',
        deviceId: 'device-1',
      );
      await client.save(connection);
      expect((await client.connection())?.clientId, connection.clientId);
      expect(() {
        client.save(
          ServerConnection(
            address: connection.address,
            token: connection.token,
            clientId: connection.clientId,
            personId: 'other-person',
            deviceId: connection.deviceId,
          ),
        );
      }, throwsA(isA<ServerConnectionException>()));
    },
  );

  testWidgets('settings does not render the removed authority enrollment', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        home: Scaffold(
          body: SettingsScreen(client: null, personalAccessGateway: null),
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('Server authority enrollment'), findsNothing);
    expect(
      find.byKey(const ValueKey('remote-authority-inspect')),
      findsNothing,
    );
  });
}

final class _MemoryCredentialStore implements ServerCredentialStore {
  String? value;

  @override
  Future<String?> read() async => value;

  @override
  Future<void> write(String value) async => this.value = value;

  @override
  Future<void> delete() async => value = null;
}
