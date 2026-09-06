import 'package:floe_client/l10n/app_localizations.dart';

import 'dart:ui';

import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../../app/design_tokens.dart';
import '../../../app/floe_feedback.dart';
import '../../../app/floe_loading.dart';
import '../../../app/floe_squircle.dart';
import '../domain/day_models.dart';
import 'calendar_event_details.dart';
import 'calendar_layout.dart';
import 'day_appearance.dart';

class CalendarAgenda extends StatefulWidget {
  const CalendarAgenda({
    super.key,
    required this.snapshot,
    required this.onConnections,
    this.onCreateEvent,
    this.draftStartsAt,
    this.loading = false,
  });
  final DaySnapshot snapshot;
  final VoidCallback onConnections;
  final ValueChanged<DateTime>? onCreateEvent;
  final DateTime? draftStartsAt;
  final bool loading;
  @override
  State<CalendarAgenda> createState() => _CalendarAgendaState();
}

class _CalendarAgendaState extends State<CalendarAgenda> {
  final scroll = ScrollController(initialScrollOffset: 480);
  double zoom = 1;
  bool restored = false;
  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (!restored) {
      zoom =
          PageStorage.maybeOf(context)
                  ?.readState(context, identifier: 'calendar-zoom')
              as double? ??
          1;
      restored = true;
    }
  }

  @override
  void dispose() {
    scroll.dispose();
    super.dispose();
  }

  @override
  void didUpdateWidget(CalendarAgenda oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.snapshot.date != widget.snapshot.date && scroll.hasClients) {
      scroll.jumpTo((480 * zoom).clamp(0, scroll.position.maxScrollExtent));
    }
  }

  void setZoom(double value) {
    final minute = scroll.hasClients ? scroll.offset / zoom : 480.0;
    setState(() => zoom = value);
    PageStorage.maybeOf(context)
        ?.writeState(context, zoom, identifier: 'calendar-zoom');
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted && scroll.hasClients) {
        scroll.jumpTo(
          (minute * zoom).clamp(0, scroll.position.maxScrollExtent),
        );
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    final snapshot = widget.snapshot;
    final events = snapshot.items.whereType<EventItem>().toList();
    final allDay = events.where((event) => event.isAllDay).toList();
    final placements = layoutCalendarEvents(
      events,
      snapshot.date,
      snapshot.timezoneOffsetSeconds,
    );
    final empty = events.isEmpty && !widget.loading;
    final confirmedEmpty =
        empty &&
        snapshot.calendar?.error == null &&
        snapshot.calendar?.lastSuccessAt != null;
    final axis = CalendarDayAxis(snapshot.date, snapshot.timezoneOffsetSeconds);
    final currentMinute = axis.minute(snapshot.generatedAt);
    final sameDay = currentMinute >= 0 && currentMinute < axis.minutes;
    return FloeSquircle(
      key: Key('timeline-card'),
      child: Stack(
        children: [
          ImageFiltered(
            imageFilter: ImageFilter.blur(
              sigmaX: widget.loading ? 3 : 0,
              sigmaY: widget.loading ? 3 : 0,
            ),
            child: ExcludeFocus(
              excluding: widget.loading,
              child: IgnorePointer(
                ignoring: widget.loading,
                child: Column(
                  children: [
                    ConstrainedBox(
                      constraints: BoxConstraints(maxHeight: 120),
                      child: SingleChildScrollView(
                        primary: false,
                        padding: EdgeInsets.symmetric(
                          horizontal: 20,
                          vertical: 16,
                        ),
                        child: Row(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            SizedBox(
                              width: 72,
                              child: Padding(
                                padding: EdgeInsets.only(top: 8),
                                child: Text(
                                  AppLocalizations.of(context).allDay,
                                  style: TextStyle(
                                    fontSize: 11,
                                    color: FloePalette.neutral600,
                                  ),
                                ),
                              ),
                            ),
                            Expanded(
                              child: Column(
                                crossAxisAlignment: CrossAxisAlignment.stretch,
                                children: [
                                  if (allDay.isEmpty)
                                    SizedBox(height: 24, child: Text('—')),
                                  for (final event in allDay)
                                    FloeTextLink(
                                      color: FloePalette.neutral950,
                                      leading: Container(
                                        width: 10,
                                        height: 10,
                                        decoration: BoxDecoration(
                                          shape: BoxShape.circle,
                                          color: DayAppearance.tone(
                                            context,
                                            event.id,
                                            ItemTone.mint,
                                          ).accent,
                                        ),
                                      ),
                                      label:
                                          '${event.title}${event.calendarName == null ? '' : '   ${event.calendarName}'}',
                                      onPressed: () => openCalendarEvent(
                                        context,
                                        event,
                                        snapshot,
                                      ),
                                    ),
                                ],
                              ),
                            ),
                          ],
                        ),
                      ),
                    ),
                    Divider(height: 1),
                    Padding(
                      padding: EdgeInsets.symmetric(
                        horizontal: 20,
                        vertical: 8,
                      ),
                      child: Row(
                        children: [
                          if (confirmedEmpty &&
                              widget.draftStartsAt == null) ...[
                            Expanded(
                              child: IgnorePointer(
                                child: _EmptyDayBanner(
                                  compact:
                                      MediaQuery.sizeOf(context).width <= 780,
                                ),
                              ),
                            ),
                            const SizedBox(width: 16),
                          ] else
                            const Spacer(),
                          IconButton(
                            tooltip: AppLocalizations.of(context).zoomOut,
                            onPressed: zoom <= 1
                                ? null
                                : () => setZoom(zoom - 1),
                            icon: Icon(LucideIcons.zoomOut, size: 16),
                          ),
                          if (MediaQuery.sizeOf(context).width > 780)
                            SizedBox(
                              width: 150,
                              child: Slider(
                                semanticFormatterCallback: (value) =>
                                    AppLocalizations.of(context)
                                        .zoomTimes(value.round()),
                                min: 1,
                                max: 12,
                                divisions: 11,
                                value: zoom,
                                onChanged: setZoom,
                              ),
                            ),
                          IconButton(
                            tooltip: AppLocalizations.of(context).zoomIn,
                            onPressed: zoom >= 12
                                ? null
                                : () => setZoom(zoom + 1),
                            icon: Icon(LucideIcons.zoomIn, size: 16),
                          ),
                          SizedBox(
                            width: 32,
                            child: Text(
                              '${zoom.round()}×',
                              textAlign: TextAlign.end,
                              style: TextStyle(fontSize: 11),
                            ),
                          ),
                        ],
                      ),
                    ),
                    if (empty &&
                        widget.draftStartsAt == null &&
                        snapshot.calendar?.error == null &&
                        !confirmedEmpty)
                      _EmptyDayStatus(onConnections: widget.onConnections),
                    Expanded(
                      child: Stack(
                        children: [
                          Positioned.fill(
                            child: Scrollbar(
                              controller: scroll,
                              thumbVisibility: true,
                              child: SingleChildScrollView(
                                key: Key('calendar-scroll'),
                                controller: scroll,
                                child: LayoutBuilder(
                                  builder: (context, constraints) => GestureDetector(
                                    key: const Key('calendar-time-grid'),
                                    behavior: HitTestBehavior.opaque,
                                    onDoubleTapDown:
                                        widget.onCreateEvent == null
                                        ? null
                                        : (details) {
                                            final minute =
                                                ((details.localPosition.dy -
                                                            16) /
                                                        zoom)
                                                    .clamp(
                                                      0.0,
                                                      axis.minutes - 1,
                                                    );
                                            if (placements.any(
                                              (placement) =>
                                                  minute >= placement.start &&
                                                  minute < placement.end,
                                            )) {
                                              return;
                                            }
                                            final snapped =
                                                (minute / 15).round() * 15;
                                            widget.onCreateEvent!(
                                              axis.start
                                                  .add(
                                                    Duration(minutes: snapped),
                                                  )
                                                  .toLocal(),
                                            );
                                          },
                                    child: SizedBox(
                                      height: axis.minutes * zoom + 32,
                                      child: Stack(
                                        children: [
                                          Positioned.fill(
                                            child: CustomPaint(
                                              painter: _CalendarGuides(zoom),
                                            ),
                                          ),
                                          for (
                                            var hour = 0;
                                            hour <= axis.minutes / 60;
                                            hour++
                                          )
                                            Positioned(
                                              top: 16 + hour * 60 * zoom - 6,
                                              left: 16,
                                              child: Text(
                                                axis.hourLabel(hour),
                                                style: TextStyle(
                                                  fontSize: 10,
                                                  height: 1.2,
                                                  color: FloePalette.neutral600,
                                                ),
                                              ),
                                            ),
                                          for (final placement in placements)
                                            Positioned(
                                              top: 16 + placement.start * zoom,
                                              left:
                                                  80 +
                                                  (constraints.maxWidth - 100) *
                                                      placement.column /
                                                      placement.columns,
                                              width:
                                                  ((constraints.maxWidth -
                                                                  100) /
                                                              placement
                                                                  .columns -
                                                          (placement.columns > 1
                                                              ? 6
                                                              : 0))
                                                      .clamp(
                                                        1,
                                                        double.infinity,
                                                      ),
                                              height:
                                                  (placement.end -
                                                      placement.start) *
                                                  zoom,
                                              child: CalendarEventCard(
                                                event: placement.event,
                                                snapshot: snapshot,
                                                height:
                                                    (placement.end -
                                                        placement.start) *
                                                    zoom,
                                              ),
                                            ),
                                          if (widget.draftStartsAt
                                              case final start?
                                              when axis.minute(start) >= 0 &&
                                                  axis.minute(start) <
                                                      axis.minutes)
                                            Positioned(
                                              top:
                                                  16 +
                                                  axis.minute(start) * zoom,
                                              left: 80,
                                              right: 20,
                                              height: 45 * zoom,
                                              child: IgnorePointer(
                                                child: DecoratedBox(
                                                  decoration: BoxDecoration(
                                                    color:
                                                        FloePalette.primary100,
                                                    border: Border.all(
                                                      color: FloePalette
                                                          .primary500,
                                                    ),
                                                    borderRadius:
                                                        BorderRadius.circular(
                                                          12,
                                                        ),
                                                  ),
                                                  child: Padding(
                                                    padding:
                                                        const EdgeInsets.symmetric(
                                                          horizontal: 12,
                                                        ),
                                                    child: Align(
                                                      alignment:
                                                          Alignment.centerLeft,
                                                      child: Text(
                                                        AppLocalizations.of(
                                                          context,
                                                        ).newEvent,
                                                        style: const TextStyle(
                                                          fontSize: 12,
                                                          fontWeight:
                                                              FontWeight.w600,
                                                          color: FloePalette
                                                              .primary700,
                                                        ),
                                                      ),
                                                    ),
                                                  ),
                                                ),
                                              ),
                                            ),
                                          if (sameDay)
                                            Positioned(
                                              top:
                                                  16 + currentMinute * zoom - 6,
                                              left: 10,
                                              right: 18,
                                              child: Row(
                                                children: [
                                                  SizedBox(
                                                    width: 56,
                                                    child: Text(
                                                      axis.time(
                                                        snapshot.generatedAt,
                                                      ),
                                                      style: TextStyle(
                                                        fontSize: 10,
                                                        color: FloePalette
                                                            .primary600,
                                                      ),
                                                    ),
                                                  ),
                                                  Container(
                                                    width: 5,
                                                    height: 5,
                                                    decoration: BoxDecoration(
                                                      shape: BoxShape.circle,
                                                      color: FloePalette
                                                          .primary500,
                                                    ),
                                                  ),
                                                  Expanded(
                                                    child: Divider(
                                                      color: FloePalette
                                                          .primary500,
                                                      height: 1,
                                                    ),
                                                  ),
                                                ],
                                              ),
                                            ),
                                        ],
                                      ),
                                    ),
                                  ),
                                ),
                              ),
                            ),
                          ),
                        ],
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ),
          if (widget.loading)
            Positioned.fill(
              child: ColoredBox(
                color: FloePalette.neutral0.withValues(alpha: .55),
                child: Center(child: FloeSpinner()),
              ),
            ),
        ],
      ),
    );
  }
}

class _EmptyDayStatus extends StatelessWidget {
  const _EmptyDayStatus({required this.onConnections});

  final VoidCallback onConnections;

  @override
  Widget build(BuildContext context) {
    final strings = AppLocalizations.of(context);
    return Semantics(
      container: true,
      liveRegion: true,
      child: Padding(
        padding: const EdgeInsets.fromLTRB(20, 8, 20, 4),
        child: Row(
          children: [
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    strings.yourDayIsStillEmpty,
                    style: const TextStyle(fontWeight: FontWeight.w600),
                  ),
                  const SizedBox(height: 2),
                  Text(
                    strings.emptyDayConnectHint,
                    style: const TextStyle(
                      fontSize: 12,
                      color: FloePalette.neutral600,
                    ),
                  ),
                ],
              ),
            ),
            FloeTextLink(
              label: strings.viewConnectedCalendars,
              onPressed: onConnections,
            ),
          ],
        ),
      ),
    );
  }
}

