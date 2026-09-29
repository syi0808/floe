import 'dart:convert';

import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/app/runtime/owner_operation.dart';
import 'package:floe_client/features/connections/domain/source_connection.dart';
import 'package:floe_client/features/conversation/application/agent_request_id.dart';

abstract interface class NativePersonalSourceGateway {
  Future<SourceConnection?> inspect(String connectorId);

  Future<SourceConnection> setup({
    required String connectorId,
    required int? expectedRevision,
    required List<String> selectedHandles,
  });
}

final class AppWireNativePersonalSourceGateway
    implements NativePersonalSourceGateway {
  const AppWireNativePersonalSourceGateway(
    this._transport, {
    required this.deviceId,
  });

  final AppWireTransport _transport;
  final String deviceId;

  @override
  Future<SourceConnection?> inspect(String connectorId) async {
    _validateConnector(connectorId);
    final result = await ownerQuery(_transport, newAgentRequestId(), {
      'kind': 'connections.native_personal.source',
      'connector_id': connectorId,
    });
    if (result['kind'] != 'native_personal_source') {
      throw const FormatException('Invalid personal source result');
    }
    final raw = result['source'];
    return raw == null ? null : _decode(raw, connectorId);
  }

  @override
  Future<SourceConnection> setup({
    required String connectorId,
    required int? expectedRevision,
    required List<String> selectedHandles,
  }) async {
    _validateConnector(connectorId);
    final handles = List<String>.of(selectedHandles)..sort();
    if (expectedRevision != null && expectedRevision <= 0 ||
        connectorId == 'contacts.apple' && handles.isEmpty ||
        connectorId != 'contacts.apple' && handles.isNotEmpty ||
        handles.length > 64 ||
        handles.toSet().length != handles.length ||
        handles.any(
          (handle) =>
              handle.isEmpty ||
              handle.trim() != handle ||
              utf8.encode(handle).length > 256 ||
              handle.contains('*') ||
              RegExp(r'[\x00-\x1f\x7f]').hasMatch(handle) ||
              handle.toLowerCase() == '00000000-0000-0000-0000-000000000000',
        )) {
      throw const FormatException('Invalid personal source setup');
    }
    final commandId = newAgentRequestId();
    final result = await ownerCommand(_transport, commandId, {
      'kind': 'connections.native_personal.setup',
      'setup': {
        'connector_id': connectorId,
        'expected_revision': expectedRevision,
        'selected_handles': handles,
      },
    });
    if (result['kind'] != 'native_personal_source' ||
        result['command_id'] != commandId) {
      throw const FormatException('Invalid personal source command result');
    }
    return _decode(result['source'], connectorId);
  }

  SourceConnection _decode(Object? raw, String connectorId) {
    if (raw is! Map) throw const FormatException('Missing personal source');
    final source = SourceConnection.fromJson(Map<String, dynamic>.from(raw));
    final owner = connectorId == 'attention.macos'
        ? 'macos:$deviceId'
        : 'apple:$deviceId';
    if (source.connectorId != connectorId ||
        source.executionOwnerId != owner ||
        source.state != 'ready') {
      throw const FormatException('Personal source identity changed');
    }
    return source;
  }

  void _validateConnector(String connectorId) {
    if (!const {
      'contacts.apple',
      'attention.macos',
      'health.apple',
    }.contains(connectorId)) {
      throw const FormatException('Invalid personal connector');
    }
  }
}
