import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/infrastructure/native/native_context_host_transport.dart';

const _person = 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa';
String _id(int n) => 'bbbbbbbb-bbbb-4bbb-8bbb-${n.toString().padLeft(12, '0')}';

void main() {
  for (final kind in ['calendar', 'attention', 'personal']) {
    test(
      '$kind callbacks use broker identity, not payload replay storage',
      () async {
        final wire = _Wire(kind);
        final host = AppWireNativeContextHostTransport(wire);
        final registration = await _register(host, kind);
        wire.jobs = [_job(registration, 1)];
        final job = (await _poll(host, kind, registration)).single;
        final result = {...job, 'raw_data': 'private-data-' * 100000};
        wire.failCommands = true;
        await expectLater(
          _complete(host, kind, registration, result),
          throwsStateError,
        );
        final firstId = wire.commandIds.last;
        await expectLater(
          _complete(host, kind, registration, result),
          throwsStateError,
        );
        expect(wire.commandIds.last, isNot(firstId));
        expect(wire.resultIds, [_id(1), _id(1)]);
      },
    );

    test(
      '$kind new owner admission retires old callback identity after failures',
      () async {
        final wire = _Wire(kind);
        final host = AppWireNativeContextHostTransport(wire);
        final registration = await _register(host, kind);
        Map<String, dynamic>? previous;
        for (var index = 1; index <= 100; index++) {
          wire.jobs = [_job(registration, index)];
          final current = (await _poll(host, kind, registration)).single;
          if (previous != null) {
            final calls = wire.commandIds.length;
            await expectLater(
              _complete(host, kind, registration, previous),
              throwsFormatException,
            );
            expect(wire.commandIds, hasLength(calls));
          }
          wire.failCommands = true;
          await expectLater(
            _complete(host, kind, registration, current),
            throwsStateError,
          );
          previous = current;
        }
      },
    );

    test(
      '$kind empty poll does not prove a pending callback settled',
      () async {
        final wire = _Wire(kind);
        final host = AppWireNativeContextHostTransport(wire);
        final registration = await _register(host, kind);
        wire.jobs = [_job(registration, 1)];
        final job = (await _poll(host, kind, registration)).single;
        wire.failCommands = true;
        await expectLater(
          _complete(host, kind, registration, job),
          throwsStateError,
        );
        wire.jobs = [];
        expect(await _poll(host, kind, registration), isEmpty);
        wire.failCommands = false;
        await _complete(host, kind, registration, job);
      },
    );

    test(
      '$kind malformed multi-job poll cannot partially admit identities',
      () async {
        final wire = _Wire(kind);
        final host = AppWireNativeContextHostTransport(wire);
        final registration = await _register(host, kind);
        wire.jobs = [_job(registration, 1), _job(registration, 2)];
        await expectLater(
          _poll(host, kind, registration),
          throwsFormatException,
        );
        await expectLater(
          _complete(host, kind, registration, wire.jobs.first),
          throwsFormatException,
        );
      },
    );

    test(
      '$kind registration replacement retires old request bindings',
      () async {
        final wire = _Wire(kind);
        final host = AppWireNativeContextHostTransport(wire);
        final old = await _register(host, kind);
        wire.jobs = [_job(old, 1)];
        final job = (await _poll(host, kind, old)).single;
        await _register(host, kind);
        await expectLater(_complete(host, kind, old, job), throwsStateError);
        await expectLater(_poll(host, kind, old), throwsStateError);
      },
    );

    test('$kind failure acknowledgement releases its binding', () async {
      final wire = _Wire(kind);
      final host = AppWireNativeContextHostTransport(wire);
      final reg = await _register(host, kind);
      wire.jobs = [_job(reg, 1)];
      final job = (await _poll(host, kind, reg)).single;
      wire.failCommands = true;
      await expectLater(_complete(host, kind, reg, job), throwsStateError);
      await expectLater(_fail(host, kind, reg, _id(1)), throwsStateError);
      wire.failCommands = false;
      await _fail(host, kind, reg, _id(1));
      await expectLater(_complete(host, kind, reg, job), throwsFormatException);
    });

    test('$kind disposal acknowledgement retires the registration', () async {
      final wire = _Wire(kind);
      final host = AppWireNativeContextHostTransport(wire);
      final reg = await _register(host, kind);
      wire.jobs = [_job(reg, 1)];
      final job = (await _poll(host, kind, reg)).single;
      wire.failCommands = true;
      await expectLater(_dispose(host, kind, reg), throwsStateError);
      wire.failCommands = false;
      await _complete(host, kind, reg, job);
      await _dispose(host, kind, reg);
      await expectLater(_poll(host, kind, reg), throwsStateError);
    });

    test(
      '$kind late old poll cannot admit after registration replacement',
      () async {
        final wire = _Wire(kind);
        final host = AppWireNativeContextHostTransport(wire);
        final old = await _register(host, kind);
        final barrier = Completer<Map<String, dynamic>>();
        wire.nextQuery = barrier.future;
        final polling = _poll(host, kind, old);
        final rejected = expectLater(polling, throwsStateError);
        final reg = await _register(host, kind);
        wire.jobs = [_job(reg, 2)];
        final current = (await _poll(host, kind, reg)).single;
        barrier.complete({
          'kind': '${kind}_acquisitions',
          'acquisitions': [_job(old, 1)],
        });
        await rejected;
        await _complete(host, kind, reg, current);
      },
    );

    test('$kind concurrent registration joins one owner replacement', () async {
      final wire = _Wire(kind);
      final host = AppWireNativeContextHostTransport(wire);
      final results = await Future.wait([
        _register(host, kind),
        _register(host, kind),
      ]);
      expect(wire.generation, 1);
      expect(identical(results[0], results[1]), isTrue);
    });

    test(
      '$kind malformed acknowledgement preserves callback identity',
      () async {
        final wire = _Wire(kind);
        final host = AppWireNativeContextHostTransport(wire);
        final reg = await _register(host, kind);
        wire.jobs = [_job(reg, 1)];
        final job = (await _poll(host, kind, reg)).single;
        wire.nextCommand = Future.value({
          'kind': 'acknowledged',
          'extra': true,
        });
        await expectLater(
          _complete(host, kind, reg, job),
          throwsFormatException,
        );
        await _complete(host, kind, reg, job);
        await expectLater(
          _complete(host, kind, reg, job),
          throwsFormatException,
        );
      },
    );

    test('$kind late disposal cannot retire a replacement host', () async {
      final wire = _Wire(kind);
      final host = AppWireNativeContextHostTransport(wire);
      final old = await _register(host, kind);
      final barrier = Completer<Map<String, dynamic>>();
      wire.nextCommand = barrier.future;
      final disposing = _dispose(host, kind, old);
      final reg = await _register(host, kind);
      wire.jobs = [_job(reg, 1)];
      final job = (await _poll(host, kind, reg)).single;
      barrier.complete({'kind': 'acknowledged'});
      await disposing;
      await _complete(host, kind, reg, job);
    });

    test('$kind foreign identities fail before the wire boundary', () async {
      final wire = _Wire(kind);
      final host = AppWireNativeContextHostTransport(wire);
      final reg = await _register(host, kind);
      wire.jobs = [_job(reg, 1)];
      final job = (await _poll(host, kind, reg)).single;
      for (final field in [
        'request_id',
        'person_id',
        'device_id',
        'host_epoch',
      ]) {
        await expectLater(
          _complete(host, kind, reg, {...job, field: 'foreign'}),
          throwsFormatException,
        );
      }
      await expectLater(_fail(host, kind, reg, _id(2)), throwsFormatException);
      expect(wire.commandIds, isEmpty);
      await _complete(host, kind, reg, job);
    });

    test(
      '$kind late completion acknowledgement cannot remove a newer binding',
      () async {
        final wire = _Wire(kind);
        final host = AppWireNativeContextHostTransport(wire);
        final registration = await _register(host, kind);
        wire.jobs = [_job(registration, 1)];
        final first = (await _poll(host, kind, registration)).single;
        final barrier = Completer<Map<String, dynamic>>();
        wire.nextCommand = barrier.future;
        final completing = _complete(host, kind, registration, first);
        wire.jobs = [_job(registration, 2)];
        final second = (await _poll(host, kind, registration)).single;
        barrier.complete({'kind': 'acknowledged'});
        await completing;
        await _complete(host, kind, registration, second);
      },
    );
  }
}

