import 'dart:async';
import 'dart:math';

import 'package:flutter/services.dart';

import 'native_transport.dart';

typedef PersonalAcquisitionReader = Future<Map<String, dynamic>> Function(
  Map<String, dynamic> request,
);

final class PersonalAcquisitionBroker {
  PersonalAcquisitionBroker({
    required LocalContextTransport transport,
    required String personId,
    String? hostEpoch,
  }) : _transport = transport,
       _personId = personId,
       _hostEpoch = hostEpoch ?? _newEpoch();

  final LocalContextTransport _transport;
  final String _personId;
  final String _hostEpoch;
  bool _started = false;
  bool _disposed = false;

  String get hostEpoch => _hostEpoch;

  Future<void> start() async {
    _ensureOpen();
    if (_started) return;
    await _transport.registerPersonalHost(
      personId: _personId,
      hostEpoch: _hostEpoch,
    );
    _started = true;
  }

  Future<bool> pollAndComplete(PersonalAcquisitionReader reader) async {
    _ensureOpen();
    await start();
    final requests = await _transport.pollPersonalAcquisitions(
      personId: _personId,
      hostEpoch: _hostEpoch,
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
        personId: _personId,
        hostEpoch: _hostEpoch,
        requestId: requestId,
        failure: _failureCode(error),
      );
      return true;
    }
    if (_disposed) return false;
    try {
      _validateResult(request, result);
      await _transport.completePersonalAcquisition(
        personId: _personId,
        hostEpoch: _hostEpoch,
        result: Map.unmodifiable(result),
      );
    } on Object catch (error) {
      if (!_disposed) {
        await _transport.failPersonalAcquisition(
          personId: _personId,
          hostEpoch: _hostEpoch,
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
        personId: _personId,
        hostEpoch: _hostEpoch,
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
      'selected_handles',
      'event_handle',
      'evidence_handles',
      'destination_latitude',
      'destination_longitude',
      'event_start_unix_ms',
      'event_end_unix_ms',
      'travel_mode',
      'deadline_unix_ms',
    };
    const optional = {'expected_native_subject_fingerprint'};
    if (request.keys.toSet().difference({
          ...required,
          ...optional,
        }).isNotEmpty ||
        !request.keys.toSet().containsAll(required) ||
        request['person_id'] != _personId ||
        request['host_epoch'] != _hostEpoch ||
        !_validOpaque(request['request_id']) ||
        !_validOpaque(request['host_epoch']) ||
        !_validOpaque(request['device_id']) ||
        !{'people', 'wellbeing', 'feasibility'}.contains(request['domain']) ||
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
    final evidence = request['evidence_handles'];
    if (selected is! List ||
        evidence is! List ||
        selected.any((value) => !_validOpaque(value)) ||
        evidence.any((value) => !_validOpaque(value))) {
      throw const FormatException('Invalid personal acquisition handles.');
    }
    switch (request['domain']) {
      case 'people':
        if (selected.isEmpty ||
            evidence.isNotEmpty ||
            request['event_handle'] != null) {
          throw const FormatException('Invalid People acquisition request.');
        }
      case 'wellbeing':
        if (selected.isNotEmpty ||
            evidence.isNotEmpty ||
            request['event_handle'] != null) {
          throw const FormatException('Invalid Wellbeing acquisition request.');
        }
      case 'feasibility':
        if (selected.isNotEmpty ||
            evidence.isEmpty ||
            request['event_handle'] is! String ||
            request['destination_latitude'] is! num ||
            request['destination_longitude'] is! num ||
            request['event_start_unix_ms'] is! int ||
            request['event_end_unix_ms'] is! int ||
            request['travel_mode'] is! String) {
          throw const FormatException(
            'Invalid Feasibility acquisition request.',
          );
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
      'native_subject_fingerprint_before',
      'native_subject_fingerprint_after',
      'permission_class',
      'provider',
      'view',
    };
    if (result.keys.toSet().difference(fields).isNotEmpty ||
        !result.keys.toSet().containsAll(fields) ||
        const [
          'request_id',
          'host_epoch',
          'person_id',
          'device_id',
          'domain',
        ].any((key) => result[key] != request[key]) ||
        !_validFingerprint(result['native_subject_fingerprint_before']) ||
        result['native_subject_fingerprint_before'] !=
            result['native_subject_fingerprint_after'] ||
        result['permission_class'] is! String ||
        result['permission_class'] == '' ||
        result['provider'] is! String ||
        result['provider'] == '' ||
        result['view'] is! Map) {
      throw const FormatException('Invalid personal acquisition result.');
    }
    if (request['domain'] == 'people' &&
            (result['view']! as Map)['view_id'] != 'people.identity' ||
        request['domain'] == 'wellbeing' &&
            (result['view']! as Map)['view_id'] != 'wellbeing.derived' ||
        request['domain'] == 'feasibility' &&
            (result['view']! as Map)['view_id'] != 'schedule.feasibility') {
      throw const FormatException(
        'Personal acquisition view does not match domain.',
      );
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

  static String _newEpoch() {
    final random = Random.secure();
    return List<String>.generate(
      32,
      (_) => random.nextInt(16).toRadixString(16),
    ).join();
  }
}

final class PersonalAcquisitionService {
  PersonalAcquisitionService({
    required PersonalAcquisitionBroker broker,
    required PersonalAcquisitionReader reader,
    Duration pollInterval = const Duration(seconds: 30),
  }) : _broker = broker,
       _reader = reader,
       _pollInterval = pollInterval;

  final PersonalAcquisitionBroker _broker;
  final PersonalAcquisitionReader _reader;
  final Duration _pollInterval;
  Timer? _timer;
  bool _disposed = false;

  Future<void> start() async {
    if (_disposed)
      throw StateError('Personal acquisition service is disposed.');
    await _broker.start();
    _timer ??= Timer.periodic(_pollInterval, (_) => unawaited(_pollOnce()));
    await _pollOnce();
  }

  Future<void> _pollOnce() async {
    if (_disposed) return;
    try {
      await _broker.pollAndComplete(_reader);
    } on Object catch (_) {
      // The broker records a typed failure for a rejected native acquisition.
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
