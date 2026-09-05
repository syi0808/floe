import 'calendar_layout.dart';

import 'package:floe_client/l10n/app_localizations.dart';
import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../../app/floe_feedback.dart';

import '../../../app/design_tokens.dart';
import '../../../app/floe_button.dart';
import '../../../app/floe_squircle.dart';
import '../application/calendar_gateway.dart';
import '../domain/day_models.dart';

class _ConnectedCalendars extends StatelessWidget {
  const _ConnectedCalendars({required this.calendars});

  final List<ConnectedCalendar> calendars;

  @override
  Widget build(BuildContext context) {
    final strings = AppLocalizations.of(context);
    final accounts = <String?, List<ConnectedCalendar>>{};
    for (final calendar in calendars) {
      (accounts[calendar.account] ??= []).add(calendar);
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          strings.connectedCalendarCount(calendars.length),
          style: TextStyle(fontSize: 18, fontWeight: FontWeight.w600),
        ),
        SizedBox(height: 24),
        LayoutBuilder(
          builder: (context, constraints) {
            final columns = ((constraints.maxWidth + 24) / 284).floor().clamp(
              1,
              3,
            );
            final width = (constraints.maxWidth - 24 * (columns - 1)) / columns;
            return Wrap(
              spacing: 24,
              runSpacing: 24,
              children: [
                for (final account in accounts.entries)
                  SizedBox(
                    width: width,
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          '${account.key ?? strings.calendarAccountFallback} · ${account.value.length}',
                          style: TextStyle(
                            fontSize: 12,
                            height: 1.7,
                            fontWeight: FontWeight.w600,
                            color: FloePalette.neutral600,
                          ),
                        ),
                        SizedBox(height: 8),
                        for (final calendar in account.value)
                          Padding(
                            padding: EdgeInsets.symmetric(vertical: 8),
                            child: FloeIconText(
                              icon: Icon(
                                LucideIcons.calendar,
                                size: 15,
                                color: FloePalette.primary500,
                              ),
                              text:
                                  '${calendar.title}${calendar.error == null ? '' : '\n${strings.couldNotCollectEventsShowingTheLast}'}${calendar.lastSuccessAt == null ? '' : '\n${formatTimestamp(context, calendar.lastSuccessAt!)}'}',
                              gap: 10,
                              style: TextStyle(fontSize: 13, height: 1.7),
                            ),
                          ),
                      ],
                    ),
                  ),
              ],
            );
          },
        ),
      ],
    );
  }
}

class CalendarPanel extends StatefulWidget {
  const CalendarPanel({
    super.key,
    required this.gateway,
    required this.query,
    required this.connection,
    required this.onChanged,
  });
  final CalendarGateway gateway;
  final DayQuery query;
  final CalendarConnection? connection;
  final Future<void> Function() onChanged;

  @override
  State<CalendarPanel> createState() => _CalendarPanelState();
}

class _CalendarPanelState extends State<CalendarPanel> {
  bool busy = false;
  String? error;

