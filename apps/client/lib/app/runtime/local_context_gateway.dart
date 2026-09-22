import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/app/runtime/native_transport.dart'
    show LocalContextTransport;
import 'package:floe_client/app/runtime/owner_operation.dart';
import 'package:floe_client/features/conversation/application/agent_request_id.dart';

final class NativeLocalContextGateway implements LocalContextTransport {
  NativeLocalContextGateway(this._transport, {required this.deviceId});
  final AppWireTransport _transport;
  final String deviceId;
  @override
  Future<void> registerAcquisitionHost({
    required String personId,
    required String hostEpoch,
  }) async {
    await _command(personId, {
      'kind': 'register_acquisition_host',
      'host_epoch': hostEpoch,
    });
  }

  @override
  Future<List<Map<String, dynamic>>> pollAcquisitions({
    required String personId,
    required String hostEpoch,
  }) async {
    final result = await _query(personId, {
      'kind': 'poll_acquisitions',
      'host_epoch': hostEpoch,
    });
    final raw = result['acquisitions'];
    if (raw is! List) throw const FormatException('Invalid acquisition poll.');
    return raw
        .map((value) {
          if (value is! Map) {
            throw const FormatException('Invalid acquisition request.');
          }
          return Map<String, dynamic>.from(value);
        })
        .toList(growable: false);
  }

  @override
  Future<void> completeAcquisition({
    required String personId,
    required String hostEpoch,
    required Map<String, dynamic> result,
  }) async {
    await _command(personId, {
      'kind': 'complete_acquisition',
      'host_epoch': hostEpoch,
      'result': _completion(personId, result),
    });
  }

  @override
  Future<void> failAcquisition({
    required String personId,
    required String hostEpoch,
    required String requestId,
    required String failure,
  }) async {
    if (!const {
      'permission_denied',
      'calendar_unavailable',
      'provider_unavailable',
    }.contains(failure)) {
      throw const FormatException('Invalid native acquisition failure.');
    }
    await _command(personId, {
      'kind': 'fail_acquisition',
      'host_epoch': hostEpoch,
      'request_id': requestId,
      'failure': failure,
    });
  }

  @override
  Future<void> disposeAcquisitionHost({
    required String personId,
    required String hostEpoch,
  }) async {
    await _command(personId, {
      'kind': 'dispose_acquisition_host',
      'host_epoch': hostEpoch,
    });
  }

  @override
  Future<void> registerAttentionHost({
    required String personId,
    required String hostEpoch,
  }) async {
    await _command(personId, {
      'kind': 'register_attention_host',
      'host_epoch': hostEpoch,
    });
  }

  @override
  Future<List<Map<String, dynamic>>> pollAttentionAcquisitions({
    required String personId,
    required String hostEpoch,
  }) async {
    final result = await _query(personId, {
      'kind': 'poll_attention_acquisitions',
      'host_epoch': hostEpoch,
    });
    final raw = result['attention_acquisitions'];
    if (raw is! List) {
      throw const FormatException('Invalid Attention acquisition poll.');
    }
    return raw
        .map((value) {
          if (value is! Map) {
            throw const FormatException('Invalid Attention request.');
          }
          return Map<String, dynamic>.from(value);
        })
        .toList(growable: false);
  }

  @override
  Future<void> completeAttentionAcquisition({
    required String personId,
    required String hostEpoch,
    required Map<String, dynamic> result,
  }) async {
    await _command(personId, {
      'kind': 'complete_attention_acquisition',
      'host_epoch': hostEpoch,
      'result': _completion(personId, result),
    });
  }

  @override
  Future<void> failAttentionAcquisition({
    required String personId,
    required String hostEpoch,
    required String requestId,
    required String failure,
  }) async {
    if (!const {
      'permission_denied',
      'attention_unavailable',
      'provider_unavailable',
      'cancelled',
    }.contains(failure)) {
      throw const FormatException('Invalid Attention acquisition failure.');
    }
    await _command(personId, {
      'kind': 'fail_attention_acquisition',
      'host_epoch': hostEpoch,
      'request_id': requestId,
      'failure': failure,
    });
  }

  @override
  Future<void> disposeAttentionHost({
    required String personId,
    required String hostEpoch,
  }) async {
    await _command(personId, {
      'kind': 'dispose_attention_host',
      'host_epoch': hostEpoch,
    });
  }

  @override
  Future<void> registerPersonalHost({
    required String personId,
    required String hostEpoch,
  }) async {
    await _command(personId, {
      'kind': 'register_personal_host',
      'host_epoch': hostEpoch,
    });
  }

