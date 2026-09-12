import 'package:floe_client/features/agent/agent_vault_gateway.dart';
import 'package:floe_client/features/server/local_server_client.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/features/server/settings_screen.dart';
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

  final route = <String, Object?>{
    'base_url': 'http://127.0.0.1:8080',
    'bearer_token': 'saved-pairing-token',
    'purpose': 'everyday_assistance',
    'external': false,
    'allow_external': false,
    'pairing': {
      'client_id': 'saved-client',
      'person_id': 'person-1',
      'device_id': 'device-1',
    },
    'calendar_connections': <Object?>[],
  };

  test(
    'inspection is read-only and review/status preserve explicit approval',
    () async {
      final calls = <Map<String, Object?>>[];
      final gateway = NativeAgentVaultGateway((request) async {
        final operation = Map<String, Object?>.from(
          request['operation']! as Map,
        );
        if (operation['kind'] == 'release') {
          return {
            'request_id': request['request_id'],
            'done': true,
            'events': <Object?>[],
            'next_sequence': 0,
          };
        }
        final action = Map<String, Object?>.from(operation['action']! as Map);
        calls.add(action);
        Map<String, dynamic> success(Map<String, Object?> payload) => {
          'request_id': request['request_id'],
          'done': true,
          'events': <Object?>[],
          'next_sequence': 0,
          ...payload,
        };
        switch (action['kind']) {
          case 'remote_authority_inspect_producer':
            return success({
              'remote_producer': producer,
              'remote_owner': owner,
            });
          case 'remote_authority_review_and_enroll':
            return success({'remote_enrollment': pending});
          case 'remote_authority_enrollment_status':
            return success({'remote_enrollment': approved});
          default:
            throw StateError('unexpected action ${action['kind']}');
        }
      }, deviceId: 'device-1');

      final inspection = await gateway.inspectRemoteProducer(
        personId: 'person-1',
        route: route,
      );
      expect(inspection.producer.fingerprint, 'producer-fingerprint');
      expect(inspection.ownerFingerprint, 'owner-fingerprint');
      expect(calls, hasLength(1));

      final status = await gateway.reviewAndEnrollRemoteProducer(
        personId: 'person-1',
        route: route,
        producer: inspection.producer,
      );
      expect(status.localConfirmed, isTrue);
      expect(status.adminApproved, isFalse);
      expect(status.active, isFalse);

      final refreshed = await gateway.remoteEnrollmentStatus(
        personId: 'person-1',
        route: route,
        enrollmentId: status.enrollmentId,
      );
      expect(refreshed.adminApproved, isTrue);
      expect(refreshed.active, isTrue);
      expect(calls.map((request) => request['kind']), [
        'remote_authority_inspect_producer',
        'remote_authority_review_and_enroll',
        'remote_authority_enrollment_status',
      ]);
    },
  );

  test('saved pairing is copied into the authority route and mismatched identity fails', () {
    final client = LocalServerClient(
      personId: 'person-1',
      deviceId: 'device-1',
      store: _MemoryCredentialStore(),
    );
    final connection = ServerConnection(
      address: 'http://127.0.0.1:8080',
      token: 'saved-pairing-token',
      clientId: 'saved-client',
      personId: 'person-1',
      deviceId: 'device-1',
    );
    final authority = client.authorityRoute(connection);
    expect(authority['pairing'], route['pairing']);
    expect(() {
      client.authorityRoute(
        ServerConnection(
          address: connection.address,
          token: connection.token,
          clientId: connection.clientId,
          personId: 'other-person',
          deviceId: connection.deviceId,
        ),
      );
    }, throwsA(isA<ServerConnectionException>()));
  });

  testWidgets('settings does not render the removed authority enrollment', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        home: Scaffold(
          body: SettingsScreen(client: null, agentVaultGateway: null),
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
