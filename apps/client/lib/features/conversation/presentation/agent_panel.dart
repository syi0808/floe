import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_markdown_plus/flutter_markdown_plus.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_button.dart';
import 'package:floe_client/app/floe_badge.dart';
import 'package:floe_client/app/floe_primitives.dart';
import 'package:floe_client/app/floe_mascot.dart';
import 'package:floe_client/app/floe_squircle.dart';
import 'package:floe_client/l10n/app_localizations.dart';
import 'package:floe_client/infrastructure/diagnostics/app_diagnostics.dart';
import 'package:floe_client/features/conversation/application/conversation_controller.dart';
import 'package:floe_client/features/conversation/domain/agent_session.dart';
import 'package:floe_client/features/conversation/domain/conversation_surface.dart';
import 'package:floe_client/features/conversation/infrastructure/app_wire_conversation_client.dart'
    show AppRunStateTerminal;
import 'package:floe_client/features/conversation/domain/agent_interaction.dart';
import 'package:floe_client/features/conversation/presentation/agent_interaction_card.dart';
import 'package:floe_client/features/actions/presentation/agent_proposal_card.dart';
import 'package:floe_client/features/day/application/day_gateway.dart';
import 'package:floe_client/app/runtime/runtime_gateway.dart';

class AgentPanel extends StatefulWidget {
  const AgentPanel({
    super.key,
    required this.controller,
    required this.dayGateway,
    required this.onClose,
    this.onOpenSourceReview,
    this.onOpenVoice,
    this.onOpenConnections,
    this.onOpenAssistantFeatureSettings,
  });

  final ConversationController controller;
  final DayGateway dayGateway;
  final VoidCallback onClose;
  final VoidCallback? onOpenVoice;
  final void Function(AgentInteractionTarget? target)? onOpenSourceReview;
  final VoidCallback? onOpenConnections;
  final void Function(
    AgentAssistantFeatureSourceTarget target,
    Future<void> Function() onBindingReplaced,
  )?
  onOpenAssistantFeatureSettings;

  @override
  State<AgentPanel> createState() => _AgentPanelState();
}

class _AgentPanelState extends State<AgentPanel> {
  final _scroll = ScrollController();
  final _actionFocus = FocusNode();
  final _messageFocus = FocusNode();
  final _composerText = TextEditingController();
  bool _wasBusy = false;
  bool _wasReady = false;
  bool _sessionRequested = false;
  bool _loadingHistory = false;
  int _historyLoadGeneration = 0;

  @override
  void initState() {
    super.initState();
    _wasBusy = widget.controller.busy;
    widget.controller.addListener(_changed);
    widget.controller.attachView();
    _ensureConversationWhenReady();
  }

