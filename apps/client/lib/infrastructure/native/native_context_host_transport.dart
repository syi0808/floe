import 'dart:convert';
import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/features/conversation/application/agent_request_id.dart';

/// Dedicated callback lane, sharing the verified Rust host without sharing its
/// product handle or blocking product request execution.
abstract interface class NativeHostWireTransport {
  Future<Map<String, dynamic>> commandV2(Map<String, dynamic> request, {Duration timeout = const Duration(seconds: 3)});
  Future<Map<String, dynamic>> queryV2(Map<String, dynamic> request, {Duration timeout = const Duration(seconds: 3)});
}

/// Internal native callback capability. Product feature interfaces never expose it.
final class NativeHostRegistration {
  const NativeHostRegistration._(this.registrationId, this.hostEpoch, this.runtimeEpoch);
  factory NativeHostRegistration.fromJson(Object? value) {
    final fields = _object(value);
    if (fields.length != 3 || !fields.keys.toSet().containsAll({'registration_id','host_epoch','runtime_epoch'}) ||
        fields['registration_id'] is! String ||
        !RegExp(r'^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$').hasMatch(fields['registration_id'] as String) ||
        fields['host_epoch'] is! String || (fields['host_epoch'] as String).isEmpty ||
        fields['runtime_epoch'] is! int || (fields['runtime_epoch'] as int) < 1) {
      throw const FormatException('Invalid native host registration.');
    }
    return NativeHostRegistration._(fields['registration_id'] as String, fields['host_epoch'] as String, fields['runtime_epoch'] as int);
  }
  final String registrationId;
  final String hostEpoch;
  final int runtimeEpoch;
  Map<String,Object?> toJson() => {'registration_id':registrationId,'host_epoch':hostEpoch,'runtime_epoch':runtimeEpoch};
}

Map<String,dynamic> _object(Object? value) {
  if (value is! Map || value.keys.any((key) => key is! String)) throw const FormatException('Invalid native host object.');
  return Map<String,dynamic>.from(value);
}

abstract interface class NativeContextHostTransport {
  Future<NativeHostRegistration> registerAcquisitionHost();
  Future<List<Map<String,dynamic>>> pollAcquisitions({required NativeHostRegistration registration});
  Future<void> completeAcquisition({required NativeHostRegistration registration, required Map<String,dynamic> result});
  Future<void> failAcquisition({required NativeHostRegistration registration, required String requestId, required String failure});
  Future<void> disposeAcquisitionHost({required NativeHostRegistration registration});
  Future<NativeHostRegistration> registerAttentionHost();
  Future<List<Map<String,dynamic>>> pollAttentionAcquisitions({required NativeHostRegistration registration});
  Future<void> completeAttentionAcquisition({required NativeHostRegistration registration, required Map<String,dynamic> result});
  Future<void> failAttentionAcquisition({required NativeHostRegistration registration, required String requestId, required String failure});
  Future<void> disposeAttentionHost({required NativeHostRegistration registration});
  Future<NativeHostRegistration> registerPersonalHost();
  Future<List<Map<String,dynamic>>> pollPersonalAcquisitions({required NativeHostRegistration registration});
  Future<void> completePersonalAcquisition({required NativeHostRegistration registration, required Map<String,dynamic> result});
  Future<void> failPersonalAcquisition({required NativeHostRegistration registration, required String requestId, required String failure});
  Future<void> disposePersonalHost({required NativeHostRegistration registration});
}

final class AppWireNativeContextHostTransport implements NativeContextHostTransport {
  AppWireNativeContextHostTransport(this._transport);
  final NativeHostWireTransport _transport;
  final Map<String,String> _pendingCommands = {};
  final Map<String, _NativeRequestBinding> _requests = {};

  @override
  Future<NativeHostRegistration> registerAcquisitionHost() async {
    final value = await _call('native_host.calendar.register', {}, query: false);
    if (value['kind'] != 'registered' || value.length != 2) throw const FormatException('Invalid native registration reply.');
    return NativeHostRegistration.fromJson(value['registration']);
  }

