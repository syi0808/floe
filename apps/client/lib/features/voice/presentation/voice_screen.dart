import 'dart:async';

import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_button.dart';
import 'package:floe_client/app/floe_squircle.dart';
import 'package:floe_client/l10n/app_localizations.dart';
import 'package:floe_client/app/runtime/local_owner_gateways_scope.dart';
import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/features/conversation/application/agent_conversation_gateway.dart';
import 'package:floe_client/features/conversation/domain/agent_session.dart';
import 'package:floe_client/features/voice/application/voice_gateway.dart';

class VoiceScreen extends StatefulWidget {
  const VoiceScreen({
    super.key,
    required this.ownerGateways,
    required this.agentGateway,
    required this.personId,
  });

  final LocalOwnerGateways ownerGateways;
  final AgentConversationGateway? agentGateway;
  final String personId;

  @override
  State<VoiceScreen> createState() => _VoiceScreenState();
}

class _VoiceScreenState extends State<VoiceScreen> {
  AgentSession? _session;
  VoiceStartStatus? _status;
  String? _message;
  bool _loading = true;
  bool _busy = false;

  VoiceGateway? get _gateway => widget.ownerGateways.voice;

  @override
  void initState() {
    super.initState();
    widget.ownerGateways.runtime?.addListener(_runtimeChanged);
    unawaited(_loadSessionAndStatus());
  }

  @override
  void didUpdateWidget(covariant VoiceScreen oldWidget) {
    super.didUpdateWidget(oldWidget);
    final oldRuntime = oldWidget.ownerGateways.runtime;
    final nextRuntime = widget.ownerGateways.runtime;
    if (oldRuntime != nextRuntime) {
      oldRuntime?.removeListener(_runtimeChanged);
      nextRuntime?.addListener(_runtimeChanged);
      if (nextRuntime?.ready ?? false) unawaited(_loadSessionAndStatus());
    }
  }

  @override
  void dispose() {
    widget.ownerGateways.runtime?.removeListener(_runtimeChanged);
    super.dispose();
  }

  void _runtimeChanged() {
    if (!_loading && (widget.ownerGateways.runtime?.ready ?? false)) {
      unawaited(_loadSessionAndStatus());
    }
  }

  Future<void> _loadSessionAndStatus() async {
    AgentSession? session;
    try {
      session = await widget.agentGateway?.resumeConversation(widget.personId);
    } on Object catch (error) {
      if (mounted) setState(() => _message = _errorMessage(error));
    }
    VoiceStartStatus? status;
    final gateway = _gateway;
    final runtime = widget.ownerGateways.runtime;
    if (gateway != null && (runtime == null || runtime.ready)) {
      try {
        status = await gateway.recoverPendingStart();
      } on Object catch (error) {
        if (mounted) setState(() => _message = _errorMessage(error));
      }
    }
    if (!mounted) return;
    setState(() {
      _session = session;
      _status = status;
      _loading = false;
    });
  }

