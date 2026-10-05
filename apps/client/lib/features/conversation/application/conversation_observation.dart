import 'dart:async';

/// Stops client reads/adoption only. It has no Run cancellation capability.
final class ConversationObservation {
  bool _stopped = false;
  final Set<void Function()> _listeners = {};

  bool get stopped => _stopped;

  void check() {
    if (_stopped) throw const ConversationObservationStopped();
  }

  void stop() {
    if (_stopped) return;
    _stopped = true;
    final listeners = _listeners.toList(growable: false);
    _listeners.clear();
    for (final listener in listeners) {
      listener();
    }
  }

  /// Detach from a read without cancelling or reinterpreting underlying work.
  Future<T> read<T>(Future<T> Function() operation) {
    check();
    final result = Completer<T>();
    void onStop() {
      if (!result.isCompleted) {
        result.completeError(const ConversationObservationStopped());
      }
    }

    _listeners.add(onStop);
    Future<T>.sync(operation).then<void>(
      (value) {
        _listeners.remove(onStop);
        if (!result.isCompleted) result.complete(value);
      },
      onError: (Object error, StackTrace stack) {
        _listeners.remove(onStop);
        if (!result.isCompleted) result.completeError(error, stack);
      },
    );
    return result.future;
  }
}

final class ConversationObservationStopped implements Exception {
  const ConversationObservationStopped();
}
