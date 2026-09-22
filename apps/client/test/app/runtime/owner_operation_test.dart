import 'package:floe_client/app/runtime/agent_vault_gateway.dart';
import 'package:floe_client/app/runtime/native_transport.dart';
import 'package:floe_client/app/runtime/owner_operation.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('observer timeout keeps the same operation without cancellation or redispatch', () async {
    final observer = OwnerOperationObserver(timeout: Duration.zero);
    String? operationId;
    var starts = 0;
    var done = false;
    var releases = 0;
    Future<String> observe() => observer.observe(
      scope: 'person',
      intent: 'exact-action',
      stage: 'calendar_action',
      resultKind: 'action_operation',
      start: (identifier) async {
        operationId = identifier;
        starts++;
        return {
          'kind': 'action_operation',
          'operation_id': identifier,
          'done': false,
        };
      },
      read: (identifier, release) async {
        expect(identifier, operationId);
        if (release) releases++;
        return {
          'kind': 'action_operation',
          'operation_id': identifier,
          'done': done,
          'value': 'uncertain-write',
        };
      },
      decode: (result) => result['value'] as String,
    );
    await expectLater(
      observe(),
      throwsA(
        isA<AgentVaultException>().having(
          (failure) => failure.failure,
          'reason',
          'deadline_exceeded',
        ),
      ),
    );
    expect(starts, 1);
    expect(releases, 0);
    done = true;
    expect(await observe(), 'uncertain-write');
    expect(starts, 1);
    expect(releases, 1);
  });

  test('decoded result survives a lost release acknowledgement and is never resubmitted', () async {
    final observer = OwnerOperationObserver();
    var starts = 0;
    var releases = 0;
    var decodes = 0;
    Future<String> observe() => observer.observe(
      scope: 'person',
      intent: 'exact-action',
      stage: 'calendar_action',
      resultKind: 'action_operation',
      start: (identifier) async {
        starts++;
        return {
          'kind': 'action_operation',
          'operation_id': identifier,
          'done': true,
        };
      },
      read: (identifier, release) async {
        expect(release, isTrue);
        releases++;
        if (releases == 1) throw StateError('lost release acknowledgement');
        throw const NativeTransportException('not_found', 'released');
      },
      decode: (result) {
        decodes++;
        return 'uncertain-write';
      },
    );
    await expectLater(observe(), throwsStateError);
    expect(await observe(), 'uncertain-write');
    expect(starts, 1);
    expect(decodes, 1);
    expect(releases, 2);
  });
}