  @override
  void didUpdateWidget(AgentPanel oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.controller != widget.controller) {
      _historyLoadGeneration++;
      _loadingHistory = false;
      oldWidget.controller.removeListener(_changed);
      oldWidget.controller.detachView();
      _composerText.clear();
      widget.controller.addListener(_changed);
      _wasBusy = widget.controller.busy;
      _wasReady = false;
      _sessionRequested = false;
      widget.controller.attachView();
      _ensureConversationWhenReady();
    }
  }

  void _ensureConversationWhenReady() {
    final controller = widget.controller;
    final ready = controller.runtimeController.ready;
    if (_wasReady && !ready) _sessionRequested = false;
    _wasReady = ready;
    if (!ready || controller.busy || _sessionRequested) return;
    _sessionRequested = true;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted &&
          identical(controller, widget.controller) &&
          controller.runtimeController.ready &&
          !controller.busy) {
        Future<void>(() async {
          if (controller.session == null) await controller.load();
          if (mounted && identical(controller, widget.controller)) {
            await controller.refreshInteractions();
          }
        });
      }
    });
  }

  void _changed() {
    _ensureConversationWhenReady();
    final restoreFocus = _wasBusy && !widget.controller.busy;
    _wasBusy = widget.controller.busy;
    final follow =
        !_loadingHistory &&
        (!_scroll.hasClients || _scroll.position.extentAfter < 64);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      if (follow && !_loadingHistory && _scroll.hasClients) {
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

  Future<void> _loadEarlier() async {
    if (_loadingHistory) return;
    _loadingHistory = true;
    final generation = ++_historyLoadGeneration;
    final position = _scroll.hasClients ? _scroll.position.pixels : null;
    final extent = _scroll.hasClients ? _scroll.position.maxScrollExtent : null;
    final controller = widget.controller;
    await controller.loadEarlierMessages();
    if (!mounted) return;
    if (generation != _historyLoadGeneration ||
        !identical(controller, widget.controller)) {
      return;
    }
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      if (generation != _historyLoadGeneration) return;
      if (position != null &&
          extent != null &&
          _scroll.hasClients &&
          identical(controller, widget.controller) &&
          (_scroll.position.pixels - position).abs() < 1) {
        final next = position + _scroll.position.maxScrollExtent - extent;
        _scroll.jumpTo(
          next.clamp(
            _scroll.position.minScrollExtent,
            _scroll.position.maxScrollExtent,
          ),
        );
      }
      _loadingHistory = false;
    });
    WidgetsBinding.instance.ensureVisualUpdate();
  }

  Future<void> _exportDiagnostics() async {
    try {
      final file = await AppDiagnostics.exportBundle();
      await Clipboard.setData(ClipboardData(text: file.path));
    } on Object catch (error, stackTrace) {
      AppDiagnostics.error(
        component: 'diagnostics',
        operation: 'export_bundle',
        error: error,
        stackTrace: stackTrace,
      );
    }
  }

  @override
  void dispose() {
    widget.controller.detachView();
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
                      if (widget.onOpenVoice != null)
                        FloeButton.icon(
                          tooltip: 'Voice',
                          onPressed: widget.onOpenVoice,
                          icon: const Icon(LucideIcons.mic, size: 18),
                        ),
                      FloeButton.icon(
                        tooltip: strings.agentNewConversation,
                        onPressed: controller.canStartConversation
                            ? () => controller.load(newSession: true)
                            : null,
                        icon: const Icon(LucideIcons.squarePen, size: 18),
                      ),
                      FloeButton.icon(
                        tooltip: strings.close,
                        onPressed: widget.onClose,
                        icon: const Icon(LucideIcons.x, size: 18),
                      ),
                    ],
                  ),
                  if (controller.workItems.isNotEmpty)
                    _workStrip(strings, controller),
                ],
              ),
            );
            final footer = Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                if (controller.hasPendingWorkAdmission)
                  FloeButton.outlined(
                    onPressed: controller.inputBusy
                        ? null
                        : controller.retryPendingWorkAdmission,
                    child: const Text('Recover pending message'),
                  ),
                if (controller.surfaceFailure case final surfaceFailure?)
                  Padding(
                    padding: const EdgeInsets.symmetric(
                      horizontal: FloeSpace.base,
                    ),
                    child: Column(
                      children: [
                        Text(surfaceFailure, style: FloeType.bodySmall),
                        FloeButton.text(
                          onPressed: controller.refreshSurface,
                          child: const Text('Refresh conversation'),
                        ),
                      ],
                    ),
                  ),
                if (controller.interactionPageFailure != null)
                  Padding(
                    padding: const EdgeInsets.symmetric(
                      horizontal: FloeSpace.base,
                    ),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        const Text('Could not load interaction history.'),
                        FloeButton.text(
                          onPressed:
                              controller.busy || controller.loadingInteractions
                              ? null
                              : controller.hasMoreInteractions
                              ? controller.loadMoreInteractions
                              : controller.refreshInteractions,
                          child: const Text('Retry interaction history'),
                        ),
                      ],
                    ),
                  ),
                _composer(strings, controller),
              ],
            );
            return LayoutBuilder(
              builder: (context, constraints) {
                final compact =
                    constraints.maxHeight < 650 ||
                    MediaQuery.textScalerOf(context).scale(14) > 20;
                final messages = controller.surfaceMessages;
                final extraInteractions = controller.interactionSnapshots
                    .toList(growable: false);
                final activity = controller.messages
                    .whereType<AgentCapabilityMessage>()
                    .toList(growable: false);
                final content =
                    messages.isEmpty &&
                        extraInteractions.isEmpty &&
                        activity.isEmpty
                    ? Padding(
                        padding: const EdgeInsets.all(FloeSpace.base),
                        child: Text(
                          strings.agentConversationEmpty,
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
                        itemCount:
                            messages.length +
                            extraInteractions.length +
                            (activity.isEmpty ? 0 : 1) +
                            (controller.hasEarlierMessages ? 1 : 0) +
                            (controller.hasMoreInteractions ? 1 : 0),
                        separatorBuilder: (_, _) =>
                            const SizedBox(height: FloeSpace.base),
                        itemBuilder: (context, index) {
                          final offset = controller.hasEarlierMessages ? 1 : 0;
                          if (offset == 1 && index == 0) {
                            return Column(
                              crossAxisAlignment: CrossAxisAlignment.start,
                              children: [
                                FloeButton.text(
                                  onPressed: controller.loadingEarlier
                                      ? null
                                      : _loadEarlier,
                                  child: Text(
                                    controller.loadingEarlier
                                        ? 'Loading earlier messages…'
                                        : 'Load earlier messages',
                                  ),
                                ),
                                if (controller.earlierFailure
                                    case final failure?)
                                  Text(failure),
                              ],
                            );
                          }
                          final contentIndex = index - offset;
                          if (contentIndex < messages.length) {
                            return _surfaceMessage(
                              strings,
                              messages[contentIndex],
                            );
                          }
                          if (contentIndex <
                              messages.length + extraInteractions.length) {
                            final interaction =
                                extraInteractions[contentIndex -
                                    messages.length];
                            return AgentInteractionCard(
                              key: ValueKey(interaction.id),
                              controller: controller,
                              interactionId: interaction.id,
                              onOpenSourceReview: widget.onOpenSourceReview,
                              onOpenConnections: widget.onOpenConnections,
                              onOpenAssistantFeatureSettings:
                                  widget.onOpenAssistantFeatureSettings,
                            );
                          }
                          if (activity.isNotEmpty &&
                              contentIndex ==
                                  messages.length + extraInteractions.length) {
                            return ExpansionTile(
                              title: const Text('Execution details'),
                              children: activity
                                  .map(
                                    (message) =>
                                        _activityMessage(strings, message),
                                  )
                                  .toList(),
                            );
                          }
                          return FloeButton.text(
                            onPressed: controller.loadingInteractions
                                ? null
                                : controller.loadMoreInteractions,
                            child: const Text('Load more interactions'),
                          );
                        },
                      );
                if (compact) {
                  return Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      header,
                      const FloeDivider(),
                      Expanded(
                        child: SingleChildScrollView(
                          controller: _scroll,
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.stretch,
                            children: [content, footer],
                          ),
                        ),
                      ),
                    ],
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

  Widget _surfaceMessage(AppLocalizations strings, AppSurfaceMessage message) =>
      switch (message) {
        AppSurfaceInput(:final text) => _surfaceText(strings, text, user: true),
        AppSurfaceResult(:final text, :final completeness) => _surfaceText(
          strings,
          text,
          partial: completeness == AppSurfaceResultCompleteness.partial,
        ),
        AppSurfaceOutcome(:final state, :final issueMessage) => Padding(
          padding: const EdgeInsets.symmetric(vertical: FloeSpace.sm),
          child: Text(
            issueMessage ??
                (state == AppSurfaceOutcomeState.blocked
                    ? 'Review required'
                    : strings.agentStopped),
            style: FloeType.bodySmall,
          ),
        ),
        AppSurfaceCancellation() => const Padding(
          padding: EdgeInsets.symmetric(vertical: FloeSpace.sm),
          child: Text('Queued work cancelled'),
        ),
      };

  Widget _surfaceText(
    AppLocalizations strings,
    String text, {
    bool user = false,
    bool partial = false,
  }) => FloeSquircle(
    size: FloeSquircleSize.md,
    fill: user ? FloePalette.neutral50 : FloePalette.primary50,
    borderWidth: 0,
    padding: const EdgeInsets.all(FloeSpace.md),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(user ? strings.agentYou : 'Floe', style: FloeType.label),
        if (partial)
          Text(
            'Partial result',
            style: FloeType.bodySmall.copyWith(color: FloePalette.neutral600),
          ),
        const SizedBox(height: FloeSpace.sm),
        if (user)
          SelectableText(text, style: FloeType.body)
        else
          _AgentMarkdown(data: text),
      ],
    ),
  );

  Widget _workStrip(
    AppLocalizations strings,
    ConversationController controller,
  ) {
    final queued = controller.workItems
        .where((item) => item.state == AppSurfaceWorkState.queued)
        .length;
    final busy = controller.workItems
        .where(
          (item) =>
              item.currentRunId != null &&
              controller.workRuns[item.currentRunId]?.state.terminal == false,
        )
        .length;
    return ExpansionTile(
      tilePadding: EdgeInsets.zero,
      title: Text(
        'Recent work · $queued queued · $busy working',
        style: FloeType.bodySmall,
      ),
      children: [
        ConstrainedBox(
          constraints: const BoxConstraints(maxHeight: 220),
          child: ListView(
            shrinkWrap: true,
            children: [
              for (final item in controller.workItems.reversed)
                Builder(
                  builder: (context) {
                    final run = controller.workRuns[item.currentRunId];
                    final label = switch (item.state) {
                      AppSurfaceWorkState.queued => 'Queued',
                      AppSurfaceWorkState.cancelled => 'Cancelled',
                      AppSurfaceWorkState.linked =>
                        run == null
                            ? 'Checking status'
                            : !run.state.terminal
                            ? 'Working'
                            : run.report?.execution ?? 'Finished',
                    };
                    return ListTile(
                      dense: true,
                      contentPadding: EdgeInsets.zero,
                      title: Text(
                        item.inputText,
                        maxLines: 2,
                        overflow: TextOverflow.ellipsis,
                        style: FloeType.bodySmall,
                      ),
                      subtitle: Text(label, style: FloeType.bodySmall),
                      trailing: item.state == AppSurfaceWorkState.queued
                          ? FloeButton.text(
                              onPressed:
                                  controller.cancellingWork(
                                    item.receipt.commandId,
                                  )
                                  ? null
                                  : () => controller.cancelQueuedWork(item),
                              child: const Text('Cancel'),
                            )
                          : run != null && !run.state.terminal
                          ? FloeButton.text(
                              onPressed:
                                  controller.cancellingWork(
                                    item.receipt.commandId,
                                  )
                                  ? null
                                  : () => controller.stopWork(item),
                              child: Text(strings.agentStop),
                            )
                          : null,
                    );
                  },
                ),
            ],
          ),
        ),
      ],
    );
  }

  Widget _activityMessage(
    AppLocalizations strings,
    AgentCapabilityMessage message,
  ) => ExpansionTile(
    tilePadding: EdgeInsets.zero,
    childrenPadding: const EdgeInsets.only(bottom: FloeSpace.sm),
    title: Text(strings.agentConversationSource, style: FloeType.controlLabel),
    subtitle: Text(
      strings.agentConversationSourceDetails,
      style: FloeType.bodySmall.copyWith(fontSize: 12),
    ),
    children: [
      Align(
        alignment: Alignment.centerLeft,
        child: SelectableText(
          _sourceText(strings, message),
          style: FloeType.bodySmall.copyWith(color: FloePalette.neutral600),
        ),
      ),
      if (message.hasArtifactMediaType(
            'application/vnd.floe.actions.calendar-proposal+json;version=1',
          ) &&
          message.executionReceipt != null) ...[
        const SizedBox(height: FloeSpace.md),
        AgentProposalCard(controller: widget.controller, message: message),
      ],
    ],
  );

  String _sourceText(AppLocalizations strings, AgentCapabilityMessage message) {
    final output = message.output;
    return output ?? strings.agentConversationSourceUnavailable;
  }

  Widget _composer(
    AppLocalizations strings,
    ConversationController controller,
  ) {
    final status = _status(strings, controller);
    final runtimeUnavailable = !controller.runtimeController.ready;
    final label = runtimeUnavailable
        ? strings.agentReload
        : controller.running
        ? strings.agentStop
        : controller.needsReload
        ? strings.agentReload
        : controller.needsRecovery
        ? strings.agentRecover
        : strings.agentConnectedSend;
    final icon = runtimeUnavailable || controller.needsReload
        ? LucideIcons.refreshCw
        : controller.running
        ? LucideIcons.square
        : controller.needsRecovery
        ? LucideIcons.rotateCcw
        : LucideIcons.arrowUp;
    final VoidCallback? action = runtimeUnavailable
        ? controller.runtimeController.canRecover
              ? controller.runtimeController.recover
              : null
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
        : controller.canSend &&
              controller.isConnectedConversation &&
              _composerText.text.trim().isNotEmpty
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
          if (controller.failureIncidentId case final incidentId?) ...[
            SelectableText(
              strings.agentIncidentId(incidentId),
              style: FloeType.bodySmall.copyWith(color: FloePalette.neutral600),
            ),
            const SizedBox(height: FloeSpace.md),
          ],
          if ((controller.recoveryAction == 'review_source' ||
                  controller.failureSafeActions.contains('review_source')) &&
              widget.onOpenSourceReview != null) ...[
            FloeButton.outlined(
              onPressed: () => widget.onOpenSourceReview?.call(null),
              size: FloeButtonSize.compact,
              child: Text(strings.agentConnectedSourceDetails),
            ),
            const SizedBox(height: FloeSpace.md),
          ],
          if (controller.failureSafeActions.contains('start_new_session')) ...[
            FloeButton.outlined(
              onPressed: controller.canStartConversation
                  ? () => controller.load(newSession: true)
                  : null,
              size: FloeButtonSize.compact,
              child: Text(strings.agentNewConversation),
            ),
            const SizedBox(height: FloeSpace.md),
          ],
          if (controller.failureSafeActions.contains('export_diagnostics')) ...[
            FloeButton.outlined(
              onPressed: _exportDiagnostics,
              size: FloeButtonSize.compact,
              child: Text(strings.agentExportDiagnostics),
            ),
            const SizedBox(height: FloeSpace.md),
          ],
          AnimatedBuilder(
            animation: _messageFocus,
            builder: (context, _) {
              final actionButton = FloeButton.icon(
                key: const Key('conversation-composer-action'),
                tooltip: label,
                onPressed: action,
                focusNode: _actionFocus,
                size: FloeButtonSize.compact,
                constraints: const BoxConstraints.tightFor(
                  width: 44,
                  height: 44,
                ),
                style: IconButton.styleFrom(
                  backgroundColor: FloePalette.primary600,
                  foregroundColor: FloePalette.neutral0,
                  disabledBackgroundColor: FloePalette.neutral100,
                  disabledForegroundColor: FloePalette.neutral400,
                ),
                icon: Icon(icon, size: 18),
              );
              if (runtimeUnavailable) {
                return Align(
                  alignment: AlignmentDirectional.centerEnd,
                  child: actionButton,
                );
              }
              return FloeSquircle(
                key: const Key('conversation-composer'),
                size: FloeSquircleSize.lg,
                fill: FloePalette.neutral50,
                borderColor: _messageFocus.hasFocus
                    ? FloePalette.primary500
                    : FloePalette.neutral200,
                padding: const EdgeInsets.fromLTRB(14, 8, 8, 8),
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.end,
                  children: [
                    Expanded(
                      child: Semantics(
                        label: strings.agentMessagePlaceholder,
                        child: TextFormField(
                          key: const Key('conversation-composer-input'),
                          controller: _composerText,
                          focusNode: _messageFocus,
                          enabled:
                              controller.canSend &&
                              controller.isConnectedConversation,
                          style: FloeType.body.copyWith(
                            color: FloeColor.textPrimary,
                          ),
                          decoration: InputDecoration(
                            hint: ExcludeSemantics(
                              child: Text(strings.agentMessagePlaceholder),
                            ),
                            floatingLabelBehavior: FloatingLabelBehavior.never,
                            filled: false,
                            border: InputBorder.none,
                            enabledBorder: InputBorder.none,
                            focusedBorder: InputBorder.none,
                            disabledBorder: InputBorder.none,
                            isDense: true,
                            contentPadding: const EdgeInsets.symmetric(
                              vertical: 10,
                            ),
                          ),
                          minLines: 1,
                          maxLines: 4,
                          textInputAction: TextInputAction.newline,
                          textCapitalization: TextCapitalization.sentences,
                          inputFormatters: [
                            LengthLimitingTextInputFormatter(8192),
                          ],
                          onChanged: (_) => setState(() {}),
                        ),
                      ),
                    ),
                    const SizedBox(width: FloeSpace.sm),
                    actionButton,
                  ],
                ),
              );
            },
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

  Future<void> _sendText(ConversationController controller) async {
    final text = _composerText.text;
    if (!controller.canSend) return;
    final sessionId = controller.session?.id;
    final accepted = await controller.sendText(text);
    if (accepted &&
        mounted &&
        identical(controller, widget.controller) &&
        sessionId == controller.session?.id &&
        _composerText.text == text) {
      _composerText.clear();
      setState(() {});
    }
  }

  String? _status(AppLocalizations strings, ConversationController controller) {
    if (controller.busy) {
      return switch (controller.progress) {
        AgentProgress.loading => strings.agentLoading,
        AgentProgress.expertModel => strings.agentExpertReasoning,
        AgentProgress.correcting => strings.agentCorrectingResponse,
        AgentProgress.capability => strings.agentConnectedReading,
        AgentProgress.stopping => strings.agentStopping,
        _ => strings.agentPreparing,
      };
    }
    if (!controller.runtimeController.ready) {
      final reason = controller.runtimeController.reasonCode;
      if (reason != null) {
        final incident = controller.runtimeController.incidentId;
        return 'Local secure storage is unavailable ($reason).'
            '${incident == null ? '' : ' Incident: $incident'}';
      }
      return switch (controller.runtimeController.state) {
        RuntimeReadinessState.preparationRequired => strings.agentPreparing,
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
      'local_model_timeout' => strings.agentLocalModelTimeout,
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
      'session_integrity' => strings.agentSessionIntegrityFailure,
      'data_release_or_policy_block' => strings.agentDataReleaseBlocked,
      'capability_access_denied' => strings.agentCapabilityAccessDenied,
      'internal_policy_invariant' => strings.agentInternalPolicyFailure,
      'conflict' =>
        controller.recoveryAction == 'refresh_session'
            ? strings.agentReloadNeeded
            : strings.agentFailure,
      'transport_unavailable' => strings.agentTransportUnavailable,
      'stalled' => strings.agentConnectedStalled,
      'stale_context' => strings.agentContextRefreshRequired,
      'access_review_required' => strings.agentAccessReviewRequired,
      'capability_unavailable' =>
        controller.recoveryAction == 'review_source'
            ? strings.agentAccessReviewRequired
            : strings.agentConnectedUnavailable,
      'budget_exceeded' ||
      'deadline_exceeded' => strings.agentConversationBudget,
      'model_input_capacity_exceeded' =>
        strings.agentModelInputCapacityExceeded,
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
