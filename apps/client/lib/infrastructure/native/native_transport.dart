import 'dart:convert';
import 'dart:ffi';
import 'dart:io';
import 'dart:isolate';

import 'package:ffi/ffi.dart';
import 'package:path_provider/path_provider.dart';

import '../../features/day_canvas/infrastructure/floe_native_bindings.dart';

const nativeProtocolVersion = 1;

final class NativeTransportException implements Exception {
  const NativeTransportException(this.code, this.message);

  final String code;
  final String message;

  @override
  String toString() => message;
}

abstract interface class LocalContextTransport {
  Future<void> publishLocalContext({
    required String personId,
    required String deviceId,
    required Map<String, dynamic> view,
  });

  Future<int> revokeLocalContext({
    required String personId,
    required String deviceId,
    String? viewId,
  });
}

final class NativeTransport implements LocalContextTransport {
  NativeTransport._(this._isolate, this._commands) {
    _finalizer.attach(this, _commands, detach: this);
  }

  static final Finalizer<SendPort> _finalizer = Finalizer(
    (commands) => commands.send(const {'operation': 'close'}),
  );

  final Isolate _isolate;
  final SendPort _commands;
  bool _closed = false;

  static Future<NativeTransport> openDefault({required String personId}) async {
    final supportDirectory = await getApplicationSupportDirectory();
    final databaseDirectory = Directory(
      '${supportDirectory.path}/people/$personId',
    );
    await databaseDirectory.create(recursive: true);
    return open(
      libraryPath: resolveLibraryPath(),
      databasePath: '${databaseDirectory.path}/floe.db',
    );
  }

  static Future<NativeTransport> open({
    required String libraryPath,
    required String databasePath,
  }) async {
    final ready = ReceivePort();
    final isolate = await Isolate.spawn(_nativeWorkerMain, {
      'ready': ready.sendPort,
      'library_path': libraryPath,
      'database_path': databasePath,
    });
    final result = _asMap(await ready.first);
    ready.close();
    if (result['status'] != 'ok') {
      isolate.kill(priority: Isolate.immediate);
      throw _exceptionFromEnvelope(_asMap(result['error']));
    }
    return NativeTransport._(isolate, result['commands']! as SendPort);
  }

  static String resolveLibraryPath() {
    final override = Platform.environment['FLOE_CORE_LIBRARY_PATH'];
    if (override != null && override.isNotEmpty) return override;
    if (Platform.isIOS) return '';
    if (Platform.isAndroid) return 'libfloe_ffi.so';
    if (Platform.isMacOS) {
      final executableDirectory = File(Platform.resolvedExecutable).parent.path;
      return '$executableDirectory/../Frameworks/libfloe_ffi.dylib';
    }
    throw UnsupportedError('The native Floe transport is unavailable.');
  }

  Future<Map<String, dynamic>> request(
    String operation,
    Map<String, dynamic> request,
  ) async {
    if (_closed) throw StateError('NativeTransport is already closed.');
    final reply = ReceivePort();
    _commands.send({
      'operation': operation,
      'request': jsonEncode(request),
      'reply': reply.sendPort,
    });
    final result = _asMap(await reply.first);
    reply.close();
    if (result['status'] != 'ok') {
      throw NativeTransportException(
        'ffi',
        result['message']?.toString() ?? 'The Rust core request failed.',
      );
    }
    return _unwrapEnvelope(result['response']! as String);
  }

  @override
  Future<void> publishLocalContext({
    required String personId,
    required String deviceId,
    required Map<String, dynamic> view,
  }) async {
    await request('local_context', {
      'schema_version': nativeProtocolVersion,
      'person_id': personId,
      'operation': {'kind': 'publish', 'device_id': deviceId, 'view': view},
    });
  }

  Future<Map<String, dynamic>> readLocalContext({
    required String personId,
    required String viewId,
    String? deviceId,
  }) async {
    final result = await request('local_context', {
      'schema_version': nativeProtocolVersion,
      'person_id': personId,
      'operation': {'kind': 'read', 'view_id': viewId, 'device_id': ?deviceId},
    });
    return _asMap(result['view']);
  }

