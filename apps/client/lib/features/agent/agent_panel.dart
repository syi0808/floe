import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_markdown_plus/flutter_markdown_plus.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../app/design_tokens.dart';
import '../../app/floe_button.dart';
import '../../app/floe_badge.dart';
import '../../app/floe_primitives.dart';
import '../../app/floe_input.dart';
import '../../app/floe_mascot.dart';
import '../../app/floe_squircle.dart';
import '../../l10n/app_localizations.dart';
import 'agent_capability_label.dart';
import 'agent_controller.dart';
import 'agent_fixture_gateway.dart';
import 'agent_proposal_card.dart';
import 'agent_vault_gateway.dart';

class AgentPanel extends StatefulWidget {
  const AgentPanel({
    super.key,
    required this.controller,
    required this.onClose,
    this.onOpenAction,
  });

  final AgentController controller;
  final VoidCallback onClose;
  final Future<void> Function(String actionId)? onOpenAction;

  @override
  State<AgentPanel> createState() => _AgentPanelState();
}

class _AgentPanelState extends State<AgentPanel> {
  final _scroll = ScrollController();
  final _actionFocus = FocusNode();
  final _messageFocus = FocusNode();
  final _composerText = TextEditingController();
  bool _wasBusy = false;

  @override
  void initState() {
    super.initState();
    _wasBusy = widget.controller.busy;
    widget.controller.addListener(_changed);
  }

