import 'dart:io';
import 'dart:math';

import 'package:flutter/services.dart';
import 'package:path_provider/path_provider.dart';

import '../../features/agent/agent_connections.dart';
import 'apple_context_gateway.dart';
import 'macos_context_gateway.dart';
import 'native_transport.dart';

const _peopleViewId = 'people.identity';
const _feasibilityViewId = 'schedule.feasibility';
const _attentionViewId = 'attention.coarse';
const _wellbeingViewId = 'wellbeing.derived';

final class LocalDeviceIdentity {
  const LocalDeviceIdentity(this.id);

  final String id;

  static Future<LocalDeviceIdentity> openDefault() async {
    final supportDirectory = await getApplicationSupportDirectory();
    final file = File('${supportDirectory.path}/local_device_id');
    if (await file.exists()) {
      final existing = (await file.readAsString()).trim();
      if (_validIdentifier(existing)) return LocalDeviceIdentity(existing);
    }
    final identity = LocalDeviceIdentity('local-${_uuid()}');
    await file.parent.create(recursive: true);
    final temporary = File('${file.path}.tmp');
    await temporary.writeAsString(identity.id, flush: true);
    await temporary.rename(file.path);
    return identity;
  }
}

final class PublishingAppleContextGateway implements AppleContextApi {
  factory PublishingAppleContextGateway({
    required AppleContextApi gateway,
    required LocalContextTransport transport,
    required String personId,
    required String deviceId,
    DateTime Function()? clock,
  }) => PublishingAppleContextGateway._(
    gateway,
    transport,
    personId,
    deviceId,
    clock ?? DateTime.now,
  );

  PublishingAppleContextGateway._(
    this._gateway,
    this._transport,
    this._personId,
    this._deviceId,
    this._clock,
  );

  final AppleContextApi _gateway;
  final LocalContextTransport _transport;
  final DateTime Function() _clock;
  final String _deviceId;
  String? _personId;

  @override
  Future<List<Map<String, dynamic>>> connections() async {
    final values = await _gateway.connections();
    for (final value in values) {
      final connection = AgentConnection.fromJson(value);
      if ({
        AgentConnectionState.disconnected,
        AgentConnectionState.revoked,
        AgentConnectionState.unsupported,
        AgentConnectionState.unavailable,
      }.contains(connection.state)) {
        for (final view in connection.descriptor.views) {
          await _revoke(view.id);
        }
      }
    }
    return values;
  }

  @override
  Future<bool> requestPermission(AppleContextSource source) async {
    final granted = await _gateway.requestPermission(source);
    if (!granted) {
      await _revoke(switch (source) {
        AppleContextSource.contacts => _peopleViewId,
        AppleContextSource.health => _wellbeingViewId,
      });
    }
    return granted;
  }

  @override
  Future<Map<String, dynamic>> readContacts({int limit = 64}) async {
    final view = await _readNative(
      _peopleViewId,
      () => _gateway.readContacts(limit: limit),
    );
    return _publishValidated(view, _peopleViewId, validateApplePeopleView);
  }

  @override
  Future<Map<String, dynamic>> readFeasibility(
    AppleFeasibilityQuery query,
  ) async {
    final result = await _readNative(
      _feasibilityViewId,
      () => _gateway.readFeasibility(query),
    );
    try {
      validateAppleFeasibilityResult(result);
      final view = _map(result['view']);
      await _publishFresh(view, _feasibilityViewId);
      return result;
    } on Object {
      await _revoke(_feasibilityViewId);
      rethrow;
    }
  }

  @override
  Future<Map<String, dynamic>> readWellbeing() async {
    final view = await _readNative(_wellbeingViewId, _gateway.readWellbeing);
    try {
      validateAppleWellbeingView(view);
      if (view['capacity'] == 'unknown' && view['recovery'] == 'unknown') {
        await _revoke(_wellbeingViewId);
        return view;
      }
      await _publishFresh(view, _wellbeingViewId);
      return view;
    } on Object {
      await _revoke(_wellbeingViewId);
      rethrow;
    }
  }

  @override
  Future<Map<String, dynamic>> screenTimeCapability() async {
    final capability = await _gateway.screenTimeCapability();
    try {
      validateAppleScreenTimeCapability(capability);
      if (capability['outcome'] != 'supported') {
        await _revoke(_attentionViewId);
      }
      return capability;
    } on Object {
      await _revoke(_attentionViewId);
      rethrow;
    }
  }

  Future<void> bindPerson(String personId) async {
    final previous = _personId;
    if (previous == personId) return;
    if (previous != null) {
      await _transport.revokeLocalContext(
        personId: previous,
        deviceId: _deviceId,
      );
    }
    _personId = personId;
  }

