import 'dart:ffi';

import 'package:ffi/ffi.dart';

typedef FloeOpenNative = Pointer<Void> Function(
  Pointer<Utf8> databasePath,
  Pointer<Pointer<Utf8>> errorJson,
);
typedef FloeOpenDart = Pointer<Void> Function(
  Pointer<Utf8> databasePath,
  Pointer<Pointer<Utf8>> errorJson,
);
typedef FloeCallNative = Pointer<Utf8> Function(
  Pointer<Void> handle,
  Pointer<Utf8> requestJson,
);
typedef FloeCallDart = Pointer<Utf8> Function(
  Pointer<Void> handle,
  Pointer<Utf8> requestJson,
);
typedef FloeFreeStringNative = Void Function(Pointer<Utf8> value);
typedef FloeFreeStringDart = void Function(Pointer<Utf8> value);
typedef FloeFreeCoreNative = Void Function(Pointer<Void> handle);
typedef FloeFreeCoreDart = void Function(Pointer<Void> handle);
typedef FloeProtocolVersionNative = Uint32 Function();
typedef FloeProtocolVersionDart = int Function();

final class FloeNativeBindings {
  FloeNativeBindings(String libraryPath)
    : _library = DynamicLibrary.open(libraryPath) {
    open = _library.lookupFunction<FloeOpenNative, FloeOpenDart>(
      'floe_core_open',
    );
    loadDay = _library.lookupFunction<FloeCallNative, FloeCallDart>(
      'floe_core_load_day',
    );
    execute = _library.lookupFunction<FloeCallNative, FloeCallDart>(
      'floe_core_execute',
    );
    freeString = _library
        .lookupFunction<FloeFreeStringNative, FloeFreeStringDart>(
          'floe_string_free',
        );
    freeCore = _library.lookupFunction<FloeFreeCoreNative, FloeFreeCoreDart>(
      'floe_core_free',
    );
    protocolVersion = _library
        .lookupFunction<FloeProtocolVersionNative, FloeProtocolVersionDart>(
          'floe_protocol_version',
        );
  }

  final DynamicLibrary _library;
  late final FloeOpenDart open;
  late final FloeCallDart loadDay;
  late final FloeCallDart execute;
  late final FloeFreeStringDart freeString;
  late final FloeFreeCoreDart freeCore;
  late final FloeProtocolVersionDart protocolVersion;
}
