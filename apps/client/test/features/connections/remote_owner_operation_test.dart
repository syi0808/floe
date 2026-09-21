import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/app/runtime/agent_vault_gateway.dart';
import 'package:floe_client/app/runtime/native_transport.dart';
import 'package:floe_client/features/connections/application/remote_owner_operation.dart';

void main() {
  test('foreign operation result is never accepted or released', () async {
    var calls = 0;
    final gateway = RemoteOwnerOperation((request) async {
      calls++;
      return {'operation_id': 'foreign', 'done': true};
    }, resultFields: {});
    await expectLater(
      gateway.perform({'kind': 'inspect_producer'}, (_) => true),
      throwsFormatException,
    );
    expect(calls, 1);
  });

  test(
    'observer deadline retains ID and does not cancel or resubmit',
    () async {
      final requests = <Map<String, dynamic>>[];
      var done = false;
      final gateway = RemoteOwnerOperation(
        (request) async {
          requests.add(request);
          final operation = request['operation'] as Map;
          return {
            'operation_id': operation['operation_id'] ?? request['request_id'],
            'done': done,
          };
        },
        resultFields: {},
        deadline: Duration.zero,
      );
      await expectLater(
        gateway.perform({'kind': 'calendar_grant_review'}, (_) => true),
        throwsA(isA<RemoteOperationPending>()),
      );
      done = true;
      await expectLater(
        gateway.perform({'kind': 'calendar_grant_review'}, (_) => true),
        throwsA(isA<RemoteOperationPending>()),
      );
      expect(requests, hasLength(1));
      expect(
        (requests.single['operation'] as Map)['kind'],
        'calendar_grant_review',
      );
    },
  );

  test('lost release acknowledgement reconciles not_found for the same accepted result', () async {
    final ids = <String>[];
    var released = false;
    final gateway = RemoteOwnerOperation((request) async {
      final operation = request['operation'] as Map;
      if (operation['kind'] == 'read_result') {
        expect(operation['operation_id'], ids.single);
        if (released) {
          throw const NativeTransportException('not_found', 'released');
        }
        released = true;
        throw const NativeTransportException('timeout', 'lost acknowledgement');
      }
      ids.add(request['request_id'] as String);
      return {'operation_id': ids.single, 'done': true};
    }, resultFields: {});
    await expectLater(
      gateway.perform({'kind': 'review_and_enroll'}, (_) => true),
      throwsA(isA<RemoteOperationPending>()),
    );
    expect(
      await gateway.perform({'kind': 'review_and_enroll'}, (_) => true),
      true,
    );
    expect(ids, hasLength(1));
  });

  test(
    'lost read acknowledgement retries the same operation without resubmitting',
    () async {
      final requests = <Map<String, dynamic>>[];
      String? operationId;
      var reads = 0;
      final gateway = RemoteOwnerOperation(
        (request) async {
          requests.add(request);
          final operation = request['operation'] as Map;
          if (operation['kind'] != 'read_result') {
            operationId = request['request_id'] as String;
            return {'operation_id': operationId, 'done': false};
          }
          expect(operation['operation_id'], operationId);
          expect(request['request_id'], isNot(operationId));
          if (reads++ == 0) {
            throw const NativeTransportException('timeout', 'lost read');
          }
          return {'operation_id': operationId, 'done': true};
        },
        resultFields: {},
        pollInterval: Duration.zero,
      );
      await expectLater(
        gateway.perform({'kind': 'calendar_grant_review'}, (_) => true),
        throwsA(
          isA<RemoteOperationPending>().having(
            (pending) => pending.phase,
            'phase',
            'read_result',
          ),
        ),
      );
      expect(
        await gateway.perform({'kind': 'calendar_grant_review'}, (_) => true),
        true,
      );
      expect(
        requests.where(
          (request) => (request['operation'] as Map)['kind'] != 'read_result',
        ),
        hasLength(1),
      );
      expect((requests.last['operation'] as Map)['release'], true);
    },
  );

  test(
    'owner failure retains exact recovery and correlation metadata',
    () async {
      String? operationId;
      final gateway = RemoteOwnerOperation((request) async {
        final operation = request['operation'] as Map;
        operationId ??= request['request_id'] as String;
        return {
          'operation_id': operation['operation_id'] ?? operationId,
          'done': true,
          'failure': {
            'schema_version': 1,
            'domain': 'source',
            'category': 'security',
            'reason_code': 'source_changed',
            'kind': 'policy_denied',
            'stage': 'remote_view_grant_review',
            'safe_actions': ['review_source'],
            'affected_refs': ['source:exact'],
            'incident_id': 'incident',
            'retry_policy': 'never',
            'retryable': false,
            'recovery_action': 'reconcile',
            'reload_required': true,
            'seal_session': false,
            'correlation_request_id': operationId,
          },
        };
      }, resultFields: {});
      await expectLater(
        gateway.perform({'kind': 'view_grant_review'}, (_) => true),
        throwsA(
          isA<AgentVaultException>()
              .having((failure) => failure.requestId, 'correlation', isNotNull)
              .having(
                (failure) => failure.metadata['kind'],
                'kind',
                'policy_denied',
              )
              .having(
                (failure) => failure.recoveryAction,
                'recovery',
                'reconcile',
              )
              .having((failure) => failure.reloadRequired, 'reload', true)
              .having((failure) => failure.sealSession, 'seal', false)
              .having((failure) => failure.affectedRefs, 'refs', [
                'source:exact',
              ])
              .having((failure) => failure.retryable, 'retryable', false),
        ),
      );
    },
  );
}
