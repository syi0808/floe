import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/app/runtime/owner_operation.dart';
import 'package:floe_client/features/connections/application/calendar_source_gateway.dart';
import 'package:floe_client/features/connections/domain/calendar_source_connection.dart';
import 'package:floe_client/features/conversation/application/agent_request_id.dart';

final class NativeCalendarSourceGateway implements CalendarSourceGateway {
  const NativeCalendarSourceGateway(this._transport, {required this.deviceId});

  final AppWireTransport _transport;
  final String deviceId;

  @override
  Future<CalendarSourceConnection?> inspectNative(String personId) async {
    final result = await ownerQuery(_transport, newAgentRequestId(), {
      'kind': 'connections.native_calendar.source',
    });
    if (result['kind'] != 'native_calendar_source') {
      throw const FormatException('Invalid Calendar source query result');
    }
    final raw = result['source'];
    if (raw == null) return null;
    return _decode(raw);
  }

  @override
  Future<CalendarSourceConnection> establishNative(
    String personId, {
    required String resourceMode,
    required List<CalendarSourceResource> resources,
  }) => _mutate({
    'type': 'establish',
    'resource_mode': resourceMode,
    'resources': resources.map((resource) => resource.toJson()).toList(),
  });

  @override
  Future<CalendarSourceConnection> configureNative(
    String personId, {
    required CalendarSourceConnection current,
    required String resourceMode,
    required List<CalendarSourceResource> resources,
  }) {
    _requireOwnedNative(current);
    return _mutate({
      'type': 'configure',
      'connection_id': current.connectionId,
      'expected_revision': current.revision,
      'resource_mode': resourceMode,
      'resources': resources.map((resource) => resource.toJson()).toList(),
    });
  }

  @override
  Future<CalendarSourceConnection> reconcileNativeInventory(
    String personId, {
    required CalendarSourceConnection current,
    required List<CalendarSourceResource> resources,
  }) {
    _requireOwnedNative(current);
    return _mutate({
      'type': 'reconcile_inventory',
      'connection_id': current.connectionId,
      'expected_revision': current.revision,
      'resources': resources.map((resource) => resource.toJson()).toList(),
    });
  }

  @override
  Future<CalendarSourceConnection> disconnectNative(
    String personId, {
    required CalendarSourceConnection current,
  }) {
    _requireOwnedNative(current);
    return _mutate({
      'type': 'disconnect',
      'connection_id': current.connectionId,
      'expected_revision': current.revision,
    });
  }

  @override
  Future<List<CalendarSourceConnection>> inspectRemote(String personId) async {
    final result = await ownerQuery(_transport, newAgentRequestId(), {
      'kind': 'connections.remote_calendar.sources',
    });
    if (result['kind'] != 'remote_calendar_sources' ||
        result['sources'] is! List) {
      throw const FormatException(
        'Invalid remote Calendar source query result',
      );
    }
    return (result['sources'] as List)
        .map((raw) => _decodeRemote(raw))
        .toList(growable: false);
  }

  @override
  Future<CalendarSourceConnection> bindRemote(
    String personId, {
    required String connectorId,
    required String connectionId,
    required List<CalendarSourceResource> resources,
    CalendarSourceConnection? current,
  }) async {
    if (current != null) {
      _requireOwnedRemote(current);
      if (current.connectionId != connectionId ||
          current.connectorId != connectorId) {
        throw const FormatException('Remote Calendar source identity changed');
      }
    }
    return _mutateRemote({
      'type': 'bind',
      'connector_id': connectorId,
      'connection_id': connectionId,
      'expected_revision': current?.revision,
      'resources': resources.map((resource) => resource.toJson()).toList(),
    });
  }

  @override
  Future<CalendarSourceConnection> disconnectRemote(
    String personId, {
    required CalendarSourceConnection current,
  }) {
    _requireOwnedRemote(current);
    return _mutateRemote({
      'type': 'disconnect',
      'connection_id': current.connectionId,
      'expected_revision': current.revision,
    });
  }

  Future<CalendarSourceConnection> _mutateRemote(
    Map<String, Object?> mutation,
  ) async {
    final commandId = newAgentRequestId();
    final result = await ownerCommand(_transport, commandId, {
      'kind': 'connections.remote_calendar.mutate',
      'mutation': mutation,
    });
    if (result['kind'] != 'remote_calendar_source' ||
        result['command_id'] != commandId) {
      throw const FormatException(
        'Invalid remote Calendar source command result',
      );
    }
    return _decodeRemote(result['source']);
  }

  CalendarSourceConnection _decodeRemote(Object? raw) {
    if (raw is! Map) {
      throw const FormatException('Missing remote Calendar source');
    }
    final source = CalendarSourceConnection.fromJson(
      Map<String, dynamic>.from(raw),
    );
    _requireOwnedRemote(source);
    return source;
  }

  void _requireOwnedRemote(CalendarSourceConnection source) {
    if (!const {
          'calendar.google',
          'calendar.microsoft',
        }.contains(source.connectorId) ||
        source.executionOwnerId != deviceId) {
      throw const FormatException('Remote Calendar source owner mismatch');
    }
  }

  Future<CalendarSourceConnection> _mutate(
    Map<String, Object?> mutation,
  ) async {
    final commandId = newAgentRequestId();
    final result = await ownerCommand(_transport, commandId, {
      'kind': 'connections.native_calendar.mutate',
      'mutation': mutation,
    });
    if (result['kind'] != 'native_calendar_source' ||
        result['command_id'] != commandId) {
      throw const FormatException('Invalid Calendar source command result');
    }
    return _decode(result['source']);
  }

  CalendarSourceConnection _decode(Object? raw) {
    if (raw is! Map) {
      throw const FormatException('Missing Calendar source');
    }
    final source = CalendarSourceConnection.fromJson(
      Map<String, dynamic>.from(raw),
    );
    _requireOwnedNative(source);
    return source;
  }

  void _requireOwnedNative(CalendarSourceConnection source) {
    if (source.connectorId != 'calendar.event_kit' ||
        source.executionOwnerId != deviceId) {
      throw const FormatException('Calendar source owner mismatch');
    }
  }
}
