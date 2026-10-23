part of '../personal_day_screen.dart';

class _AdaptiveNavigation {
  const _AdaptiveNavigation({
    required this.selected,
    required this.onSelected,
    required this.onAssistant,
    required this.assistantFocus,
  });
  final _DestinationView selected;
  final ValueChanged<_DestinationView> onSelected;
  final VoidCallback onAssistant;
  final FocusNode assistantFocus;

  Widget _more(BuildContext context) => PopupMenuButton<_DestinationView>(
    tooltip: AppLocalizations.of(context).navigationMore,
    icon: const Icon(LucideIcons.ellipsis, size: 20),
    onSelected: onSelected,
    itemBuilder: (context) => [
      const PopupMenuItem(
        value: _DestinationView.activity,
        child: Text('Activity'),
      ),
      PopupMenuItem(
        value: _DestinationView.connections,
        child: Text(AppLocalizations.of(context).connectedServices),
      ),
      PopupMenuItem(
        value: _DestinationView.settings,
        child: Text(AppLocalizations.of(context).settings),
      ),
    ],
  );

  Widget _assistant({bool compact = false}) => FloeButton.text(
    key: const Key('assistant-entry'),
    focusNode: assistantFocus,
    onPressed: onAssistant,
    style: ButtonStyle(
      padding: WidgetStatePropertyAll(
        EdgeInsets.symmetric(horizontal: compact ? 4 : 8, vertical: 8),
      ),
      minimumSize: const WidgetStatePropertyAll(Size(72, 56)),
    ),
    child: const Padding(
      padding: EdgeInsets.symmetric(vertical: 4),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          ExcludeSemantics(child: FloeMascot(size: 24)),
          SizedBox(height: 4),
          Text('Floe', style: FloeType.caption),
        ],
      ),
    ),
  );

  Widget compactHeader(BuildContext context) => Padding(
    padding: const EdgeInsets.fromLTRB(16, 4, 8, 12),
    child: Row(
      key: const Key('compact-navigation-header'),
      children: [
        const Text('Floe', style: FloeType.title),
        const Spacer(),
        _more(context),
      ],
    ),
  );

  Widget compactDock() => Padding(
    padding: const EdgeInsets.fromLTRB(12, 12, 12, 8),
    child: FloeSquircle(
      key: const Key('compact-navigation-dock'),
      size: FloeSquircleSize.lg,
      elevation: 2,
      padding: const EdgeInsets.all(4),
      child: Row(
        children: [
          for (final view in _primaryDestinations)
            Expanded(
              child: _DestinationButton(
                view: view,
                selected: selected == view,
                onPressed: () => onSelected(view),
                compact: true,
              ),
            ),
          Expanded(child: _assistant(compact: true)),
        ],
      ),
    ),
  );

  Widget desktopRail() => LayoutBuilder(
    builder: (context, constraints) => SingleChildScrollView(
      key: const Key('desktop-navigation-scroll'),
      primary: false,
      child: ConstrainedBox(
        constraints: BoxConstraints(minHeight: constraints.maxHeight),
        child: IntrinsicHeight(
          child: Column(
            children: [
              const Padding(
                padding: EdgeInsets.only(bottom: 32),
                child: Text('Floe', style: FloeType.title),
              ),
              for (final view in _primaryDestinations) ...[
                _DestinationButton(
                  view: view,
                  selected: selected == view,
                  onPressed: () => onSelected(view),
                ),
                const SizedBox(height: 8),
              ],
              const SizedBox(height: 20),
              _assistant(),
              const Spacer(),
              _more(context),
            ],
          ),
        ),
      ),
    ),
  );
}

class _DestinationButton extends StatelessWidget {
  const _DestinationButton({
    required this.view,
    required this.selected,
    required this.onPressed,
    this.compact = false,
  });
  final _DestinationView view;
  final bool selected;
  final VoidCallback onPressed;
  final bool compact;

