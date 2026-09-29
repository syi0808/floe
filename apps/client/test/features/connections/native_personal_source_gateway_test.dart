import 'package:floe_client/features/connections/application/native_personal_source_gateway.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../support/app_wire_transport.dart';

const deviceId = 'mac-local';

Map<String, Object?> source(List<String> handles, int revision) => {
  'connector_id': 'contacts.apple',
  'connection_id': 'contacts.apple.local',
  'execution_owner_id': 'apple:$deviceId',
  'state': 'ready',
  'revision': revision,
  'source_authority': {
    'incarnation': '00000000-0000-4000-8000-000000000010',
    'epoch': revision,
  },
  'resource_mode': 'selected',
  'resources': [
    for (final handle in handles) {'handle': handle, 'label': handle},
  ],
  'native_subject_fingerprint': 'a' * 64,
};

void main() {
  test('Contacts edit sends only Connections setup with source CAS', () async {
    final calls = <Map<String, Object?>>[];
    final gateway = AppWireNativePersonalSourceGateway(
      CallbackAppWireTransport((request) async {
        final command = Map<String, Object?>.from(request['command'] as Map);
        calls.add(command);
        return {
          'kind': 'native_personal_source',
          'command_id': request['command_id'],
          'source': source(['A', 'B'], 2),
        };
      }),
      deviceId: deviceId,
    );
    final updated = await gateway.setup(
      connectorId: 'contacts.apple',
      expectedRevision: 1,
      selectedHandles: ['B', 'A'],
    );
    expect(updated.resources.map((resource) => resource.handle), ['A', 'B']);
    expect(calls.single['kind'], 'connections.native_personal.setup');
    final setup = calls.single['setup'] as Map;
    expect(setup['expected_revision'], 1);
    expect(setup['selected_handles'], ['A', 'B']);
    expect(setup.containsKey('grant_id'), isFalse);
    expect(setup.containsKey('expected_grant_authority'), isFalse);
  });

  test('source decoder rejects foreign owner and absent subject', () async {
    final gateway = AppWireNativePersonalSourceGateway(
      CallbackAppWireTransport(
        (request) async => {
          'kind': 'native_personal_source',
          'source': {
            ...source(['A'], 1),
            'execution_owner_id': 'other-device',
          },
        },
      ),
      deviceId: deviceId,
    );
    await expectLater(gateway.inspect('contacts.apple'), throwsFormatException);
  });
}
