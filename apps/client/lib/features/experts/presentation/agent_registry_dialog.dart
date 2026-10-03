import 'package:flutter/material.dart';

import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_button.dart';
import 'package:floe_client/app/floe_selection.dart';
import 'package:floe_client/app/floe_squircle.dart';
import 'package:floe_client/app/floe_switch.dart';
import 'package:floe_client/l10n/app_localizations.dart';
import 'package:floe_client/features/conversation/domain/agent_interaction.dart';
import 'package:floe_client/features/experts/application/agent_registry_controller.dart';
import 'package:floe_client/features/experts/domain/agent_registry.dart';

class AgentRegistrySettings extends StatefulWidget {
  const AgentRegistrySettings({
    super.key,
    required this.controller,
    this.focus,
    this.onBindingReplaced,
  });

  final AgentRegistryController controller;
  final AgentExpertBindingTarget? focus;
  final Future<void> Function()? onBindingReplaced;

  @override
  State<AgentRegistrySettings> createState() => _AgentRegistrySettingsState();
}

class _AgentRegistrySettingsState extends State<AgentRegistrySettings> {
  bool _replacementAcknowledged = false;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) => _load());
  }

  @override
  void didUpdateWidget(AgentRegistrySettings oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.focus?.review.reviewRef.id !=
        widget.focus?.review.reviewRef.id) {
      WidgetsBinding.instance.addPostFrameCallback((_) => _load());
    }
  }

  void _load() {
    if (!mounted) return;
    _replacementAcknowledged = false;
    widget.controller.load();
    final focus = widget.focus;
    if (focus != null) widget.controller.usePreparedReview(focus.review);
  }

  Future<void> _retryPending() async {
    final result = await widget.controller.retryPendingCommand();
    if (!mounted || result == null) return;
    if (result.kind == AgentRegistryCommandKind.bindingReplace) {
      await _handleBindingReplaced();
    }
  }

  Future<void> _handleBindingReplaced() async {
    if (mounted) setState(() => _replacementAcknowledged = true);
    await widget.onBindingReplaced?.call();
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: widget.controller,
    builder: (context, _) {
      final strings = AppLocalizations.of(context);
      final controller = widget.controller;
      final directory = controller.directory;
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(strings.agentRegistryTitle, style: FloeType.title),
          if (widget.focus != null) ...[
            const SizedBox(height: FloeSpace.xs),
            Text(widget.focus!.review.requirementRef, style: FloeType.body),
          ],
          const SizedBox(height: FloeSpace.xs),
          Text(
            strings.agentRegistryBoundary,
            style: FloeType.body.copyWith(color: FloePalette.neutral600),
          ),
          const SizedBox(height: FloeSpace.base),
          if (!controller.available)
            Text(strings.agentRegistryFailure)
          else if (controller.failure != null)
            Semantics(
              liveRegion: true,
              child: Text(strings.agentRegistryFailure),
            )
          else if (controller.loaded &&
              (directory == null || directory.installations.isEmpty))
            Text(strings.agentRegistryEmpty)
          else if (directory != null)
            for (final installation in directory.installations)
              Padding(
                padding: const EdgeInsets.only(bottom: FloeSpace.sm),
                child: _InstallationCard(
                  controller: controller,
                  installation: installation,
                  focusedReview: _replacementAcknowledged
                      ? null
                      : widget.focus?.review,
                  onBindingReplaced: _handleBindingReplaced,
                ),
              ),
          if (controller.reviewFailure != null)
            Semantics(
              liveRegion: true,
              child: Text(strings.agentRegistryFailure),
            ),
          if (controller.pendingCommandKind != null) ...[
            const SizedBox(height: FloeSpace.xs),
            Align(
              alignment: Alignment.centerLeft,
              child: FloeButton.outlined(
                key: const ValueKey('expert-command-retry'),
                onPressed: controller.canRetryPending ? _retryPending : null,
                child: const Text('Retry pending Expert change'),
              ),
            ),
          ],
          Align(
            alignment: Alignment.centerLeft,
            child: FloeButton.text(
              onPressed: controller.canRead ? controller.load : null,
              child: Text(strings.agentRegistryRefresh),
            ),
          ),
        ],
      );
    },
  );
}

class _InstallationCard extends StatelessWidget {
  const _InstallationCard({
    required this.controller,
    required this.installation,
    required this.focusedReview,
    required this.onBindingReplaced,
  });

  final AgentRegistryController controller;
  final AgentInstallation installation;
  final AgentBindingReview? focusedReview;
  final Future<void> Function()? onBindingReplaced;

