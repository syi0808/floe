import 'package:floe_client/features/connections/infrastructure/app_wire_connections_gateway.dart';

import 'dart:async';

import 'package:floe_client/app/runtime/local_profile_selection.dart';
import 'package:floe_client/features/knowledge/infrastructure/app_wire_memory_gateway.dart';
import 'package:floe_client/features/conversation/infrastructure/app_wire_conversation_gateway.dart';
import 'package:floe_client/features/actions/application/calendar_action_facade.dart';
import 'package:floe_client/features/experts/infrastructure/app_wire_registry_gateway.dart';
import 'package:floe_client/features/vault/infrastructure/app_wire_vault_gateway.dart';
import 'package:floe_client/infrastructure/native/native_context_host_transport.dart';
import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/app/runtime/owner_failure.dart';
import 'package:floe_client/app/runtime/local_owner_gateways_scope.dart';
import 'package:floe_client/app/runtime/app_read_model.dart';
import 'package:floe_client/features/conversation/infrastructure/app_wire_conversation_client.dart';
import 'package:floe_client/app/runtime/native_transport.dart';

/// Thrown when a request through the app transport fails.
final class AppRuntimeException implements Exception {
  const AppRuntimeException(
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

  @override
  String toString() => message;
}

/// The app-lifetime objects: the native transport, the runtime client and the
/// read model.
///
/// These outlive any single screen, so no feature owns them. Features receive
/// owner gateways built on the admitted AppWire.
final class AppRuntime {
  AppRuntime._(this._transport, this.deviceId, this.personId);

  final NativeTransport _transport;
  final String deviceId;
  final String personId;

  late final AppWireConversationClient client = AppWireConversationClient(
    _transport,
  );
  late final AppReadModel readModel = AppReadModel();
  late final vault = AppWireVaultGateway(_transport);
  late final conversation = AppWireConversationGateway(
    _transport,
    runtimeClient: client,
    readModel: readModel,
  );
  late final registry = AppWireRegistryGateway(_transport);
  late final memory = AppWireMemoryGateway(_transport);
  late final connections = AppWireConnectionsGateway(_transport);
  late final actions = CalendarActionFacade(this);
  late final owners = LocalOwnerGateways(
    vault: vault,
    registry: registry,
    memory: memory,
    memoryReview: memory,
    actions: actions,
  );
  late final NativeContextHostTransport nativeHostTransport =
      AppWireNativeContextHostTransport(_transport.nativeCallbacks);
  AppWireTransport get wireTransport => _transport;

  static Future<AppRuntime> openSelected({
    required String deviceId,
    required ExistingLocalProfile profile,
  }) async => AppRuntime._(
    await _open(
      NativeTransport.open(
        libraryPath: resolveLibraryPath(),
        databasePath: profile.databasePath,
      ),
    ),
    deviceId,
    profile.personId,
  );

  static Future<AppRuntime> open({
    required String libraryPath,
    required String databasePath,
    required String deviceId,
    required String personId,
  }) async => AppRuntime._(
    await _open(
      NativeTransport.open(
        libraryPath: libraryPath,
        databasePath: databasePath,
      ),
    ),
    deviceId,
    personId,
  );

  static String resolveLibraryPath() => NativeTransport.resolveLibraryPath();

  static Future<NativeTransport> _open(Future<NativeTransport> pending) async {
    try {
      return await pending;
    } on NativeTransportException catch (error) {
      throw AppRuntimeException(
        error.code,
        error.message,
        field: error.field,
        metadata: error.metadata,
        ownerFailure: error.ownerFailure,
      );
    }
  }

  Future<void> close() async {
    readModel.dispose();
    await _transport.close();
  }
}
