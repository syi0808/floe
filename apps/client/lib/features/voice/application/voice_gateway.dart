enum VoiceStartState { preparing, bound, cancelled }

enum VoiceMediaState { preparing, muted, capturing, stopped }

enum VoiceRemoteState {
  notStarted,
  dispatching,
  connected,
  closeRequested,
  closed,
  unknown,
}

final class VoiceStartStatus {
  const VoiceStartStatus({
    required this.commandId,
    required this.surfaceSessionId,
    required this.state,
    this.media,
    this.remote,
    this.stopRequested,
    this.localLeaseReleased,
  });

  final String commandId;
  final String surfaceSessionId;
  final VoiceStartState state;
  final VoiceMediaState? media;
  final VoiceRemoteState? remote;
  final bool? stopRequested;
  final bool? localLeaseReleased;
}

abstract interface class VoiceGateway {
  String? get pendingCommandId;

  /// Reads the one unresolved preparation or active call from Voice's durable
  /// journal and restores its exact command identity into this AppRuntime.
  Future<VoiceStartStatus?> recoverPendingStart();

  Future<VoiceStartStatus> start(String surfaceSessionId);

  /// Cancels only a pre-binding Start and uses the recovered command ID.
  Future<VoiceStartStatus> cancelPendingStart();

  /// Fences the bound media lease and waits for its exact native Stop receipt.
  Future<VoiceStartStatus> stop();

  /// A read-only Voice journal query. It never prepares a model or native audio.
  Future<VoiceStartStatus?> readStatus(String commandId);
}
