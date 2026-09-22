import 'dart:async';
import 'dart:math';

import 'package:flutter/services.dart';

import 'package:floe_client/app/runtime/native_transport.dart';

typedef CalendarAcquisitionReader = Future<Map<String, dynamic>> Function(
  Map<String, dynamic> request,
);

final class CalendarAcquisitionBroker {
  CalendarAcquisitionBroker({
    required this._transport,
    required this._personId,
    String? hostEpoch,
  }) : _hostEpoch = hostEpoch ?? _newEpoch();

  final LocalContextTransport _transport;
  final String _personId;
  final String _hostEpoch;
  bool _started = false;
  bool _disposed = false;

  String get hostEpoch => _hostEpoch;

  Future<void> start() async {
    _ensureOpen();
    if (_started) return;
    await _transport.registerAcquisitionHost(
      personId: _personId,
      hostEpoch: _hostEpoch,
    );
    _started = true;
  }

  Future<bool> pollAndComplete(CalendarAcquisitionReader reader) async {
    _ensureOpen();
    await start();
    final requests = await _transport.pollAcquisitions(
      personId: _personId,
      hostEpoch: _hostEpoch,
    );
    if (requests.isEmpty) return false;
    if (requests.length != 1) {
      throw const FormatException('Native acquisition returned multiple jobs.');
    }
    final request = _strictMap(requests.single);
    late final Map<String, dynamic> result;
    try {
      result = await reader(Map.unmodifiable(request));
    } on Object catch (error) {
      if (_disposed) return false;
      await _transport.failAcquisition(
        personId: _personId,
        hostEpoch: _hostEpoch,
        requestId: request['request_id']! as String,
        failure: _failureCode(error),
      );
      return true;
    }
    if (_disposed) return false;
    await _completeResult(request, result);
    return true;
  }

  Future<void> _completeResult(
    Map<String, dynamic> request,
    Map<String, dynamic> result,
  ) async {
    try {
      _validateResultIdentity(request, result);
    } on Object catch (error) {
      await _transport.failAcquisition(
        personId: _personId,
        hostEpoch: _hostEpoch,
        requestId: request['request_id']! as String,
        failure: _failureCode(error),
      );
      rethrow;
    }
    await _transport.completeAcquisition(
      personId: _personId,
      hostEpoch: _hostEpoch,
      result: Map.unmodifiable(result),
    );
  }

  Future<void> dispose() async {
    if (_disposed) return;
    _disposed = true;
    if (_started) {
      await _transport.disposeAcquisitionHost(
        personId: _personId,
        hostEpoch: _hostEpoch,
      );
    }
  }

  void _ensureOpen() {
    if (_disposed) throw StateError('Calendar acquisition broker is disposed.');
  }