  @override
  Future<List<Map<String,dynamic>>> pollAcquisitions({required NativeHostRegistration registration}) async {
    final value = await _call('native_host.calendar.poll', {'registration': registration.toJson()}, query: true);
    if (value['kind'] != 'calendar_acquisitions' || value.length != 2 || value['acquisitions'] is! List) throw const FormatException('Invalid native poll reply.');
    return (value['acquisitions'] as List).map((entry) {
      final request = _object(entry);
      _remember(registration, request);
      return request;
    }).toList(growable: false);
  }

  @override
  Future<void> completeAcquisition({required NativeHostRegistration registration, required Map<String,dynamic> result}) async {
    final value = await _call('native_host.calendar.complete', {'registration': registration.toJson() , 'result': _completion(registration, result)}, query: false);
    if (value['kind'] != 'acknowledged' || value.length != 1) throw const FormatException('Invalid native acknowledgement.');
    _requests.remove(result['request_id']);
  }

  @override
  Future<void> failAcquisition({required NativeHostRegistration registration, required String requestId, required String failure}) async {
    final value = await _call('native_host.calendar.fail', {'registration': registration.toJson(), 'request_id': requestId, 'failure': failure}, query: false);
    if (value['kind'] != 'acknowledged' || value.length != 1) throw const FormatException('Invalid native acknowledgement.');
    _requests.remove(requestId);
  }

  @override
  Future<void> disposeAcquisitionHost({required NativeHostRegistration registration}) async {
    final value = await _call('native_host.calendar.dispose', {'registration': registration.toJson()}, query: false);
    if (value['kind'] != 'acknowledged' || value.length != 1) throw const FormatException('Invalid native acknowledgement.');
    _requests.removeWhere((_, value) => value.registrationId == registration.registrationId);
  }

  @override
  Future<NativeHostRegistration> registerAttentionHost() async {
    final value = await _call('native_host.attention.register', {}, query: false);
    if (value['kind'] != 'registered' || value.length != 2) throw const FormatException('Invalid native registration reply.');
    return NativeHostRegistration.fromJson(value['registration']);
  }

  @override
  Future<List<Map<String,dynamic>>> pollAttentionAcquisitions({required NativeHostRegistration registration}) async {
    final value = await _call('native_host.attention.poll', {'registration': registration.toJson()}, query: true);
    if (value['kind'] != 'attention_acquisitions' || value.length != 2 || value['acquisitions'] is! List) throw const FormatException('Invalid native poll reply.');
    return (value['acquisitions'] as List).map((entry) {
      final request = _object(entry);
      _remember(registration, request);
      return request;
    }).toList(growable: false);
  }

  @override
  Future<void> completeAttentionAcquisition({required NativeHostRegistration registration, required Map<String,dynamic> result}) async {
    final value = await _call('native_host.attention.complete', {'registration': registration.toJson() , 'result': _completion(registration, result)}, query: false);
    if (value['kind'] != 'acknowledged' || value.length != 1) throw const FormatException('Invalid native acknowledgement.');
    _requests.remove(result['request_id']);
  }

  @override
  Future<void> failAttentionAcquisition({required NativeHostRegistration registration, required String requestId, required String failure}) async {
    final value = await _call('native_host.attention.fail', {'registration': registration.toJson(), 'request_id': requestId, 'failure': failure}, query: false);
    if (value['kind'] != 'acknowledged' || value.length != 1) throw const FormatException('Invalid native acknowledgement.');
    _requests.remove(requestId);
  }

  @override
  Future<void> disposeAttentionHost({required NativeHostRegistration registration}) async {
    final value = await _call('native_host.attention.dispose', {'registration': registration.toJson()}, query: false);
    if (value['kind'] != 'acknowledged' || value.length != 1) throw const FormatException('Invalid native acknowledgement.');
    _requests.removeWhere((_, value) => value.registrationId == registration.registrationId);
  }

  @override
  Future<NativeHostRegistration> registerPersonalHost() async {
    final value = await _call('native_host.personal.register', {}, query: false);
    if (value['kind'] != 'registered' || value.length != 2) throw const FormatException('Invalid native registration reply.');
    return NativeHostRegistration.fromJson(value['registration']);
  }