  Future<void> logout() async {
    final personId = _personId;
    if (personId == null) return;
    await _transport.revokeLocalContext(
      personId: personId,
      deviceId: _deviceId,
    );
    _personId = null;
  }

  Future<Map<String, dynamic>> _publishValidated(
    Map<String, dynamic> view,
    String viewId,
    void Function(Map<String, dynamic>) validate,
  ) async {
    try {
      validate(view);
      await _publishFresh(view, viewId);
      return view;
    } on Object {
      await _revoke(viewId);
      rethrow;
    }
  }

  Future<T> _readNative<T>(String viewId, Future<T> Function() read) async {
    try {
      return await read();
    } on PlatformException catch (error) {
      if ({'permission_denied', 'authorization_denied'}.contains(error.code)) {
        await _revoke(viewId);
      }
      rethrow;
    }
  }

  Future<void> _publishFresh(Map<String, dynamic> view, String viewId) async {
    if (view['view_id'] != viewId ||
        (view['expires_at_unix_ms'] as int) <=
            _clock().toUtc().millisecondsSinceEpoch) {
      throw const FormatException('Cannot publish stale local context.');
    }
    final personId = _personId;
    if (personId == null) {
      throw StateError('No Person is bound to local context publication.');
    }
    await _transport.publishLocalContext(
      personId: personId,
      deviceId: _deviceId,
      view: view,
    );
  }

  Future<void> _revoke(String viewId) async {
    final personId = _personId;
    if (personId == null) return;
    await _transport.revokeLocalContext(
      personId: personId,
      deviceId: _deviceId,
      viewId: viewId,
    );
  }
}

final class PublishingMacOSContextGateway {
  factory PublishingMacOSContextGateway({
    required MacOSContextApi gateway,
    required LocalContextTransport transport,
    required String personId,
    required String deviceId,
    DateTime Function()? clock,
  }) => PublishingMacOSContextGateway._(
    gateway,
    transport,
    personId,
    deviceId,
    clock ?? DateTime.now,
  );

  PublishingMacOSContextGateway._(
    this._gateway,
    this._transport,
    this._personId,
    this._deviceId,
    this._clock,
  );

  final MacOSContextApi _gateway;
  final LocalContextTransport _transport;
  final DateTime Function() _clock;
  final String _deviceId;
  String? _personId;

  Future<Map<String, dynamic>> readAttention() async {
    final view = await _gateway.readAttention();
    try {
      validateMacOSAttentionView(view);
      final personId = _personId;
      if (personId == null) {
        throw StateError('No Person is bound to local context publication.');
      }
      if (view['state'] == 'unknown' ||
          (view['expires_at_unix_ms'] as int) <=
              _clock().toUtc().millisecondsSinceEpoch) {
        await _revoke();
      } else {
        await _transport.publishLocalContext(
          personId: personId,
          deviceId: _deviceId,
          view: view,
        );
      }
      return view;
    } on Object {
      await _revoke();
      rethrow;
    }
  }

  Future<void> bindPerson(String personId) async {
    final previous = _personId;
    if (previous == personId) return;
    if (previous != null) {
      await _transport.revokeLocalContext(
        personId: previous,
        deviceId: _deviceId,
      );
    }
    _personId = personId;
  }

  Future<void> logout() async {
    final personId = _personId;
    if (personId == null) return;
    await _transport.revokeLocalContext(
      personId: personId,
      deviceId: _deviceId,
    );
    _personId = null;
  }

  Future<void> _revoke() async {
    final personId = _personId;
    if (personId == null) return;
    await _transport.revokeLocalContext(
      personId: personId,
      deviceId: _deviceId,
      viewId: _attentionViewId,
    );
  }
}

Map<String, dynamic> _map(Object? value) {
  if (value is! Map) throw const FormatException('Expected a View map.');
  return Map<String, dynamic>.from(value);
}

bool _validIdentifier(String value) =>
    value.isNotEmpty && value.length <= 128 && !value.contains(RegExp(r'\s'));

String _uuid() {
  final random = Random.secure();
  final bytes = List.generate(16, (_) => random.nextInt(256));
  bytes[6] = (bytes[6] & 15) | 64;
  bytes[8] = (bytes[8] & 63) | 128;
  final hex = bytes
      .map((byte) => byte.toRadixString(16).padLeft(2, '0'))
      .join();
  return '${hex.substring(0, 8)}-${hex.substring(8, 12)}-${hex.substring(12, 16)}-${hex.substring(16, 20)}-${hex.substring(20)}';
}
