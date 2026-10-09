import 'package:flutter/material.dart';

import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_button.dart';
import 'package:floe_client/app/floe_selection.dart';
import 'package:floe_client/app/floe_squircle.dart';
import 'package:floe_client/app/floe_switch.dart';
import 'package:floe_client/app/runtime/runtime_controller.dart';
import 'package:floe_client/features/conversation/assistant_features/application/assistant_feature_controller.dart';
import 'package:floe_client/features/conversation/assistant_features/domain/assistant_feature.dart';
import 'package:floe_client/features/conversation/domain/agent_interaction.dart';
import 'package:floe_client/l10n/app_localizations.dart';

class AssistantFeatureSettings extends StatefulWidget {
  const AssistantFeatureSettings({
    super.key,
    required this.controller,
    required this.runtime,
    this.focus,
    this.onConfigured,
  });

  final AssistantFeatureController controller;
  final RuntimeController runtime;
  final AgentAssistantFeatureSourceTarget? focus;
  final Future<void> Function()? onConfigured;

  @override
  State<AssistantFeatureSettings> createState() =>
      _AssistantFeatureSettingsState();
}

class _AssistantFeatureSettingsState extends State<AssistantFeatureSettings> {
  bool _wasReady = false;
  int _focusGeneration = 0;

  @override
  void initState() {
    super.initState();
    widget.runtime.addListener(_readinessChanged);
    _readinessChanged();
  }

  void _readinessChanged() {
    final ready = widget.runtime.ready;
    if (!_wasReady && ready) {
      WidgetsBinding.instance.addPostFrameCallback((_) => _load());
    }
    _wasReady = ready;
  }

  @override
  void dispose() {
    widget.runtime.removeListener(_readinessChanged);
    super.dispose();
  }

  @override
  void didUpdateWidget(AssistantFeatureSettings oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.runtime != widget.runtime) {
      oldWidget.runtime.removeListener(_readinessChanged);
      widget.runtime.addListener(_readinessChanged);
      _wasReady = false;
      _readinessChanged();
    }
    if (!_sameReviewTarget(oldWidget.focus, widget.focus) ||
        !identical(oldWidget.onConfigured, widget.onConfigured)) {
      _focusGeneration++;
      WidgetsBinding.instance.addPostFrameCallback((_) => _load());
    }
  }

  Future<void> _load() async {
    if (!mounted || !widget.runtime.ready) return;
    final focus = widget.focus;
    final focusGeneration = _focusGeneration;
    await widget.controller.load();
    if (mounted &&
        focus != null &&
        focusGeneration == _focusGeneration &&
        _sameReviewTarget(focus, widget.focus)) {
      widget.controller.usePreparedReview(focus.review);
    }
  }

  Future<void> _retryPending() async {
    final focus = widget.focus;
    final callback = widget.onConfigured;
    final focusGeneration = _focusGeneration;
    final result = await widget.controller.retryPendingCommand();
    if (!mounted || result == null) return;
    if (result.kind == AssistantFeatureCommandKind.configure &&
        focus != null &&
        widget.controller.didConfigureReview(focus.review.reviewRef) &&
        _isCurrentInteraction(focus, callback, focusGeneration)) {
      widget.controller.acknowledgeConfiguredReview(focus.review.reviewRef);
      await callback?.call();
    }
  }

  Future<void> _configure(AssistantFeature feature) async {
    final focus = widget.focus;
    final callback = widget.onConfigured;
    final focusGeneration = _focusGeneration;
    final saved = await widget.controller.configure(feature);
    if (!saved ||
        !mounted ||
        focus == null ||
        !widget.controller.didConfigureReview(focus.review.reviewRef) ||
        !_isCurrentInteraction(focus, callback, focusGeneration)) {
      return;
    }
    widget.controller.acknowledgeConfiguredReview(focus.review.reviewRef);
    await callback?.call();
  }

  bool _isCurrentInteraction(
    AgentAssistantFeatureSourceTarget focus,
    Future<void> Function()? callback,
    int focusGeneration,
  ) =>
      callback != null &&
      identical(widget.onConfigured, callback) &&
      focusGeneration == _focusGeneration &&
      _sameReviewTarget(focus, widget.focus);

  bool _sameReviewTarget(
    AgentAssistantFeatureSourceTarget? left,
    AgentAssistantFeatureSourceTarget? right,
  ) => left == null
      ? right == null
      : right != null &&
            left.review.reviewRef.matches(right.review.reviewRef) &&
            left.review.sourceScopeRef == right.review.sourceScopeRef &&
            left.review.sourceRequirementRef ==
                right.review.sourceRequirementRef;

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: widget.controller,
    builder: (context, _) {
      final strings = AppLocalizations.of(context);
      final controller = widget.controller;
      final snapshot = controller.snapshot;
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(strings.assistantFeatureTitle, style: FloeType.title),
          const SizedBox(height: FloeSpace.xs),
          Text(
            strings.assistantFeatureBoundary,
            style: FloeType.body.copyWith(color: FloePalette.neutral600),
          ),
          const SizedBox(height: FloeSpace.base),
          if (!controller.available)
            Text(strings.assistantFeatureFailure)
          else if (controller.failure != null)
            Semantics(
              liveRegion: true,
              child: Text(strings.assistantFeatureFailure),
            )
          else if (controller.loaded &&
              (snapshot == null || snapshot.features.isEmpty))
            Text(strings.assistantFeatureEmpty)
          else if (snapshot != null)
            for (final feature in snapshot.features)
              Padding(
                padding: const EdgeInsets.only(bottom: FloeSpace.sm),
                child: _AssistantFeatureCard(
                  controller: controller,
                  feature: feature,
                  focusedReview: controller.focusedReview,
                  onConfigure: _configure,
                ),
              ),
          if (controller.reviewFailure != null)
            Semantics(
              liveRegion: true,
              child: Text(strings.assistantFeatureFailure),
            ),
          if (controller.pendingCommandKind != null) ...[
            const SizedBox(height: FloeSpace.xs),
            Align(
              alignment: Alignment.centerLeft,
              child: FloeButton.outlined(
                key: const ValueKey('assistant-feature-command-retry'),
                onPressed: controller.canRetryPending ? _retryPending : null,
                child: Text(strings.assistantFeatureRetryPending),
              ),
            ),
          ],
          Align(
            alignment: Alignment.centerLeft,
            child: FloeButton.text(
              onPressed: controller.canRead ? _load : null,
              child: Text(strings.assistantFeatureRefresh),
            ),
          ),
        ],
      );
    },
  );
}