  @override
  void didUpdateWidget(AgentPanel oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.controller != widget.controller) {
      oldWidget.controller.removeListener(_changed);
      widget.controller.addListener(_changed);
      _wasBusy = widget.controller.busy;
    }
  }

  void _changed() {
    final restoreFocus = _wasBusy && !widget.controller.busy;
    _wasBusy = widget.controller.busy;
    final follow = !_scroll.hasClients || _scroll.position.extentAfter < 64;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      if (follow && _scroll.hasClients) {
        _scroll.jumpTo(_scroll.position.maxScrollExtent);
      }
      if (restoreFocus) {
        if (widget.controller.isConnectedConversation) {
          _messageFocus.requestFocus();
        } else {
          _actionFocus.requestFocus();
        }
      }
    });
  }

  @override
  void dispose() {
    widget.controller.removeListener(_changed);
    _scroll.dispose();
    _actionFocus.dispose();
    _messageFocus.dispose();
    _composerText.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => CallbackShortcuts(
    bindings: {
      const SingleActivator(LogicalKeyboardKey.escape): widget.onClose,
    },
    child: FocusTraversalGroup(
      child: FloeSquircle(
        size: FloeSquircleSize.xl,
        child: AnimatedBuilder(
          animation: widget.controller,
          builder: (context, _) {
            final strings = AppLocalizations.of(context);
            final controller = widget.controller;
            final header = Padding(
              padding: const EdgeInsets.all(FloeSpace.base),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    children: [
                      const FloeMascot(size: 32),
                      const SizedBox(width: FloeSpace.sm),
                      const Expanded(
                        child: Text('Floe', style: FloeType.titleLarge),
                      ),
                      FloeButton.icon(
                        tooltip: controller.isCalendarConversation
                            ? strings.agentConnectedNewConversation
                            : strings.agentNewConversation,
                        onPressed:
                            !controller.canSend ||
                                controller.needsReload ||
                                controller.needsRecovery
                            ? null
                            : () => controller.load(newSession: true),
                        icon: const Icon(LucideIcons.squarePen, size: 18),
                      ),
                      FloeButton.icon(
                        tooltip: strings.close,
                        onPressed: widget.onClose,
                        icon: const Icon(LucideIcons.x, size: 18),
                      ),
                    ],
                  ),
                ],
              ),
            );
            final footer = _composer(strings, controller);
            return LayoutBuilder(
              builder: (context, constraints) {
                final compact =
                    constraints.maxHeight < 650 ||
                    MediaQuery.textScalerOf(context).scale(14) > 20;
                final messages = controller.messages;
                final content = messages.isEmpty
                    ? Padding(
                        padding: const EdgeInsets.all(FloeSpace.base),
                        child: Text(
                          controller.isCalendarConversation
                              ? strings.agentConnectedEmpty
                              : strings.agentConversationEmpty,
                          style: FloeType.body.copyWith(
                            color: FloePalette.neutral600,
                          ),
                        ),
                      )
                    : ListView.separated(
                        controller: compact ? null : _scroll,
                        shrinkWrap: compact,
                        physics: compact
                            ? const NeverScrollableScrollPhysics()
                            : null,
                        padding: const EdgeInsets.all(FloeSpace.base),
                        itemCount: messages.length,
                        separatorBuilder: (_, _) =>
                            const SizedBox(height: FloeSpace.base),
                        itemBuilder: (context, index) =>
                            _message(strings, messages[index]),
                      );
                if (compact) {
                  return SingleChildScrollView(
                    controller: _scroll,
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [header, const FloeDivider(), content, footer],
                    ),
                  );
                }
                return Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    header,
                    const FloeDivider(),
                    Expanded(child: content),
                    footer,
                  ],
                );
              },
            );
          },
        ),
      ),
    ),
  );

  Widget _message(AppLocalizations strings, AgentMessage message) =>
      switch (message) {
        AgentTextMessage(:final kind, :final text) => FloeSquircle(
          size: FloeSquircleSize.md,
          fill: kind != AgentMessageKind.user
              ? FloePalette.primary50
              : FloePalette.neutral50,
          borderWidth: 0,
          padding: const EdgeInsets.all(FloeSpace.md),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                kind != AgentMessageKind.user ? 'Floe' : strings.agentYou,
                style: FloeType.label,
              ),
              const SizedBox(height: FloeSpace.sm),
              if (kind != AgentMessageKind.user)
                _AgentMarkdown(data: text)
              else
                SelectableText(text, style: FloeType.body),
            ],
          ),
        ),
        AgentCapabilityMessage() => ExpansionTile(
          tilePadding: EdgeInsets.zero,
          childrenPadding: const EdgeInsets.only(bottom: FloeSpace.sm),
          title: Text(
            widget.controller.isCalendarConversation
                ? strings.agentConnectedSource
                : strings.agentConversationSource,
            style: FloeType.controlLabel,
          ),
          subtitle: Text(
            widget.controller.isCalendarConversation
                ? strings.agentConnectedSourceDetails
                : strings.agentConversationSourceDetails,
            style: FloeType.bodySmall.copyWith(fontSize: 12),
          ),
          children: [
            Align(
              alignment: Alignment.centerLeft,
              child: SelectableText(
                _sourceText(strings, message),
                style: FloeType.bodySmall.copyWith(
                  color: FloePalette.neutral600,
                ),
              ),
            ),
            if (widget.controller.expertResult(message)?.proposal != null) ...[
              const SizedBox(height: FloeSpace.md),
              AgentProposalCard(
                controller: widget.controller,
                message: message,
                onOpenAction: widget.onOpenAction,
              ),
            ],
          ],
        ),
      };

  String _sourceText(AppLocalizations strings, AgentCapabilityMessage message) {
    final result = widget.controller.expertResult(message);
    if (result == null) {
      final output = message.output;
      return output == null || output.trimLeft().startsWith('{')
          ? widget.controller.isCalendarConversation
                ? strings.agentConnectedSourceUnavailable
                : strings.agentConversationSourceUnavailable
          : output;
    }
    String clock(DateTime time) =>
        '${time.hour.toString().padLeft(2, '0')}:${time.minute.toString().padLeft(2, '0')}';
    return [
      strings.agentExpertSource(agentCapabilityTitle(result.expert)),
      for (final insight in result.insights)
        switch (insight.kind) {
          'commitment' =>
            widget.controller.isCalendarConversation
                ? strings.agentConnectedCommitment(
                    insight.title!,
                    clock(insight.start!),
                    clock(insight.end!),
                  )
                : strings.agentConversationCommitment(
                    insight.title!,
                    clock(insight.start!),
                    clock(insight.end!),
                  ),
          'focus_window' =>
            widget.controller.isCalendarConversation
                ? strings.agentConnectedFocusTime(
                    clock(insight.start!),
                    clock(insight.end!),
                  )
                : strings.agentConversationFocusTime(
                    clock(insight.start!),
                    clock(insight.end!),
                  ),
          _ => strings.agentExpertNoFocus,
        },
    ].join('\n');
  }

  Widget _composer(AppLocalizations strings, AgentController controller) {
    final status = _status(strings, controller);
    final storageLocked =
        controller.usesVault && controller.vaultState != AgentVaultState.ready;
    final label = storageLocked
        ? strings.agentReload
        : controller.running
        ? strings.agentStop
        : controller.needsReload
        ? strings.agentReload
        : controller.needsRecovery
        ? strings.agentRecover
        : strings.agentConnectedSend;
    final icon = storageLocked || controller.needsReload
        ? LucideIcons.refreshCw
        : controller.running
        ? LucideIcons.square
        : controller.needsRecovery
        ? LucideIcons.rotateCcw
        : LucideIcons.arrowUp;
    final VoidCallback? action = storageLocked
        ? controller.busy
              ? null
              : () => controller.load()
        : controller.running
        ? controller.progress == AgentProgress.stopping
              ? null
              : () => controller.stop()
        : controller.busy
        ? null
        : controller.needsReload
        ? () => controller.load()
        : controller.needsRecovery
        ? () => controller.recover()
        : controller.canSend && controller.isConnectedConversation
        ? () => _sendText(controller)
        : null;
    return Padding(
      padding: const EdgeInsets.all(FloeSpace.md),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          if (status != null) ...[
            Semantics(
              liveRegion: true,
              child: Align(
                alignment: AlignmentDirectional.centerStart,
                child: FloeBadge(
                  label: status,
                  tone: controller.failure != null
                      ? FloeBadgeTone.danger
                      : controller.busy
                      ? FloeBadgeTone.info
                      : FloeBadgeTone.neutral,
                ),
              ),
            ),
            const SizedBox(height: FloeSpace.md),
          ],
          Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              if (!storageLocked) ...[
                Expanded(
                  child: FloeInput(
                    label: controller.isGeneralConversation
                        ? strings.agentConversationPrompt
                        : strings.agentConnectedPrompt,
                    controller: _composerText,
                    focusNode: _messageFocus,
                    enabled:
                        controller.canSend &&
                        controller.isConnectedConversation,
                    placeholder: controller.isGeneralConversation
                        ? strings.agentConversationEmpty
                        : strings.agentConnectedEmpty,
                    minLines: 1,
                    maxLines: 4,
                    compact: true,
                    textInputAction: TextInputAction.newline,
                    textCapitalization: TextCapitalization.sentences,
                    inputFormatters: [LengthLimitingTextInputFormatter(8192)],
                    onChanged: (_) => setState(() {}),
                  ),
                ),
                const SizedBox(width: FloeSpace.sm),
              ] else
                const Spacer(),
              FloeButton.icon(
                tooltip: label,
                onPressed: action,
                focusNode: _actionFocus,
                size: FloeButtonSize.compact,
                icon: Icon(icon, size: 16),
              ),
            ],
          ),
          if (controller.canContinue || controller.canRetry) ...[
            const SizedBox(height: FloeSpace.sm),
            FloeButton.text(
              onPressed: controller.canContinue
                  ? controller.continueTurn
                  : controller.retry,
              size: FloeButtonSize.compact,
              child: Text(
                controller.canContinue
                    ? strings.agentContinue
                    : strings.agentRetry,
              ),
            ),
          ],
        ],
      ),
    );
  }

  Future<void> _sendText(AgentController controller) async {
    final text = _composerText.text.trim();
    if (text.isEmpty || !controller.canSend) return;
    _composerText.clear();
    setState(() {});
    await controller.sendText(text);
  }

  String? _status(AppLocalizations strings, AgentController controller) {
    if (controller.busy) {
      return switch (controller.progress) {
        AgentProgress.loading => strings.agentLoading,
        AgentProgress.expertModel => strings.agentExpertReasoning,
        AgentProgress.correcting => strings.agentCorrectingResponse,
        AgentProgress.capability =>
          controller.isCalendarConversation
              ? strings.agentConnectedReading
              : strings.agentPreparing,
        AgentProgress.stopping => strings.agentStopping,
        _ =>
          controller.isCalendarConversation
              ? strings.agentConnectedPreparing
              : strings.agentPreparing,
      };
    }
    if (controller.usesVault &&
        controller.vaultState != AgentVaultState.ready) {
      return switch (controller.vaultState) {
        AgentVaultState.missing => strings.agentStorageMissing,
        AgentVaultState.locked => strings.agentStorageLocked,
        _ => strings.agentStorageUnavailable,
      };
    }
    if (controller.needsRecovery) return strings.agentInterrupted;
    if (controller.canContinue) return strings.agentConnectedSoftStop;
    final failure = switch (controller.failure) {
      null => null,
      'cancelled' => strings.agentStopped,
      'interrupted' => strings.agentRecovered,
      'model_unavailable' =>
        controller.isConnectedConversation
            ? strings.agentConnectedUnavailable
            : strings.agentFailure,
      'local_model_unavailable' => strings.agentLocalModelUnavailable,
      'server_model_unavailable' => strings.agentServerModelUnavailable,
      'server_model_timeout' => strings.agentServerModelTimeout,
      'server_model_request_rejected' =>
        strings.agentServerModelRequestRejected,
      'local_model_invalid_output' => strings.agentLocalModelInvalidOutput,
      'server_model_invalid_output' => strings.agentServerModelInvalidOutput,
      'consent_required' => strings.agentRemoteConsentRequired,
      'credential_expired' => strings.agentRemoteCredentialExpired,
      'quota_exceeded' => strings.agentRemoteQuotaExceeded,
      'policy_denied' => strings.agentModelPolicyDenied,
      'transport_unavailable' => strings.agentTransportUnavailable,
      'stalled' =>
        controller.isCalendarConversation
            ? strings.agentConnectedStalled
            : strings.agentFailure,
      'stale_context' => strings.agentConnectedStale,
      'budget_exceeded' || 'deadline_exceeded' =>
        controller.isCalendarConversation
            ? strings.agentConnectedBudget
            : strings.agentConversationBudget,
      _ => strings.agentFailure,
    };
    if (failure != null) return failure;
    if (controller.needsReload) return strings.agentReloadNeeded;
    return null;
  }
}

