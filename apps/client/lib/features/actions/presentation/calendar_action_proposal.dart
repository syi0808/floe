import 'dart:async';
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:floe_client/l10n/app_localizations.dart';

import 'package:floe_client/app/floe_button.dart';
import 'package:floe_client/app/floe_feedback.dart';
import 'package:floe_client/app/floe_input.dart';
import 'package:floe_client/app/floe_selection.dart';
import 'package:floe_client/features/actions/application/calendar_action_controller.dart';
import 'package:floe_client/features/actions/domain/calendar_action.dart';
import 'package:floe_client/features/day/domain/day_models.dart';
import 'package:floe_client/features/day/presentation/calendar_date_time_field.dart';

class CalendarEventComposer extends StatefulWidget {
  const CalendarEventComposer({
    super.key,
    required this.controller,
    this.initialStart,
    this.event,
  });

  final CalendarActionController controller;
  final DateTime? initialStart;
  final EventItem? event;

  @override
  State<CalendarEventComposer> createState() => _CalendarEventComposerState();
}

class _CalendarEventComposerState extends State<CalendarEventComposer> {
  final form = GlobalKey<FormState>();
  final title = TextEditingController();
  late DateTime start;
  late DateTime end;
  String? destinationRef;
  CalendarAction? submittedAction;
  bool saving = false;

  @override
  void initState() {
    super.initState();
    final now = DateTime.now();
    final next =
        widget.initialStart ??
        DateTime(now.year, now.month, now.day, now.hour + 1);
    final event = widget.event;
    start = widget.initialStart ?? event?.startsAt.toLocal() ?? next;
    end = event == null
        ? start.add(const Duration(minutes: 45))
        : start.add(event.endsAt.difference(event.startsAt));
    title.text = widget.event?.title ?? '';
    final destinations = widget.controller.destinations;
    destinationRef = destinations.isEmpty
        ? null
        : destinations.first.destinationRef;
    if (!widget.controller.loaded || !widget.controller.destinationsLoaded) {
      unawaited(widget.controller.load());
    }
  }

  @override
  void dispose() {
    title.dispose();
    super.dispose();
  }

  bool _validTitle(String? value) {
    if (value == null || value.trim().isEmpty) return false;
    return utf8.encode(value.trim()).length <= 1024 &&
        !value.trim().runes.any(
          (rune) => rune < 32 || (rune >= 127 && rune <= 159),
        );
  }

  Future<void> save() async {
    if (saving ||
        widget.controller.busy ||
        !widget.controller.calendarChangesAvailable ||
        submittedAction != null ||
        !form.currentState!.validate()) {
      return;
    }
    final event = widget.event;
    final target = event?.actionTarget;
    if (event == null &&
            !widget.controller.destinations.any(
              (choice) => choice.destinationRef == destinationRef,
            ) ||
        event != null && target == null) {
      return;
    }
    setState(() => saving = true);
    final schedule = ActionSchedule(
      startsAt: start.toUtc(),
      endsAt: end.toUtc(),
      timezone:
          event?.timezone ?? calendarStorageTimezone(start.timeZoneOffset),
    );
    final ActionIntent intent = event == null
        ? DirectCreate(
            destinationRef: destinationRef!,
            title: title.text.trim(),
            schedule: schedule,
          )
        : DirectUpdate(
            eventRef: target!.eventId,
            expectedRevision: target.expectedRevision,
            title: title.text.trim(),
            schedule: schedule,
          );
    try {
      final action = await widget.controller.submit(intent);
      if (!mounted) return;
      setState(() {
        submittedAction = action;
        saving = false;
      });
    } on Object {
      if (!mounted) return;
      setState(() => saving = false);
    }
  }

  Future<void> reconcile(CalendarAction action) async {
    try {
      final latest = widget.controller.find(action.actionRef) ?? action;
      final result = await widget.controller.reconcile(latest);
      if (mounted) setState(() => submittedAction = result);
    } on Object {
      // The controller retains the owner result state for this view.
    }
  }

  String _statusText(CalendarAction action) => switch (action.status.state) {
    CalendarActionState.pendingReview => 'Action submitted for review.',
    CalendarActionState.approved =>
      'Action approved; the owner is processing it.',
    CalendarActionState.rejected =>
      'Action rejected. No Calendar change was made.',
    CalendarActionState.cancelled => 'Action cancelled.',
    CalendarActionState.expired => 'Action expired.',
    CalendarActionState.executing => 'Action is in progress.',
    CalendarActionState.blocked =>
      'Action blocked: ${action.status.blockedReason!.name}.',
    CalendarActionState.failed => switch (action.status.failedReason) {
      ActionNotAppliedReason.sourceChanged => 'The Calendar source changed before this action. No change was applied.',
      ActionNotAppliedReason.cancelled =>
        'Cancelled before any Calendar change was made.',
      ActionNotAppliedReason.timeout =>
        'Timed out before any Calendar change was made.',
      _ => 'The owner confirmed the change was not applied.',
    },
    CalendarActionState.unknown => 'Action result is unconfirmed. Reconcile this Action before creating another.',
    CalendarActionState.succeeded =>
      action.status.collection == ActionCollectionStatus.pending
          ? 'Calendar change succeeded; Day collection is pending.'
          : 'Calendar change succeeded and was collected.',
  };

