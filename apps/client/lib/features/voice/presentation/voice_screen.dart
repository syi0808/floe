import 'dart:async';

import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_button.dart';
import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/app/runtime/local_owner_gateways_scope.dart';
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
      if (mounted) setState(() => _message = error.toString());
    }
    VoiceStartStatus? status;
    final gateway = _gateway;
    final runtime = widget.ownerGateways.runtime;
    if (gateway != null && (runtime == null || runtime.ready)) {
      try {
        status = await gateway.recoverPendingStart();
      } on Object catch (error) {
        if (mounted) setState(() => _message = error.toString());
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
              ? 'No Voice Start record was found.'
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

  String _messageFor(VoiceStartStatus status) => switch (status.state) {
    VoiceStartState.preparing =>
      'Voice Start is unresolved. Cancel it before trying another Start.',
    VoiceStartState.bound => 'A Voice call is bound to this conversation.',
    VoiceStartState.cancelled =>
      'Voice Start was cancelled. A later Start uses a new command ID.',
  };

  String _errorMessage(Object error) => switch (error) {
    AppWireTransportException() => error.message,
    _ => 'Voice status could not be confirmed. Retry the same Start to observe it.',
  };

  @override
  Widget build(BuildContext context) {
    final gateway = _gateway;
    final runtimeReady = widget.ownerGateways.runtime?.ready ?? false;
    final canStart =
        !_loading &&
        !_busy &&
        runtimeReady &&
        _session != null &&
        gateway != null &&
        _status?.state != VoiceStartState.preparing &&
        _status?.state != VoiceStartState.bound;
    return ListView(
      padding: const EdgeInsets.fromLTRB(12, 20, 12, 36),
      children: [
        Text('Voice', style: FloeType.display.copyWith(fontSize: 30)),
        const SizedBox(height: 8),
        Text(
          'Start and status use the durable Voice command journal.',
          style: FloeType.body.copyWith(color: FloePalette.neutral600),
        ),
        const SizedBox(height: 20),
        Card(
          child: Padding(
            padding: const EdgeInsets.all(20),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    const Icon(LucideIcons.mic, size: 20),
                    const SizedBox(width: 10),
                    Text('Live presentation', style: FloeType.title),
                  ],
                ),
                const SizedBox(height: 12),
                Text(
                  'This build has no native audio backend. A Start attempt is '
                  'durably cancelled; it does not request microphone permission, '
                  'start capture, or create a provider call.',
                  style: FloeType.body,
                ),
                if (_status != null) ...[
                  const SizedBox(height: 16),
                  Text('Status: ${_status!.state.name}'),
                  Text('Media: ${_status!.media?.name ?? 'not bound'}'),
                  if (_status!.remote != null)
                    Text('Provider: ${_status!.remote!.name}'),
                ],
                if (_message != null) ...[
                  const SizedBox(height: 12),
                  Text(
                    _message!,
                    style: TextStyle(
                      color: Theme.of(context).colorScheme.error,
                    ),
                  ),
                ],
                if (_session == null && !_loading) ...[
                  const SizedBox(height: 12),
                  const Text(
                    'Open or resume a conversation before trying Voice.',
                  ),
                ],
                if (!runtimeReady && !_loading) ...[
                  const SizedBox(height: 8),
                  const Text('Prepare Runtime before using Voice.'),
                ],
                const SizedBox(height: 20),
                Row(
                  children: [
                    FloeButton.filled(
                      onPressed: canStart ? _start : null,
                      loading: _busy,
                      icon: const Icon(LucideIcons.mic),
                      child: const Text('Start voice'),
                    ),
                    const SizedBox(width: 12),
                    FloeButton.outlined(
                      onPressed: !_busy && gateway?.pendingCommandId != null
                          ? _refresh
                          : null,
                      child: const Text('Refresh status'),
                    ),
                  ],
                ),
                if (_status?.state == VoiceStartState.preparing) ...[
                  const SizedBox(height: 12),
                  FloeButton.outlined(
                    onPressed: !_busy ? _cancelStart : null,
                    loading: _busy,
                    child: const Text('Cancel pending Start'),
                  ),
                ],
              ],
            ),
          ),
        ),
      ],
    );
  }
}