Map<String, dynamic> _job(NativeHostRegistration registration, int n) => {
  'request_id': _id(n),
  'person_id': _person,
  'device_id': 'test-device',
  'host_epoch': registration.hostEpoch,
};
Future<NativeHostRegistration> _register(
  NativeContextHostTransport host,
  String kind,
) => switch (kind) {
  'calendar' => host.registerAcquisitionHost(),
  'attention' => host.registerAttentionHost(),
  _ => host.registerPersonalHost(),
};
Future<List<Map<String, dynamic>>> _poll(
  NativeContextHostTransport host,
  String kind,
  NativeHostRegistration reg,
) => switch (kind) {
  'calendar' => host.pollAcquisitions(registration: reg),
  'attention' => host.pollAttentionAcquisitions(registration: reg),
  _ => host.pollPersonalAcquisitions(registration: reg),
};
Future<void> _complete(
  NativeContextHostTransport host,
  String kind,
  NativeHostRegistration reg,
  Map<String, dynamic> result,
) => switch (kind) {
  'calendar' => host.completeAcquisition(registration: reg, result: result),
  'attention' => host.completeAttentionAcquisition(
    registration: reg,
    result: result,
  ),
  _ => host.completePersonalAcquisition(registration: reg, result: result),
};

Future<void> _fail(
  NativeContextHostTransport host,
  String kind,
  NativeHostRegistration reg,
  String requestId,
) => switch (kind) {
  'calendar' => host.failAcquisition(
    registration: reg,
    requestId: requestId,
    failure: 'provider_unavailable',
  ),
  'attention' => host.failAttentionAcquisition(
    registration: reg,
    requestId: requestId,
    failure: 'provider_unavailable',
  ),
  _ => host.failPersonalAcquisition(
    registration: reg,
    requestId: requestId,
    failure: 'provider_unavailable',
  ),
};
Future<void> _dispose(
  NativeContextHostTransport host,
  String kind,
  NativeHostRegistration reg,
) => switch (kind) {
  'calendar' => host.disposeAcquisitionHost(registration: reg),
  'attention' => host.disposeAttentionHost(registration: reg),
  _ => host.disposePersonalHost(registration: reg),
};

