import 'package:floe_client/infrastructure/native/calendar_system_access_gateway.dart';
import 'package:floe_client/features/connections/presentation/connections_controller.dart';
import 'package:floe_client/app/runtime/runtime_controller.dart';
import 'package:floe_client/app/runtime/app_wire_runtime_gateway.dart';
import 'package:floe_client/features/experts/application/agent_registry_controller.dart';
import 'package:floe_client/features/knowledge/application/agent_memory_controller.dart';
import 'package:floe_client/features/connections/infrastructure/app_wire_connections_gateway.dart';

import 'dart:async';

import 'package:path_provider/path_provider.dart';
import 'package:floe_client/features/knowledge/infrastructure/app_wire_memory_gateway.dart';
import 'package:floe_client/features/conversation/infrastructure/app_wire_conversation_gateway.dart';
import 'package:floe_client/features/actions/application/calendar_action_facade.dart';
import 'package:floe_client/features/experts/infrastructure/app_wire_registry_gateway.dart';
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
  AppRuntime._(NativeTransport transport)
    : _transport = transport,
      deviceId = transport.deviceId,
      personId = transport.personId;

  final NativeTransport _transport;
  final String deviceId;
  final String personId;

  late final AppWireConversationClient client = AppWireConversationClient(
    _transport,
  );
  late final AppReadModel readModel = AppReadModel();
  late final runtimeGateway = AppWireRuntimeGateway(_transport);
  late final conversation = AppWireConversationGateway(
    _transport,
    runtimeClient: client,
    readModel: readModel,
  );
  late final registry = AppWireRegistryGateway(_transport);
  late final memory = AppWireMemoryGateway(_transport);
  late final connections = AppWireConnectionsGateway(_transport);
  late final actions = CalendarActionFacade(this);
  late final connectionsController = ConnectionsController(
    connections,
    runtime: runtimeController,
    calendarSystemAccess: const EventKitSystemAccessGateway(),
  );
  late final runtimeController = RuntimeController(
    gateway: runtimeGateway,
    personId: personId,
  )..addListener(_readinessChanged);
  late final registryController = AgentRegistryController(
    gateway: registry,
    canOperate: () => runtimeController.ready,
    onFatalFailure: runtimeController.reportFailure,
  );
  late final memoryController = AgentMemoryController(
    memoryGateway: memory,
    reviewGateway: memory,
    personId: personId,
    canOperate: () => runtimeController.ready,
    onFatalFailure: runtimeController.reportFailure,
  );
  bool _wasReady = false;
  void _readinessChanged() {
    final ready = runtimeController.ready;
    if (_wasReady != ready) {
      registryController.clear();
      memoryController.clear();
    }
    _wasReady = ready;
  }

  late final owners = LocalOwnerGateways(
    runtime: runtimeController,
    registry: registryController,
    memory: memoryController,
    actions: actions,
  );
  late final NativeContextHostTransport nativeHostTransport =
      AppWireNativeContextHostTransport(_transport.nativeCallbacks);
  AppWireTransport get wireTransport => _transport;

  static Future<AppRuntime> openDefault() async {
    final supportDirectory = await getApplicationSupportDirectory();
    final transport = await _open(
      NativeTransport.openDefault(
        libraryPath: resolveLibraryPath(),
        supportDirectory: supportDirectory.path,
      ),
    );
    return AppRuntime._(transport);
  }

  static Future<AppRuntime> open({
    required String libraryPath,
    required String databasePath,
    required String deviceId,
    required String personId,
  }) async {
    final transport = await _open(
      NativeTransport.open(
        libraryPath: libraryPath,
        databasePath: databasePath,
      ),
    );
    if (transport.deviceId != deviceId || transport.personId != personId) {
      await transport.close();
      throw const AppRuntimeException(
        'policy_denied',
        'The opened profile does not match the requested identity.',
      );
    }
    return AppRuntime._(transport);
  }

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

  Future<void> startRuntime() => runtimeController.open();

  /// Closes local Vault-backed presentation admission before app retirement.
  void closeAdmission() => runtimeController.closeAdmission();

  Future<void>? _closing;
  Future<void> close() => _closing ??= _close();

  Future<void> _close() async {
    closeAdmission();
    try {
      await _transport.close();
    } finally {
      connectionsController.dispose();
      registryController.dispose();
      memoryController.dispose();
      runtimeController.removeListener(_readinessChanged);
      runtimeController.dispose();
      readModel.dispose();
    }
  }
}
