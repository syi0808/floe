import 'dart:async';

import 'package:flutter/material.dart';
import 'package:floe_client/l10n/app_localizations.dart';

import '../../../app/floe_button.dart';
import '../../../app/floe_feedback.dart';
import '../../../app/floe_squircle.dart';
import '../application/calendar_action_controller.dart';
import '../domain/calendar_action.dart';
import '../domain/day_models.dart';
import '../application/calendar_action_gateway.dart';
import 'calendar_action_proposal.dart';

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

class CalendarActionPanel extends StatelessWidget {
  const CalendarActionPanel({
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
      return FloeSquircle(
        padding: const EdgeInsets.all(24),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Text(
              strings.calendarProposals,
              style: const TextStyle(fontSize: 16, fontWeight: FontWeight.w600),
            ),
            const SizedBox(height: 12),
            if (controller.gateway is CalendarActionExecutionGateway)
              FloeButton.outlined(
                onPressed: controller.canPropose && connection() != null
                    ? () => showFloeDialog<void>(
                        context,
                        (_) => CalendarActionProposal(
                          controller: controller,
                          connection: connection,
                        ),
                      )
                    : null,
                child: Text(strings.actionNewProposal),
              ),
            if (controller.failed) Text(strings.actionReloadRequired),
            if (controller.busy) Text(strings.actionLoading),
            if (!controller.busy &&
                !controller.failed &&
                controller.actions.isEmpty)
              Text(strings.actionEmpty),
            for (final action in controller.actions)
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
                          (_) => CalendarActionDialog(
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
      );
    },
  );
}

class CalendarActionDialog extends StatefulWidget {
  const CalendarActionDialog({
    super.key,
    required this.controller,
    required this.actionId,
    required this.connection,
  });
  final CalendarActionController controller;
  final String actionId;
  final CalendarConnection? Function() connection;

  @override
  State<CalendarActionDialog> createState() => _CalendarActionDialogState();
}

class _CalendarActionDialogState extends State<CalendarActionDialog> {
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
      final canApprove =
          action != null &&
          controller.canApprove(action, widget.connection(), DateTime.now());
      return FloeDetailDialog(
        title: strings.actionReview,
        children: [
          if (action == null)
            Text(strings.actionMissing)
          else ...[
            Text(
              action.title,
              style: const TextStyle(fontSize: 18, fontWeight: FontWeight.w600),
            ),
            const SizedBox(height: 16),
            for (final entry in <String, String>{
              strings.actionDestination:
                  '${action.calendarName} · ${action.provider}\n${action.calendarId}',
              strings.actionStart: action.startsAt.toUtc().toIso8601String(),
              strings.actionEnd: action.endsAt.toUtc().toIso8601String(),
              strings.sourceTimeZone: action.timezone,
              strings.actionPerson: action.personId,
              strings.actionExpires: action.expiresAt.toUtc().toIso8601String(),
              strings.actionProposalId: action.id,
              strings.actionExecutionId: action.executionId,
              if (action.approvedAt != null)
                strings.actionApprovedAt: action.approvedAt!
                    .toUtc()
                    .toIso8601String(),
              if (action.externalId != null)
                strings.actionExternalId: action.externalId!,
              if (action.reason != null) strings.actionReason: action.reason!,
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
            Text(strings.actionNoExtras),
            const SizedBox(height: 16),
            Semantics(
              liveRegion: true,
              child: Text(_status(strings, action.status)),
            ),
            const SizedBox(height: 12),
            Text(
              controller.writesEnabled
                  ? strings.actionWriteEnabled
                  : strings.actionWriteDisabled,
            ),
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
              Text(strings.actionApprovalUnavailable),
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
          ],
          if (controller.busy) Text(strings.actionLoading),
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
