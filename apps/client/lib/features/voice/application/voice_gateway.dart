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

  Future<VoiceStartStatus> start(String surfaceSessionId);

  /// A read-only Voice journal query. It never prepares a model or native audio.
  Future<VoiceStartStatus?> readStatus(String commandId);
}