class _AssistantFeatureCard extends StatelessWidget {
  const _AssistantFeatureCard({
    required this.controller,
    required this.feature,
    required this.focusedReview,
    required this.onConfigure,
  });

  final AssistantFeatureController controller;
  final AssistantFeature feature;
  final AssistantFeatureSourceReview? focusedReview;
  final Future<void> Function(AssistantFeature feature) onConfigure;

  @override
  Widget build(BuildContext context) {
    final strings = AppLocalizations.of(context);
    return FloeSquircle(
      size: FloeSquircleSize.md,
      fill: FloePalette.neutral50,
      borderWidth: 0,
      padding: const EdgeInsets.symmetric(
        horizontal: FloeSpace.base,
        vertical: FloeSpace.sm,
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          FloeSwitch(
            key: ValueKey('assistant-feature-${feature.featureRef}'),
            value: controller.enabledDraft(feature),
            onChanged: controller.canManage
                ? (enabled) => controller.setEnabledDraft(feature, enabled)
                : null,
            label: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(feature.displayName, style: FloeType.controlLabel),
                const SizedBox(height: FloeSpace.xxs),
                Text(
                  feature.description,
                  style: FloeType.bodySmall.copyWith(
                    color: FloePalette.neutral600,
                    fontSize: 12,
                    height: 1.4,
                  ),
                ),
              ],
            ),
          ),
          for (final group in feature.sourceGroups) ...[
            const SizedBox(height: FloeSpace.xs),
            Text(group.displayName, style: FloeType.controlLabel),
            for (final requirement in group.requirements)
              _SourceRequirementSection(
                controller: controller,
                feature: feature,
                group: group,
                requirement: requirement,
                focusedReview:
                    focusedReview?.sourceScopeRef == group.sourceScopeRef &&
                        focusedReview?.sourceRequirementRef ==
                            requirement.requirementRef
                    ? focusedReview
                    : null,
              ),
          ],
          if (feature.sourceGroups.isEmpty)
            Padding(
              padding: const EdgeInsets.only(top: FloeSpace.xs),
              child: Text(
                strings.assistantFeatureNoSourceSettings,
                style: FloeType.bodySmall.copyWith(
                  color: FloePalette.neutral600,
                ),
              ),
            ),
          if (controller.hasStagedChanges(feature)) ...[
            const SizedBox(height: FloeSpace.sm),
            FloeButton.filled(
              key: ValueKey('assistant-feature-save-${feature.featureRef}'),
              onPressed: controller.canConfigure(feature)
                  ? () => onConfigure(feature)
                  : null,
              child: Text(strings.assistantFeatureSaveChanges),
            ),
          ],
        ],
      ),
    );
  }
}

class _SourceRequirementSection extends StatefulWidget {
  const _SourceRequirementSection({
    required this.controller,
    required this.feature,
    required this.group,
    required this.requirement,
    required this.focusedReview,
  });

  final AssistantFeatureController controller;
  final AssistantFeature feature;
  final AssistantFeatureSourceGroup group;
  final AssistantFeatureSourceRequirement requirement;
  final AssistantFeatureSourceReview? focusedReview;

  @override
  State<_SourceRequirementSection> createState() =>
      _SourceRequirementSectionState();
}

class _SourceRequirementSectionState extends State<_SourceRequirementSection> {
  bool expanded = false;
  final expansion = ExpansibleController();

