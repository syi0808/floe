import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/app/runtime/owner_operation.dart';
import 'package:floe_client/features/connections/domain/connection_observe.dart';

abstract interface class ConnectionObserveGateway {
  Future<ConnectionObserveOverview> inspect({
    required String connectorId,
    required String connectionId,
  });

  Future<ConnectionObserveReview> review({
    required String connectorId,
    required String connectionId,
  });

  Future<ConnectionObserveOverview> setEnabled({
    required String connectorId,
    required String connectionId,
    required bool enabled,
    bool disconnecting = false,
    ConnectionObserveReview? expected,
  });
}

final class AppWireConnectionObserveGateway
    implements ConnectionObserveGateway {
  AppWireConnectionObserveGateway(this._transport);

  final AppWireTransport _transport;
  final OwnerOperationObserver _operations = OwnerOperationObserver();

  @override
  Future<ConnectionObserveOverview> inspect({
    required String connectorId,
    required String connectionId,
  }) => _perform(
    connectorId: connectorId,
    connectionId: connectionId,
    intent: {
      'kind': 'connection_observe.inspect',
      'connector_id': connectorId,
      'connection_id': connectionId,
    },
    command: false,
    stage: 'connection_observe_inspect',
    decode: (result) => _overview(result, connectorId, connectionId),
  );

  @override
  Future<ConnectionObserveReview> review({
    required String connectorId,
    required String connectionId,
  }) => _perform(
    connectorId: connectorId,
    connectionId: connectionId,
    intent: {
      'kind': 'connection_observe.review',
      'connector_id': connectorId,
      'connection_id': connectionId,
    },
    command: false,
    stage: 'connection_observe_review',
    decode: (result) {
      final reviewed = ConnectionObserveReview.fromJson(result['reviewed']);
      if (reviewed.connectorId != connectorId ||
          reviewed.connectionId != connectionId) {
        throw const FormatException('Observe review identity changed');
      }
      return reviewed;
    },
  );

  @override
  Future<ConnectionObserveOverview> setEnabled({
    required String connectorId,
    required String connectionId,
    required bool enabled,
    bool disconnecting = false,
    ConnectionObserveReview? expected,
  }) {
    if (enabled == (expected == null) ||
        (enabled && disconnecting) ||
        (expected != null &&
            (expected.connectorId != connectorId ||
                expected.connectionId != connectionId))) {
      throw const FormatException('Invalid Observe mutation');
    }
    return _perform(
      connectorId: connectorId,
      connectionId: connectionId,
      intent: {
        'kind': 'connection_observe.set_enabled',
        'mutation': {
          'connector_id': connectorId,
          'connection_id': connectionId,
          'enabled': enabled,
          'disconnecting': disconnecting,
          'expected': expected?.toJson(),
        },
      },
      command: true,
      stage: enabled
          ? 'connection_observe_enable'
          : 'connection_observe_disable',
      decode: (result) => _overview(result, connectorId, connectionId),
    );
  }

  Future<T> _perform<T>({
    required String connectorId,
    required String connectionId,
    required Map<String, Object?> intent,
    required bool command,
    required String stage,
    required T Function(Map<String, dynamic>) decode,
  }) => _operations.observe(
    scope: '$connectorId:$connectionId',
    intent: ownerIntent(intent),
    stage: stage,
    resultKind: 'connection_observe',
    start: (operationId) => command
        ? ownerCommand(_transport, operationId, intent)
        : ownerQuery(_transport, operationId, intent),
    read: (operationId, release) => ownerResult(
      _transport,
      'connection_observe.read_result',
      operationId,
      release,
    ),
    decode: decode,
  );

  ConnectionObserveOverview _overview(
    Map<String, dynamic> result,
    String connectorId,
    String connectionId,
  ) {
    final overview = ConnectionObserveOverview.fromJson(result['overview']);
    if (overview.connectorId != connectorId ||
        overview.connectionId != connectionId) {
      throw const FormatException('Observe overview identity changed');
    }
    return overview;
  }
}