  static void _validateResultIdentity(
    Map<String, dynamic> request,
    Map<String, dynamic> result,
  ) {
    const fields = {
      'request_id',
      'host_epoch',
      'person_id',
      'device_id',
      'connection_id',
      'connection_revision',
      'provider',
      'mode',
      'calendar_ids',
      'range_start_unix_ms',
      'range_end_unix_ms',
      'native_subject_fingerprint_before',
      'native_subject_fingerprint_after',
      'available_calendar_ids',
      'permission_class',
      'batches',
    };
    if (result.keys.toSet().difference(fields).isNotEmpty ||
        !result.keys.toSet().containsAll(fields)) {
      throw const FormatException('Invalid native acquisition result.');
    }
    for (final key in [
      'request_id',
      'host_epoch',
      'person_id',
      'device_id',
      'connection_id',
      'provider',
      'mode',
      'calendar_ids',
      'range_start_unix_ms',
      'range_end_unix_ms',
    ]) {
      if (!_deepEqual(request[key], result[key])) {
        throw const FormatException('Native acquisition identity changed.');
      }
    }
    if (request['connection_revision'] != result['connection_revision']) {
      throw const FormatException('Native acquisition generation changed.');
    }
    if (request['mode'] != 'inspect_subject' &&
        request['mode'] != 'read_events') {
      throw const FormatException('Invalid native acquisition mode.');
    }
    final before = result['native_subject_fingerprint_before'];
    final after = result['native_subject_fingerprint_after'];
    if (before is! String ||
        after is! String ||
        !_validFingerprint(before) ||
        before != after) {
      throw const FormatException('Invalid native subject evidence.');
    }
    if (request['mode'] == 'read_events' &&
        before != request['expected_native_subject_fingerprint']) {
      throw const FormatException('Native subject changed before acquisition.');
    }
    final available = result['available_calendar_ids'];
    if (available is! List ||
        available.any((value) => value is! String) ||
        !_sortedUnique(available.cast<String>()) ||
        !(request['calendar_ids'] as List).every(available.contains) ||
        result['permission_class'] is! String ||
        (result['permission_class'] as String).isEmpty ||
        (result['permission_class'] as String).length > 64 ||
        result['batches'] is! List ||
        (request['mode'] == 'read_events' &&
            (result['batches']! as List).length !=
                (request['calendar_ids']! as List).length) ||
        (request['mode'] == 'inspect_subject' &&
            (result['batches']! as List).isNotEmpty)) {
      throw const FormatException('Invalid native acquisition batches.');
    }
  }

  static String _failureCode(Object error) {
    final code = error is PlatformException ? error.code : '';
    if (code == 'stale_context') return 'permission_denied';
    return const {
          'permission_denied',
          'calendar_unavailable',
          'provider_unavailable',
        }.contains(code)
        ? code
        : 'provider_unavailable';
  }

  static void _ignore(Object error) {}

  static bool _validFingerprint(Object? value) =>
      value is String &&
      value.length == 64 &&
      RegExp(r'^[0-9a-f]{64}$').hasMatch(value);

  static bool _sortedUnique(List<String> values) {
    for (var index = 1; index < values.length; index++) {
      if (values[index - 1].compareTo(values[index]) >= 0) return false;
    }
    return true;
  }

  static Map<String, dynamic> _strictMap(Map<String, dynamic> value) => value;

  static bool _deepEqual(Object? left, Object? right) {
    if (left is List && right is List) {
      return left.length == right.length &&
          List.generate(
            left.length,
            (index) => _deepEqual(left[index], right[index]),
          ).every((value) => value);
    }
    return left == right;
  }

  static String _newEpoch() {
    final random = Random.secure();
    return List<String>.generate(
      32,
      (_) => random.nextInt(16).toRadixString(16),
    ).join();
  }
}

final class CalendarAcquisitionService {
  CalendarAcquisitionService({
    required this._broker,
    required this._reader,
    this._pollInterval = const Duration(milliseconds: 100),
  });

  final CalendarAcquisitionBroker _broker;
  final CalendarAcquisitionReader _reader;
  final Duration _pollInterval;
  Timer? _timer;
  bool _polling = false;
  bool _disposed = false;
  DateTime? _retryAt;
  Duration _backoff = const Duration(milliseconds: 100);

  Future<void> start() async {
    if (_disposed) {
      throw StateError('Calendar acquisition service is disposed.');
    }
    if (_timer != null) return;
    await _broker.start();
    _timer = Timer.periodic(_pollInterval, (_) => unawaited(_pollOnce()));
    await _pollOnce();
  }

  Future<void> _pollOnce() async {
    if (_disposed || _polling) return;
    final retryAt = _retryAt;
    if (retryAt != null && DateTime.now().isBefore(retryAt)) return;
    _polling = true;
    try {
      await _broker.pollAndComplete(_reader);
      _retryAt = null;
      _backoff = const Duration(milliseconds: 100);
    } on Object catch (error) {
      CalendarAcquisitionBroker._ignore(error);
      _retryAt = DateTime.now().add(_backoff);
      _backoff = Duration(
        milliseconds: (_backoff.inMilliseconds * 2).clamp(100, 2000),
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
