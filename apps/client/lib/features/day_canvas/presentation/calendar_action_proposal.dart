import 'package:flutter/material.dart';
import 'package:floe_client/l10n/app_localizations.dart';

import '../../../app/floe_button.dart';
import '../../../app/floe_feedback.dart';
import '../application/calendar_action_controller.dart';
import '../domain/day_models.dart';
import 'calendar_action_panel.dart';

class CalendarActionProposal extends StatefulWidget {
  const CalendarActionProposal({
    super.key,
    required this.controller,
    required this.connection,
  });
  final CalendarActionController controller;
  final CalendarConnection? Function() connection;

  @override
  State<CalendarActionProposal> createState() => _CalendarActionProposalState();
}

class _CalendarActionProposalState extends State<CalendarActionProposal> {
  final form = GlobalKey<FormState>();
  final title = TextEditingController();
  final start = TextEditingController();
  final end = TextEditingController();
  final timezone = TextEditingController(text: 'Etc/UTC');
  String? calendarId;
  bool saving = false;
  bool failed = false;

  @override
  void initState() {
    super.initState();
    final now = DateTime.now().toUtc();
    final next = DateTime.utc(now.year, now.month, now.day, now.hour + 1);
    start.text = next.toIso8601String();
    end.text = next.add(const Duration(minutes: 45)).toIso8601String();
    calendarId = widget.connection()?.selectedCalendarIds.firstOrNull;
  }

  @override
  void dispose() {
    title.dispose();
    start.dispose();
    end.dispose();
    timezone.dispose();
    super.dispose();
  }

  DateTime? parse(String text) {
    if (!text.endsWith('Z')) return null;
    return DateTime.tryParse(text);
  }

  Future<void> save() async {
    if (saving || !form.currentState!.validate()) return;
    final connection = widget.connection();
    if (connection == null ||
        !connection.selectedCalendarIds.contains(calendarId)) {
      setState(() => failed = true);
      return;
    }
    setState(() {
      saving = true;
      failed = false;
    });
    final action = await widget.controller.propose(
      calendarId: calendarId!,
      title: title.text.trim(),
      startsAt: parse(start.text)!,
      endsAt: parse(end.text)!,
      timezone: timezone.text.trim(),
    );
    if (!mounted) return;
    if (action == null) {
      setState(() {
        saving = false;
        failed = true;
      });
      return;
    }
    final navigator = Navigator.of(context);
    final parent = navigator.context;
    if (!parent.mounted) return;
    navigator.pop();
    showFloeDialog<void>(
      parent,
      (_) => CalendarActionDialog(
        controller: widget.controller,
        actionId: action.id,
        connection: widget.connection,
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final strings = AppLocalizations.of(context);
    final calendars =
        widget.connection()?.connectedCalendars ?? <ConnectedCalendar>[];
    return FloeDetailDialog(
      title: strings.actionNewProposal,
      children: [
        Text(strings.actionProposalExplanation),
        const SizedBox(height: 16),
        Form(
          key: form,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              DropdownButtonFormField<String>(
                initialValue:
                    calendars.any((calendar) => calendar.id == calendarId)
                    ? calendarId
                    : null,
                isExpanded: true,
                decoration: InputDecoration(
                  labelText: strings.actionDestination,
                ),
                items: calendars
                    .map(
                      (calendar) => DropdownMenuItem(
                        value: calendar.id,
                        child: Text(
                          calendar.name,
                          overflow: TextOverflow.ellipsis,
                        ),
                      ),
                    )
                    .toList(),
                onChanged: saving
                    ? null
                    : (value) => setState(() => calendarId = value),
                validator: (value) =>
                    value == null ? strings.actionFormInvalid : null,
              ),
              TextFormField(
                controller: title,
                enabled: !saving,
                decoration: InputDecoration(labelText: strings.actionTitle),
                validator: (value) => value == null || value.trim().isEmpty
                    ? strings.actionFormInvalid
                    : null,
              ),
              TextFormField(
                controller: start,
                enabled: !saving,
                decoration: InputDecoration(labelText: strings.actionStart),
                validator: (value) =>
                    parse(value ?? '')?.isAfter(DateTime.now()) == true
                    ? null
                    : strings.actionFormInvalid,
              ),
              TextFormField(
                controller: end,
                enabled: !saving,
                decoration: InputDecoration(labelText: strings.actionEnd),
                validator: (value) {
                  final starts = parse(start.text);
                  final ends = parse(value ?? '');
                  return starts != null &&
                          ends != null &&
                          ends.isAfter(starts) &&
                          ends.difference(starts) <= const Duration(hours: 24)
                      ? null
                      : strings.actionFormInvalid;
                },
              ),
              TextFormField(
                controller: timezone,
                enabled: !saving,
                decoration: InputDecoration(labelText: strings.sourceTimeZone),
                validator: (value) => value == null || value.trim().isEmpty
                    ? strings.actionFormInvalid
                    : null,
              ),
              const SizedBox(height: 16),
              if (failed) Text(strings.actionReloadRequired),
              FloeButton.filled(
                onPressed: saving ? null : save,
                child: Text(strings.actionPrepareReview),
              ),
            ],
          ),
        ),
      ],
    );
  }
}