  Future<void> _run(Future<void> Function() operation) async {
    if (busy) return;
    setState(() {
      busy = true;
      error = null;
    });
    try {
      await operation();
    } on Object {
      if (mounted) {
        setState(
          () =>
              error = AppLocalizations.of(context)
                  .couldNotConnectOrCollectEventsCheck,
        );
      }
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }

  Future<void> _disconnect() => _run(() async {
    final strings = AppLocalizations.of(context);
    final confirmed = await showFloeDialog<bool>(
      context,
      (context) => AlertDialog(
        title: Text(strings.disconnectCalendar),
        content: Text(strings.disconnectCalendarExplanation),
        actions: [
          FloeButton.text(
            onPressed: () => Navigator.pop(context, false),
            child: Text(strings.cancel),
          ),
          FloeButton.filled(
            onPressed: () => Navigator.pop(context, true),
            child: Text(strings.disconnectCalendar),
          ),
        ],
      ),
    );
    if (confirmed != true || !mounted) return;
    await widget.gateway.disconnectCalendar(widget.query);
    await widget.onChanged();
  });

  Future<void> _connect() => _run(() async {
    final confirmed = await showFloeDialog<bool>(
      context,
      (context) => AlertDialog(
        title: Text(AppLocalizations.of(context).connectCalendar),
        content: Text(
          AppLocalizations.of(context).eventsFromTheSelectedCalendarAreSaved,
        ),
        actions: [
          FloeButton.text(
            onPressed: () => Navigator.pop(context, false),
            child: Text(AppLocalizations.of(context).cancel),
          ),
          FloeButton.filled(
            onPressed: () => Navigator.pop(context, true),
            child: Text(AppLocalizations.of(context).continueAction),
          ),
        ],
      ),
    );
    if (confirmed != true || !mounted) return;
    final calendars = await widget.gateway.calendars();
    if (!mounted) return;
    if (calendars.isEmpty) {
      setState(
        () =>
            error = AppLocalizations.of(context)
                .noCalendarsAreAvailableAddACalendar,
      );
      return;
    }
    final selected =
        widget.connection?.selectedCalendarIds.toSet() ?? <String>{};
    var includeAll = widget.connection?.includeAll ?? true;
    final choices = await showFloeDialog<List<CalendarChoice>>(
      context,
      (context) => StatefulBuilder(
        builder: (context, setDialogState) => AlertDialog(
          title: Text(AppLocalizations.of(context).chooseACalendar),
          content: SizedBox(
            width: 420,
            child: SingleChildScrollView(
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  CheckboxListTile(
                    value: includeAll,
                    title: Text(
                      AppLocalizations.of(context).allCalendarsIncludingNew,
                    ),
                    subtitle: includeAll
                        ? null
                        : Text(
                            AppLocalizations.of(context).selectedCalendarsOnly,
                          ),
                    onChanged: (value) =>
                        setDialogState(() => includeAll = value ?? false),
                  ),
                  for (final calendar in calendars)
                    CheckboxListTile(
                      value: includeAll || selected.contains(calendar.id),
                      title: Text(calendar.name),
                      controlAffinity: ListTileControlAffinity.leading,
                      onChanged: includeAll
                          ? null
                          : (checked) => setDialogState(() {
                              if (checked == true) {
                                selected.add(calendar.id);
                              } else {
                                selected.remove(calendar.id);
                              }
                            }),
                    ),
                ],
              ),
            ),
          ),
          actions: [
            FloeButton.text(
              onPressed: () => Navigator.pop(context),
              child: Text(AppLocalizations.of(context).cancel),
            ),
            FloeButton.filled(
              onPressed:
                  includeAll ||
                      calendars.any(
                        (calendar) => selected.contains(calendar.id),
                      )
                  ? () => Navigator.pop(
                      context,
                      calendars
                          .where(
                            (calendar) =>
                                includeAll || selected.contains(calendar.id),
                          )
                          .toList(),
                    )
                  : null,
              child: Text(AppLocalizations.of(context).continueAction),
            ),
          ],
        ),
      ),
    );
    if (choices == null || !mounted) return;
    final query = widget.query;
    await widget.gateway.selectCalendars(
      choices,
      query,
      includeAll: includeAll,
    );
    try {
      await widget.gateway.syncCalendar(query);
    } finally {
      await widget.onChanged();
    }
  });

  @override
  Widget build(BuildContext context) {
    final connection = widget.connection;
    final failure = connection?.error;
    final status = switch (failure) {
      'permission_denied' => AppLocalizations.of(
        context,
      ).calendarAccessWasDeniedOrRevokedAllow,
      'calendar_unavailable' => AppLocalizations.of(
        context,
      ).theSelectedCalendarIsUnavailablePleaseReconnect,
      'provider_unavailable' => AppLocalizations.of(
        context,
      ).couldNotCollectEventsShowingTheLast,
      _ =>
        connection?.lastSuccessAt == null
            ? AppLocalizations.of(context).notCollectedYet
            : AppLocalizations.of(context).lastCollectedCache(
                formatTimestamp(context, connection!.lastSuccessAt!),
              ),
    };
    return FloeSquircle(
      padding: EdgeInsets.all(24),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              FloeSquircle(
                size: FloeSquircleSize.md,
                fill: FloePalette.primary50,
                borderWidth: 0,
                padding: EdgeInsets.all(14),
                child: Icon(
                  LucideIcons.calendarDays,
                  size: 26,
                  color: FloePalette.primary600,
                ),
              ),
              SizedBox(width: 16),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      AppLocalizations.of(context).macosCalendar,
                      style: TextStyle(
                        fontSize: 18,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                    SizedBox(height: 6),
                    Text(
                      AppLocalizations.of(context).calendarsAlreadyOnThisMac,
                      style: TextStyle(
                        fontSize: 13,
                        color: FloePalette.neutral600,
                      ),
                    ),
                  ],
                ),
              ),
              FloeReadOnlyPill(),
            ],
          ),
          SizedBox(height: 24),
          Text(
            AppLocalizations.of(context).bringYourCalendarIntoOneDayFloe,
            style: TextStyle(
              fontSize: 14,
              height: 1.7,
              color: FloePalette.neutral600,
            ),
          ),
          SizedBox(height: 28),
          Divider(height: 1),
          SizedBox(height: 28),
          Text(
            AppLocalizations.of(context).connectedCalendar,
            style: TextStyle(
              fontSize: 11,
              letterSpacing: 1,
              color: FloePalette.neutral600,
            ),
          ),
          SizedBox(height: 12),
          if (connection == null)
            Text(
              AppLocalizations.of(context).makeRoomForYourDay,
              style: TextStyle(fontSize: 22, fontWeight: FontWeight.w600),
            )
          else
            _ConnectedCalendars(calendars: connection.connectedCalendars),
          SizedBox(height: 12),
          Text(
            connection == null
                ? AppLocalizations.of(context).chooseACalendarToStartThisClient
                : status,
            style: TextStyle(
              fontSize: 13,
              height: 1.7,
              color: FloePalette.neutral600,
            ),
          ),
          if (connection != null) ...[
            SizedBox(height: 28),
            Divider(height: 1),
            SizedBox(height: 20),
            for (final entry in <String, String>{
              AppLocalizations.of(context).calendarScope: connection.includeAll
                  ? AppLocalizations.of(context).allCalendarsIncludingNew
                  : AppLocalizations.of(context).selectedCalendarsOnly,
              AppLocalizations.of(context).person: AppLocalizations.of(context)
                  .youThisDevice,
              AppLocalizations.of(context)
                  .storedRangeLabel: connection.rangeStart == null
                  ? AppLocalizations.of(context).notCollectedYet
                  : AppLocalizations.of(context).storedRange(
                      connection.rangeStart!,
                      connection.rangeEnd ?? '—',
                    ),
              AppLocalizations.of(context).lastSuccessfulRead:
                  (connection.lastSuccessAt == null
                      ? null
                      : formatTimestamp(context, connection.lastSuccessAt!)) ??
                  AppLocalizations.of(context).notCollectedYet,
            }.entries)
              Padding(
                padding: EdgeInsets.symmetric(vertical: 12),
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Expanded(
                      child: Text(
                        entry.key,
                        style: TextStyle(
                          fontSize: 13,
                          color: FloePalette.neutral600,
                        ),
                      ),
                    ),
                    Expanded(
                      child: Text(entry.value, style: TextStyle(fontSize: 13)),
                    ),
                  ],
                ),
              ),
          ],
          if (error != null)
            Padding(
              padding: EdgeInsets.only(top: 16),
              child: Text(
                error!,
                style: TextStyle(color: FloePalette.coral700),
              ),
            ),
          SizedBox(height: 24),
          Wrap(
            spacing: 12,
            runSpacing: 12,
            children: [
              if (connection != null)
                FloeButton.filled(
                  onPressed: busy
                      ? null
                      : () => _run(() async {
                          try {
                            await widget.gateway.syncCalendar(widget.query);
                          } finally {
                            await widget.onChanged();
                          }
                        }),
                  child: Text(AppLocalizations.of(context).refreshSelectedDay),
                ),
              FloeButton.outlined(
                onPressed: busy ? null : _connect,
                child: Text(
                  connection == null
                      ? AppLocalizations.of(context).connect
                      : AppLocalizations.of(context).reconnectOrChange,
                ),
              ),
              FloeButton.text(
                onPressed: busy
                    ? null
                    : () => _run(widget.gateway.openCalendarSettings),
                child: Text(AppLocalizations.of(context).manageAccess),
              ),
              if (connection != null)
                FloeButton.text(
                  onPressed: busy ? null : _disconnect,
                  child: Text(AppLocalizations.of(context).disconnectCalendar),
                ),
            ],
          ),
          if (busy)
            Padding(
              padding: EdgeInsets.only(top: 20),
              child: Center(child: FloeDotSpinner()),
            ),
        ],
      ),
    );
  }
}
