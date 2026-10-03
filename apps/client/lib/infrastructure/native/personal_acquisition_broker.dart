import 'dart:async';

import 'package:flutter/services.dart';

import 'package:floe_client/infrastructure/native/native_context_host_transport.dart';

typedef PersonalAcquisitionReader = Future<Map<String, dynamic>> Function(
  Map<String, dynamic> request,
);

final class PersonalAcquisitionBroker {
  PersonalAcquisitionBroker({required NativeContextHostTransport transport})
      : _transport = transport;

  final NativeContextHostTransport _transport;
  NativeHostRegistration? _registration;
  String get _hostEpoch => _registration!.hostEpoch;
  bool _started = false;
  bool _disposed = false;

  String get hostEpoch => _hostEpoch;

  Future<void> start() async {
    _ensureOpen();
    if (_started) return;
    _registration = await _transport.registerPersonalHost(
    );
    if (_disposed) {
      await _transport.disposePersonalHost(registration: _registration!);
      throw StateError('Native host was detached during registration.');
    }
    _started = true;
  }

  Future<bool> pollAndComplete(PersonalAcquisitionReader reader) async {
    _ensureOpen();
    await start();
    final requests = await _transport.pollPersonalAcquisitions(
      registration: _registration!,
    );
    if (requests.isEmpty) return false;
    if (requests.length != 1) {
      throw const FormatException(
        'Personal acquisition returned multiple jobs.',
      );
    }
    final request = Map<String, dynamic>.unmodifiable(requests.single);
    _validateRequest(request);
    final requestId = request['request_id']! as String;
    late final Map<String, dynamic> result;
    try {
      result = await reader(request);
    } on Object catch (error) {
      if (_disposed) return false;
      await _transport.failPersonalAcquisition(
        registration: _registration!,
        requestId: requestId,
        failure: _failureCode(error),
      );
      return true;
    }
    if (_disposed) return false;
    try {
      _validateResult(request, result);
      await _transport.completePersonalAcquisition(
        registration: _registration!,
        result: Map.unmodifiable(result),
      );
    } on Object catch (error) {
      if (!_disposed) {
        await _transport.failPersonalAcquisition(
          registration: _registration!,
          requestId: requestId,
          failure: _failureCode(error),
        );
      }
      rethrow;
    }
    return true;
  }

  Future<void> dispose() async {
    if (_disposed) return;
    _disposed = true;
    if (_started) {
      await _transport.disposePersonalHost(
        registration: _registration!,
      );
    }
  }

  void _ensureOpen() {
    if (_disposed) throw StateError('Personal acquisition broker is disposed.');
  }

  void _validateRequest(Map<String, dynamic> request) {
    const required = {
      'request_id',
      'host_epoch',
      'person_id',
      'device_id',
      'domain',
      'mode',
      'selected_handles',
      'deadline_unix_ms',
    };
    const optional = {'expected_native_subject_fingerprint'};
    if (request.keys.toSet().difference({
          ...required,
          ...optional,
        }).isNotEmpty ||
        !request.keys.toSet().containsAll(required) ||
        !_validOpaque(request['person_id']) ||
        request['host_epoch'] != _hostEpoch ||
        !_validOpaque(request['request_id']) ||
        !_validOpaque(request['host_epoch']) ||
        !_validOpaque(request['device_id']) ||
        !{'people', 'wellbeing'}.contains(request['domain']) ||
        !{'read_projection','inspect_subject','inspect_catalog','request_permission'}.contains(request['mode']) ||
        request['deadline_unix_ms'] is! int ||
        (request['deadline_unix_ms']! as int) <=
            DateTime.now().toUtc().millisecondsSinceEpoch ||
        request['expected_native_subject_fingerprint'] != null &&
            !_validFingerprint(
              request['expected_native_subject_fingerprint'],
            )) {
      throw const FormatException('Invalid personal acquisition request.');
    }
    final selected = request['selected_handles'];
    if (selected is! List || selected.length > 64 || selected.toSet().length != selected.length || selected.any((value) => !_validOpaque(value))) {
      throw const FormatException('Invalid personal acquisition handles.');
    }
    switch (request['domain']) {
      case 'people':
        if ({'inspect_catalog','request_permission'}.contains(request['mode']) ? selected.isNotEmpty : selected.isEmpty) {
          throw const FormatException('Invalid People acquisition request.');
        }
      case 'wellbeing':
        if (selected.isNotEmpty) {
          throw const FormatException('Invalid Wellbeing acquisition request.');
        }
    }
  }