  @override
  Widget build(BuildContext context) {
    final copy = AppLocalizations.of(context);
    final label = switch (view) {
      _DestinationView.today => copy.today,
      _DestinationView.tasks => copy.tasks,
      _DestinationView.notes => copy.notes,
      _ => '',
    };
    final icon = switch (view) {
      _DestinationView.today => LucideIcons.calendarDays,
      _DestinationView.tasks => LucideIcons.listTodo,
      _ => LucideIcons.notebookPen,
    };
    return Semantics(
      selected: selected,
      child: FloeTooltip(
        message: label,
        child: FloeButton.text(
          onPressed: onPressed,
          style: ButtonStyle(
            backgroundColor: WidgetStateProperty.resolveWith(
              (states) => states.contains(WidgetState.hovered)
                  ? FloePalette.neutral100
                  : selected
                  ? FloePalette.primary100
                  : Colors.transparent,
            ),
            side: WidgetStateProperty.resolveWith(
              (states) => BorderSide(
                color: states.contains(WidgetState.focused)
                    ? FloePalette.primary600
                    : Colors.transparent,
                width: 2,
              ),
            ),
            foregroundColor: WidgetStatePropertyAll(
              selected ? FloePalette.primary700 : FloeColor.textSecondary,
            ),
            padding: WidgetStatePropertyAll(
              EdgeInsets.symmetric(horizontal: compact ? 4 : 8, vertical: 8),
            ),
            minimumSize: const WidgetStatePropertyAll(Size(72, 56)),
          ),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(icon, size: 20),
              const SizedBox(height: 4),
              Text(
                label,
                style: FloeType.caption.copyWith(
                  color: selected
                      ? FloePalette.primary700
                      : FloeColor.textSecondary,
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _DayToolbar extends StatelessWidget {
  const _DayToolbar(
    this.controller, {
    required this.narrow,
    required this.onCreateEvent,
    required this.statusLabel,
    required this.onStatusPressed,
  });
  final PersonalDayController controller;
  final bool narrow;
  final VoidCallback? onCreateEvent;
  final String? statusLabel;
  final VoidCallback? onStatusPressed;

  @override
  Widget build(BuildContext context) {
    final copy = AppLocalizations.of(context);
    final fullDate = _date(context, controller.query.date);
    final dateStyle = DefaultTextStyle.of(context).style
        .merge(FloeType.headline);
    final shortDate = DateFormat.MMMd(copy.localeName)
        .format(controller.query.date);
    double measureDate(String value) {
      final painter = TextPainter(
        text: TextSpan(text: value, style: dateStyle),
        textDirection: Directionality.of(context),
        textScaler: MediaQuery.textScalerOf(context),
        locale: Localizations.localeOf(context),
        maxLines: 1,
      )..layout();
      final width = painter.width;
      painter.dispose();
      return width;
    }

    final fullDateWidth = measureDate(fullDate);
    final shortDateWidth = measureDate(shortDate);
    final date = LayoutBuilder(
      builder: (context, constraints) {
        final fullDateFits = fullDateWidth <= constraints.maxWidth;
        return Text(
          key: const Key('day-selected-date'),
          fullDateFits ? fullDate : shortDate,
          semanticsLabel: fullDate,
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
          style: dateStyle,
        );
      },
    );
    final today = FloeButton.text(
      key: const Key('day-go-today'),
      onPressed:
          DateUtils.isSameDay(controller.query.date, controller.query.now)
          ? null
          : controller.goToday,
      child: Text(copy.today),
    );
    final arrows = [
      for (final direction in [-1, 1])
        FloeButton.icon(
          tooltip: direction == -1 ? copy.previousDay : copy.nextDay,
          onPressed: () => controller.moveDay(direction),
          icon: Icon(
            direction == -1
                ? LucideIcons.chevronLeft
                : LucideIcons.chevronRight,
            size: 20,
          ),
        ),
    ];
    Widget dateNavigation() {
      Widget arrow(int direction) => FloeButton.icon(
        tooltip: direction == -1 ? copy.previousDay : copy.nextDay,
        onPressed: () => controller.moveDay(direction),
        constraints: const BoxConstraints.tightFor(width: 48, height: 48),
        icon: Icon(
          direction == -1 ? LucideIcons.chevronLeft : LucideIcons.chevronRight,
          size: 20,
        ),
      );

      return LayoutBuilder(
        builder: (context, constraints) {
          const arrowWidth = 48.0;
          const gap = 4.0;
          final availableDateWidth =
              (constraints.maxWidth - arrowWidth * 2 - gap * 2).clamp(
                0.0,
                double.infinity,
              );
          final displayDate = fullDateWidth <= availableDateWidth
              ? fullDate
              : shortDate;
          final displayWidth =
              (displayDate == fullDate ? fullDateWidth : shortDateWidth).clamp(
                0.0,
                availableDateWidth,
              );
          return SizedBox(
            key: const Key('day-date-navigation'),
            width: arrowWidth * 2 + gap * 2 + displayWidth,
            child: Row(
              children: [
                arrow(-1),
                const SizedBox(width: gap),
                SizedBox(
                  width: displayWidth,
                  child: Text(
                    key: const Key('day-selected-date'),
                    displayDate,
                    semanticsLabel: fullDate,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: dateStyle,
                  ),
                ),
                const SizedBox(width: gap),
                arrow(1),
              ],
            ),
          );
        },
      );
    }

    final actions = [
      if (onCreateEvent != null)
        FloeButton.icon(
          tooltip: copy.createEvent,
          onPressed: onCreateEvent,
          icon: const Icon(LucideIcons.plus, size: 18),
        ),
      if (controller.canRefresh)
        FloeButton.icon(
          tooltip: copy.refreshCalendar,
          loading: controller.loadState == DayLoadState.loading,
          onPressed: () async {
            await controller.refresh();
            if (!context.mounted ||
                controller.loadState != DayLoadState.ready ||
                controller.errorMessage != null ||
                controller.snapshot == null) {
              return;
            }
            final complete = hasCompleteCalendarCoverage(
              controller.snapshot!,
              controller.query,
            );
            FloeToastHost.of(context).show(
              title: complete ? copy.calendarsRefreshed : 'Refresh completed; Calendar coverage remains incomplete for this date.',
            );
          },
          icon: const Icon(LucideIcons.refreshCw, size: 18),
        ),
      if (statusLabel != null && onStatusPressed != null)
        Semantics(
          key: const Key('day-status-warning'),
          label: statusLabel,
          button: true,
          container: true,
          liveRegion: true,
          onTap: onStatusPressed,
          child: ExcludeSemantics(
            child: FloeButton.icon(
              tooltip: statusLabel,
              onPressed: onStatusPressed,
              style: IconButton.styleFrom(
                foregroundColor: FloePalette.warning600,
              ),
              icon: const Icon(LucideIcons.triangleAlert, size: 18),
            ),
          ),
        ),
    ];
    final needsToday = !DateUtils.isSameDay(
      controller.query.date,
      controller.query.now,
    );
    final compactActions = Row(
      mainAxisSize: MainAxisSize.min,
      children: [if (needsToday) today, ...actions],
    );
    return Padding(
      padding: const EdgeInsets.only(top: 8, bottom: 12),
      child: narrow
          ? Wrap(
              key: const Key('day-compact-toolbar'),
              alignment: WrapAlignment.spaceBetween,
              crossAxisAlignment: WrapCrossAlignment.center,
              spacing: 4,
              runSpacing: 4,
              children: [dateNavigation(), compactActions],
            )
          : Row(
              children: [
                ...arrows,
                const SizedBox(width: 16),
                Flexible(child: date),
                const SizedBox(width: 12),
                today,
                const Spacer(),
                ...actions,
              ],
            ),
    );
  }
}
