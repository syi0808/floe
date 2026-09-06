import 'dart:async';

import 'package:flutter/material.dart';
import 'package:floe_client/l10n/app_localizations.dart';
import 'package:intl/intl.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../../app/floe_button.dart';
import '../../../app/floe_feedback.dart';
import '../../../app/floe_loading.dart';
import '../../../app/floe_squircle.dart';
import '../application/calendar_action_controller.dart';
import '../domain/calendar_action.dart';
import '../domain/day_models.dart';
import '../application/calendar_action_gateway.dart';

String _status(AppLocalizations strings, CalendarActionStatus status) =>
    switch (status) {
      CalendarActionStatus.pending => strings.actionPending,
      CalendarActionStatus.approved => strings.actionApproved,
      CalendarActionStatus.rejected => strings.actionRejected,
      CalendarActionStatus.executing => strings.actionExecuting,
      CalendarActionStatus.blocked => strings.actionBlocked,
      CalendarActionStatus.unknown => strings.actionUnknown,
      CalendarActionStatus.succeeded => strings.actionSucceeded,
    };

class ReviewRequestPanel extends StatelessWidget {
  const ReviewRequestPanel({
    super.key,
    required this.controller,
    required this.connection,
  });
  final CalendarActionController controller;
  final CalendarConnection? Function() connection;

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: controller,
    builder: (context, _) {
      final strings = AppLocalizations.of(context);
      final requests = controller.actions
          .where((action) => action.status.needsReview)
          .toList(growable: false);
      return FloeSquircle(
        padding: const EdgeInsets.all(24),
        child: FloeLoadingOverlay(
          loading: controller.busy,
          label: strings.actionLoading,
          blockInteraction: false,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Text(
                strings.calendarProposals,
                style: const TextStyle(
                  fontSize: 16,
                  fontWeight: FontWeight.w600,
                ),
              ),
              const SizedBox(height: 12),
              if (controller.failed) Text(strings.actionReloadRequired),
              if (!controller.busy && !controller.failed && requests.isEmpty)
                Text(strings.actionEmpty),
              for (final action in requests)
                Padding(
                  padding: const EdgeInsets.only(top: 12),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      Text(action.title),
                      Text(_status(strings, action.status)),
                      FloeButton.text(
                        onPressed: () {
                          controller.load();
                          showFloeDialog<void>(
                            context,
                            (_) => ActionReviewDialog(
                              controller: controller,
                              actionId: action.id,
                              connection: connection,
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
  const ActivityPanel({
    super.key,
    required this.controller,
    required this.connection,
  });

  final CalendarActionController controller;
  final CalendarConnection? Function() connection;

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: controller,
    builder: (context, _) {
      final strings = AppLocalizations.of(context);
      final history = controller.actions
          .where((action) => !action.status.needsReview)
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
                const Expanded(
                  child: Text(
                    'Activity',
                    style: TextStyle(fontSize: 28, fontWeight: FontWeight.w700),
                  ),
                ),
                FloeButton.icon(
                  tooltip: 'Reload activity',
                  onPressed: controller.busy ? null : controller.load,
                  icon: const Icon(LucideIcons.refreshCw, size: 18),
                ),
              ],
            ),
            const SizedBox(height: 10),
            const Text(
              'Automatic actions, decisions, and completed reviews appear here.',
            ),
            const SizedBox(height: 28),
            if (controller.failed) Text(strings.actionReloadRequired),
            if (!controller.busy && !controller.failed && history.isEmpty)
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
                            Text(
                              action.title,
                              style: const TextStyle(
                                fontWeight: FontWeight.w600,
                              ),
                            ),
                            const SizedBox(height: 4),
                            Text(_status(strings, action.status)),
                          ],
                        ),
                      ),
                      FloeButton.text(
                        onPressed: () => showFloeDialog<void>(
                          context,
                          (_) => ActionReviewDialog(
                            controller: controller,
                            actionId: action.id,
                            connection: connection,
                          ),
                        ),
                        child: const Text('View details'),
                      ),
                    ],
                  ),
                ),
              ),
          ],
        ),
      );
    },
  );
}

class ActionReviewDialog extends StatefulWidget {
  const ActionReviewDialog({
    super.key,
    required this.controller,
    required this.actionId,
    required this.connection,
  });
  final CalendarActionController controller;
  final String actionId;
  final CalendarConnection? Function() connection;

  @override
  State<ActionReviewDialog> createState() => _ActionReviewDialogState();
}

class _ActionReviewDialogState extends State<ActionReviewDialog> {
  late final Timer _timer;

  @override
  void initState() {
    super.initState();
    _timer = Timer.periodic(const Duration(seconds: 1), (_) => setState(() {}));
  }

