import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/features/conversation/application/agent_request_id.dart';
import 'package:floe_client/features/voice/application/voice_gateway.dart';

final class AppWireVoiceGateway implements VoiceGateway {
  AppWireVoiceGateway(this._transport);

  final AppWireTransport _transport;
  _PendingStart? _pending;

  @override
  String? get pendingCommandId => _pending?.commandId;

  @override
  Future<VoiceStartStatus> start(String surfaceSessionId) async {
    if (!_wireId(surfaceSessionId)) {
      throw const FormatException('Invalid Voice surface identity.');
    }
    var pending = _pending;
    if (pending == null) {
      pending = _PendingStart(
        commandId: newAgentRequestId(),
        surfaceSessionId: surfaceSessionId,
      );
      _pending = pending;
    } else if (pending.surfaceSessionId != surfaceSessionId) {
      throw StateError(
        'Resolve the previous Voice Start before changing session.',
      );
    }

    final previouslySubmitted = pending.submitted;
    pending.submitted = true;
    try {
      final result = await _transport.commandV2({
        'schema_version': appWireProtocolVersion,
        'request_id': newAgentRequestId(),
        'command_id': pending.commandId,
        'command': {
          'kind': 'voice.start',
          'surface_session_id': pending.surfaceSessionId,
        },
      }, timeout: const Duration(seconds: 10));
      final status = _decodeStatus(result, pending.commandId);
      if (status.state == VoiceStartState.cancelled) _pending = null;
      return status;
    } on AppWireTransportException catch (error) {
      if (mayDiscardPendingCommand(
        error.commandOutcome,
        previouslySubmitted: previouslySubmitted,
      )) {
        _pending = null;
      }
      rethrow;
    }
  }

  @override
  Future<VoiceStartStatus?> readStatus(String commandId) async {
    if (!_wireId(commandId)) {
      throw const FormatException('Invalid Voice command identity.');
    }
    final result = await _transport.queryV2({
      'schema_version': appWireProtocolVersion,
      'request_id': newAgentRequestId(),
      'query': {'kind': 'voice.start.status', 'command_id': commandId},
    }, timeout: const Duration(seconds: 5));
    if (result['kind'] != 'voice.start.status' ||
        !result.containsKey('status') ||
        result.keys.toSet().difference(const {'kind', 'status'}).isNotEmpty) {
      throw const FormatException('Invalid Voice status response.');
    }
    final raw = result['status'];
    if (raw == null) return null;
    final status = _decodeStatus({
      'kind': 'voice.start',
      'status': raw,
    }, commandId);
    if (status.state == VoiceStartState.cancelled &&
        _pending?.commandId == commandId) {
      _pending = null;
    }
    return status;
  }

  VoiceStartStatus _decodeStatus(
    Map<String, dynamic> value,
    String expectedCommandId,
  ) {
    if (value['kind'] != 'voice.start' || value['status'] is! Map) {
      throw const FormatException('Invalid Voice Start response.');
    }
    final raw = (value['status'] as Map).map(
      (key, value) => MapEntry(key.toString(), value),
    );
    const keys = {
      'command_id',
      'surface_session_id',
      'state',
      'media',
      'remote',
      'stop_requested',
      'local_lease_released',
    };
    if (raw.keys.toSet().difference(keys).isNotEmpty ||
        !raw.keys.toSet().containsAll(keys) ||
        raw['command_id'] != expectedCommandId ||
        !_wireId(raw['surface_session_id']) ||
        raw['stop_requested'] != null && raw['stop_requested'] is! bool ||
        raw['local_lease_released'] != null &&
            raw['local_lease_released'] is! bool) {
      throw const FormatException('Voice status identity mismatch.');
    }
    final state = switch (raw['state']) {
      'preparing' => VoiceStartState.preparing,
      'bound' => VoiceStartState.bound,
      'cancelled' => VoiceStartState.cancelled,
      _ => throw const FormatException('Invalid Voice Start state.'),
    };
    final media = switch (raw['media']) {
      null => null,
      'preparing' => VoiceMediaState.preparing,
      'muted' => VoiceMediaState.muted,
      'capturing' => VoiceMediaState.capturing,
      'stopped' => VoiceMediaState.stopped,
      _ => throw const FormatException('Invalid Voice media state.'),
    };
    final remote = switch (raw['remote']) {
      null => null,
      'not_started' => VoiceRemoteState.notStarted,
      'dispatching' => VoiceRemoteState.dispatching,
      'connected' => VoiceRemoteState.connected,
      'close_requested' => VoiceRemoteState.closeRequested,
      'closed' => VoiceRemoteState.closed,
      'unknown' => VoiceRemoteState.unknown,
      _ => throw const FormatException('Invalid Voice remote state.'),
    };
    final statusFields = [
      media != null,
      remote != null,
      raw['stop_requested'] != null,
      raw['local_lease_released'] != null,
    ];
    final hasCompleteCall = statusFields.every((present) => present);
    final hasAnyCallField = statusFields.any((present) => present);
    if ((state == VoiceStartState.bound) != hasCompleteCall ||
        hasAnyCallField != hasCompleteCall) {
      throw const FormatException('Voice status lifecycle mismatch.');
    }
    return VoiceStartStatus(
      commandId: expectedCommandId,
      surfaceSessionId: raw['surface_session_id']! as String,
      state: state,
      media: media,
      remote: remote,
      stopRequested: raw['stop_requested'] as bool?,
      localLeaseReleased: raw['local_lease_released'] as bool?,
    );
  }
}

final class _PendingStart {
  _PendingStart({required this.commandId, required this.surfaceSessionId});

  final String commandId;
  final String surfaceSessionId;
  bool submitted = false;
}

bool _wireId(Object? value) =>
    value is String &&
    RegExp(r'^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$')
        .hasMatch(value);
