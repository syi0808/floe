import 'dart:async';
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:floe_client/l10n/app_localizations.dart';

import 'package:floe_client/app/floe_button.dart';
import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_feedback.dart';
import 'package:floe_client/app/floe_input.dart';
import 'package:floe_client/app/floe_selection.dart';
import 'package:floe_client/features/day/application/day_gateway.dart';
import 'package:floe_client/features/day/domain/day_models.dart';
import 'package:floe_client/features/day/presentation/calendar_date_time_field.dart';

class CalendarEventComposer extends StatefulWidget {
  const CalendarEventComposer({
    super.key,
    required this.gateway,
    this.initialStart,
    this.event,
  });

  final DayGateway gateway;
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
  List<ManualCalendarDestination> destinations = const [];
  bool loadingDestinations = false;
  String? destinationError;
  bool saving = false;
  int _destinationLoadGeneration = 0;
  int _saveGeneration = 0;

  @override
  void initState() {
    super.initState();
    _resetDraft();
    if (widget.event == null) {
      loadingDestinations = true;
      unawaited(_loadDestinations(markLoading: false));
    }
  }

  void _resetDraft() {
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
  }

  @override
  void didUpdateWidget(covariant CalendarEventComposer oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (identical(oldWidget.gateway, widget.gateway) &&
        _sameEventTargetSnapshot(oldWidget.event, widget.event) &&
        oldWidget.initialStart == widget.initialStart) {
      return;
    }

    _destinationLoadGeneration++;
    _saveGeneration++;
    _resetDraft();
    destinationRef = null;
    destinations = const [];
    destinationError = null;
    saving = false;
    loadingDestinations = widget.event == null;
    if (widget.event == null) {
      unawaited(_loadDestinations(markLoading: false));
    }
  }

  bool _sameEventTargetSnapshot(EventItem? first, EventItem? second) {
    if (first == null || second == null) return first == null && second == null;
    final firstTarget = first.actionTarget;
    final secondTarget = second.actionTarget;
    return first.id == second.id &&
        first.revision == second.revision &&
        firstTarget?.eventId == secondTarget?.eventId &&
        firstTarget?.expectedRevision == secondTarget?.expectedRevision;
  }

  @override
  void dispose() {
    title.dispose();
    super.dispose();
  }

  Future<void> _loadDestinations({bool markLoading = true}) async {
    if (markLoading && loadingDestinations) return;
    final gateway = widget.gateway;
    final generation = ++_destinationLoadGeneration;
    if (markLoading) {
      setState(() {
        loadingDestinations = true;
        destinationError = null;
      });
    } else {
      destinationError = null;
    }
    try {
      final values = await gateway.loadExternalCalendarDestinations();
      if (!_isCurrentDestinationLoad(gateway, generation)) return;
      setState(() {
        destinations = values;
        destinationRef = values.isEmpty ? null : values.first.destinationRef;
        loadingDestinations = false;
      });
    } on Object catch (error) {
      if (!_isCurrentDestinationLoad(gateway, generation)) return;
      setState(() {
        destinationError = error.toString();
        loadingDestinations = false;
      });
    }
  }

  bool _isCurrentDestinationLoad(DayGateway gateway, int generation) =>
      mounted &&
      generation == _destinationLoadGeneration &&
      identical(widget.gateway, gateway);

  bool _validTitle(String? value) {
    if (value == null || value.trim().isEmpty) return false;
    return utf8.encode(value.trim()).length <= 1024 &&
        !value.trim().runes.any(
          (rune) => rune < 32 || (rune >= 127 && rune <= 159),
        );
  }

