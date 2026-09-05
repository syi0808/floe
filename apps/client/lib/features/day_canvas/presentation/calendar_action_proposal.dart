import 'package:flutter/material.dart';
import 'package:floe_client/l10n/app_localizations.dart';
import 'package:intl/intl.dart';

import '../../../app/floe_button.dart';
import '../../../app/floe_feedback.dart';
import '../../../app/floe_selection.dart';
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
  String? calendarId;
  bool saving = false;
  bool failed = false;

  @override
  void initState() {
    super.initState();
    final now = DateTime.now();
    final next = DateTime(now.year, now.month, now.day, now.hour + 1);
    start.text = _localInput(next);
    end.text = _localInput(next.add(const Duration(minutes: 45)));
    calendarId = widget.connection()?.selectedCalendarIds.firstOrNull;
  }

  @override
  void dispose() {
    title.dispose();
    start.dispose();
    end.dispose();
    super.dispose();
  }

  DateTime? parse(String text) {
    final match = RegExp(r'^(\d{4})-(\d{2})-(\d{2}) (\d{2}):(\d{2})$')
        .firstMatch(text.trim());
    if (match == null) return null;
    final value = DateTime(
      int.parse(match[1]!),
      int.parse(match[2]!),
      int.parse(match[3]!),
      int.parse(match[4]!),
      int.parse(match[5]!),
    );
    return _localInput(value) == text.trim() ? value : null;
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
    final startsAt = parse(start.text)!;
    final action = await widget.controller.propose(
      calendarId: calendarId!,
      title: title.text.trim(),
      startsAt: startsAt,
      endsAt: parse(end.text)!,
      timezone: _storageTimezone(startsAt.timeZoneOffset),
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
              FloeSelect<String>(
                label: strings.actionDestination,
                value: calendars.any((calendar) => calendar.id == calendarId)
                    ? calendarId
                    : null,
                options: calendars
                    .map(
                      (calendar) => FloeSelectOption(
                        value: calendar.id,
                        label: calendar.name,
                      ),
                    )
                    .toList(),
                enabled: !saving,
                onChanged: (value) => setState(() => calendarId = value),
                validator: (value) =>
                    value == null ? strings.actionFormInvalid : null,
              ),
              const SizedBox(height: 12),
              TextFormField(
                controller: title,
                enabled: !saving,
                decoration: InputDecoration(labelText: strings.actionTitle),
                validator: (value) => value == null || value.trim().isEmpty
                    ? strings.actionFormInvalid
                    : null,
              ),
              const SizedBox(height: 12),
              TextFormField(
                controller: start,
                enabled: !saving,
                decoration: InputDecoration(labelText: strings.actionStart),
                validator: (value) =>
                    parse(value ?? '')?.isAfter(DateTime.now()) == true
                    ? null
                    : strings.actionFormInvalid,
              ),
              const SizedBox(height: 12),
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

String _localInput(DateTime value) =>
    DateFormat('yyyy-MM-dd HH:mm').format(value);

String _storageTimezone(Duration offset) {
  final sign = offset.isNegative ? '-' : '+';
  final minutes = offset.inMinutes.abs();
  final hours = (minutes ~/ 60).toString().padLeft(2, '0');
  final remainder = (minutes % 60).toString().padLeft(2, '0');
  return 'UTC$sign$hours:$remainder';
}