  @override
  Future<List<Map<String, dynamic>>> pollPersonalAcquisitions({
    required String personId,
    required String hostEpoch,
  }) async {
    final result = await _query(personId, {
      'kind': 'poll_personal_acquisitions',
      'host_epoch': hostEpoch,
    });
    final raw = result['personal_acquisitions'];
    if (raw is! List) {
      throw const FormatException('Invalid personal acquisition poll.');
    }
    return raw
        .map((value) {
          if (value is! Map) {
            throw const FormatException(
              'Invalid personal acquisition request.',
            );
          }
          return Map<String, dynamic>.from(value);
        })
        .toList(growable: false);
  }

  @override
  Future<void> completePersonalAcquisition({
    required String personId,
    required String hostEpoch,
    required Map<String, dynamic> result,
  }) async {
    await _command(personId, {
      'kind': 'complete_personal_acquisition',
      'host_epoch': hostEpoch,
      'result': _completion(personId, result),
    });
  }

  @override
  Future<void> failPersonalAcquisition({
    required String personId,
    required String hostEpoch,
    required String requestId,
    required String failure,
  }) async {
    await _command(personId, {
      'kind': 'fail_personal_acquisition',
      'host_epoch': hostEpoch,
      'request_id': requestId,
      'failure': failure,
    });
  }

  @override
  Future<void> disposePersonalHost({
    required String personId,
    required String hostEpoch,
  }) async {
    await _command(personId, {
      'kind': 'dispose_personal_host',
      'host_epoch': hostEpoch,
    });
  }

  @override
  Future<void> publishLocalContext({
    required String personId,
    required String deviceId,
    required Map<String, dynamic> view,
  }) async {
    await _command(personId, {'kind': 'publish', 'view': view});
  }

  @override
  Future<void> publishCalendarObservation({
    required String personId,
    required String deviceId,
    required String connectionId,
    required int connectionRevision,
    required String provider,
    required List<String> calendarIds,
    required DateTime observedAt,
    required DateTime expiresAt,
    required DateTime rangeStart,
    required DateTime rangeEnd,
    required List<Map<String, dynamic>> batches,
  }) async {
    await _command(personId, {
      'kind': 'publish_calendar_observation',

      'connection_id': connectionId,
      'connection_revision': connectionRevision,
      'provider': provider,
      'calendar_ids': calendarIds,
      'observed_at_unix_ms': observedAt.toUtc().millisecondsSinceEpoch,
      'expires_at_unix_ms': expiresAt.toUtc().millisecondsSinceEpoch,
      'range_start_unix_ms': rangeStart.toUtc().millisecondsSinceEpoch,
      'range_end_unix_ms': rangeEnd.toUtc().millisecondsSinceEpoch,
      'batches': batches,
    });
  }

  Future<Map<String, dynamic>> readLocalContext({
    required String personId,
    required String viewId,
    String? deviceId,
  }) async {
    final result = await _query(personId, {'kind': 'read', 'view_id': viewId});
    return _asMap(result['view']);
  }

  @override
  Future<int> revokeLocalContext({
    required String personId,
    required String deviceId,
    String? viewId,
  }) async {
    final result = await _command(personId, {
      'kind': 'revoke',

      'view_id': ?viewId,
    });
    return result['removed_count']! as int;
  }

  Map<String, dynamic> _completion(
    String personId,
    Map<String, dynamic> result,
  ) {
    if (result['person_id'] != personId || result['device_id'] != deviceId) {
      throw const FormatException('Stale native completion identity');
    }
    return Map<String, dynamic>.from(result)
      ..remove('person_id')
      ..remove('device_id');
  }

  Future<Map<String, dynamic>> _command(
    String personId,
    Map<String, dynamic> command,
  ) async {
    final commandId = newAgentRequestId();
    final result = await ownerCommand(_transport, commandId, {
      'kind': 'context.apply',
      'command': command,
    });
    if (result['kind'] != 'context_applied' ||
        result['command_id'] != commandId) {
      throw const FormatException('Invalid Context command correlation');
    }
    return _context(personId, result);
  }

  Future<Map<String, dynamic>> _query(
    String personId,
    Map<String, dynamic> query,
  ) async {
    final result = await ownerQuery(_transport, newAgentRequestId(), {
      'kind': 'context.read',
      'query': query,
    });
    if (result['kind'] != 'context_read') {
      throw const FormatException('Invalid Context query response');
    }
    return _context(personId, result);
  }

  Map<String, dynamic> _context(String personId, Map<String, dynamic> result) {
    final context = _asMap(result['context']);
    if (context['person_id'] != personId) {
      throw const FormatException('Stale Context result');
    }
    return context;
  }
}

Map<String, dynamic> _asMap(Object? value) =>
    Map<String, dynamic>.from(value! as Map);
