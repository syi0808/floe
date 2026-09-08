import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../app/design_tokens.dart';
import '../../app/floe_button.dart';
import '../../app/floe_input.dart';
import '../../app/floe_mascot.dart';
import '../../app/floe_selection.dart';
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
  AgentFixturePrompt _prompt = AgentFixturePrompt.today;
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
        if (widget.controller.isCalendarConversation) {
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
                        child: Text(
                          'Floe',
                          style: TextStyle(
                            fontSize: 18,
                            fontWeight: FontWeight.w600,
                          ),
                        ),
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
                  const SizedBox(height: FloeSpace.sm),
                  Text(
                    controller.isCalendarConversation
                        ? strings.agentConnectedTitle
                        : strings.agentSampleTitle,
                    style: const TextStyle(fontWeight: FontWeight.w600),
                  ),
                  const SizedBox(height: FloeSpace.xs),
                  Text(
                    controller.usesVault
                        ? controller.isCalendarConversation
                              ? strings.agentConnectedBoundary
                              : strings.agentSecureSampleBoundary
                        : strings.agentSampleBoundary,
                    style: const TextStyle(
                      fontSize: 12,
                      color: FloePalette.neutral600,
                    ),
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
                              : strings.agentEmpty,
                          style: const TextStyle(color: FloePalette.neutral600),
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
                      children: [
                        header,
                        const Divider(height: 1),
                        content,
                        footer,
                      ],
                    ),
                  );
                }
                return Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    header,
                    const Divider(height: 1),
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

  Widget _message(
    AppLocalizations strings,
    AgentMessage message,
  ) => switch (message) {
    AgentTextMessage(:final kind, :final text) => FloeSquircle(
      size: FloeSquircleSize.md,
      fill: kind == AgentMessageKind.assistant
          ? FloePalette.primary50
          : FloePalette.neutral50,
      borderWidth: 0,
      padding: const EdgeInsets.all(FloeSpace.md),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            kind == AgentMessageKind.assistant ? 'Floe' : strings.agentYou,
            style: const TextStyle(fontSize: 12, fontWeight: FontWeight.w600),
          ),
          const SizedBox(height: FloeSpace.sm),
          SelectableText(
            text,
            style: const TextStyle(fontSize: 14, height: 1.5),
          ),
        ],
      ),
    ),
    AgentCapabilityMessage() => ExpansionTile(
      tilePadding: EdgeInsets.zero,
      childrenPadding: const EdgeInsets.only(bottom: FloeSpace.sm),
      title: Text(
        widget.controller.isCalendarConversation
            ? strings.agentConnectedSource
            : strings.agentSource,
        style: const TextStyle(fontSize: 13, fontWeight: FontWeight.w600),
      ),
      subtitle: Text(
        widget.controller.isCalendarConversation
            ? strings.agentConnectedSourceDetails
            : strings.agentSourceDetails,
        style: const TextStyle(fontSize: 12),
      ),
      children: [
        Align(
          alignment: Alignment.centerLeft,
          child: SelectableText(
            _sourceText(strings, message),
            style: const TextStyle(fontSize: 13, color: FloePalette.neutral600),
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
                : strings.agentSourceUnavailable
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
                : strings.agentExpertCommitment(
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
                : strings.agentExpertFocus(
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
        : controller.isCalendarConversation
        ? strings.agentConnectedSend
        : strings.agentSend;
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
        : controller.canSend
        ? controller.isCalendarConversation
              ? () => _sendText(controller)
              : () => controller.send(_prompt)
        : null;
    return Padding(
      padding: const EdgeInsets.all(FloeSpace.base),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          if (status != null) ...[
            Semantics(
              liveRegion: true,
              child: Text(
                status,
                style: const TextStyle(
                  fontSize: 13,
                  color: FloePalette.neutral600,
                ),
              ),
            ),
            const SizedBox(height: FloeSpace.md),
          ],
          if (!storageLocked && controller.isCalendarConversation)
            FloeInput(
              label: strings.agentConnectedPrompt,
              controller: _composerText,
              focusNode: _messageFocus,
              enabled: controller.canSend,
              placeholder: strings.agentConnectedEmpty,
              minLines: 2,
              maxLines: 5,
              textInputAction: TextInputAction.newline,
              textCapitalization: TextCapitalization.sentences,
              inputFormatters: [LengthLimitingTextInputFormatter(8192)],
              onChanged: (_) => setState(() {}),
            )
          else if (!storageLocked)
            FloeSelect<AgentFixturePrompt>(
              label: strings.agentPrompt,
              value: _prompt,
              enabled: controller.canSend,
              options: [
                FloeSelectOption(
                  value: AgentFixturePrompt.today,
                  label: strings.agentBriefing,
                ),
                FloeSelectOption(
                  value: AgentFixturePrompt.followUp,
                  label: strings.agentFollowUp,
                ),
              ],
              onChanged: (value) {
                if (value != null) setState(() => _prompt = value);
              },
            ),
          const SizedBox(height: FloeSpace.md),
          FloeButton.filled(
            onPressed: action,
            focusNode: _actionFocus,
            child: Text(label),
          ),
          if (controller.canRetry) ...[
            const SizedBox(height: FloeSpace.sm),
            FloeButton.text(
              onPressed: controller.retry,
              child: Text(strings.agentRetry),
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
    await controller.sendCalendarText(text);
  }

  String? _status(AppLocalizations strings, AgentController controller) {
    if (controller.busy) {
      return switch (controller.progress) {
        AgentProgress.loading => strings.agentLoading,
        AgentProgress.capability =>
          controller.isCalendarConversation
              ? strings.agentConnectedReading
              : strings.agentReading,
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
    if (controller.needsReload) return strings.agentReloadNeeded;
    if (controller.needsRecovery) return strings.agentInterrupted;
    return switch (controller.failure) {
      null => null,
      'cancelled' => strings.agentStopped,
      'interrupted' => strings.agentRecovered,
      'model_unavailable' =>
        controller.isCalendarConversation
            ? strings.agentConnectedUnavailable
            : strings.agentUnavailable,
      'consent_required' => strings.agentRemoteConsentRequired,
      'credential_expired' => strings.agentRemoteCredentialExpired,
      'quota_exceeded' => strings.agentRemoteQuotaExceeded,
      'stalled' =>
        controller.isCalendarConversation
            ? strings.agentConnectedStalled
            : strings.agentStalled,
      'stale_context' => strings.agentConnectedStale,
      'budget_exceeded' || 'deadline_exceeded' =>
        controller.isCalendarConversation
            ? strings.agentConnectedBudget
            : strings.agentBudget,
      _ => strings.agentFailure,
    };
  }
}
