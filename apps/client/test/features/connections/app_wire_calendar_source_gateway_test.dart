import 'package:floe_client/features/connections/application/app_wire_calendar_source_gateway.dart';
import 'package:floe_client/features/connections/domain/calendar_source_connection.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../support/app_wire_transport.dart';

const _person = '00000000-0000-4000-8000-000000000001';
const _device = 'mac-local';

Map<String, Object?> _source({int revision = 1}) => {
  'connector_id': 'calendar.event_kit',
  'connection_id': 'calendar-source',
  'execution_owner_id': _device,
  'state': 'ready',
  'revision': revision,
  'source_authority': {
    'incarnation': '00000000-0000-4000-8000-000000000010',
    'epoch': 1,
  },
  'resource_mode': 'selected',
  'resources': [
    {'handle': 'home', 'label': 'Home'},
  ],
  'native_subject_fingerprint': 'a' * 64,
};

Map<String, Object?> _remoteSource({int revision = 1}) => {
  ..._source(revision: revision),
  'connector_id': 'calendar.google',
  'connection_id': 'server-connection',
  'native_subject_fingerprint': null,
};

void main() {
  test('native source query decodes Connections authority', () async {
    final transport = CallbackAppWireTransport((request) async {
      expect(request['query'], {'kind': 'connections.native_calendar.source'});
      return {'kind': 'native_calendar_source', 'source': _source()};
    });
    final gateway = AppWireCalendarSourceGateway(transport, deviceId: _device);
    final source = await gateway.inspectNative(_person);
    expect(source?.connectionId, 'calendar-source');
    expect(source?.selectedCalendarIds, ['home']);
    expect(source?.sourceAuthority.epoch, 1);
  });

  test(
    'configure submits expected revision, not a caller-computed next',
    () async {
      final transport = CallbackAppWireTransport((request) async {
        final mutation = (request['command'] as Map)['mutation'] as Map;
        expect(mutation['type'], 'configure');
        expect(mutation['connection_id'], 'calendar-source');
        expect(mutation['expected_revision'], 7);
        expect(mutation.containsKey('connection_revision'), isFalse);
        expect(mutation.containsKey('source_authority'), isFalse);
        expect(mutation['resources'], [
          {'handle': 'work', 'label': 'Work'},
        ]);
        return {
          'kind': 'native_calendar_source',
          'command_id': request['command_id'],
          'source': _source(revision: 8),
        };
      });
      final gateway = AppWireCalendarSourceGateway(
        transport,
        deviceId: _device,
      );
      final current = CalendarSourceConnection.fromJson(
        Map<String, dynamic>.from(_source(revision: 7)),
      );
      final updated = await gateway.configureNative(
        _person,
        current: current,
        resourceMode: 'selected',
        resources: const [
          CalendarSourceResource(handle: 'work', label: 'Work'),
        ],
      );
      expect(updated.revision, 8);
    },
  );

  test('foreign execution owner cannot be configured', () async {
    final gateway = AppWireCalendarSourceGateway(
      CallbackAppWireTransport((_) async => throw StateError('unexpected I/O')),
      deviceId: _device,
    );
    final current = CalendarSourceConnection.fromJson({
      ..._source(),
      'execution_owner_id': 'foreign-device',
    });
    expect(
      () => gateway.disconnectNative(_person, current: current),
      throwsFormatException,
    );
  });

  test(
    'remote binding sends local expected revision without producer revision',
    () async {
      final transport = CallbackAppWireTransport((request) async {
        final command = request['command'] as Map;
        expect(command['kind'], 'connections.remote_calendar.mutate');
        final mutation = command['mutation'] as Map;
        expect(mutation['type'], 'bind');
        expect(mutation['connector_id'], 'calendar.google');
        expect(mutation['connection_id'], 'server-connection');
        expect(mutation['expected_revision'], 4);
        expect(mutation.containsKey('connection_revision'), isFalse);
        expect(mutation.containsKey('source_authority'), isFalse);
        return {
          'kind': 'remote_calendar_source',
          'command_id': request['command_id'],
          'source': _remoteSource(revision: 5),
        };
      });
      final gateway = AppWireCalendarSourceGateway(
        transport,
        deviceId: _device,
      );
      final current = CalendarSourceConnection.fromJson(
        Map<String, dynamic>.from(_remoteSource(revision: 4)),
      );
      final updated = await gateway.bindRemote(
        _person,
        connectorId: 'calendar.google',
        connectionId: 'server-connection',
        current: current,
        resources: const [
          CalendarSourceResource(handle: 'home', label: 'Home'),
        ],
      );
      expect(updated.revision, 5);
    },
  );

  test('remote source query rejects a foreign execution owner', () async {
    final transport = CallbackAppWireTransport(
      (request) async => {
        'kind': 'remote_calendar_sources',
        'sources': [
          {..._remoteSource(), 'execution_owner_id': 'foreign-device'},
        ],
      },
    );
    final gateway = AppWireCalendarSourceGateway(transport, deviceId: _device);
    await expectLater(gateway.inspectRemote(_person), throwsFormatException);
  });

  test('source decoder rejects malformed resource and native subject', () {
    for (final malformed in [
      {
        ..._source(),
        'resources': [42],
      },
      {
        ..._source(),
        'resources': [
          {'handle': 'work', 'label': 'Work'},
          {'handle': 'home', 'label': 'Home'},
        ],
      },
      {..._source(), 'native_subject_fingerprint': null},
    ]) {
      expect(
        () => CalendarSourceConnection.fromJson(malformed),
        throwsFormatException,
      );
    }
  });
}