class _Wire implements NativeHostWireTransport {
  _Wire(this.kind);
  final String kind;
  int generation = 0;
  List<Map<String, dynamic>> jobs = [];
  bool failCommands = false;
  final commandIds = <String>[];
  final resultIds = <String>[];
  Future<Map<String, dynamic>>? nextCommand;
  Future<Map<String, dynamic>>? nextQuery;
  @override
  Future<Map<String, dynamic>> commandV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) async {
    final command = request['command'] as Map;
    if (command['kind'] == 'native_host.$kind.register') {
      generation++;
      return {
        'kind': 'registered',
        'registration': {
          'registration_id': _id(1000 + generation),
          'host_epoch': 'epoch-$kind-$generation',
          'runtime_epoch': 1,
        },
      };
    }
    commandIds.add(request['command_id'] as String);
    if (command['result'] case final Map result) {
      resultIds.add(result['request_id'] as String);
    }
    final pending = nextCommand;
    nextCommand = null;
    if (pending != null) return pending;
    if (failCommands) throw StateError('Lost callback acknowledgement');
    return {'kind': 'acknowledged'};
  }

  @override
  Future<Map<String, dynamic>> queryV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) async {
    final pending = nextQuery;
    nextQuery = null;
    if (pending != null) return pending;
    return {'kind': '${kind}_acquisitions', 'acquisitions': jobs};
  }
}