class _AgentMarkdown extends StatelessWidget {
  const _AgentMarkdown({required this.data});

  final String data;

  @override
  Widget build(BuildContext context) {
    final body = FloeType.body.copyWith(color: FloePalette.neutral950);
    return MarkdownBody(
      data: data,
      selectable: true,
      fitContent: true,
      imageBuilder: (_, _, alt) => Text(
        alt?.trim().isNotEmpty == true ? alt! : '[image]',
        style: body.copyWith(
          color: FloePalette.neutral600,
          fontStyle: FontStyle.italic,
        ),
      ),
      styleSheet: MarkdownStyleSheet.fromTheme(Theme.of(context)).copyWith(
        a: body.copyWith(
          color: FloePalette.primary700,
          decoration: TextDecoration.underline,
        ),
        p: body,
        code: body.copyWith(
          fontFamily: 'monospace',
          fontSize: 13,
          backgroundColor: FloePalette.neutral100,
        ),
        h1: body.copyWith(fontSize: 18, fontWeight: FontWeight.w600),
        h2: body.copyWith(fontSize: 16, fontWeight: FontWeight.w600),
        h3: body.copyWith(fontWeight: FontWeight.w600),
        blockSpacing: FloeSpace.xs,
        listIndent: FloeSpace.lg,
        blockquoteDecoration: const BoxDecoration(
          border: Border(
            left: BorderSide(color: FloePalette.primary300, width: 3),
          ),
        ),
        codeblockPadding: const EdgeInsets.all(FloeSpace.md),
        codeblockDecoration: BoxDecoration(
          color: FloePalette.neutral100,
          borderRadius: BorderRadius.circular(FloeRadius.xs),
        ),
      ),
    );
  }
}
