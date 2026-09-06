import 'package:flutter/material.dart';
import 'package:floe_client/l10n/app_localizations.dart';

import '../../../app/floe_button.dart';
import '../../../app/floe_feedback.dart';
import '../../../app/floe_selection.dart';
import '../../../app/floe_toast.dart';
import '../application/calendar_action_controller.dart';
import '../domain/day_models.dart';
import 'calendar_date_time_field.dart';
import '../domain/calendar_action.dart';

class CalendarEventComposer extends StatefulWidget {
  const CalendarEventComposer({
    super.key,
    required this.controller,
    required this.connection,
    this.initialStart,
    this.event,
  });
  final CalendarActionController controller;
  final CalendarConnection? Function() connection;
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
  String? calendarId;
  bool saving = false;
  bool failed = false;

  @override
  void initState() {
    super.initState();
    final now = DateTime.now();
    final next =
        widget.initialStart ??
        DateTime(now.year, now.month, now.day, now.hour + 1);
    start = widget.event?.startsAt.toLocal() ?? next;
    end =
        widget.event?.endsAt.toLocal() ?? next.add(const Duration(minutes: 45));
    title.text = widget.event?.title ?? '';
    calendarId =
        widget.event?.calendarId ??
        widget.connection()?.selectedCalendarIds.firstOrNull;
  }

  @override
  void dispose() {
    title.dispose();
    super.dispose();
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
    final action = await widget.controller.direct(
      calendarId: calendarId!,
      title: title.text.trim(),
      startsAt: start,
      endsAt: end,
      timezone: calendarStorageTimezone(start.timeZoneOffset),
      eventId: widget.event?.id,
      eventRevision: widget.event?.revision,
    );
    if (!mounted) return;
    if (action == null || action.status != CalendarActionStatus.succeeded) {
      setState(() {
        saving = false;
        failed = true;
      });
      return;
    }
    if (widget.controller.collection[action.id] == 'failed') {
      FloeToastHost.of(context)
          .show(title: 'Saved. Calendar refresh failed; retry in Activity.');
    }
    Navigator.of(context).pop();
  }

  @override
  Widget build(BuildContext context) {
    final strings = AppLocalizations.of(context);
    final calendars =
        widget.connection()?.connectedCalendars ?? <ConnectedCalendar>[];
    return FloeDetailDialog(
      title: widget.event == null ? 'New event' : 'Edit event',
      children: [
        const Text(
          'Changes are saved to your calendar. Existing alerts are preserved. Recurring events and invitations are managed in the source calendar.',
        ),
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
                enabled: !saving && widget.event == null,
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
              CalendarDateTimeField(
                label: strings.actionStart,
                value: start,
                enabled: !saving,
                onChanged: (value) => setState(() {
                  end = value.add(end.difference(start));
                  start = value;
                }),
              ),
              const SizedBox(height: 12),
              CalendarDateTimeField(
                label: strings.actionEnd,
                value: end,
                enabled: !saving,
                onChanged: (value) => setState(() => end = value),
                validator: (value) {
                  return end.isAfter(start) &&
                          end.difference(start) <= const Duration(hours: 24)
                      ? null
                      : strings.actionFormInvalid;
                },
              ),
              const SizedBox(height: 16),
              if (failed)
                const Text(
                  'The change could not be confirmed. Check Activity and reload your calendar before trying again.',
                ),
              FloeButton.filled(
                onPressed:
                    saving ||
                        !(widget.event == null
                            ? widget.controller.canDirect
                            : widget.controller.canModify(widget.event!))
                    ? null
                    : save,
                loading: saving,
                child: Text(
                  widget.event == null ? 'Create event' : 'Save changes',
                ),
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
