import 'package:floe_client/features/connections/application/native_personal_source_gateway.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../support/app_wire_transport.dart';

void main() {
  test(
    'Contacts source selection rejects invalid resource handles before I/O',
    () async {
      final gateway = AppWireNativePersonalSourceGateway(
        CallbackAppWireTransport(
          (_) async => throw StateError('unexpected I/O'),
        ),
        deviceId: 'device',
      );
      for (final handles in <List<String>>[
        [],
        ['same', 'same'],
        ['*'],
        [' leading'],
        ['bad\u0001handle'],
        [List<String>.filled(129, 'é').join()],
        ['00000000-0000-0000-0000-000000000000'],
        List<String>.generate(65, (index) => 'contact:$index'),
      ]) {
        await expectLater(
          gateway.setup(
            connectorId: 'contacts.apple',
            expectedRevision: null,
            selectedHandles: handles,
          ),
          throwsFormatException,
        );
      }
    },
  );
}
