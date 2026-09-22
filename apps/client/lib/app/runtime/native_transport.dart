import 'dart:convert';
import 'dart:ffi';
import 'dart:io';
import 'dart:isolate';

import 'package:ffi/ffi.dart';
import 'package:path_provider/path_provider.dart';

import 'package:floe_client/features/day/infrastructure/floe_native_bindings.dart';
import 'package:floe_client/app/runtime/app_wire_transport.dart';

const nativeProtocolVersion = 1;

final class NativeTransportException implements Exception {
  const NativeTransportException(
    this.code,
    this.message, {
    this.field,
    this.metadata = const {},
  });

  final String code;
  final String message;
  final String? field;
  final Map<String, String> metadata;

  factory NativeTransportException.fromEnvelope(Map<String, dynamic> envelope) {
    final error = envelope['error'] is Map
        ? _asMap(envelope['error'])
        : envelope;
    return NativeTransportException(
      error['code']?.toString() ?? 'internal',
      error['message']?.toString() ?? 'Could not open Rust core.',
      field: error['field']?.toString(),
      metadata: error['metadata'] is Map
          ? Map<String, String>.unmodifiable(
              (error['metadata'] as Map).map(
                (key, value) => MapEntry(key.toString(), value.toString()),
              ),
            )
          : const {},
    );
  }

  @override
  String toString() => message;
}

abstract interface class LocalContextTransport {
  Future<void> registerAcquisitionHost({
    required String personId,
    required String hostEpoch,
  });

  Future<List<Map<String, dynamic>>> pollAcquisitions({
    required String personId,
    required String hostEpoch,
  });

  Future<void> completeAcquisition({
    required String personId,
    required String hostEpoch,
    required Map<String, dynamic> result,
  });

  Future<void> failAcquisition({
    required String personId,
    required String hostEpoch,
    required String requestId,
    required String failure,
  });

  Future<void> disposeAcquisitionHost({
    required String personId,
    required String hostEpoch,
  });

  Future<void> registerAttentionHost({
    required String personId,
    required String hostEpoch,
  });

  Future<List<Map<String, dynamic>>> pollAttentionAcquisitions({
    required String personId,
    required String hostEpoch,
  });

  Future<void> completeAttentionAcquisition({
    required String personId,
    required String hostEpoch,
    required Map<String, dynamic> result,
  });

  Future<void> failAttentionAcquisition({
    required String personId,
    required String hostEpoch,
    required String requestId,
    required String failure,
  });

  Future<void> disposeAttentionHost({
    required String personId,
    required String hostEpoch,
  });

  Future<void> registerPersonalHost({
    required String personId,
    required String hostEpoch,
  });

  Future<List<Map<String, dynamic>>> pollPersonalAcquisitions({
    required String personId,
    required String hostEpoch,
  });

  Future<void> completePersonalAcquisition({
    required String personId,
    required String hostEpoch,
    required Map<String, dynamic> result,
  });

  Future<void> failPersonalAcquisition({
    required String personId,
    required String hostEpoch,
    required String requestId,
    required String failure,
  });

  Future<void> disposePersonalHost({
    required String personId,
    required String hostEpoch,
  });

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

  Future<void> publishCalendarObservation({
    required String personId,
    required String deviceId,
    required String connectionId,
    required int connectionRevision,
    required String provider,
    required List<String> calendarIds,
    required DateTime observedAt,
    required DateTime expiresAt,
    required DateTime rangeStart,
    required DateTime rangeEnd,
    required List<Map<String, dynamic>> batches,
  });
}

final class NativeTransport implements AppWireTransport {
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
    if (Platform.isIOS) {
      final executableDirectory = File(Platform.resolvedExecutable).parent.path;
      return '$executableDirectory/Frameworks/libfloe_ffi.dylib';
    }
    if (Platform.isAndroid) return 'libfloe_ffi.so';
    if (Platform.isMacOS) {
      final executableDirectory = File(Platform.resolvedExecutable).parent.path;
      return '$executableDirectory/../Frameworks/libfloe_ffi.dylib';
    }
    throw UnsupportedError('The native Floe transport is unavailable.');
  }

  @override
  Future<Map<String, dynamic>> commandV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) => _appWireRequest('command_v2', request, timeout);

  @override
  Future<Map<String, dynamic>> queryV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) => _appWireRequest('query_v2', request, timeout);

  @override
  Future<Map<String, dynamic>> eventsV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) => _appWireRequest('events_v2', request, timeout);

  Future<Map<String, dynamic>> remotePairingV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) => _appWireRequest('remote_pairing_v2', request, timeout);

  Future<Map<String, dynamic>> remoteAccessV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  }) => _appWireRequest('remote_access_v2', request, timeout);

  Future<Map<String, dynamic>> _appWireRequest(
    String operation,
    Map<String, dynamic> request,
    Duration timeout,
  ) async {
    if (_closed) throw StateError('NativeTransport is already closed.');
    final requestId = request['request_id'];
    if (request['schema_version'] != appWireProtocolVersion ||
        requestId is! String ||
        requestId.isEmpty) {
      throw const FormatException('Invalid app-wire request envelope.');
    }
    final reply = ReceivePort();
    _commands.send({
      'operation': operation,
      'request': jsonEncode(request),
      'reply': reply.sendPort,
    });
    try {
      final result = _asMap(
        await reply.first.timeout(
          timeout,
          onTimeout: () => throw const NativeTransportException(
            'timeout',
            'The app request timed out; query the command or run state.',
          ),
        ),
      );
      if (result['status'] != 'ok') {
        throw NativeTransportException(
          'ffi',
          result['message']?.toString() ?? 'The Rust core request failed.',
        );
      }
      final envelope = _asMap(jsonDecode(result['response']! as String));
      if (envelope['schema_version'] != appWireProtocolVersion ||
          envelope['request_id'] != requestId) {
        throw const FormatException('Mismatched app-wire response envelope.');
      }
      if (envelope['status'] == 'error') {
        throw NativeTransportException.fromEnvelope(envelope);
      }
      if (envelope['status'] != 'ok' || envelope['result'] is! Map) {
        throw const FormatException('Invalid app-wire response envelope.');
      }
      return _asMap(envelope['result']);
    } finally {
      reply.close();
    }
  }

  @override
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

NativeTransportException _exceptionFromEnvelope(Map<String, dynamic> envelope) {
  return NativeTransportException.fromEnvelope(envelope);
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
          'command_v2' => bindings.commandV2(handle, input),
          'query_v2' => bindings.queryV2(handle, input),
          'events_v2' => bindings.eventsV2(handle, input),
          'remote_pairing_v2' => bindings.remotePairingV2(handle, input),
          'remote_access_v2' => bindings.remoteAccessV2(handle, input),
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