  @override
  Widget build(BuildContext context) {
    final strings = AppLocalizations.of(context);
    return AnimatedBuilder(
      animation: widget.controller,
      builder: (context, _) {
        final destinations = widget.controller.destinations;
        final event = widget.event;
        final target = event?.actionTarget;
        final submitted = submittedAction == null
            ? null
            : widget.controller.find(submittedAction!.actionRef) ??
                  submittedAction;
        final destinationError = widget.controller.destinationsError;
        final canSubmit =
            !saving &&
            !widget.controller.busy &&
            widget.controller.error == null &&
            widget.controller.calendarChangesAvailable &&
            submittedAction == null &&
            (event == null
                ? destinations.any(
                    (choice) => choice.destinationRef == destinationRef,
                  )
                : target != null);
        return FloeDetailDialog(
          title: event == null ? 'New event' : 'Edit event',
          loading: saving,
          loadingLabel: strings.actionLoading,
          children: [
            const Text(
              'Changes are submitted to Actions, which applies its current review and safety policy. Existing alerts are preserved. Recurring events and invitations are managed in the source calendar.',
            ),
            const SizedBox(height: 16),
            Form(
              key: form,
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  if (event == null)
                    FloeSelect<String>(
                      label: strings.actionDestination,
                      value:
                          destinations.any(
                            (choice) => choice.destinationRef == destinationRef,
                          )
                          ? destinationRef
                          : null,
                      options: destinations
                          .map(
                            (choice) => FloeSelectOption(
                              value: choice.destinationRef,
                              label: choice.label,
                            ),
                          )
                          .toList(growable: false),
                      enabled:
                          !saving &&
                          !widget.controller.busy &&
                          submittedAction == null,
                      onChanged: (value) =>
                          setState(() => destinationRef = value),
                      validator: (value) =>
                          value == null ? strings.actionFormInvalid : null,
                    )
                  else
                    Text(
                      target == null
                          ? 'This event has no safe Actions target and cannot be edited.'
                          : 'The current event is the target for this update.',
                    ),
                  if (event == null) const SizedBox(height: 12),
                  FloeInput(
                    label: strings.actionTitle,
                    controller: title,
                    enabled:
                        !saving &&
                        !widget.controller.busy &&
                        submittedAction == null,
                    validator: (value) =>
                        _validTitle(value) ? null : strings.actionFormInvalid,
                  ),
                  const SizedBox(height: 12),
                  CalendarDateTimeField(
                    label: strings.actionStart,
                    value: start,
                    enabled:
                        !saving &&
                        !widget.controller.busy &&
                        submittedAction == null,
                    onChanged: (value) => setState(() {
                      end = value.add(end.difference(start));
                      start = value;
                    }),
                  ),
                  CalendarDateTimeField(
                    label: strings.actionEnd,
                    value: end,
                    enabled:
                        !saving &&
                        !widget.controller.busy &&
                        submittedAction == null,
                    onChanged: (value) => setState(() => end = value),
                    validator: (value) =>
                        end.isAfter(start) &&
                            end.difference(start) <= const Duration(hours: 24)
                        ? null
                        : strings.actionFormInvalid,
                  ),
                  if (!widget.controller.calendarChangesAvailable) ...[
                    const SizedBox(height: 16),
                    Text(
                      widget.controller.busy
                          ? 'Checking whether Calendar changes are available…'
                          : 'Calendar changes are unavailable. No writable Calendar destination could be confirmed.',
                    ),
                    if (destinationError != null)
                      Text(destinationError.message),
                    FloeButton.text(
                      onPressed: widget.controller.busy
                          ? null
                          : widget.controller.load,
                      child: Text(strings.actionReload),
                    ),
                  ],
                  if (widget.controller.error case final error?) ...[
                    const SizedBox(height: 16),
                    Text(
                      error.isVaultLocked
                          ? 'Vault locked. Unlock it before submitting an Action.'
                          : error.message,
                    ),
                    FloeButton.text(
                      onPressed: widget.controller.busy
                          ? null
                          : widget.controller.load,
                      child: Text(strings.actionReload),
                    ),
                  ],
                  if (submitted case final action?) ...[
                    const SizedBox(height: 16),
                    Text(_statusText(action)),
                    if (action.allowedActions.contains(
                      ActionAllowedAction.reconcile,
                    ))
                      FloeButton.outlined(
                        onPressed: saving || widget.controller.busy
                            ? null
                            : () => reconcile(action),
                        child: Text(strings.actionCheckCalendar),
                      ),
                    FloeButton.text(
                      onPressed: () => Navigator.of(context).pop(),
                      child: const Text('Close'),
                    ),
                  ] else
                    FloeButton.filled(
                      onPressed: canSubmit ? save : null,
                      loading: saving,
                      child: Text(
                        event == null ? 'Create event' : 'Save changes',
                      ),
                    ),
                ],
              ),
            ),
          ],
        );
      },
    );
  }
}

String calendarStorageTimezone(Duration offset) {
  final sign = offset.isNegative ? '-' : '+';
  final minutes = offset.inMinutes.abs();
  final hours = (minutes ~/ 60).toString().padLeft(2, '0');
  final remainder = (minutes % 60).toString().padLeft(2, '0');
  return 'UTC$sign$hours:$remainder';
}
