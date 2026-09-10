part of '../personal_day_screen.dart';

class _AdaptiveNavigation extends StatelessWidget {
  const _AdaptiveNavigation({
    required this.narrow,
    required this.selected,
    required this.onSelected,
  });
  final bool narrow;
  final _DestinationView selected;
  final ValueChanged<_DestinationView> onSelected;

  @override
  Widget build(BuildContext context) {
    final settings = _DestinationButton(
      view: _DestinationView.settings,
      selected: selected == _DestinationView.settings,
      onPressed: () => onSelected(_DestinationView.settings),
    );
    if (narrow) {
      return Positioned(
        right: FloeSpace.md,
        bottom: 10,
        left: FloeSpace.md,
        child: FloeSquircle(
          size: FloeSquircleSize.lg,
          elevation: 4,
          padding: EdgeInsets.all(7),
          child: Row(
            children: [
              for (final view in _primaryDestinations)
                Expanded(
                  child: _DestinationButton(
                    view: view,
                    selected: selected == view,
                    onPressed: () => onSelected(view),
                  ),
                ),
              Expanded(child: settings),
            ],
          ),
        ),
      );
    }
    return Positioned(
      top: 28,
      bottom: 28,
      left: 18,
      width: 64,
      child: Column(
        children: [
          SizedBox(height: FloeSpace.xs),
          FloeMascot(size: 40),
          SizedBox(height: FloeSpace.xl),
          for (final view in _primaryDestinations) ...[
            _DestinationButton(
              view: view,
              selected: selected == view,
              onPressed: () => onSelected(view),
            ),
            SizedBox(height: FloeSpace.sm),
          ],
          Spacer(),
          settings,
        ],
      ),
    );
  }
}

class _DestinationButton extends StatefulWidget {
  const _DestinationButton({
    required this.view,
    required this.selected,
    required this.onPressed,
  });

  final _DestinationView view;
  final bool selected;
  final VoidCallback onPressed;

  @override
  State<_DestinationButton> createState() => _DestinationButtonState();
}

class _DestinationButtonState extends State<_DestinationButton> {
  bool hovered = false;
  bool focused = false;
  bool get selected => widget.selected;
  _DestinationView get view => widget.view;

  String get label => switch (view) {
    _DestinationView.today => AppLocalizations.of(context).calendar,
    _DestinationView.tasks => AppLocalizations.of(context).tasks,
    _DestinationView.notes => AppLocalizations.of(context).notes,
    _DestinationView.activity => 'Activity',
    _DestinationView.connections => AppLocalizations.of(context).connect,
    _DestinationView.settings => AppLocalizations.of(context).settings,
  };

  IconData get icon => switch (view) {
    _DestinationView.today => LucideIcons.calendarDays,
    _DestinationView.tasks => LucideIcons.listTodo,
    _DestinationView.notes => LucideIcons.notebookPen,
    _DestinationView.activity => LucideIcons.history,
    _DestinationView.connections => LucideIcons.link,
    _DestinationView.settings => LucideIcons.settings,
  };

  @override
  Widget build(BuildContext context) => Semantics(
    selected: selected,
    label: label,
    button: true,
    child: PressableScale(
      scale: 0.98,
      builder: (states) => FloeSquircle(
        size: FloeSquircleSize.md,
        fill: selected
            ? FloePalette.primary100
            : hovered
            ? FloePalette.neutral100
            : Colors.transparent,
        borderColor: focused ? FloePalette.primary600 : Colors.transparent,
        borderWidth: focused ? 2 : 0,
        child: FloePressable(
          size: FloeSquircleSize.md,
          statesController: states,
          onPressed: widget.onPressed,
          onHover: (value) => setState(() => hovered = value),
          onFocusChange: (value) => setState(() => focused = value),
          child: SizedBox(
            width: MediaQuery.sizeOf(context).width <= 780 ? null : 58,
            height: 40 + MediaQuery.textScalerOf(context).scale(20),
            child: FloeTooltip(
              message: label,
              child: Center(
                child: Icon(
                  icon,
                  size: 20,
                  color: selected
                      ? FloePalette.primary600
                      : FloePalette.neutral600,
                ),
              ),
            ),
          ),
        ),
      ),
    ),
  );
}

class _DayToolbar extends StatelessWidget {
  const _DayToolbar(
    this.controller, {
    required this.narrow,
    required this.onCreateEvent,
  });
  final PersonalDayController controller;
  final bool narrow;
  final VoidCallback? onCreateEvent;

  @override
  Widget build(BuildContext context) {
    final compact = MediaQuery.sizeOf(context).width <= 430;
    final leading = Row(
      children: [
        for (final direction in [-1, 1]) ...[
          FloeSquircle(
            size: FloeSquircleSize.md,
            child: FloeButton.icon(
              tooltip: direction == -1
                  ? AppLocalizations.of(context).previousDay
                  : AppLocalizations.of(context).nextDay,
              onPressed: () => controller.moveDay(direction),
              icon: Icon(
                direction == -1
                    ? LucideIcons.chevronLeft
                    : LucideIcons.chevronRight,
                size: 20,
              ),
            ),
          ),
          SizedBox(width: compact ? 6 : 12),
        ],
        SizedBox(width: narrow ? 2 : 14),
        Flexible(
          child: Text(
            _date(context, controller.query.date),
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: FloeType.headline.copyWith(
              fontSize: compact ? 17 : 20,
              fontWeight: FontWeight.w600,
            ),
          ),
        ),
        SizedBox(width: narrow ? 8 : 12),
        FloeButton.text(
          style: ButtonStyle(
            padding: const WidgetStatePropertyAll(EdgeInsets.zero),
            minimumSize: const WidgetStatePropertyAll(Size(0, 40)),
            tapTargetSize: MaterialTapTargetSize.shrinkWrap,
            textStyle: WidgetStatePropertyAll(
              FloeType.body.copyWith(fontSize: compact ? 12 : 16),
            ),
          ),
          onPressed:
              DateUtils.isSameDay(controller.query.date, controller.query.now)
              ? null
              : controller.goToday,
          child: Text(
            DateUtils.isSameDay(controller.query.date, controller.query.now)
                ? AppLocalizations.of(context).today
                : AppLocalizations.of(context).goToToday,
          ),
        ),
      ],
    );
    return Padding(
      padding: EdgeInsets.only(top: 12, bottom: 16),
      child: Row(
        children: [
          Expanded(child: leading),
          FloeButton.icon(
            tooltip: AppLocalizations.of(context).createEvent,
            onPressed: onCreateEvent,
            icon: const Icon(LucideIcons.plus, size: 18),
          ),
          FloeButton.icon(
            tooltip: AppLocalizations.of(context).refreshCalendar,
            loading: controller.loadState == DayLoadState.loading,
            onPressed: () async {
              await controller.refresh();
              if (!context.mounted ||
                  controller.loadState != DayLoadState.ready ||
                  controller.snapshot?.calendar?.error != null) {
                return;
              }
              FloeToastHost.of(context).show(
                title: AppLocalizations.of(context).calendarsRefreshed,
                description: AppLocalizations.of(context)
                    .localTasksAndNotesUnchanged,
              );
            },
            icon: Icon(LucideIcons.refreshCw, size: 18),
          ),
        ],
      ),
    );
  }
}