class _EmptyDayBanner extends StatelessWidget {
  const _EmptyDayBanner({required this.compact});

  final bool compact;

  @override
  Widget build(BuildContext context) {
    final strings = AppLocalizations.of(context);
    return Semantics(
      container: true,
      liveRegion: true,
      child: FloeSquircle(
        key: const Key('empty-day-banner'),
        size: FloeSquircleSize.md,
        fill: FloePalette.primary50,
        borderColor: FloePalette.primary200,
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
        child: Row(
          children: [
            const Icon(
              LucideIcons.cloudSun,
              size: 16,
              color: FloePalette.primary600,
            ),
            const SizedBox(width: 10),
            Expanded(
              child: Row(
                children: [
                  Text(
                    strings.aLittleBreathingRoom,
                    maxLines: 1,
                    style: const TextStyle(
                      fontSize: 11,
                      height: 1.3,
                      fontWeight: FontWeight.w600,
                      color: FloePalette.neutral950,
                    ),
                  ),
                  if (!compact) ...[
                    const SizedBox(width: 10),
                    Expanded(
                      child: Text(
                        strings.emptyDayCreateHint,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: const TextStyle(
                          fontSize: 11,
                          height: 1.3,
                          color: FloePalette.neutral600,
                        ),
                      ),
                    ),
                  ],
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class CalendarEventCard extends StatelessWidget {
  const CalendarEventCard({
    super.key,
    required this.event,
    required this.snapshot,
    required this.height,
  });
  final EventItem event;
  final DaySnapshot snapshot;
  final double height;
  @override
  Widget build(BuildContext context) {
    final tone = DayAppearance.tone(context, event.id, ItemTone.blue);
    final textScale = MediaQuery.textScalerOf(context).scale(12) / 12;
    final label =
        '${event.title} · ${calendarRange(context, event, snapshot.timezoneOffsetSeconds, date: snapshot.date)}';
    return Tooltip(
      message: label,
      child: Semantics(
        label: label,
        button: true,
        child: Material(
          color: tone.fill,
          borderRadius: BorderRadius.circular(height < 24 ? 3 : 16),
          clipBehavior: Clip.antiAlias,
          child: InkWell(
            mouseCursor: WidgetStateMouseCursor.clickable,
            onTap: () => openCalendarEvent(context, event, snapshot),
            onDoubleTap: () => openCalendarEvent(context, event, snapshot),
            hoverColor: tone.border,
            child: height < 24 * textScale
                ? Align(
                    alignment: Alignment.centerLeft,
                    child: Container(
                      margin: EdgeInsets.only(left: 4),
                      width: 3,
                      height: height * .7,
                      color: tone.accent,
                    ),
                  )
                : Padding(
                    padding: EdgeInsets.symmetric(horizontal: 12),
                    child: Row(
                      children: [
                        Container(
                          width: 10,
                          height: 10,
                          decoration: BoxDecoration(
                            color: tone.accent,
                            shape: BoxShape.circle,
                          ),
                        ),
                        SizedBox(width: 10),
                        Expanded(
                          child: Column(
                            mainAxisAlignment: MainAxisAlignment.center,
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Text(
                                event.title,
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                                style: TextStyle(
                                  fontSize: 12,
                                  height: 1.2,
                                  fontWeight: FontWeight.w600,
                                ),
                              ),
                              if (height >= 42 * textScale) ...[
                                SizedBox(height: 3),
                                Text(
                                  calendarRange(
                                    context,
                                    event,
                                    snapshot.timezoneOffsetSeconds,
                                    date: snapshot.date,
                                  ),
                                  maxLines: 1,
                                  overflow: TextOverflow.ellipsis,
                                  style: TextStyle(
                                    fontSize: 11,
                                    height: 1.2,
                                    color: FloePalette.neutral600,
                                  ),
                                ),
                              ],
                              if (height >= 58 * textScale &&
                                  event.calendarName != null) ...[
                                SizedBox(height: 3),
                                Text(
                                  event.calendarName!,
                                  maxLines: 1,
                                  overflow: TextOverflow.ellipsis,
                                  style: TextStyle(
                                    fontSize: 10,
                                    height: 1.2,
                                    color: FloePalette.neutral600,
                                  ),
                                ),
                              ],
                            ],
                          ),
                        ),
                        if (event.externalId != null && height >= 30)
                          Padding(
                            padding: EdgeInsets.only(left: 6),
                            child: Icon(
                              LucideIcons.lockKeyhole,
                              size: 13,
                              color: FloePalette.neutral500,
                            ),
                          ),
                      ],
                    ),
                  ),
          ),
        ),
      ),
    );
  }
}

class _CalendarGuides extends CustomPainter {
  const _CalendarGuides(this.zoom);
  final double zoom;
  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()..strokeWidth = 1;
    for (var halfHour = 0; halfHour <= 48; halfHour++) {
      final vertical = 16 + halfHour * 30 * zoom;
      paint.color = halfHour.isEven
          ? FloePalette.primary200
          : FloePalette.neutral50;
      if (halfHour.isEven) {
        for (
          double horizontal = 72;
          horizontal < size.width - 18;
          horizontal += 7
        ) {
          canvas.drawLine(
            Offset(horizontal, vertical),
            Offset((horizontal + 3).clamp(0, size.width - 18), vertical),
            paint,
          );
        }
      } else {
        canvas.drawLine(
          Offset(72, vertical),
          Offset(size.width - 18, vertical),
          paint,
        );
      }
    }
  }

  @override
  bool shouldRepaint(_CalendarGuides oldDelegate) => oldDelegate.zoom != zoom;
}