  Future<void> save() async {
    if (saving || loadingDestinations || !form.currentState!.validate()) {
      return;
    }
    final gateway = widget.gateway;
    final event = widget.event;
    final target = event?.actionTarget;
    if (event == null &&
            !destinations.any(
              (choice) => choice.destinationRef == destinationRef,
            ) ||
        event != null && target == null) {
      return;
    }
    final requestTitle = title.text.trim();
    final requestStart = start;
    final requestEnd = end;
    final ManualCalendarOperationIntent intent = event == null
        ? CreateManualCalendarEvent(
            destinationRef: destinationRef!,
            title: requestTitle,
            startsAt: requestStart.toUtc(),
            endsAt: requestEnd.toUtc(),
            timezone: calendarStorageTimezone(requestStart.timeZoneOffset),
          )
        : UpdateManualCalendarEvent(
            eventRef: target!.eventId,
            expectedRevision: target.expectedRevision,
            title: requestTitle,
            startsAt: requestStart.toUtc(),
            endsAt: requestEnd.toUtc(),
            timezone:
                event.timezone ??
                calendarStorageTimezone(requestStart.timeZoneOffset),
          );
    final generation = ++_saveGeneration;
    setState(() => saving = true);
    try {
      final receipt = await gateway.executeExternalCalendarOperation(intent);
      if (!mounted ||
          generation != _saveGeneration ||
          !identical(widget.gateway, gateway)) {
        return;
      }
      Navigator.of(context).pop(receipt);
    } on Object catch (error) {
      if (!mounted ||
          generation != _saveGeneration ||
          !identical(widget.gateway, gateway)) {
        return;
      }
      setState(() {
        saving = false;
        destinationError = error.toString();
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final strings = AppLocalizations.of(context);
    final event = widget.event;
    final target = event?.actionTarget;
    final canSubmit =
        !saving &&
        !loadingDestinations &&
        (event == null
            ? destinations.any(
                (choice) => choice.destinationRef == destinationRef,
              )
            : target != null);
    return FloeDetailDialog(
      title: event == null ? 'New event' : 'Edit event',
      loading: saving || (event == null && loadingDestinations),
      loadingLabel: strings.actionLoading,
      children: [
        Form(
          key: form,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              if (event == null) ...[
                if (loadingDestinations)
                  const Text('Checking writable Calendar destinations…')
                else if (destinations.isNotEmpty)
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
                    enabled: !saving,
                    onChanged: (value) =>
                        setState(() => destinationRef = value),
                    validator: (value) =>
                        value == null ? strings.actionFormInvalid : null,
                  )
                else
                  const Text(
                    'Calendar changes are unavailable. No writable Calendar destination could be confirmed.',
                  ),
              ] else if (target == null)
                Text(strings.eventCannotEdit)
              else
                Padding(
                  padding: const EdgeInsets.only(bottom: 16),
                  child: Text(
                    event.calendarLabel ?? strings.eventConnectedCalendar,
                  ),
                ),
              if (event == null) const SizedBox(height: 12),
              FloeInput(
                label: strings.actionTitle,
                controller: title,
                enabled: !saving && !(event == null && loadingDestinations),
                validator: (value) =>
                    _validTitle(value) ? null : strings.actionFormInvalid,
              ),
              const SizedBox(height: 12),
              CalendarDateTimeField(
                label: strings.actionStart,
                value: start,
                enabled: !saving,
                onChanged: (value) => setState(() {
                  end = value.add(end.difference(start));
                  start = value;
                }),
              ),
              CalendarDateTimeField(
                label: strings.actionEnd,
                value: end,
                enabled: !saving,
                onChanged: (value) => setState(() => end = value),
                validator: (value) =>
                    end.isAfter(start) &&
                        end.difference(start) <= const Duration(hours: 24)
                    ? null
                    : strings.actionFormInvalid,
              ),
              if (destinationError case final error?) ...[
                const SizedBox(height: 16),
                Text(error),
                FloeButton.text(
                  onPressed: saving ? null : _loadDestinations,
                  child: Text(strings.actionReload),
                ),
              ],
              const SizedBox(height: 20),
              Text(
                event == null
                    ? strings.eventCreateScope
                    : strings.eventEditScope,
                style: FloeType.bodySmall.copyWith(
                  color: FloeColor.textSecondary,
                ),
              ),
              const SizedBox(height: 16),
              FloeButton.filled(
                onPressed: canSubmit ? save : null,
                loading: saving,
                child: Text(event == null ? 'Create event' : 'Save changes'),
              ),
            ],
          ),
        ),
      ],
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