  @override
  Future<int> revokeLocalContext({
    required String personId,
    required String deviceId,
    String? viewId,
  }) async {
    final result = await request('local_context', {
      'schema_version': nativeProtocolVersion,
      'person_id': personId,
      'operation': {
        'kind': 'revoke',
        'device_id': deviceId,
        'view_id': ?viewId,
      },
    });
    return result['removed_count']! as int;
  }

  Future<void> close() async {
    if (_closed) return;
    _closed = true;
    _finalizer.detach(this);
    final reply = ReceivePort();
    _commands.send({'operation': 'close', 'reply': reply.sendPort});
    await reply.first;
    reply.close();
    _isolate.kill(priority: Isolate.immediate);
  }
}

Map<String, dynamic> _unwrapEnvelope(String source) {
  final envelope = _asMap(jsonDecode(source));
  if (envelope['schema_version'] != nativeProtocolVersion) {
    throw const NativeTransportException(
      'unsupported_version',
      'Unsupported Rust protocol version.',
    );
  }
  if (envelope['status'] == 'error') {
    throw _exceptionFromEnvelope(envelope);
  }
  if (envelope['status'] != 'ok') {
    throw const FormatException('Unknown Rust response status.');
  }
  return _asMap(envelope['data']);
}

NativeTransportException _exceptionFromEnvelope(Map<String, dynamic> envelope) {
  final error = envelope['error'] is Map ? _asMap(envelope['error']) : envelope;
  return NativeTransportException(
    error['code']?.toString() ?? 'internal',
    error['message']?.toString() ?? 'Could not open Rust core.',
  );
}

Map<String, dynamic> _asMap(Object? value) =>
    Map<String, dynamic>.from(value! as Map);

Future<void> _nativeWorkerMain(Map<String, Object?> configuration) async {
  final ready = configuration['ready']! as SendPort;
  FloeNativeBindings? bindings;
  Pointer<Void> handle = nullptr;
  try {
    bindings = FloeNativeBindings(configuration['library_path']! as String);
    if (bindings.protocolVersion() != nativeProtocolVersion) {
      throw StateError('Rust protocol version does not match Flutter.');
    }
    final path = (configuration['database_path']! as String).toNativeUtf8();
    final error = calloc<Pointer<Utf8>>();
    try {
      handle = bindings.open(path, error);
      if (handle == nullptr) {
        final pointer = error.value;
        final source = pointer == nullptr ? null : pointer.toDartString();
        if (pointer != nullptr) bindings.freeString(pointer);
        throw source == null
            ? StateError('Could not open Rust core.')
            : _exceptionFromEnvelope(_asMap(jsonDecode(source)));
      }
    } finally {
      calloc.free(error);
      calloc.free(path);
    }
  } on Object catch (error) {
    ready.send({
      'status': 'error',
      'error': {'code': 'ffi_open', 'message': error.toString()},
    });
    return;
  }

  final commands = ReceivePort();
  ready.send({'status': 'ok', 'commands': commands.sendPort});
  await for (final raw in commands) {
    final message = _asMap(raw);
    final operation = message['operation'];
    if (operation == 'close') {
      bindings.freeCore(handle);
      (message['reply'] as SendPort?)?.send(true);
      commands.close();
      return;
    }
    final reply = message['reply']! as SendPort;
    try {
      final input = (message['request']! as String).toNativeUtf8();
      Pointer<Utf8> output = nullptr;
      try {
        output = switch (operation) {
          'load_day' => bindings.loadDay(handle, input),
          'execute' => bindings.execute(handle, input),
          'calendar_actions' => bindings.calendarActions(handle, input),
          'agent_fixture' => bindings.agentFixture(handle, input),
          'agent_fixture_run' => bindings.agentFixtureRun(handle, input),
          'agent_vault' => bindings.agentVault(handle, input),
          'local_context' => bindings.localContext(handle, input),
          _ => throw StateError('Unknown core operation: $operation'),
        };
        if (output == nullptr) {
          throw StateError('Rust core returned an empty response.');
        }
        reply.send({'status': 'ok', 'response': output.toDartString()});
      } finally {
        if (output != nullptr) bindings.freeString(output);
        calloc.free(input);
      }
    } on Object catch (error) {
      reply.send({'status': 'error', 'message': error.toString()});
    }
  }
}
