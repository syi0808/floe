import 'dart:async';
import 'dart:math';

import 'package:flutter/services.dart';

import 'native_transport.dart';

typedef AttentionAcquisitionReader = Future<Map<String, dynamic>> Function(
  Map<String, dynamic> request,
);

final class AttentionAcquisitionBroker {
  AttentionAcquisitionBroker({
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

  Future<void> start() async {
    _ensureOpen();
    if (_started) return;
    await _transport.registerAttentionHost(
      personId: _personId,
      hostEpoch: _hostEpoch,
    );
    _started = true;
  }

  Future<bool> pollAndComplete(AttentionAcquisitionReader reader) async {
    _ensureOpen();
    await start();
    final requests = await _transport.pollAttentionAcquisitions(
      personId: _personId,
      hostEpoch: _hostEpoch,
    );
    if (requests.isEmpty) return false;
    if (requests.length != 1) {
      throw const FormatException(
        'Attention acquisition returned multiple jobs.',
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
      await _transport.failAttentionAcquisition(
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
      await _transport.completeAttentionAcquisition(
        personId: _personId,
        hostEpoch: _hostEpoch,
        result: Map.unmodifiable(result),
      );
    } on Object catch (error) {
      if (!_disposed) {
        await _transport.failAttentionAcquisition(
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
      await _transport.disposeAttentionHost(
        personId: _personId,
        hostEpoch: _hostEpoch,
      );
    }
  }

  void _ensureOpen() {
    if (_disposed)
      throw StateError('Attention acquisition broker is disposed.');
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
      'mode',
      'native_subject_fingerprint_before',
      'native_subject_fingerprint_after',
      'permission_class',
      'view',
    };
    if (result.keys.toSet().difference(fields).isNotEmpty ||
        !result.keys.toSet().containsAll(fields) ||
        result['request_id'] != request['request_id'] ||
        result['host_epoch'] != request['host_epoch'] ||
        result['person_id'] != request['person_id'] ||
        result['device_id'] != request['device_id'] ||
        result['mode'] != request['mode']) {
      throw const FormatException('Attention result identity changed.');
    }
    final before = result['native_subject_fingerprint_before'];
    final after = result['native_subject_fingerprint_after'];
    if (!_validFingerprint(before) || before != after) {
      throw const FormatException('Invalid Attention subject evidence.');
    }
    if (request['mode'] == 'read_projection' &&
        before != request['expected_native_subject_fingerprint']) {
      throw const FormatException('Attention subject changed before read.');
    }
    final permission = result['permission_class'];
    if (permission is! String || permission.isEmpty || permission.length > 64) {
      throw const FormatException('Invalid Attention permission evidence.');
    }
    if (request['mode'] == 'inspect_subject' && result['view'] != null ||
        request['mode'] == 'read_projection' && result['view'] is! Map) {
      throw const FormatException('Invalid Attention projection.');
    }
  }

  void _validateRequest(Map<String, dynamic> request) {
    const required = {
      'request_id',
      'host_epoch',
      'person_id',
      'device_id',
      'mode',
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
        !_validOpaque(request['request_id'], maximum: 128) ||
        !_validOpaque(request['host_epoch'], maximum: 128) ||
        !_validOpaque(request['device_id'], maximum: 128) ||
        request['mode'] is! String ||
        !{'inspect_subject', 'read_projection'}.contains(request['mode']) ||
        request['deadline_unix_ms'] is! int ||
        (request['deadline_unix_ms']! as int) <=
            DateTime.now().toUtc().millisecondsSinceEpoch ||
        request['expected_native_subject_fingerprint'] != null &&
            !_validFingerprint(
              request['expected_native_subject_fingerprint'],
            ) ||
        request['mode'] == 'read_projection' &&
            !_validFingerprint(
              request['expected_native_subject_fingerprint'],
            )) {
      throw const FormatException('Invalid Attention acquisition request.');
    }
  }

  static String _failureCode(Object error) {
    final code = error is PlatformException ? error.code : '';
    return const {
          'permission_denied',
          'attention_unavailable',
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

  static bool _validOpaque(Object? value, {required int maximum}) =>
      value is String &&
      value.isNotEmpty &&
      value.length <= maximum &&
      !value.contains(RegExp(r'\s'));

  static String _newEpoch() {
    final random = Random.secure();
    return List<String>.generate(
      32,
      (_) => random.nextInt(16).toRadixString(16),
    ).join();
  }
}

final class AttentionAcquisitionService {
  AttentionAcquisitionService({
    required AttentionAcquisitionBroker broker,
    required AttentionAcquisitionReader reader,
    Duration pollInterval = const Duration(milliseconds: 100),
  }) : _broker = broker,
       _reader = reader,
       _pollInterval = pollInterval;

  final AttentionAcquisitionBroker _broker;
  final AttentionAcquisitionReader _reader;
  final Duration _pollInterval;
  Timer? _timer;
  bool _polling = false;
  bool _disposed = false;
  int _pollFailures = 0;
  DateTime _retryNotBefore = DateTime.fromMillisecondsSinceEpoch(0);

  Future<void> start() async {
    if (_disposed)
      throw StateError('Attention acquisition service is disposed.');
    if (_timer != null) return;
    await _broker.start();
    _timer = Timer.periodic(_pollInterval, (_) => unawaited(_pollOnce()));
    await _pollOnce();
  }

  Future<void> _pollOnce() async {
    if (_disposed || _polling || DateTime.now().isBefore(_retryNotBefore)) {
      return;
    }
    _polling = true;
    try {
      await _broker.pollAndComplete(_reader);
      _pollFailures = 0;
      _retryNotBefore = DateTime.fromMillisecondsSinceEpoch(0);
    } on Object {
      _pollFailures = (_pollFailures + 1).clamp(1, 6);
      final delay = _pollInterval * (1 << (_pollFailures - 1));
      _retryNotBefore = DateTime.now().add(
        delay > const Duration(seconds: 2) ? const Duration(seconds: 2) : delay,
      );
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