  Future<void> _start() async {
    final session = _session;
    final gateway = _gateway;
    if (_busy || session == null || gateway == null) return;
    setState(() {
      _busy = true;
      _message = null;
    });
    try {
      final status = await gateway.start(session.id);
      if (!mounted) return;
      setState(() {
        _status = status;
        _message = _messageFor(status);
      });
    } on Object catch (error) {
      VoiceStartStatus? status;
      final commandId = gateway.pendingCommandId;
      if (commandId != null) {
        try {
          status = await gateway.readStatus(commandId);
        } on Object {
          // Keep the exact command identity so an explicit retry replays it.
        }
      }
      if (!mounted) return;
      setState(() {
        _status = status;
        _message = status == null ? _errorMessage(error) : _messageFor(status);
      });
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _refresh() async {
    final gateway = _gateway;
    final commandId = gateway?.pendingCommandId;
    if (gateway == null || commandId == null || _busy) return;
    setState(() => _busy = true);
    try {
      final status = await gateway.readStatus(commandId);
      if (mounted) {
        setState(() {
          _status = status;
          _message = status == null
              ? AppLocalizations.of(context).voiceNoPendingConnection
              : _messageFor(status);
        });
      }
    } on Object catch (error) {
      if (mounted) setState(() => _message = _errorMessage(error));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _cancelStart() async {
    final gateway = _gateway;
    if (gateway == null || _busy) return;
    setState(() {
      _busy = true;
      _message = null;
    });
    try {
      final status = await gateway.cancelPendingStart();
      if (!mounted) return;
      setState(() {
        _status = status;
        _message = _messageFor(status);
      });
    } on Object catch (error) {
      VoiceStartStatus? status;
      final commandId = gateway.pendingCommandId;
      if (commandId != null) {
        try {
          status = await gateway.readStatus(commandId);
        } on Object {
          // Keep the exact command identity until its cancellation is known.
        }
      }
      if (!mounted) return;
      setState(() {
        _status = status;
        _message = status == null ? _errorMessage(error) : _messageFor(status);
      });
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _stop() async {
    final gateway = _gateway;
    if (gateway == null || _busy) return;
    setState(() {
      _busy = true;
      _message = null;
    });
    try {
      final status = await gateway.stop();
      if (!mounted) return;
      setState(() {
        _status = status;
        _message = _messageFor(status);
      });
    } on Object catch (error) {
      VoiceStartStatus? status;
      final commandId = gateway.pendingCommandId;
      if (commandId != null) {
        try {
          status = await gateway.readStatus(commandId);
        } on Object {
          // Keep the exact call identity until its Stop is confirmed.
        }
      }
      if (!mounted) return;
      setState(() {
        _status = status;
        _message = status == null ? _errorMessage(error) : _messageFor(status);
      });
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  AppLocalizations get _copy => AppLocalizations.of(context);

  String _messageFor(VoiceStartStatus status) => switch (status.state) {
    VoiceStartState.preparing => _copy.voicePendingMessage,
    VoiceStartState.bound when status.localLeaseReleased == true =>
      _copy.voiceStoppedMessage,
    VoiceStartState.bound => _copy.voiceBoundMessage,
    VoiceStartState.cancelled => _copy.voiceCancelledMessage,
  };

  String _errorMessage(Object error) => switch (error) {
    // Preserve the owner's specific failure/recovery reason; a generic
    // connection message must not erase permission or availability limits.
    AppWireTransportException() => error.message,
    _ => _copy.voiceUnconfirmedMessage,
  };

  String _statusLabel(VoiceStartStatus status) => switch (status.state) {
    VoiceStartState.preparing => _copy.voiceConnecting,
    VoiceStartState.bound when status.localLeaseReleased == true =>
      _copy.voiceStopped,
    VoiceStartState.bound => _copy.voiceSessionOpen,
    VoiceStartState.cancelled => _copy.voiceCancelled,
  };

  String _mediaLabel(VoiceMediaState? media) => switch (media) {
    VoiceMediaState.preparing => _copy.voiceMicrophonePreparing,
    VoiceMediaState.muted => _copy.voiceMicrophoneMuted,
    VoiceMediaState.capturing => _copy.voiceMicrophoneOn,
    VoiceMediaState.stopped => _copy.voiceMicrophoneStopped,
    null => _copy.voiceMicrophoneNotStarted,
  };

  String _remoteLabel(VoiceRemoteState remote) => switch (remote) {
    VoiceRemoteState.notStarted => _copy.voiceServiceNotStarted,
    VoiceRemoteState.dispatching => _copy.voiceServiceConnecting,
    VoiceRemoteState.connected => _copy.voiceServiceConnected,
    VoiceRemoteState.closeRequested => _copy.voiceServiceClosing,
    VoiceRemoteState.closed => _copy.voiceServiceClosed,
    VoiceRemoteState.unknown => _copy.voiceServiceUnknown,
  };

  @override
  Widget build(BuildContext context) {
    final gateway = _gateway;
    final runtimeReady = widget.ownerGateways.runtime?.ready ?? false;
    final callActive =
        _status?.state == VoiceStartState.bound &&
        _status?.localLeaseReleased != true;
    final canStart =
        !_loading &&
        !_busy &&
        runtimeReady &&
        _session != null &&
        gateway != null &&
        _status?.state != VoiceStartState.preparing &&
        !callActive;
    final setupMessage = _loading
        ? _copy.voiceCheckingAvailability
        : !runtimeReady || gateway == null
        ? _copy.voiceNotAvailable
        : _session == null
        ? _copy.voiceOpenConversation
        : null;
    final messageIsError =
        _message != null &&
        (_status == null || _message != _messageFor(_status!));
    return ListView(
      padding: const EdgeInsets.fromLTRB(12, 20, 12, 36),
      children: [
        Align(
          alignment: Alignment.topLeft,
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 720),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Text(_copy.voiceTitle, style: FloeType.display),
                const SizedBox(height: FloeSpace.sm),
                Text(
                  _copy.voiceSubtitle,
                  style: FloeType.body.copyWith(color: FloeColor.textSecondary),
                ),
                const SizedBox(height: FloeSpace.lg),
                FloeCard(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Row(
                        children: [
                          FloeSquircle(
                            size: FloeSquircleSize.sm,
                            fill: FloePalette.primary50,
                            borderWidth: 0,
                            padding: const EdgeInsets.all(12),
                            child: const Icon(
                              LucideIcons.mic,
                              size: 22,
                              color: FloePalette.primary600,
                            ),
                          ),
                          const SizedBox(width: FloeSpace.md),
                          Expanded(
                            child: Text(
                              _copy.voiceConversation,
                              style: FloeType.headline,
                            ),
                          ),
                        ],
                      ),
                      const SizedBox(height: FloeSpace.lg),
                      if (_status != null) ...[
                        Semantics(
                          liveRegion: true,
                          child: Text(
                            _statusLabel(_status!),
                            style: FloeType.title,
                          ),
                        ),
                        const SizedBox(height: FloeSpace.sm),
                        Text(_mediaLabel(_status!.media), style: FloeType.body),
                        if (_status!.remote != null)
                          Text(
                            _remoteLabel(_status!.remote!),
                            style: FloeType.body.copyWith(
                              color: FloeColor.textSecondary,
                            ),
                          ),
                        const SizedBox(height: FloeSpace.md),
                      ],
                      if (_message != null &&
                          (messageIsError ||
                              _status?.state == VoiceStartState.preparing)) ...[
                        Semantics(
                          liveRegion: true,
                          child: Text(
                            _message!,
                            style: FloeType.body.copyWith(
                              color: messageIsError
                                  ? Theme.of(context).colorScheme.error
                                  : FloeColor.textSecondary,
                            ),
                          ),
                        ),
                        const SizedBox(height: FloeSpace.md),
                      ],
                      if (setupMessage != null &&
                          !callActive &&
                          _status?.state != VoiceStartState.preparing) ...[
                        Text(
                          setupMessage,
                          style: FloeType.body.copyWith(
                            color: FloeColor.textSecondary,
                          ),
                        ),
                        const SizedBox(height: FloeSpace.md),
                      ],
                      if (!callActive &&
                          _status?.state != VoiceStartState.preparing) ...[
                        const SizedBox(height: FloeSpace.md),
                        const _VoiceDisclosure(),
                        const SizedBox(height: FloeSpace.lg),
                      ],
                      Wrap(
                        spacing: FloeSpace.md,
                        runSpacing: FloeSpace.sm,
                        children: [
                          if (callActive)
                            FloeButton.filled(
                              onPressed: !_busy ? _stop : null,
                              loading: _busy,
                              icon: const Icon(LucideIcons.square, size: 18),
                              child: Text(_copy.voiceStop),
                            )
                          else if (_status?.state == VoiceStartState.preparing)
                            FloeButton.outlined(
                              onPressed: !_busy ? _cancelStart : null,
                              loading: _busy,
                              child: Text(_copy.voiceCancelConnection),
                            )
                          else
                            FloeButton.filled(
                              onPressed: canStart ? _start : null,
                              loading: _busy,
                              icon: const Icon(LucideIcons.mic, size: 18),
                              child: Text(_copy.voiceStart),
                            ),
                          if (gateway?.pendingCommandId != null)
                            FloeButton.text(
                              onPressed: !_busy ? _refresh : null,
                              child: Text(_copy.voiceCheckStatus),
                            ),
                        ],
                      ),
                      if (callActive ||
                          _status?.state == VoiceStartState.preparing) ...[
                        const SizedBox(height: FloeSpace.lg),
                        const _VoiceDisclosure(),
                      ],
                    ],
                  ),
                ),
              ],
            ),
          ),
        ),
      ],
    );
  }
}

class _VoiceDisclosure extends StatelessWidget {
  const _VoiceDisclosure();
  @override
  Widget build(BuildContext context) {
    final copy = AppLocalizations.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(copy.voiceBeforeStart, style: FloeType.title),
        const SizedBox(height: FloeSpace.sm),
        Text(
          copy.voiceDisclosure,
          style: FloeType.body.copyWith(color: FloeColor.textSecondary),
        ),
        const SizedBox(height: FloeSpace.md),
        Text(
          copy.voicePlaybackDisclosure,
          style: FloeType.body.copyWith(color: FloeColor.textSecondary),
        ),
      ],
    );
  }
}
