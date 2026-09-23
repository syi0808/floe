import 'dart:async';

import 'package:floe_client/features/connections/application/remote_access_gateway.dart';
import 'package:floe_client/features/connections/application/remote_pairing_gateway.dart';

import 'package:floe_client/app/local_identity.dart';
import 'package:floe_client/app/runtime/local_owner_gateways.dart';
import 'package:floe_client/app/runtime/local_context_gateway.dart';
import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/app/runtime/local_owner_gateways_scope.dart';
import 'package:floe_client/app/runtime/app_read_model.dart';
import 'package:floe_client/app/runtime/floe_client.dart';
import 'package:floe_client/app/runtime/native_transport.dart';

/// Thrown when a request through the app transport fails.
final class AppRuntimeException implements Exception {
  const AppRuntimeException(
    this.code,
    this.message, {
    this.field,
    this.metadata = const {},
  });

  final String code;
  final String message;
  final String? field;
  final Map<String, String> metadata;

  @override
  String toString() => message;
}

/// The app-lifetime objects: the native transport, the runtime client and the
/// read model.
///
/// These outlive any single screen, so no feature owns them. Features receive
/// owner gateways built on the admitted AppWire.
final class AppRuntime {
  AppRuntime._(this._transport, this.deviceId);

  final NativeTransport _transport;
  final String deviceId;

  late final FloeClient client = FloeClient(_transport);
  late final AppReadModel readModel = AppReadModel();
  late final RemoteAccessGateway remoteAccess = NativeRemoteAccessGateway(
    remoteAccessV2,
  );
  late final RemotePairingGateway pairing = NativeRemotePairingGateway(
    remotePairingV2,
    expectedPersonId: defaultLocalPersonId,
    expectedDeviceId: deviceId,
  );
  late final vault = NativeVaultLifecycleGateway(_transport);
  late final conversation = NativeConversationSessionGateway(
    _transport,
    runtimeClient: client,
    readModel: readModel,
  );
  late final registry = NativeRegistryGateway(_transport);
  late final personalAccess = NativePersonalAccessGateway(
    _transport,
    deviceId: deviceId,
  );
  late final memory = NativeMemoryGateway(_transport);
  late final connections = NativeConnectionsGateway(_transport);
  late final proposals = NativeProposalGateway(_transport);
  late final owners = LocalOwnerGateways(
    vault: vault,
    registry: registry,
    personalAccess: personalAccess,
    memory: memory,
    memoryReview: memory,
    connections: connections,
    proposals: proposals,
  );
  late final LocalContextTransport localContextTransport =
      NativeLocalContextGateway(_transport, deviceId: deviceId);
  AppWireTransport get wireTransport => _transport;

  Future<Map<String, dynamic>> remotePairingV2(Map<String, dynamic> request) =>
      _transport.remotePairingV2(request);

  Future<Map<String, dynamic>> remoteAccessV2(Map<String, dynamic> request) =>
      _transport.remoteAccessV2(request);

  static Future<AppRuntime> openDefault({required String deviceId}) async =>
      AppRuntime._(
        await _open(
          NativeTransport.openDefault(personId: defaultLocalPersonId),
        ),
        deviceId,
      );

  static Future<AppRuntime> open({
    required String libraryPath,
    required String databasePath,
    required String deviceId,
  }) async => AppRuntime._(
    await _open(
      NativeTransport.open(
        libraryPath: libraryPath,
        databasePath: databasePath,
      ),
    ),
    deviceId,
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
      );
    }
  }

  Future<void> close() async {
    readModel.dispose();
    await _transport.close();
  }
}
