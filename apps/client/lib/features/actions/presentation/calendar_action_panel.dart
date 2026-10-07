import 'dart:async';

import 'package:flutter/material.dart';
import 'package:intl/intl.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_badge.dart';
import 'package:floe_client/app/floe_button.dart';
import 'package:floe_client/app/floe_feedback.dart';
import 'package:floe_client/app/floe_loading.dart';
import 'package:floe_client/app/floe_squircle.dart';
import 'package:floe_client/features/actions/application/calendar_action_controller.dart';
import 'package:floe_client/features/actions/domain/calendar_action.dart';
import 'package:floe_client/l10n/app_localizations.dart';

String _statusLabel(AppLocalizations strings, CalendarAction action) =>
    switch (action.status.state) {
      CalendarActionState.pendingReview => strings.actionPending,
      CalendarActionState.approved => strings.actionApproved,
      CalendarActionState.rejected => strings.actionRejected,
      CalendarActionState.cancelled => 'Cancelled',
      CalendarActionState.expired => 'Expired',
      CalendarActionState.executing => strings.actionExecuting,
      CalendarActionState.blocked => strings.actionBlocked,
      CalendarActionState.failed => 'Not applied',
      CalendarActionState.unknown => strings.actionUnknown,
      CalendarActionState.succeeded => switch (action.effect) {
        CreateActionEffect() => strings.actionSucceeded,
        UpdateActionEffect() => 'Updated in Calendar.',
        DeleteActionEffect() => 'Deleted from Calendar.',
      },
    };

FloeBadgeTone _statusTone(CalendarAction action) =>
    switch (action.status.state) {
      CalendarActionState.succeeded => FloeBadgeTone.success,
      CalendarActionState.rejected ||
      CalendarActionState.cancelled ||
      CalendarActionState.expired ||
      CalendarActionState.blocked ||
      CalendarActionState.failed => FloeBadgeTone.danger,
      CalendarActionState.pendingReview ||
      CalendarActionState.approved ||
      CalendarActionState.executing ||
      CalendarActionState.unknown => FloeBadgeTone.info,
    };

bool _hasDecision(CalendarAction action) =>
    action.allowedActions.contains(ActionAllowedAction.approve) ||
    action.allowedActions.contains(ActionAllowedAction.reject) ||
    action.allowedActions.contains(ActionAllowedAction.cancel);

class ReviewRequestPanel extends StatelessWidget {
  const ReviewRequestPanel({super.key, required this.controller});

  final CalendarActionController controller;

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: controller,
    builder: (context, _) {
      final strings = AppLocalizations.of(context);
      final requests = controller.actions.where(_hasDecision).toList();
      return FloeSquircle(
        padding: const EdgeInsets.all(24),
        child: FloeLoadingOverlay(
          loading: controller.busy,
          label: strings.actionLoading,
          blockInteraction: false,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Text(strings.calendarProposals, style: FloeType.title),
              const SizedBox(height: 12),
              if (controller.error case final error?) Text(error.message),
              if (!controller.loaded &&
                  !controller.busy &&
                  controller.error == null)
                const Text('Load Actions to see requests for review.'),
              if (controller.loaded &&
                  !controller.busy &&
                  controller.error == null &&
                  requests.isEmpty)
                Text(strings.actionEmpty),
              for (final action in requests)
                Padding(
                  padding: const EdgeInsets.only(top: 12),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      Text(action.title),
                      Align(
                        alignment: AlignmentDirectional.centerStart,
                        child: FloeBadge(
                          label: _statusLabel(strings, action),
                          tone: _statusTone(action),
                        ),
                      ),
                      FloeButton.text(
                        onPressed: () {
                          unawaited(
                            controller
                                .inspect(action.actionRef)
                                .then<void>((_) {}, onError: (Object _) {}),
                          );
                          showFloeDialog<void>(
                            context,
                            (_) => ActionReviewDialog(
                              controller: controller,
                              actionRef: action.actionRef,
                            ),
                          );
                        },
                        child: Text(strings.actionReview),
                      ),
                    ],
                  ),
                ),
              FloeButton.text(
                onPressed: controller.busy ? null : controller.load,
                child: Text(strings.actionReload),
              ),
            ],
          ),
        ),
      );
    },
  );
}

class ActivityPanel extends StatelessWidget {
  const ActivityPanel({super.key, required this.controller});

