import 'package:floe_client/app/runtime/owner_failure.dart';

const appWireProtocolVersion = 2;

/// AppWire command disposition shared by product and Runtime callers without
/// exposing which transport implementation decoded the envelope.
enum CommandOutcome { notApplied, notAdmitted, admitted, indeterminate }

/// AppWire boundary failure shared by all transport implementations.
final class AppWireTransportException implements Exception {
  const AppWireTransportException(
    this.code,
    this.message, {
    this.field,
    this.metadata = const {},
    this.ownerFailure,
    this.commandOutcome,
  });

  final String code;
  final String message;
  final String? field;
  final Map<String, String> metadata;
  final OwnerFailure? ownerFailure;

  /// Null for open, query, event and native-callback failures.
  final CommandOutcome? commandOutcome;

  factory AppWireTransportException.fromEnvelope(
    Map<String, dynamic> envelope, {
    CommandOutcome? commandOutcome,
  }) {
    final rawError = envelope['error'];
    final error = rawError is Map
        ? rawError.map((key, value) => MapEntry(key.toString(), value))
        : envelope;
    return AppWireTransportException(
      error['code']?.toString() ?? 'internal',
      error['message']?.toString() ?? 'Could not open Rust core.',
      field: error['field']?.toString(),
      commandOutcome: commandOutcome,
      ownerFailure: error['owner_failure'] == null
          ? null
          : OwnerFailure.fromJson(error['owner_failure']),
      metadata: error['metadata'] is Map
          ? Map<String, String>.unmodifiable(
              (error['metadata'] as Map).map(
                (key, value) => MapEntry(key.toString(), value.toString()),
              ),
            )
          : const {},
    );
  }

  @override
  String toString() => message;
}

/// Whether an uncertain command identity can be discarded after a failure.
///
/// Command recovery follows the AppWire disposition. Owner reason codes do not
/// prove whether the command was admitted or committed.
bool mayDiscardPendingCommand(
  AppWireTransportException error, {
  required bool previouslySubmitted,
}) {
  return error.commandOutcome == CommandOutcome.notApplied ||
      (!previouslySubmitted &&
          error.commandOutcome == CommandOutcome.notAdmitted);
}

abstract interface class AppWireTransport {
  Future<Map<String, dynamic>> commandV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  });

  Future<Map<String, dynamic>> queryV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  });

  Future<Map<String, dynamic>> eventsV2(
    Map<String, dynamic> request, {
    Duration timeout = const Duration(seconds: 3),
  });

  Future<void> close();
}