  @override
  Widget build(BuildContext context) {
    final strings = AppLocalizations.of(context);
    final directory = controller.directory!;
    final assignments = directory.assignments
        .where((entry) => entry.installationRef == installation.installationRef)
        .toList(growable: false);
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
            key: ValueKey('installation-${installation.installationRef}'),
            value: installation.enabled,
            onChanged: controller.canManage
                ? (enabled) =>
                      controller.setInstallationEnabled(installation, enabled)
                : null,
            label: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(installation.displayName, style: FloeType.controlLabel),
                const SizedBox(height: FloeSpace.xxs),
                Text(
                  installation.version,
                  style: FloeType.bodySmall.copyWith(
                    color: FloePalette.neutral600,
                    fontSize: 12,
                    height: 1.4,
                  ),
                ),
              ],
            ),
          ),
          for (final assignment in assignments) ...[
            const SizedBox(height: FloeSpace.xs),
            Text(
              '${assignment.displayName} · ${assignment.enabled ? 'Enabled' : 'Disabled'}',
              style: FloeType.controlLabel,
            ),
            for (final requirement in assignment.requirements)
              _RequirementSection(
                controller: controller,
                assignment: assignment,
                requirement: requirement,
                focusedReview:
                    focusedReview?.assignmentRef == assignment.assignmentRef &&
                        focusedReview?.requirementRef ==
                            requirement.requirementRef
                    ? focusedReview
                    : null,
                onBindingReplaced: onBindingReplaced,
              ),
          ],
          if (assignments.isEmpty)
            Text(
              strings.expertSourceNoCompatible,
              style: FloeType.bodySmall.copyWith(color: FloePalette.neutral600),
            ),
        ],
      ),
    );
  }
}

class _RequirementSection extends StatefulWidget {
  const _RequirementSection({
    required this.controller,
    required this.assignment,
    required this.requirement,
    required this.focusedReview,
    required this.onBindingReplaced,
  });

  final AgentRegistryController controller;
  final AgentAssignment assignment;
  final AgentSourceRequirement requirement;
  final AgentBindingReview? focusedReview;
  final Future<void> Function()? onBindingReplaced;

  @override
  State<_RequirementSection> createState() => _RequirementSectionState();
}

class _RequirementSectionState extends State<_RequirementSection> {
  bool expanded = false;
  String? loadedReviewRef;
  Set<String> draft = {};

  @override
  void initState() {
    super.initState();
    expanded = widget.focusedReview != null;
    final focus = widget.focusedReview;
    if (focus != null) _resetDraft(focus);
  }

  @override
  void didUpdateWidget(_RequirementSection oldWidget) {
    super.didUpdateWidget(oldWidget);
    final focus = widget.focusedReview;
    if (focus != null &&
        oldWidget.focusedReview?.reviewRef.id != focus.reviewRef.id) {
      expanded = true;
      _resetDraft(focus);
    }
  }

  void _resetDraft(AgentBindingReview review) {
    loadedReviewRef = review.reviewRef.id;
    draft = review.candidates
        .where((candidate) => candidate.selected)
        .map((candidate) => candidate.candidateRef)
        .toSet();
  }

  bool _matchesReview(AgentBindingReview? review) =>
      review?.assignmentRef == widget.assignment.assignmentRef &&
      review?.requirementRef == widget.requirement.requirementRef;

  bool _matchesInspection(AgentBindingInspection? inspection) =>
      inspection?.assignmentRef == widget.assignment.assignmentRef &&
      inspection?.requirementRef == widget.requirement.requirementRef;

