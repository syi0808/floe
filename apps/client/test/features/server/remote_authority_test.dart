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

  testWidgets(
    'settings requires inspect and explicit review before status refresh',
    (tester) async {
      final client = _FakeServerClient(
        ServerConnection(
          address: 'http://127.0.0.1:8080',
          token: 'a' * 32,
          clientId: 'saved-client',
          personId: 'person-1',
          deviceId: 'device-1',
        ),
      );
      Object? statusFailure;
      var reviewFails = false;
      final calls = <String>[];
      final gateway = NativeAgentVaultGateway((request) async {
        final operation = Map<String, Object?>.from(
          request['operation']! as Map,
        );
        if (operation['kind'] == 'release') {
          return _success(request);
        }
        final action = Map<String, Object?>.from(operation['action']! as Map);
        final kind = action['kind']! as String;
        calls.add(kind);
        if (kind == 'remote_authority_inspect_producer') {
          return _success(request, {
            'remote_producer': producer,
            'remote_owner': owner,
          });
        }
        if (kind == 'remote_authority_review_and_enroll') {
          if (reviewFails) throw const AgentVaultException('policy_denied');
          return _success(request, {'remote_enrollment': pending});
        }
        if (kind == 'remote_authority_enrollment_status') {
          if (statusFailure != null) throw statusFailure!;
          return _success(request, {'remote_enrollment': approved});
        }
        throw StateError('unexpected action $kind');
      }, deviceId: 'device-1');

      await tester.pumpWidget(
        MaterialApp(
          theme: FloeTheme.light,
          home: Scaffold(
            body: SingleChildScrollView(
              child: SettingsScreen(client: client, agentVaultGateway: gateway),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();
      expect(
        find.byKey(const ValueKey('remote-authority-review')),
        findsNothing,
      );
      expect(calls, isEmpty);

      final inspectButton = find.byKey(
        const ValueKey('remote-authority-inspect'),
      );
      await tester.ensureVisible(inspectButton);
      await tester.tap(inspectButton);
      await tester.pumpAndSettle();
      expect(
        find.text('Producer fingerprint: producer-fingerprint'),
        findsOneWidget,
      );
      expect(
        find.text('Producer instance: ${producer['instance_id']}'),
        findsOneWidget,
      );
      expect(
        find.text('Local owner fingerprint: owner-fingerprint'),
        findsOneWidget,
      );
      expect(calls, ['remote_authority_inspect_producer']);

      final reviewButton = find.byKey(
        const ValueKey('remote-authority-review'),
      );
      await tester.ensureVisible(reviewButton);
      await tester.tap(reviewButton);
      await tester.pumpAndSettle();
      expect(
        find.textContaining('Pending explicit server admin approval'),
        findsOneWidget,
      );
      expect(calls, [
        'remote_authority_inspect_producer',
        'remote_authority_review_and_enroll',
      ]);

      final refreshButton = find.byKey(
        const ValueKey('remote-authority-refresh'),
      );
      await tester.ensureVisible(refreshButton);
      await tester.tap(refreshButton);
      await tester.pumpAndSettle();
      expect(find.textContaining('Enrollment status: Active'), findsOneWidget);
      expect(calls.last, 'remote_authority_enrollment_status');

      statusFailure = const FormatException('malformed status');
      await tester.ensureVisible(refreshButton);
      await tester.tap(refreshButton);
      await tester.pumpAndSettle();
      expect(find.textContaining('Enrollment status: Active'), findsNothing);
      expect(
        find.text('The server enrollment status could not be read.'),
        findsOneWidget,
      );

      reviewFails = true;
      await tester.ensureVisible(reviewButton);
      await tester.tap(reviewButton);
      await tester.pumpAndSettle();
      expect(
        find.text('Producer fingerprint: producer-fingerprint'),
        findsOneWidget,
      );
      expect(
        find.textContaining(
          'The server identity changed or enrollment was refused.',
        ),
        findsOneWidget,
      );
    },
  );

  testWidgets('calendar source choices require preview before grant review', (
    tester,
  ) async {
    final client = _FakeServerClient(
      ServerConnection(
        address: 'http://127.0.0.1:8080',
        token: 'a' * 32,
        clientId: 'saved-client',
        personId: 'person-1',
        deviceId: 'device-1',
      ),
    );
    final calls = <String>[];
    final gateway = NativeAgentVaultGateway((request) async {
      final operation = Map<String, Object?>.from(request['operation']! as Map);
      if (operation['kind'] == 'release') return _success(request);
      final action = Map<String, Object?>.from(operation['action']! as Map);
      final kind = action['kind']! as String;
      calls.add(kind);
      if (kind == 'remote_calendar_grant_preview') {
        return _success(request, {
          'remote_calendar_preview': {
            'schema_version': 1,
            'person_id': 'person-1',
            'connector_id': 'calendar.google',
            'connection_id': '00000000-0000-4000-8000-000000000010',
            'resource': 'primary',
            'source_authority': {
              'incarnation': '00000000-0000-4000-8000-000000000011',
              'epoch': 1,
            },
            'provider_identity': 'google:subject-a',
            'execution_owner': producer['execution_owner'],
            'producer': producer,
            'consumer': 'calendar.expert',
            'purpose': 'everyday_assistance',
            'recipient': 'local_only',
          },
        });
      }
      if (kind == 'remote_calendar_grant_review') {
        return _success(request, {
          'remote_calendar_grant': {
            'schema_version': 1,
            'grant_id': '00000000-0000-4000-8000-000000000012',
            'grant_authority': {
              'incarnation': '00000000-0000-4000-8000-000000000013',
              'epoch': 1,
            },
            'state': 'active',
            'connector_id': 'calendar.google',
            'resource': 'primary',
            'recipient': 'local_only',
          },
        });
      }
      throw StateError('unexpected action $kind');
    }, deviceId: 'device-1');
    await tester.pumpWidget(
      MaterialApp(
        theme: FloeTheme.light,
        home: Scaffold(
          body: SingleChildScrollView(
            child: SettingsScreen(client: client, agentVaultGateway: gateway),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(calls, isEmpty);
    final selection = find.byKey(const ValueKey('remote-calendar-selection'));
    await tester.ensureVisible(selection);
    await tester.tap(selection);
    await tester.pumpAndSettle();
    await tester.tap(find.text('calendar.google · primary').last);
    await tester.pumpAndSettle();
    expect(calls, isEmpty);
    final preview = find.byKey(const ValueKey('remote-calendar-preview'));
    await tester.ensureVisible(preview);
    await tester.tap(preview);
    await tester.pumpAndSettle();
    expect(calls, ['remote_calendar_grant_preview']);
    expect(
      find.text('Verified provider identity: google:subject-a'),
      findsOneWidget,
    );
    final review = find.byKey(const ValueKey('remote-calendar-review'));
    await tester.ensureVisible(review);
    await tester.tap(review);
    await tester.pumpAndSettle();
    expect(calls, [
      'remote_calendar_grant_preview',
      'remote_calendar_grant_review',
    ]);
    expect(find.text('Grant status: active'), findsOneWidget);
  });
}

Map<String, dynamic> _success(
  Map<String, dynamic> request, [
  Map<String, Object?> payload = const {},
]) => {
  'request_id': request['request_id'],
  'done': true,
  'events': <Object?>[],
  'next_sequence': 0,
  ...payload,
};

final class _MemoryCredentialStore implements ServerCredentialStore {
  String? value;

  @override
  Future<String?> read() async => value;

  @override
  Future<void> write(String value) async => this.value = value;

  @override
  Future<void> delete() async => value = null;
}

final class _FakeServerClient extends LocalServerClient {
  _FakeServerClient(this.saved)
    : super(
        personId: saved.personId,
        deviceId: saved.deviceId,
        store: _MemoryCredentialStore(),
      );

  final ServerConnection saved;

  @override
  Future<ServerConnection?> connection() async => saved;

  @override
  Future<void> checkConnection(ServerConnection value) async {}

  @override
  Future<List<Map<String, dynamic>>> connections(
    ServerConnection value,
  ) async => [
    {
      'connector_id': 'calendar.google',
      'connection_id': '00000000-0000-4000-8000-000000000010',
      'scope': {'calendar_id': 'primary'},
    },
  ];
}
