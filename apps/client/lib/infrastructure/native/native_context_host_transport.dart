import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/features/conversation/application/agent_request_id.dart';

/// Dedicated callback lane, sharing the verified Rust host without sharing its
/// product handle or blocking product request execution.
abstract interface class NativeHostWireTransport {
  Future<Map<String, dynamic>> commandV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  });
  Future<Map<String, dynamic>> queryV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  });
}

/// Internal native callback capability. Product feature interfaces never expose it.
final class NativeHostRegistration {
  const NativeHostRegistration._(
    this.registrationId,
    this.hostEpoch,
    this.runtimeEpoch,
  );
  factory NativeHostRegistration.fromJson(Object? value) {
    final fields = _object(value);
    if (fields.length != 3 ||
        !fields.keys.toSet().containsAll({
          'registration_id',
          'host_epoch',
          'runtime_epoch',
        }) ||
        fields['registration_id'] is! String ||
        !RegExp(
          r'^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$',
        ).hasMatch(fields['registration_id'] as String) ||
        fields['host_epoch'] is! String ||
        (fields['host_epoch'] as String).isEmpty ||
        fields['runtime_epoch'] is! int ||
        (fields['runtime_epoch'] as int) < 1) {
      throw const FormatException('Invalid native host registration.');
    }
    return NativeHostRegistration._(
      fields['registration_id'] as String,
      fields['host_epoch'] as String,
      fields['runtime_epoch'] as int,
    );
  }
  final String registrationId;
  final String hostEpoch;
  final int runtimeEpoch;
  Map<String, Object?> toJson() => {
    'registration_id': registrationId,
    'host_epoch': hostEpoch,
    'runtime_epoch': runtimeEpoch,
  };
}

Map<String, dynamic> _object(Object? value) {
  if (value is! Map || value.keys.any((key) => key is! String))
    throw const FormatException('Invalid native host object.');
  return Map<String, dynamic>.from(value);
}

abstract interface class NativeContextHostTransport {
  Future<NativeHostRegistration> registerAcquisitionHost();
  Future<List<Map<String, dynamic>>> pollAcquisitions({
    required NativeHostRegistration registration,
  });
  Future<void> completeAcquisition({
    required NativeHostRegistration registration,
    required Map<String, dynamic> result,
  });
  Future<void> failAcquisition({
    required NativeHostRegistration registration,
    required String requestId,
    required String failure,
  });
  Future<void> disposeAcquisitionHost({
    required NativeHostRegistration registration,
  });
  Future<NativeHostRegistration> registerAttentionHost();
  Future<List<Map<String, dynamic>>> pollAttentionAcquisitions({
    required NativeHostRegistration registration,
  });
  Future<void> completeAttentionAcquisition({
    required NativeHostRegistration registration,
    required Map<String, dynamic> result,
  });
  Future<void> failAttentionAcquisition({
    required NativeHostRegistration registration,
    required String requestId,
    required String failure,
  });
  Future<void> disposeAttentionHost({
    required NativeHostRegistration registration,
  });
  Future<NativeHostRegistration> registerPersonalHost();
  Future<List<Map<String, dynamic>>> pollPersonalAcquisitions({
    required NativeHostRegistration registration,
  });
  Future<void> completePersonalAcquisition({
    required NativeHostRegistration registration,
    required Map<String, dynamic> result,
  });
  Future<void> failPersonalAcquisition({
    required NativeHostRegistration registration,
    required String requestId,
    required String failure,
  });
  Future<void> disposePersonalHost({
    required NativeHostRegistration registration,
  });
}

// The Rust broker owns one in-flight request for each of these lanes.
// These callback commands are not product commands with durable replay receipts.
enum _NativeHostKind { calendar, attention, personal }