  final CalendarActionController controller;

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: controller,
    builder: (context, _) {
      final strings = AppLocalizations.of(context);
      final history = controller.actions
          .where((action) => !_hasDecision(action))
          .toList(growable: false);
      return FloeLoadingOverlay(
        loading: controller.busy,
        label: strings.actionLoading,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Expanded(child: Text('Activity', style: FloeType.pageTitle)),
                FloeButton.icon(
                  tooltip: 'Reload activity',
                  onPressed: controller.busy ? null : controller.load,
                  icon: const Icon(LucideIcons.refreshCw, size: 18),
                ),
              ],
            ),
            const SizedBox(height: 10),
            const Text('Actions submitted by you or Experts appear here.'),
            const SizedBox(height: 28),
            if (controller.error case final error?) Text(error.message),
            if (!controller.loaded &&
                !controller.busy &&
                controller.error == null)
              const Text('Load Actions to see Activity.'),
            if (controller.loaded &&
                !controller.busy &&
                controller.error == null &&
                history.isEmpty)
              const Text('No activity yet.'),
            for (final action in history)
              Padding(
                padding: const EdgeInsets.only(bottom: 12),
                child: FloeSquircle(
                  padding: const EdgeInsets.all(20),
                  child: Row(
                    children: [
                      Expanded(
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(action.title, style: FloeType.controlLabel),
                            const SizedBox(height: 4),
                            Text(action.destinationLabel),
                            const SizedBox(height: 4),
                            FloeBadge(
                              label: _statusLabel(strings, action),
                              tone: _statusTone(action),
                            ),
                            if (action.status.state ==
                                    CalendarActionState.succeeded &&
                                action.status.collection ==
                                    ActionCollectionStatus.pending) ...[
                              const SizedBox(height: 6),
                              const Text(
                                'Calendar change succeeded; Day collection is pending.',
                              ),
                            ],
                          ],
                        ),
                      ),
                      FloeButton.text(
                        onPressed: () => showFloeDialog<void>(
                          context,
                          (_) => ActionReviewDialog(
                            controller: controller,
                            actionRef: action.actionRef,
                          ),
                        ),
                        child: const Text('View details'),
                      ),
                    ],
                  ),
                ),
              ),
            if (controller.nextCursor != null)
              FloeButton.text(
                onPressed: controller.busy ? null : controller.loadMore,
                child: const Text('Load more'),
              ),
          ],
        ),
      );
    },
  );
}

class ActionReviewDialog extends StatelessWidget {
  const ActionReviewDialog({
    super.key,
    required this.controller,
    required this.actionRef,
  });

