import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/infrastructure/diagnostics/app_diagnostics.dart';
import 'package:floe_client/infrastructure/native/native_acquisition_service.dart';
import 'package:floe_client/infrastructure/native/platform_acquisition_services.dart';

void main() {
  group('PlatformAcquisitionServices', () {
    test('repeated start joins one registration pass', () async {
      final events = <String>[];
      final services = PlatformAcquisitionServices(_nativeServices(events));

      final firstStart = services.start();
      final repeatedStart = services.start();
      expect(identical(firstStart, repeatedStart), isTrue);
      await firstStart;

      expect(events, [
        'calendar.create',
        'calendar.start',
        'attention.create',
        'attention.start',
      ]);
      await services.close();
      expect(events, [
        'calendar.create',
        'calendar.start',
        'attention.create',
        'attention.start',
        'calendar.dispose',
        'attention.dispose',
      ]);
    });

    test('disposes a failed optional service and continues registration', () async {
      final events = <String>[];
      final services = PlatformAcquisitionServices([
        _NativeService(
          'calendar',
          events,
          startFailure: StateError('registration failed'),
        ),
        _NativeService('personal', events),
      ]);

      await services.start();

      expect(events, [
        'calendar.start',
        'calendar.dispose',
        'personal.start',
      ]);
      expect(
        AppDiagnostics.records.any(
          (record) => record.operation == 'calendar_startup',
        ),
        isTrue,
      );
      await services.close();
      expect(events.last, 'personal.dispose');
    });

    test('close during registration prevents creation of later services', () async {
      final events = <String>[];
      final registrationBarrier = Completer<void>();
      Iterable<NativeAcquisitionService> serviceSource() sync* {
        events.add('calendar.create');
        yield _NativeService(
          'calendar',
          events,
          startBarrier: registrationBarrier.future,
        );
        events.add('attention.create');
        yield _NativeService('attention', events);
      }

      final services = PlatformAcquisitionServices(serviceSource());

      final starting = services.start();
      expect(events, ['calendar.create', 'calendar.start']);

      final firstClose = services.close();
      final repeatedClose = services.close();
      expect(identical(firstClose, repeatedClose), isTrue);
      await firstClose;
      expect(events.last, 'calendar.dispose');

      registrationBarrier.complete();
      await starting;
      expect(events, [
        'calendar.create',
        'calendar.start',
        'calendar.dispose',
      ]);
    });

    test('start after close fails before creating native services', () async {
      final events = <String>[];
      final services = PlatformAcquisitionServices(_nativeServices(events));

      await services.close();

      await expectLater(services.start(), throwsStateError);
      expect(events, isEmpty);
    });

    test('attempts all disposals and exposes cleanup failure after the batch', () async {
      final events = <String>[];
      final disposalFailure = StateError('dispose failed');
      final services = PlatformAcquisitionServices([
        _NativeService(
          'calendar',
          events,
          disposeFailure: disposalFailure,
        ),
        _NativeService('attention', events),
        _NativeService('personal', events),
      ]);
      await services.start();

      final firstClose = services.close();
      final repeatedClose = services.close();
      expect(identical(firstClose, repeatedClose), isTrue);
      await expectLater(firstClose, throwsA(same(disposalFailure)));
      await expectLater(repeatedClose, throwsA(same(disposalFailure)));

      expect(
        events.where((event) => event.endsWith('.dispose')),
        ['calendar.dispose', 'attention.dispose', 'personal.dispose'],
      );
      expect(
        AppDiagnostics.records.any(
          (record) => record.operation == 'calendar_dispose',
        ),
        isTrue,
      );
    });
  });
}

Iterable<NativeAcquisitionService> _nativeServices(List<String> events) sync* {
  events.add('calendar.create');
  yield _NativeService('calendar', events);
  events.add('attention.create');
  yield _NativeService('attention', events);
}

final class _NativeService implements NativeAcquisitionService {
  _NativeService(
    this.diagnosticOperation,
    this.events, {
    this.startBarrier,
    this.startFailure,
    this.disposeFailure,
  });

  @override
  final String diagnosticOperation;
  final List<String> events;
  final Future<void>? startBarrier;
  final Object? startFailure;
  final Object? disposeFailure;

  @override
  Future<void> start() async {
    events.add('$diagnosticOperation.start');
    final barrier = startBarrier;
    if (barrier != null) await barrier;
    if (startFailure case final error?) throw error;
  }

  @override
  Future<void> dispose() async {
    events.add('$diagnosticOperation.dispose');
    if (disposeFailure case final error?) throw error;
  }
}
