import 'dart:async';
import 'dart:convert';
import 'dart:ffi';
import 'dart:io';
import 'dart:isolate';

import 'package:ffi/ffi.dart';

import 'package:floe_client/infrastructure/native/floe_native_bindings.dart';
import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/app/runtime/owner_failure.dart';
import 'package:floe_client/infrastructure/native/native_context_host_transport.dart';

const nativeProtocolVersion = 1;

final class NativeTransportException implements Exception {
  const NativeTransportException(
    this.code,
    this.message, {
    this.field,
    this.metadata = const {},
    this.ownerFailure,
  });

  final String code;
  final String message;
  final String? field;
  final Map<String, String> metadata;
  final OwnerFailure? ownerFailure;

  factory NativeTransportException.fromEnvelope(Map<String, dynamic> envelope) {
    final error = envelope['error'] is Map
        ? _asMap(envelope['error'])
        : envelope;
    return NativeTransportException(
      error['code']?.toString() ?? 'internal',
      error['message']?.toString() ?? 'Could not open Rust core.',
      field: error['field']?.toString(),
      ownerFailure: error['owner_failure'] == null ? null : OwnerFailure.fromJson(error['owner_failure']),
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

final class NativeTransport implements AppWireTransport {
  NativeTransport._(this._commands, this._nativeCallbacks) {
    _disposal = _NativeTransportDisposal(_commands, _nativeCallbacks._commands);
    _finalizer.attach(this, _disposal, detach: this);
  }

  static final Finalizer<_NativeTransportDisposal> _finalizer = Finalizer((value) => unawaited(value.close()));
  final SendPort _commands;
  final _NativeCallbackTransport _nativeCallbacks;
  late final _NativeTransportDisposal _disposal;
  NativeHostWireTransport get nativeCallbacks => _nativeCallbacks;
  bool _closed = false;
  Future<void>? _closing;

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
    final commands = result['commands']! as SendPort;
    try {
      final callbacks = await _NativeCallbackTransport.start(libraryPath, result['native_lane_address']! as int);
      return NativeTransport._(commands, callbacks);
    } on Object {
      await _closeNativePort(commands);
      rethrow;
    }
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

  Future<Map<String, dynamic>> _appWireRequest(
    String operation,
    Map<String, dynamic> request,
    Duration timeout,
  ) async {
    if (_closed) throw StateError('NativeTransport is already closed.');
    return _sendNativeWire(_commands, operation, request, timeout);
  }

  @override
  Future<void> close() => _closing ??= _close();

  Future<void> _close() async {
    if (_closed) return;
    _closed = true;
    _nativeCallbacks._closed = true;
    _finalizer.detach(this);
    // Callback free closes registrations and wakes outstanding owner requests
    // before the product lane reaches its own queued close.
    await _disposal.close();
  }
}

final class _NativeTransportDisposal {
  _NativeTransportDisposal(this.product, this.native);
  final SendPort product;
  final SendPort native;
  Future<void>? _closing;
  Future<void> close() => _closing ??= _close();
  Future<void> _close() async {
    try { await _closeNativePort(native); }
    finally { await _closeNativePort(product); }
  }
}

Future<void> _closeNativePort(SendPort commands) async {
  final reply = ReceivePort();
  commands.send({'operation': 'close', 'reply': reply.sendPort});
  try { await reply.first.timeout(const Duration(seconds: 5)); }
  on TimeoutException {
    // The close stays queued. Killing an isolate while a native call still owns
    // a handle would invalidate its cleanup; its bounded call finishes/free once.
  } finally { reply.close(); }
}

final class _NativeCallbackTransport implements NativeHostWireTransport {
  _NativeCallbackTransport(this._commands);
  final SendPort _commands;
  bool _closed = false;
  static Future<_NativeCallbackTransport> start(String libraryPath, int address) async {
    final ready = ReceivePort();
    try {
      await Isolate.spawn(_nativeCallbackWorkerMain, {'ready': ready.sendPort, 'library_path': libraryPath, 'lane_address': address});
    } on Object {
      ready.close();
      // Spawn failed before ownership could transfer to the worker.
      FloeNativeBindings(libraryPath).freeNativeHost(Pointer<Void>.fromAddress(address));
      rethrow;
    }
    try {
      final result = _asMap(await ready.first.timeout(const Duration(seconds: 5)));
      if (result['status'] != 'ok') throw NativeTransportException('ffi_native_host', result['message'] as String);
      final commands = result['commands']! as SendPort;
      commands.send({'operation': 'adopt'});
      return _NativeCallbackTransport(commands);
    } finally { ready.close(); }
  }
  @override
  Future<Map<String, dynamic>> commandV2(Map<String, dynamic> request, {Duration timeout = const Duration(seconds: 3)}) => _call('command_v2', request, timeout);
  @override
  Future<Map<String, dynamic>> queryV2(Map<String, dynamic> request, {Duration timeout = const Duration(seconds: 3)}) => _call('query_v2', request, timeout);
  Future<Map<String, dynamic>> _call(String operation, Map<String, dynamic> request, Duration timeout) {
    if (_closed) throw StateError('The native callback lane is closed.');
    final intent = request[operation == 'command_v2' ? 'command' : 'query'];
    if (intent is! Map || intent['kind'] is! String || !(intent['kind'] as String).startsWith('native_host.')) {
      throw const FormatException('Only native host callbacks may use this lane.');
    }
    return _sendNativeWire(_commands, operation, request, timeout);
  }
}

Future<Map<String, dynamic>> _sendNativeWire(SendPort commands, String operation, Map<String, dynamic> request, Duration timeout) async {
  final requestId = request['request_id'];
  if (request['schema_version'] != appWireProtocolVersion || requestId is! String || requestId.isEmpty) {
    throw const FormatException('Invalid app-wire request envelope.');
  }
  final reply = ReceivePort();
  commands.send({'operation': operation, 'request': jsonEncode(request), 'reply': reply.sendPort});
  try {
    final result = _asMap(await reply.first.timeout(timeout,
      onTimeout: () => throw const NativeTransportException('timeout', 'The app request timed out; query the command or run state.')));
    if (result['status'] != 'ok') throw NativeTransportException('ffi', result['message']?.toString() ?? 'The Rust core request failed.');
    final envelope = _asMap(jsonDecode(result['response']! as String));
    if (envelope['schema_version'] != appWireProtocolVersion || envelope['request_id'] != requestId) {
      throw const FormatException('Mismatched app-wire response envelope.');
    }
    if (envelope['status'] == 'error') throw NativeTransportException.fromEnvelope(envelope);
    if (envelope['status'] != 'ok' || envelope['result'] is! Map) throw const FormatException('Invalid app-wire response envelope.');
    return _asMap(envelope['result']);
  } finally { reply.close(); }
}

Future<void> _nativeCallbackWorkerMain(Map<String, Object?> configuration) async {
  final ready = configuration['ready']! as SendPort;
  final lane = Pointer<Void>.fromAddress(configuration['lane_address']! as int);
  FloeNativeBindings? ownedBindings;
  final commands = ReceivePort();
  final inputMessages = StreamIterator<dynamic>(commands);
  try {
    final bindings = FloeNativeBindings(configuration['library_path']! as String);
    ownedBindings = bindings;
    final command = bindings.nativeHostCommandV2;
    final query = bindings.nativeHostQueryV2;
    ready.send({'status': 'ok', 'commands': commands.sendPort});
    // A startup caller that disappears cannot strand an independent native box.
    final adopted = await inputMessages.moveNext().timeout(const Duration(seconds: 5));
    if (!adopted || _asMap(inputMessages.current)['operation'] != 'adopt') return;
    while (await inputMessages.moveNext()) {
      final message = _asMap(inputMessages.current);
      if (message['operation'] == 'close') {
        bindings.freeNativeHost(lane);
        ownedBindings = null;
        (message['reply'] as SendPort?)?.send(true);
        return;
      }
      final reply = message['reply']! as SendPort;
      final input = (message['request']! as String).toNativeUtf8();
      Pointer<Utf8> output = nullptr;
      try {
        output = switch (message['operation']) {
          'command_v2' => command(lane, input),
          'query_v2' => query(lane, input),
          _ => throw StateError('Invalid native callback operation.'),
        };
        if (output == nullptr) throw StateError('Native callback returned no response.');
        reply.send({'status':'ok', 'response': output.toDartString()});
      } on Object catch (error) { reply.send({'status':'error', 'message':error.toString()}); }
      finally { if (output != nullptr) bindings.freeString(output); calloc.free(input); }
    }
  } on Object catch (error) {
    ready.send({'status':'error', 'message':error.toString()});
  } finally {
    ownedBindings?.freeNativeHost(lane);
    commands.close();
    await inputMessages.cancel();
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
  Pointer<Void> nativeLane = nullptr;
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
    // Independent handle owns only the callback admission lane, never core.
    final laneError = calloc<Pointer<Utf8>>();
    try {
      bindings.nativeHostCommandV2;
      bindings.nativeHostQueryV2;
      bindings.freeNativeHost;
      nativeLane = bindings.acquireNativeHost(handle, laneError);
      if (nativeLane == nullptr) {
        final message = laneError.value;
        final reason = message == nullptr ? null : message.toDartString();
        if (message != nullptr) bindings.freeString(message);
        throw reason == null ? StateError('Could not acquire native callback lane.')
          : _exceptionFromEnvelope(_asMap(jsonDecode(reason)));
      }
    } finally { calloc.free(laneError); }
  } on Object catch (error) {
    if (nativeLane != nullptr) bindings?.freeNativeHost(nativeLane);
    if (handle != nullptr) bindings?.freeCore(handle);
    ready.send({
      'status': 'error',
      'error': {'code': 'ffi_open', 'message': error.toString()},
    });
    return;
  }

  final commands = ReceivePort();
  ready.send({'status': 'ok', 'commands': commands.sendPort, 'native_lane_address': nativeLane.address});
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
