import 'package:floe_client/app/runtime/local_owner_gateways.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../support/app_wire_transport.dart';

void main() {
  test(
    'Contacts source selection rejects invalid resource handles before I/O',
    () async {
      final gateway = NativePersonalAccessGateway(
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
          gateway.inspectPersonalContacts('person', handles),
          throwsFormatException,
        );
      }
    },
  );
}
