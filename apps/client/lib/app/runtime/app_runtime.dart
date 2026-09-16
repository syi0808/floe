import 'dart:async';

import 'package:floe_client/app/local_identity.dart';
import 'package:floe_client/app/runtime/agent_vault_gateway.dart';
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
/// [request] and the gateways built from it.
final class AppRuntime {
  AppRuntime._(this._transport, this.deviceId);

  final NativeTransport _transport;
  final String deviceId;

  late final FloeClient client = FloeClient(_transport);
  late final AppReadModel readModel = AppReadModel();
  late final AgentVaultGateway vault = NativeAgentVaultGateway(
    _vaultRequest,
    deviceId: deviceId,
    runtimeClient: client,
    readModel: readModel,
  );

  LocalContextTransport get localContextTransport => _transport;

  static Future<AppRuntime> openDefault({required String deviceId}) async =>
      AppRuntime._(
        await _open(NativeTransport.openDefault(personId: defaultLocalPersonId)),
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

  Future<Map<String, dynamic>> request(
    String operation,
    Map<String, dynamic> request,
  ) async {
    try {
      return await _transport.request(operation, request);
    } on NativeTransportException catch (error) {
      throw AppRuntimeException(
        error.code,
        error.message,
        field: error.field,
        metadata: error.metadata,
      );
    }
  }

  Future<Map<String, dynamic>> _vaultRequest(
    Map<String, Object?> request,
  ) async {
    try {
      return await this.request('agent_vault', request);
    } on AppRuntimeException catch (error) {
      throw AgentVaultException(
        error.metadata['agent_failure'] ?? error.code,
        requestId: error.metadata['request_id'],
        stage: error.metadata['stage'],
        metadata: error.metadata,
      );
    }
  }

  Future<void> close() async {
    readModel.dispose();
    await _transport.close();
  }
}