  static void _validateResult(
    Map<String, dynamic> request,
    Map<String, dynamic> result,
  ) {
    const fields = {
      'request_id',
      'host_epoch',
      'person_id',
      'device_id',
      'domain',
      'mode',
      'native_subject_fingerprint_before',
      'native_subject_fingerprint_after',
      'permission_class',
      'provider',
      'view',
      'transform_operation_id',
      'resources',
      'catalog_complete',
    };
    if (result.keys.toSet().difference(fields).isNotEmpty ||
        !result.keys.toSet().containsAll(fields) ||
        const [
          'request_id',
          'host_epoch',
          'person_id',
          'device_id',
          'domain',
          'mode',
        ].any((key) => result[key] != request[key]) ||
        !_validFingerprint(result['native_subject_fingerprint_before']) ||
        !_validFingerprint(result['native_subject_fingerprint_after']) ||
        request['mode'] != 'request_permission' && result['native_subject_fingerprint_before'] !=
            result['native_subject_fingerprint_after'] ||
        result['permission_class'] is! String ||
        result['permission_class'] == '' ||
        result['provider'] is! String ||
        result['provider'] == '') {
      throw const FormatException('Invalid personal acquisition result.');
    }
    final transform = result['transform_operation_id'];
    final resources = result['resources'];
    if (resources is! List || resources.length > 256 || result['catalog_complete'] is! bool) {
      throw const FormatException('Invalid native resource catalog.');
    }
    final mode = request['mode'];
    if (mode == 'inspect_catalog') {
      final handles = <String>{};
      for (final value in resources) {
        if (value is! Map || value.length != 2 || !_validOpaque(value['handle']) ||
            value['label'] is! String || (value['label'] as String).isEmpty ||
            !handles.add(value['handle'] as String)) {
          throw const FormatException('Invalid native resource metadata.');
        }
      }
    } else if (resources.isNotEmpty || result['catalog_complete'] != false) {
      throw const FormatException('Unexpected native catalog.');
    }
    if (mode == 'request_permission' && !{'request_completed','denied','unavailable'}.contains(result['permission_class'])) {
      throw const FormatException('Invalid permission outcome.');
    }
    if (mode != 'read_projection') {
      if (result['view'] != null || transform != null) throw const FormatException('Native inspection returned source data.');
      return;
    }
    if (result['view'] is! Map ||
        (request['domain'] == 'wellbeing'
          ? transform is! String || !RegExp(r'^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$').hasMatch(transform)
          : transform != null)) {
      throw const FormatException('Invalid native projection evidence.');
    }
    if (request['domain'] == 'people' && (result['view'] as Map)['view_id'] != 'people.identity' ||
        request['domain'] == 'wellbeing' && (result['view'] as Map)['view_id'] != 'wellbeing.derived') {
      throw const FormatException('Personal acquisition view does not match domain.');
    }
  }

  static String _failureCode(Object error) {
    final code = error is PlatformException ? error.code : '';
    return const {
          'permission_denied',
          'provider_unavailable',
          'cancelled',
        }.contains(code)
        ? code
        : 'provider_unavailable';
  }

  static bool _validFingerprint(Object? value) =>
      value is String &&
      value.length == 64 &&
      RegExp(r'^[0-9a-f]{64}$').hasMatch(value);

  static bool _validOpaque(Object? value) =>
      value is String &&
      value.isNotEmpty &&
      value.length <= 512 &&
      !value.contains(RegExp(r'\s'));


}

final class PersonalAcquisitionService {
  PersonalAcquisitionService({
    required PersonalAcquisitionBroker broker,
    required PersonalAcquisitionReader reader,
    Duration pollInterval = const Duration(milliseconds: 100),
  }) : _broker = broker, _reader = reader, _pollInterval = pollInterval;

  final PersonalAcquisitionBroker _broker;
  final PersonalAcquisitionReader _reader;
  final Duration _pollInterval;
  Timer? _timer;
  bool _disposed = false;
  bool _polling = false;

  Future<void> start() async {
    if (_disposed) {
      throw StateError('Personal acquisition service is disposed.');
    }
    await _broker.start();
    _timer ??= Timer.periodic(_pollInterval, (_) => unawaited(_pollOnce()));
    await _pollOnce();
  }

  Future<void> _pollOnce() async {
    if (_disposed || _polling) return;
    _polling = true;
    try {
      await _broker.pollAndComplete(_reader);
    } on Object catch (_) {
      // The broker records a typed failure for a rejected native acquisition.
    } finally {
      _polling = false;
    }
  }

  Future<void> dispose() async {
    if (_disposed) return;
    _disposed = true;
    _timer?.cancel();
    _timer = null;
    await _broker.dispose();
  }
}