  @override
  Widget build(BuildContext context) {
    final strings = AppLocalizations.of(context);
    final review = _matchesReview(widget.controller.review)
        ? widget.controller.review
        : widget.focusedReview;
    final inspection = _matchesInspection(widget.controller.inspection)
        ? widget.controller.inspection
        : null;
    if (review != null && loadedReviewRef != review.reviewRef.id) {
      _resetDraft(review);
    }
    return ExpansionTile(
      key: ValueKey(
        'requirement-${widget.assignment.assignmentRef}-${widget.requirement.requirementRef}',
      ),
      initiallyExpanded: expanded,
      onExpansionChanged: (value) {
        setState(() => expanded = value);
        if (value && inspection == null && review == null) {
          widget.controller.inspectBinding(
            widget.assignment,
            widget.requirement,
          );
        }
      },
      title: Text(widget.requirement.label),
      subtitle: Text(
        '${widget.requirement.minimumSources == 0 ? strings.expertSourceOptional : strings.expertSourceRequired} · ${widget.requirement.selectedCount} ${strings.expertSourceSelected}',
      ),
      children: [
        Text(strings.expertSourceSelectionBoundary),
        if (widget.controller.busy && inspection == null && review == null)
          const CircularProgressIndicator(),
        if (inspection != null)
          for (final candidate in inspection.candidates)
            _InspectionRow(candidate: candidate),
        if (inspection == null &&
            review == null &&
            widget.requirement.selectedCount > 0)
          Text(strings.expertSourceUnavailable),
        if (inspection != null && inspection.candidates.isEmpty)
          Text(strings.expertSourceNoCompatible),
        if (review != null) ...[
          for (final candidate in review.candidates)
            _ReviewChoice(
              candidate: candidate,
              selected: draft.contains(candidate.candidateRef),
              onChanged: (value) => _setCandidate(review, candidate, value),
              enabled: _canChange(review, candidate),
            ),
          if (draft.length < widget.requirement.minimumSources)
            const Text(
              'This Expert needs more sources before it can run a Task.',
            ),
          if (review.expired) Text(strings.agentInteractionExpired),
          Wrap(
            spacing: FloeSpace.sm,
            children: [
              FloeButton.text(
                onPressed: widget.controller.canManage && review.canRefresh
                    ? () => widget.controller.refreshReview(
                        widget.assignment,
                        widget.requirement,
                      )
                    : null,
                child: Text(strings.agentRegistryRefresh),
              ),
              FloeButton.filled(
                onPressed: _canReplace(review) ? () => _replace(draft) : null,
                child: Text(strings.expertSourceSave),
              ),
              if (draft.isNotEmpty)
                FloeButton.text(
                  onPressed: widget.controller.canManage && review.canReplace
                      ? () => _replace(const {})
                      : null,
                  child: Text(strings.expertSourceRemove),
                ),
            ],
          ),
        ] else
          FloeButton.text(
            onPressed: widget.controller.canManage
                ? () => widget.controller.prepareReview(
                    widget.assignment,
                    widget.requirement,
                  )
                : null,
            child: const Text('Review sources'),
          ),
      ],
    );
  }

  bool _canChange(AgentBindingReview review, AgentBindingCandidate candidate) {
    if (!widget.controller.canManage || !review.canReplace) return false;
    if (candidate.availability == AgentCandidateAvailability.available) {
      return draft.contains(candidate.candidateRef) || draft.length < 16;
    }
    return candidate.selected && draft.contains(candidate.candidateRef);
  }

  void _setCandidate(
    AgentBindingReview review,
    AgentBindingCandidate candidate,
    bool? value,
  ) {
    if (value == null || !_canChange(review, candidate)) return;
    setState(() {
      if (value) {
        if (draft.length < 16) draft.add(candidate.candidateRef);
      } else {
        draft.remove(candidate.candidateRef);
      }
    });
  }

  bool _canReplace(AgentBindingReview review) =>
      widget.controller.canManage && review.canReplace && draft.length <= 16;

  Future<void> _replace(Set<String> refs) async {
    final replaced = await widget.controller.replaceBinding(refs);
    if (replaced && mounted) await widget.onBindingReplaced?.call();
  }
}

class _InspectionRow extends StatelessWidget {
  const _InspectionRow({required this.candidate});

  final AgentBindingInspectionCandidate candidate;

  @override
  Widget build(BuildContext context) => ListTile(
    dense: true,
    title: Text(candidate.label),
    subtitle: candidate.availability == AgentCandidateAvailability.unavailable
        ? Text(AppLocalizations.of(context).expertSourceUnavailable)
        : null,
    trailing: candidate.selected
        ? const Icon(Icons.check_circle_outline)
        : null,
  );
}

class _ReviewChoice extends StatelessWidget {
  const _ReviewChoice({
    required this.candidate,
    required this.selected,
    required this.onChanged,
    required this.enabled,
  });

  final AgentBindingCandidate candidate;
  final bool selected;
  final ValueChanged<bool?> onChanged;
  final bool enabled;

  @override
  Widget build(BuildContext context) => FloeCheckboxTile(
    value: selected,
    onChanged: enabled ? onChanged : null,
    title: Text(candidate.label),
    subtitle: Text(
      candidate.availability == AgentCandidateAvailability.unavailable
          ? AppLocalizations.of(context).expertSourceUnavailable
          : AppLocalizations.of(context).expertSourceSelectionBoundary,
    ),
  );
}