  @override
  Future<List<Map<String,dynamic>>> pollPersonalAcquisitions({required NativeHostRegistration registration}) async {
    final value = await _call('native_host.personal.poll', {'registration': registration.toJson()}, query: true);
    if (value['kind'] != 'personal_acquisitions' || value.length != 2 || value['acquisitions'] is! List) throw const FormatException('Invalid native poll reply.');
    return (value['acquisitions'] as List).map((entry) {
      final request = _object(entry);
      _remember(registration, request);
      return request;
    }).toList(growable: false);
  }

  @override
  Future<void> completePersonalAcquisition({required NativeHostRegistration registration, required Map<String,dynamic> result}) async {
    final value = await _call('native_host.personal.complete', {'registration': registration.toJson() , 'result': _completion(registration, result)}, query: false);
    if (value['kind'] != 'acknowledged' || value.length != 1) throw const FormatException('Invalid native acknowledgement.');
    _requests.remove(result['request_id']);
  }

  @override
  Future<void> failPersonalAcquisition({required NativeHostRegistration registration, required String requestId, required String failure}) async {
    final value = await _call('native_host.personal.fail', {'registration': registration.toJson(), 'request_id': requestId, 'failure': failure}, query: false);
    if (value['kind'] != 'acknowledged' || value.length != 1) throw const FormatException('Invalid native acknowledgement.');
    _requests.remove(requestId);
  }

  @override
  Future<void> disposePersonalHost({required NativeHostRegistration registration}) async {
    final value = await _call('native_host.personal.dispose', {'registration': registration.toJson()}, query: false);
    if (value['kind'] != 'acknowledged' || value.length != 1) throw const FormatException('Invalid native acknowledgement.');
    _requests.removeWhere((_, value) => value.registrationId == registration.registrationId);
  }

  void _remember(NativeHostRegistration registration, Map<String,dynamic> request) {
    final id = request['request_id'];
    final person = request['person_id'];
    final device = request['device_id'];
    if (id is! String || person is! String || device is! String || device.isEmpty || device.length > 128 ||
        request['host_epoch'] != registration.hostEpoch ||
        !RegExp(r'^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$').hasMatch(person)) {
      throw const FormatException('Invalid admitted native request identity.');
    }
    final old = _requests[id];
    if (old != null && (old.registrationId != registration.registrationId || old.personId != person || old.deviceId != device)) {
      throw const FormatException('Native request identity changed.');
    }
    _requests[id] = _NativeRequestBinding(registration.registrationId, registration.hostEpoch, person, device);
  }

  Map<String,dynamic> _completion(NativeHostRegistration registration, Map<String,dynamic> value) {
    final expected = _requests[value['request_id']];
    if (expected == null || expected.registrationId != registration.registrationId ||
        value['person_id'] != expected.personId || value['device_id'] != expected.deviceId ||
        value['host_epoch'] != expected.hostEpoch) {
      throw const FormatException('Foreign native completion.');
    }
    return Map<String,dynamic>.from(value)..remove('person_id')..remove('device_id');
  }

  Future<Map<String,dynamic>> _call(String kind, Map<String,Object?> fields, {required bool query}) async {
    final input = <String,Object?>{'kind':kind,...fields};
    final identity = jsonEncode(input);
    final commandId = query ? null : _pendingCommands.putIfAbsent(identity, newAgentRequestId);
    final request = <String,dynamic>{
      'schema_version':appWireProtocolVersion,
      'request_id':newAgentRequestId(),
      if (commandId != null) 'command_id':commandId,
      query ? 'query' : 'command':input,
    };
    final result = query ? await _transport.queryV2(request) : await _transport.commandV2(request);
    if (!query) {
      if (kind.endsWith('.register')) {
        if (result['kind'] != 'registered' || result.length != 2) throw const FormatException('Invalid native registration acknowledgement.');
        NativeHostRegistration.fromJson(result['registration']);
      } else if (result['kind'] != 'acknowledged' || result.length != 1) {
        throw const FormatException('Invalid native command acknowledgement.');
      }
      _pendingCommands.remove(identity);
    }
    return result;
  }
}

final class _NativeRequestBinding {
  const _NativeRequestBinding(this.registrationId, this.hostEpoch, this.personId, this.deviceId);
  final String registrationId;
  final String hostEpoch;
  final String personId;
  final String deviceId;
}