final class AppWireNativeContextHostTransport
    implements NativeContextHostTransport {
  AppWireNativeContextHostTransport(this._transport);
  final NativeHostWireTransport _transport;
  final _hosts = {
    for (final kind in _NativeHostKind.values) kind: _NativeHostState(),
  };

  @override
  Future<NativeHostRegistration> registerAcquisitionHost() =>
      _register(_NativeHostKind.calendar);

  @override
  Future<List<Map<String, dynamic>>> pollAcquisitions({
    required NativeHostRegistration registration,
  }) => _poll(_NativeHostKind.calendar, registration);

  @override
  Future<void> completeAcquisition({
    required NativeHostRegistration registration,
    required Map<String, dynamic> result,
  }) => _complete(_NativeHostKind.calendar, registration, result);

  @override
  Future<void> failAcquisition({
    required NativeHostRegistration registration,
    required String requestId,
    required String failure,
  }) => _fail(_NativeHostKind.calendar, registration, requestId, failure);

  @override
  Future<void> disposeAcquisitionHost({
    required NativeHostRegistration registration,
  }) => _dispose(_NativeHostKind.calendar, registration);

  @override
  Future<NativeHostRegistration> registerAttentionHost() =>
      _register(_NativeHostKind.attention);

  @override
  Future<List<Map<String, dynamic>>> pollAttentionAcquisitions({
    required NativeHostRegistration registration,
  }) => _poll(_NativeHostKind.attention, registration);

  @override
  Future<void> completeAttentionAcquisition({
    required NativeHostRegistration registration,
    required Map<String, dynamic> result,
  }) => _complete(_NativeHostKind.attention, registration, result);

  @override
  Future<void> failAttentionAcquisition({
    required NativeHostRegistration registration,
    required String requestId,
    required String failure,
  }) => _fail(_NativeHostKind.attention, registration, requestId, failure);

  @override
  Future<void> disposeAttentionHost({
    required NativeHostRegistration registration,
  }) => _dispose(_NativeHostKind.attention, registration);

  @override
  Future<NativeHostRegistration> registerPersonalHost() =>
      _register(_NativeHostKind.personal);

  @override
  Future<List<Map<String, dynamic>>> pollPersonalAcquisitions({
    required NativeHostRegistration registration,
  }) => _poll(_NativeHostKind.personal, registration);

  @override
  Future<void> completePersonalAcquisition({
    required NativeHostRegistration registration,
    required Map<String, dynamic> result,
  }) => _complete(_NativeHostKind.personal, registration, result);

  @override
  Future<void> failPersonalAcquisition({
    required NativeHostRegistration registration,
    required String requestId,
    required String failure,
  }) => _fail(_NativeHostKind.personal, registration, requestId, failure);

  @override
  Future<void> disposePersonalHost({
    required NativeHostRegistration registration,
  }) => _dispose(_NativeHostKind.personal, registration);

  Future<NativeHostRegistration> _register(_NativeHostKind kind) {
    final state = _hosts[kind]!;
    // Concurrent starts share one host replacement instead of racing epochs.
    return state.registering ??= _registerOnce(kind, state);
  }

  Future<NativeHostRegistration> _registerOnce(
    _NativeHostKind kind,
    _NativeHostState state,
  ) async {
    try {
      final value = await _call(kind, 'register', {});
      final registration = NativeHostRegistration.fromJson(
        value['registration'],
      );
      state.registration = registration;
      state.request = null;
      return registration;
    } finally {
      state.registering = null;
    }
  }

  Future<List<Map<String, dynamic>>> _poll(
    _NativeHostKind kind,
    NativeHostRegistration registration,
  ) async {
    final state = _requireRegistration(kind, registration);
    final value = await _call(kind, 'poll', {
      'registration': registration.toJson(),
    }, query: true);
    _requireRegistration(kind, registration);
    final entries = value['acquisitions'];
    if (value['kind'] != '${kind.name}_acquisitions' ||
        value.length != 2 ||
        entries is! List ||
        entries.length > 1) {
      throw const FormatException('Invalid native poll reply.');
    }
    // Empty means either idle OR an existing in-flight request; it cannot
    // resolve a lost completion acknowledgement.
    if (entries.isEmpty) return const [];
    final request = _object(entries.single);
    final binding = _NativeRequestBinding.fromRequest(registration, request);
    final old = state.request;
    if (old != null &&
        old.requestId == binding.requestId &&
        (old.personId != binding.personId ||
            old.deviceId != binding.deviceId)) {
      throw const FormatException('Native request identity changed.');
    }
    // A different owner-issued job proves the previous single in-flight slot
    // has retired. Keep only identity, never an acquisition/result payload.
    state.request = binding;
    return [request];
  }

  Future<void> _complete(
    _NativeHostKind kind,
    NativeHostRegistration registration,
    Map<String, dynamic> result,
  ) async {
    final state = _requireRegistration(kind, registration);
    final binding = state.request;
    if (binding == null ||
        result['request_id'] != binding.requestId ||
        result['person_id'] != binding.personId ||
        result['device_id'] != binding.deviceId ||
        result['host_epoch'] != registration.hostEpoch) {
      throw const FormatException('Foreign native completion.');
    }
    await _call(kind, 'complete', {
      'registration': registration.toJson(),
      'result': Map<String, dynamic>.from(result)
        ..remove('person_id')
        ..remove('device_id'),
    });
    if (identical(state.request, binding)) state.request = null;
  }

  Future<void> _fail(
    _NativeHostKind kind,
    NativeHostRegistration registration,
    String requestId,
    String failure,
  ) async {
    final state = _requireRegistration(kind, registration);
    final binding = state.request;
    if (binding == null || binding.requestId != requestId) {
      throw const FormatException('Foreign native failure.');
    }
    await _call(kind, 'fail', {
      'registration': registration.toJson(),
      'request_id': requestId,
      'failure': failure,
    });
    if (identical(state.request, binding)) state.request = null;
  }

  Future<void> _dispose(
    _NativeHostKind kind,
    NativeHostRegistration registration,
  ) async {
    final state = _requireRegistration(kind, registration);
    await _call(kind, 'dispose', {'registration': registration.toJson()});
    // A delayed old acknowledgement cannot retire a replacement registration.
    if (_sameRegistration(state.registration, registration)) {
      state.registration = null;
      state.request = null;
    }
  }

  _NativeHostState _requireRegistration(
    _NativeHostKind kind,
    NativeHostRegistration registration,
  ) {
    final state = _hosts[kind]!;
    if (!_sameRegistration(state.registration, registration)) {
      throw StateError('Native callback registration is not current.');
    }
    return state;
  }

  static bool _sameRegistration(
    NativeHostRegistration? current,
    NativeHostRegistration supplied,
  ) =>
      current != null &&
      current.registrationId == supplied.registrationId &&
      current.hostEpoch == supplied.hostEpoch &&
      current.runtimeEpoch == supplied.runtimeEpoch;

  Future<Map<String, dynamic>> _call(
    _NativeHostKind kind,
    String operation,
    Map<String, Object?> fields, {
    bool query = false,
  }) async {
    final request = <String, dynamic>{
      'schema_version': appWireProtocolVersion,
      'request_id': newAgentRequestId(),
      // The native lane does not consume command_id as a replay key. Its
      // authority is the exact registration and outstanding broker request ID.
      // Never retain raw native data in a product-style command identity map.
      if (!query) 'command_id': newAgentRequestId(),
      query ? 'query' : 'command': {
        'kind': 'native_host.${kind.name}.$operation',
        ...fields,
      },
    };
    final result = query
        ? await _transport.queryV2(request)
        : await _transport.commandV2(request);
    if (!query) {
      if (operation == 'register') {
        if (result['kind'] != 'registered' || result.length != 2) {
          throw const FormatException(
            'Invalid native registration acknowledgement.',
          );
        }
        NativeHostRegistration.fromJson(result['registration']);
      } else if (result['kind'] != 'acknowledged' || result.length != 1) {
        throw const FormatException('Invalid native command acknowledgement.');
      }
    }
    return result;
  }
}

final class _NativeHostState {
  NativeHostRegistration? registration;
  Future<NativeHostRegistration>? registering;
  _NativeRequestBinding? request;
}

final class _NativeRequestBinding {
  const _NativeRequestBinding(this.requestId, this.personId, this.deviceId);

  factory _NativeRequestBinding.fromRequest(
    NativeHostRegistration registration,
    Map<String, dynamic> request,
  ) {
    final id = request['request_id'];
    final person = request['person_id'];
    final device = request['device_id'];
    final uuid = RegExp(
      r'^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$',
    );
    if (id is! String ||
        !uuid.hasMatch(id) ||
        person is! String ||
        !uuid.hasMatch(person) ||
        device is! String ||
        device.isEmpty ||
        device.length > 128 ||
        request['host_epoch'] != registration.hostEpoch) {
      throw const FormatException('Invalid admitted native request identity.');
    }
    return _NativeRequestBinding(id, person, device);
  }

  final String requestId;
  final String personId;
  final String deviceId;
}