  @override
  void initState() {
    super.initState();
    expanded = widget.focusedReview != null;
  }

  @override
  void didUpdateWidget(_SourceRequirementSection oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.focusedReview?.reviewRef.id !=
        widget.focusedReview?.reviewRef.id) {
      expanded = widget.focusedReview != null;
    }
  }

  @override
  void dispose() {
    expansion.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final strings = AppLocalizations.of(context);
    final review = widget.controller.reviewFor(
      widget.group,
      widget.requirement,
    );
    final selected = review == null
        ? <String>{}
        : widget.controller.selectedCandidates(review);
    return ExpansionTile(
      controller: expansion,
      key: ValueKey(
        'assistant-source-${widget.group.sourceScopeRef}-${widget.requirement.requirementRef}',
      ),
      initiallyExpanded: expanded,
      onExpansionChanged: (value) {
        setState(() => expanded = value);
        if (value && review == null) {
          widget.controller.prepareSourceReview(
            widget.feature,
            widget.group,
            widget.requirement,
          );
        }
      },
      title: Text(widget.requirement.label),
      subtitle: Text(
        '${widget.requirement.minimumSources == 0 ? strings.assistantFeatureSourceOptional : strings.assistantFeatureSourceRequired} · ${widget.requirement.selectedCount} ${strings.assistantFeatureSourceSelected}',
      ),
      children: [
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: FloeSpace.sm),
          child: Text(
            strings.assistantFeatureSourceBoundary,
            style: FloeType.bodySmall.copyWith(color: FloePalette.neutral600),
          ),
        ),
        if (widget.controller.busy && review == null)
          const Padding(
            padding: EdgeInsets.all(FloeSpace.sm),
            child: CircularProgressIndicator(),
          ),
        if (review == null && widget.requirement.selectedCount > 0)
          Padding(
            padding: const EdgeInsets.all(FloeSpace.sm),
            child: Text(strings.assistantFeatureSourceUnavailable),
          ),
        if (review != null) ...[
          for (final candidate in review.candidates)
            _SourceChoice(
              candidate: candidate,
              selected: selected.contains(candidate.candidateRef),
              enabled: _canChange(review, candidate, selected),
              onChanged: (value) =>
                  _setCandidate(review, candidate, selected, value),
            ),
          if (review.expired)
            Padding(
              padding: const EdgeInsets.all(FloeSpace.sm),
              child: Text(strings.agentInteractionExpired),
            ),
          Align(
            alignment: Alignment.centerLeft,
            child: FloeButton.text(
              onPressed: widget.controller.canManage && review.canRefresh
                  ? () => widget.controller.prepareSourceReview(
                      widget.feature,
                      widget.group,
                      widget.requirement,
                    )
                  : null,
              child: Text(strings.assistantFeatureRefresh),
            ),
          ),
        ] else
          Align(
            alignment: Alignment.centerLeft,
            child: FloeButton.text(
              onPressed: widget.controller.canManage
                  ? () => widget.controller.prepareSourceReview(
                      widget.feature,
                      widget.group,
                      widget.requirement,
                    )
                  : null,
              child: Text(strings.assistantFeatureLoadSources),
            ),
          ),
      ],
    );
  }

  bool _canChange(
    AssistantFeatureSourceReview review,
    AssistantFeatureSourceCandidate candidate,
    Set<String> selected,
  ) {
    if (!widget.controller.canManage || !review.canReplace) return false;
    if (candidate.availability ==
        AssistantFeatureSourceAvailability.available) {
      return selected.contains(candidate.candidateRef) || selected.length < 16;
    }
    return candidate.selected && selected.contains(candidate.candidateRef);
  }

  void _setCandidate(
    AssistantFeatureSourceReview review,
    AssistantFeatureSourceCandidate candidate,
    Set<String> selected,
    bool? value,
  ) {
    if (value == null || !_canChange(review, candidate, selected)) return;
    final updated = selected.toSet();
    if (value) {
      if (updated.length < 16) updated.add(candidate.candidateRef);
    } else {
      updated.remove(candidate.candidateRef);
    }
    widget.controller.setSourceCandidates(review, updated);
  }
}

class _SourceChoice extends StatelessWidget {
  const _SourceChoice({
    required this.candidate,
    required this.selected,
    required this.enabled,
    required this.onChanged,
  });

  final AssistantFeatureSourceCandidate candidate;
  final bool selected;
  final bool enabled;
  final ValueChanged<bool?> onChanged;

  @override
  Widget build(BuildContext context) => FloeCheckboxTile(
    value: selected,
    onChanged: enabled ? onChanged : null,
    title: Text(candidate.label),
    subtitle: Text(
      candidate.availability == AssistantFeatureSourceAvailability.unavailable
          ? AppLocalizations.of(context).assistantFeatureSourceUnavailable
          : AppLocalizations.of(context).assistantFeatureSourceBoundary,
    ),
  );
}
