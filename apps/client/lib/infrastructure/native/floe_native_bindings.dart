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
typedef FloeAcquireNativeHostNative = Pointer<Void> Function(
  Pointer<Void> core,
  Pointer<Pointer<Utf8>> errorJson,
);
typedef FloeAcquireNativeHostDart = Pointer<Void> Function(
  Pointer<Void> core,
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
typedef FloeIdentityNative = Pointer<Utf8> Function(Pointer<Void> handle);
typedef FloeIdentityDart = Pointer<Utf8> Function(Pointer<Void> handle);

typedef FloeFreeStringNative = Void Function(Pointer<Utf8> value);
typedef FloeFreeStringDart = void Function(Pointer<Utf8> value);
typedef FloeFreeCoreNative = Void Function(Pointer<Void> handle);
typedef FloeFreeCoreDart = void Function(Pointer<Void> handle);
typedef FloeProtocolVersionNative = Uint32 Function();
typedef FloeProtocolVersionDart = int Function();

final class FloeNativeBindings {
  FloeNativeBindings(String libraryPath)
    : _library = libraryPath.isEmpty
          ? DynamicLibrary.process()
          : DynamicLibrary.open(libraryPath) {
    open = _library.lookupFunction<FloeOpenNative, FloeOpenDart>(
      'floe_core_open',
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
  late final FloeOpenDart openDefault = _library
      .lookupFunction<FloeOpenNative, FloeOpenDart>('floe_core_open_default');
  late final FloeIdentityDart identity = _library
      .lookupFunction<FloeIdentityNative, FloeIdentityDart>(
        'floe_core_identity',
      );
  late final FloeCallDart commandV2 = _library
      .lookupFunction<FloeCallNative, FloeCallDart>('floe_core_command_v2');
  late final FloeCallDart queryV2 = _library
      .lookupFunction<FloeCallNative, FloeCallDart>('floe_core_query_v2');
  late final FloeCallDart eventsV2 = _library
      .lookupFunction<FloeCallNative, FloeCallDart>('floe_core_events_v2');
  late final FloeFreeStringDart freeString;
  late final FloeFreeCoreDart freeCore;
  late final FloeProtocolVersionDart protocolVersion;
  late final FloeProtocolVersionDart storageProfile = _library
      .lookupFunction<FloeProtocolVersionNative, FloeProtocolVersionDart>(
        'floe_storage_profile',
      );
  late final FloeAcquireNativeHostDart acquireNativeHost = _library
      .lookupFunction<FloeAcquireNativeHostNative, FloeAcquireNativeHostDart>(
        'floe_native_host_acquire',
      );
  late final FloeCallDart nativeHostCommandV2 = _library
      .lookupFunction<FloeCallNative, FloeCallDart>(
        'floe_native_host_command_v2',
      );
  late final FloeCallDart nativeHostQueryV2 = _library
      .lookupFunction<FloeCallNative, FloeCallDart>(
        'floe_native_host_query_v2',
      );
  late final FloeFreeCoreDart freeNativeHost = _library
      .lookupFunction<FloeFreeCoreNative, FloeFreeCoreDart>(
        'floe_native_host_free',
      );
}
