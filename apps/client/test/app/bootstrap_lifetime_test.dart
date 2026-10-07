import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/infrastructure/diagnostics/app_diagnostics.dart';
import 'package:floe_client/infrastructure/native/native_acquisition_service.dart';
import 'package:floe_client/infrastructure/native/platform_acquisition_services.dart';

void main() {
  group('PlatformAcquisitionServices', () {
    test('starts callback services before runtime preparation', () async {
      final events = <String>[];
      final services = PlatformAcquisitionServices([
        _NativeService('calendar', events),
        _NativeService('attention', events),
      ]);

      await services.startBefore(() async => events.add('runtime.prepare'));

      expect(events, [
        'calendar.start',
        'attention.start',
        'runtime.prepare',
      ]);
    });

    test('disposes a failed optional service and continues startup', () async {
      final events = <String>[];
      final failed = _NativeService(
        'calendar',
        events,
        startFailure: StateError('registration failed'),
      );
      final available = _NativeService('personal', events);
      final services = PlatformAcquisitionServices([failed, available]);

      await services.startBefore(() async => events.add('runtime.prepare'));

      expect(events, [
        'calendar.start',
        'calendar.dispose',
        'personal.start',
        'runtime.prepare',
      ]);
      expect(
        AppDiagnostics.records.any(
          (record) => record.operation == 'calendar_startup',
        ),
        isTrue,
      );
      await services.close();
      expect(events, hasLength(5));
      expect(events.last, 'personal.dispose');
    });

    test(
      'preparation failure preserves its error when cleanup also fails',
      () async {
        final events = <String>[];
        final services = PlatformAcquisitionServices([
          _NativeService(
            'calendar',
            events,
            disposeFailure: StateError('dispose failed'),
          ),
          _NativeService('attention', events),
        ]);
        final startupError = StateError('runtime preparation failed');

        await expectLater(
          services.startBefore(() async => throw startupError),
          throwsA(same(startupError)),
        );
        await services.close();

        expect(events, [
          'calendar.start',
          'attention.start',
          'calendar.dispose',
          'attention.dispose',
        ]);
        expect(
          AppDiagnostics.records.any(
            (record) => record.operation == 'calendar_dispose',
          ),
          isTrue,
        );
      },
    );

    test(
      'attempts every disposal and repeated close joins the same future',
      () async {
        final events = <String>[];
        final services = PlatformAcquisitionServices([
          _NativeService(
            'calendar',
            events,
            disposeFailure: StateError('dispose failed'),
          ),
          _NativeService('attention', events),
          _NativeService('personal', events),
        ]);

        final firstClose = services.close();
        final repeatedClose = services.close();
        expect(identical(firstClose, repeatedClose), isTrue);
        await firstClose;
        await services.close();

        expect(events, [
          'calendar.dispose',
          'attention.dispose',
          'personal.dispose',
        ]);
        expect(
          AppDiagnostics.records.any(
            (record) => record.operation == 'calendar_dispose',
          ),
          isTrue,
        );
      },
    );
  });
}

final class _NativeService implements NativeAcquisitionService {
  _NativeService(
    this.diagnosticOperation,
    this.events, {
    this.startFailure,
    this.disposeFailure,
  });

  @override
  final String diagnosticOperation;
  final List<String> events;
  final Object? startFailure;
  final Object? disposeFailure;

  @override
  Future<void> start() async {
    events.add('$diagnosticOperation.start');
    if (startFailure case final error?) throw error;
  }

  @override
  Future<void> dispose() async {
    events.add('$diagnosticOperation.dispose');
    if (disposeFailure case final error?) throw error;
  }
}