  @override
  void dispose() {
    _timer.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: widget.controller,
    builder: (context, _) {
      final strings = AppLocalizations.of(context);
      final controller = widget.controller;
      final action = controller.find(widget.actionId);
      final locale = Localizations.localeOf(context).toLanguageTag();
      String localDateTime(DateTime value) =>
          DateFormat.yMMMd(locale).add_jm().format(value.toLocal());
      String localInterval(CalendarAction value) {
        final start = value.startsAt.toLocal();
        final end = value.endsAt.toLocal();
        final sameDay =
            start.year == end.year &&
            start.month == end.month &&
            start.day == end.day;
        return '${localDateTime(start)} – ${sameDay ? DateFormat.jm(locale).format(end) : localDateTime(end)}';
      }

      final canApprove =
          action != null &&
          controller.canApprove(action, widget.connection(), DateTime.now());
      return FloeDetailDialog(
        title: strings.actionReview,
        loading: controller.busy,
        loadingLabel: strings.actionLoading,
        children: [
          if (action == null)
            Text(strings.actionMissing)
          else ...[
            Text(
              action.title,
              style: const TextStyle(fontSize: 18, fontWeight: FontWeight.w600),
            ),
            const SizedBox(height: 16),
            Text(
              strings.actionDestination,
              style: const TextStyle(fontWeight: FontWeight.w600),
            ),
            Text(action.calendarName),
            const SizedBox(height: 12),
            Text(
              strings.actionWhen,
              style: const TextStyle(fontWeight: FontWeight.w600),
            ),
            Text(localInterval(action)),
            const SizedBox(height: 12),
            Text(strings.actionNoExtras),
            const SizedBox(height: 16),
            if (!action.status.canDecide)
              Semantics(
                liveRegion: true,
                child: Text(_status(strings, action.status)),
              ),
            if (action.status == CalendarActionStatus.blocked)
              Text(switch (action.reason) {
                'schedule_conflict' => strings.actionConflictReason,
                'expired' => strings.actionExpiredReason,
                'permission_denied' => strings.actionPermissionReason,
                'calendar_changed' => strings.actionChangedReason,
                'invalid_timezone' => strings.actionTimezoneReason,
                _ => strings.actionUnavailableReason,
              }),
            const SizedBox(height: 12),
            if (!controller.writesEnabled) Text(strings.actionWriteDisabled),
            if (controller.phase case final phase?)
              Text(switch (phase) {
                'executing' => strings.actionCheckingCreating,
                'recovering' => strings.actionLookingUp,
                _ => strings.actionCollecting,
              }),
            if (controller.collection[action.id] case final collected?)
              Text(
                collected == 'collected'
                    ? strings.actionCollected
                    : collected == 'failed'
                    ? strings.actionReadFailed
                    : strings.actionCollecting,
              ),
            if (action.status == CalendarActionStatus.approved &&
                controller.writesEnabled)
              FloeButton.filled(
                onPressed:
                    controller.canApprove(
                      action,
                      widget.connection(),
                      DateTime.now(),
                      approved: true,
                    )
                    ? () => controller.run(
                        action.id,
                        connection: widget.connection(),
                      )
                    : null,
                child: Text(strings.actionExecuteApproved),
              ),
            if ((action.status == CalendarActionStatus.unknown ||
                    action.status == CalendarActionStatus.executing) &&
                controller.gateway is CalendarActionExecutionGateway)
              FloeButton.outlined(
                onPressed: controller.busy || controller.needsReload
                    ? null
                    : () => controller.run(action.id, recover: true),
                child: Text(strings.actionCheckCalendar),
              ),
            if (action.status == CalendarActionStatus.succeeded)
              FloeButton.outlined(
                onPressed: controller.busy
                    ? null
                    : () => controller.retryRead(action.id),
                child: Text(strings.actionRetryRead),
              ),
            if (action.status.canDecide && !canApprove) ...[
              const SizedBox(height: 12),
              Text(
                !DateTime.now().isBefore(action.expiresAt)
                    ? strings.actionExpiredReason
                    : strings.actionApprovalUnavailable,
              ),
            ],
            if (action.status.canDecide) ...[
              const SizedBox(height: 16),
              Wrap(
                spacing: 12,
                runSpacing: 12,
                children: [
                  FloeButton.outlined(
                    onPressed: controller.busy || controller.needsReload
                        ? null
                        : () => controller.decide(
                            action.id,
                            CalendarActionDecision.reject,
                            widget.connection(),
                            DateTime.now(),
                          ),
                    child: Text(strings.actionDecline),
                  ),
                  FloeButton.filled(
                    onPressed: canApprove
                        ? () => controller.decide(
                            action.id,
                            CalendarActionDecision.approve,
                            widget.connection(),
                            DateTime.now(),
                          )
                        : null,
                    child: Text(
                      controller.writesEnabled
                          ? strings.actionApproveCreate
                          : strings.actionSaveApproval,
                    ),
                  ),
                ],
              ),
            ],
            ExpansionTile(
              title: Text(strings.actionTechnicalDetails),
              tilePadding: EdgeInsets.zero,
              children: [
                for (final entry in <String, String>{
                  strings.actionProvider: action.provider,
                  strings.actionCalendarId: action.calendarId,
                  strings.actionStart: localDateTime(action.startsAt),
                  strings.actionEnd: localDateTime(action.endsAt),
                  strings.actionPerson: action.personId,
                  strings.actionExpires: localDateTime(action.expiresAt),
                  strings.actionProposalId: action.id,
                  strings.actionExecutionId: action.executionId,
                  if (action.approvedAt != null)
                    strings.actionApprovedAt: localDateTime(action.approvedAt!),
                  if (action.externalId != null)
                    strings.actionExternalId: action.externalId!,
                  if (action.reason != null)
                    strings.actionReason: action.reason!,
                }.entries)
                  Padding(
                    padding: const EdgeInsets.only(bottom: 12),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        Text(
                          entry.key,
                          style: const TextStyle(fontWeight: FontWeight.w600),
                        ),
                        SelectableText(entry.value),
                      ],
                    ),
                  ),
              ],
            ),
          ],
          if (controller.failed) ...[
            const SizedBox(height: 16),
            Text(strings.actionReloadRequired),
          ],
          FloeButton.text(
            onPressed: controller.busy ? null : controller.load,
            child: Text(strings.actionReload),
          ),
        ],
      );
    },
  );
}