  final CalendarActionController controller;
  final String actionRef;

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: controller,
    builder: (context, _) {
      final strings = AppLocalizations.of(context);
      final action = controller.find(actionRef);
      final locale = Localizations.localeOf(context).toLanguageTag();
      String localDateTime(DateTime value) =>
          DateFormat.yMMMd(locale).add_jm().format(value.toLocal());
      String localInterval(ActionSchedule value) {
        final start = value.startsAt.toLocal();
        final end = value.endsAt.toLocal();
        final sameDay =
            start.year == end.year &&
            start.month == end.month &&
            start.day == end.day;
        return '${localDateTime(start)} – ${sameDay ? DateFormat.jm(locale).format(end) : localDateTime(end)}';
      }

      return FloeDetailDialog(
        title: action?.origin == CalendarActionOrigin.direct
            ? 'Calendar activity'
            : strings.actionReview,
        loading: controller.busy,
        loadingLabel: strings.actionLoading,
        children: [
          if (action == null)
            Text(strings.actionMissing)
          else ...[
            Text(action.title, style: FloeType.titleLarge),
            if (action.origin == CalendarActionOrigin.expert) ...[
              const SizedBox(height: 8),
              Text(strings.actionSuggestedByFloe),
            ],
            const SizedBox(height: 16),
            Text(strings.actionDestination, style: FloeType.controlLabel),
            Text(action.destinationLabel),
            const SizedBox(height: 12),
            Text(strings.actionWhen, style: FloeType.controlLabel),
            Text(localInterval(action.schedule)),
            if (action.effect case UpdateActionEffect(
              :final previousTitle,
            )) ...[
              const SizedBox(height: 12),
              const Text('Current event title', style: FloeType.controlLabel),
              Text(previousTitle),
            ],
            if (action.effect case DeleteActionEffect()) ...[
              const SizedBox(height: 12),
              Text(strings.actionNoExtras),
            ],
            const SizedBox(height: 16),
            Semantics(
              liveRegion: true,
              child: Align(
                alignment: AlignmentDirectional.centerStart,
                child: FloeBadge(
                  label: _statusLabel(strings, action),
                  tone: _statusTone(action),
                ),
              ),
            ),
            if (action.status.state == CalendarActionState.blocked)
              Text(_blockedMessage(strings, action.status.blockedReason)),
            if (action.status.state == CalendarActionState.failed)
              Text(_notAppliedMessage(action.status.failedReason)),
            if (action.status.state == CalendarActionState.unknown)
              Text(strings.actionUnknown),
            if (action.status.state == CalendarActionState.succeeded &&
                action.status.collection == ActionCollectionStatus.pending) ...[
              const SizedBox(height: 8),
              const Text(
                'Calendar change succeeded; Day collection is pending.',
              ),
            ],
            if (action.status.state == CalendarActionState.succeeded &&
                action.status.collection ==
                    ActionCollectionStatus.collected) ...[
              const SizedBox(height: 8),
              Text(strings.actionCollected),
            ],
            if (controller.error case final error?) ...[
              const SizedBox(height: 12),
              Text(error.message),
            ],
            const SizedBox(height: 12),
            if (action.allowedActions.contains(ActionAllowedAction.approve) &&
                !controller.calendarChangesAvailable) ...[
              const Text(
                'Calendar changes are unavailable. Refresh to check for writable Calendar destinations.',
              ),
              FloeButton.text(
                onPressed: controller.busy ? null : controller.load,
                child: Text(strings.actionReload),
              ),
            ],
            if (action.allowedActions.contains(ActionAllowedAction.approve))
              FloeButton.filled(
                onPressed:
                    controller.busy || !controller.calendarChangesAvailable
                    ? null
                    : () => _decide(
                        controller,
                        action,
                        CalendarActionDecision.approve,
                      ),
                child: Text(strings.actionApproveCreate),
              ),
            if (action.allowedActions.contains(ActionAllowedAction.reject))
              FloeButton.outlined(
                onPressed: controller.busy
                    ? null
                    : () => _decide(
                        controller,
                        action,
                        CalendarActionDecision.reject,
                      ),
                child: Text(strings.actionDecline),
              ),
            if (action.allowedActions.contains(ActionAllowedAction.cancel))
              FloeButton.outlined(
                onPressed: controller.busy
                    ? null
                    : () => _decide(
                        controller,
                        action,
                        CalendarActionDecision.cancel,
                      ),
                child: const Text('Cancel Action'),
              ),
            if (action.allowedActions.contains(ActionAllowedAction.reconcile))
              FloeButton.outlined(
                onPressed: controller.busy
                    ? null
                    : () => _reconcile(controller, action),
                child: Text(strings.actionCheckCalendar),
              ),
            ExpansionTile(
              title: Text(strings.actionTechnicalDetails),
              tilePadding: EdgeInsets.zero,
              children: [
                for (final entry in <String, String>{
                  strings.actionProposalId: action.actionRef,
                  'Review reference': action.reviewRef.id,
                  'Revision': action.revision.toString(),
                  if (action.effect case UpdateActionEffect(:final eventRef))
                    'Event reference': eventRef,
                  if (action.effect case DeleteActionEffect(:final eventRef))
                    'Event reference': eventRef,
                  if (action.status.blockedReason case final reason?)
                    'Blocked reason': reason.name,
                  if (action.status.failedReason case final reason?)
                    'Not applied reason': reason.name,
                  if (action.status.unknownReason case final reason?)
                    'Unconfirmed reason': reason.name,
                }.entries)
                  Padding(
                    padding: const EdgeInsets.only(bottom: 12),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        Text(entry.key, style: FloeType.controlLabel),
                        SelectableText(entry.value),
                      ],
                    ),
                  ),
              ],
            ),
          ],
          FloeButton.text(
            onPressed: controller.busy
                ? null
                : () => unawaited(
                    controller
                        .inspect(actionRef)
                        .then<void>((_) {}, onError: (Object _) {}),
                  ),
            child: Text(strings.actionReload),
          ),
        ],
      );
    },
  );
}

String _blockedMessage(AppLocalizations strings, ActionBlockedReason? reason) =>
    switch (reason) {
      ActionBlockedReason.permissionDenied => strings.actionPermissionReason,
      ActionBlockedReason.policyDenied =>
        'Blocked by the current Actions policy.',
      ActionBlockedReason.sourceChanged =>
        'The Calendar source changed. Review a new Action.',
      ActionBlockedReason.executorUnavailable =>
        'The Calendar action executor is unavailable.',
      ActionBlockedReason.scheduleConflict => strings.actionConflictReason,
      null => strings.actionUnavailableReason,
    };

String _notAppliedMessage(ActionNotAppliedReason? reason) => switch (reason) {
  ActionNotAppliedReason.permissionDenied => 'The owner confirmed the change was not applied because write permission was denied.',
  ActionNotAppliedReason.providerRejected => 'The owner confirmed the provider rejected the change before it was applied.',
  ActionNotAppliedReason.providerUnavailable => 'The owner confirmed the provider was unavailable before the change was applied.',
  ActionNotAppliedReason.sourceChanged =>
    'The Calendar source changed before this action. No change was applied.',
  ActionNotAppliedReason.cancelled =>
    'Cancelled before any Calendar change was made.',
  ActionNotAppliedReason.timeout =>
    'Timed out before any Calendar change was made.',
  null => 'The owner confirmed this change was not applied.',
};

void _decide(
  CalendarActionController controller,
  CalendarAction action,
  CalendarActionDecision decision,
) {
  if (decision == CalendarActionDecision.approve &&
      !controller.calendarChangesAvailable)
    return;
  unawaited(
    controller
        .decide(action, decision)
        .then<void>((_) {}, onError: (Object _) {}),
  );
}

void _reconcile(CalendarActionController controller, CalendarAction action) {
  unawaited(
    controller.reconcile(action).then<void>((_) {}, onError: (Object _) {}),
  );
}
