import 'package:intl/intl.dart';

import 'dart:async';

import 'package:floe_client/l10n/app_localizations.dart';
import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../../app/design_tokens.dart';
import '../../../app/floe_action_card.dart';
import '../../../app/floe_selection.dart';
import '../../../app/floe_button.dart';
import '../../../app/floe_input.dart';
import '../../../app/floe_mascot.dart';
import '../../../app/floe_loading.dart';
import '../../../app/floe_motion.dart';
import '../../../app/floe_primitives.dart';
import '../../../app/floe_squircle.dart';
import '../../../app/floe_theme.dart';
import '../../../app/floe_toast.dart';
import '../application/day_gateway.dart';
import '../application/calendar_gateway.dart';
import '../application/calendar_action_gateway.dart';
import '../application/calendar_action_controller.dart';
import 'calendar_action_panel.dart';
import 'calendar_action_proposal.dart';
import '../application/personal_day_controller.dart';
import '../domain/day_models.dart';
import '../domain/calendar_action.dart';
import 'day_appearance.dart';
import 'calendar_agenda.dart';
import 'calendar_context_rail.dart';
import 'connector_screen.dart';
import '../../../app/floe_feedback.dart';
import '../../server/settings_screen.dart';
import '../../server/local_server_client.dart';
import '../../agent/agent_fixture_gateway.dart';
import '../../agent/agent_controller.dart';
import '../../agent/agent_calendar_sources.dart';
import '../../agent/agent_panel.dart';
import '../../../infrastructure/native/android_context_gateway.dart';

part 'personal_day/navigation.dart';
part 'personal_day/tasks.dart';
part 'personal_day/notes.dart';
part 'personal_day/day_row.dart';
part 'personal_day/feedback.dart';

enum _DestinationView { today, tasks, notes, activity, connections, settings }

const _primaryDestinations = [
  _DestinationView.today,
  _DestinationView.tasks,
  _DestinationView.notes,
  _DestinationView.activity,
  _DestinationView.connections,
];

class PersonalDayScreen extends StatefulWidget {
  const PersonalDayScreen({
    super.key,
    required this.gateway,
    required this.query,
    this.agentGateway,
    this.serverClient,
    this.androidContext,
  });
  final DayGateway gateway;
  final DayQuery query;
  final AgentFixtureStreamingGateway? agentGateway;
  final LocalServerClient? serverClient;
  final AndroidContextApi? androidContext;
  @override
  State<PersonalDayScreen> createState() => _PersonalDayScreenState();
}

class _PersonalDayScreenState extends State<PersonalDayScreen> {
  late final PersonalDayController controller;
  CalendarActionController? actionController;
  AgentController? agentController;
  bool assistantOpen = false;
  final assistantEntryFocus = FocusNode();
  late final Listenable screenState;
  _DestinationView destination = _DestinationView.today;
  String? selectedTaskId;
  DateTime? draftEventStart;

  @override
  void initState() {
    super.initState();
    controller = PersonalDayController(
      gateway: widget.gateway,
      query: widget.query,
    );
    _loadCalendar();
    if (widget.gateway case final CalendarActionGateway gateway) {
      actionController = CalendarActionController(
        gateway: gateway,
        personId: widget.query.personId,
        collect: _collectAction,
      )..load();
    }
    screenState = Listenable.merge([controller, ?actionController]);
    final agentGateway = widget.agentGateway;
    if (agentGateway != null) {
      agentController = AgentController(
        gateway: agentGateway,
        personId: widget.query.personId,
      );
    }
  }

  @override
  void dispose() {
    controller.dispose();
    actionController?.dispose();
    agentController?.dispose();
    assistantEntryFocus.dispose();
    super.dispose();
  }

  Future<void> _loadCalendar() async {
    await controller.load();
    if (mounted && controller.snapshot?.calendar?.provider == 'event_kit') {
      await controller.refresh();
    }
  }

  Future<void> _collectAction(CalendarAction action) async {
    if (widget.gateway is! CalendarGateway) {
      throw StateError('Calendar read unavailable');
    }
    final gateway = widget.gateway as CalendarGateway;
    final start = action.startsAt.toLocal();
    final end = action.endsAt
        .subtract(const Duration(microseconds: 1))
        .toLocal();
    var day = DateTime(start.year, start.month, start.day);
    final last = DateTime(end.year, end.month, end.day);
    var matched = false;
    while (!day.isAfter(last)) {
      final snapshot = await gateway.syncCalendar(
        DayQuery.local(
          personId: action.personId,
          date: day,
          now: DateTime.now(),
        ),
      );
      final connection = snapshot.calendar;
      if (connection == null ||
          connection.error != null ||
          connection.calendars.any(
            (calendar) =>
                calendar.id == action.calendarId && calendar.error != null,
          )) {
        throw StateError('Calendar collection failed');
      }
      matched =
          matched ||
          snapshot.items.whereType<EventItem>().any(
            (event) =>
                event.externalId == action.externalId &&
                event.calendarId == action.calendarId,
          );
      day = DateTime(day.year, day.month, day.day + 1);
    }
    if (mounted) await controller.load();
    final deleting = action.mutation?['delete'] == true;
    if (deleting ? matched : !matched) {
      throw StateError('Calendar change has not been collected yet');
    }
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: screenState,
    builder: (context, _) => LayoutBuilder(
      builder: (context, constraints) {
        final narrow = constraints.maxWidth <= 780;
        return FloeScaffold(
          backgroundColor: FloePalette.neutral25,
          body: SafeArea(
            child: Stack(
              children: [
                Positioned.fill(child: _page(narrow)),
                _AdaptiveNavigation(
                  narrow: narrow,
                  selected: destination,
                  onSelected: _selectDestination,
                ),
              ],
            ),
          ),
        );
      },
    ),
  );

  Widget _page(bool narrow) {
    final padding = EdgeInsets.fromLTRB(
      narrow ? FloeSpace.md : 120,
      narrow ? 16 : 24,
      narrow ? FloeSpace.md : 36,
      narrow ? 96 : 24,
    );
    final workspace = FloeScreenEntrance(
      identity: selectedTaskId ?? destination,
      child: _workspace(narrow),
    );
    if (destination == _DestinationView.settings) {
      return Padding(padding: padding, child: workspace);
    }
    if (destination != _DestinationView.today || selectedTaskId != null) {
      return SingleChildScrollView(padding: padding, child: workspace);
    }
    return Padding(padding: padding, child: workspace);
  }

  Widget _fillDay(Widget child) =>
      destination == _DestinationView.today ? Expanded(child: child) : child;

  Widget _workspace(bool narrow) {
    if (destination == _DestinationView.connections) {
      return ConnectorScreen(
        gateway: widget.gateway is CalendarGateway
            ? widget.gateway as CalendarGateway
            : null,
        query: controller.query,
        connection: controller.snapshot?.calendar,
        onChanged: controller.load,
      );
    }
    if (destination == _DestinationView.settings) {
      return SettingsScreen(
        client: widget.serverClient,
        actionController: actionController,
        agentController: agentController,
        androidContext: widget.androidContext,
        calendarSources: _agentCalendarSources,
        calendarSourceChanges: controller,
      );
    }
    if (destination == _DestinationView.activity) {
      final actions = actionController;
      return actions == null
          ? const Text('Activity is available in the native Floe app.')
          : ActivityPanel(
              controller: actions,
              connection: () => controller.snapshot?.calendar,
            );
    }
    if (controller.loadState == DayLoadState.failure) {
      return _FailureDay(
        retry: controller.load,
        message: controller.errorMessage,
      );
    }
    final snapshot =
        controller.snapshot ??
        DaySnapshot(
          personId: controller.query.personId,
          date: controller.query.date,
          generatedAt: controller.query.now,
          timezoneOffsetSeconds: controller.query.timezoneOffsetSeconds,
          items: [],
        );
    final selectedTask = _taskById(snapshot, selectedTaskId);
    if (selectedTask != null) {
      return _TaskDetailScreen(
        task: selectedTask,
        snapshot: snapshot,
        narrow: narrow,

        onComplete: _setTaskCompleted,
      );
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (controller.errorMessage case final message?)
          _ErrorNotice(message: message, dismiss: controller.clearError),
        _fillDay(
          FloeLoadingOverlay(
            loading: controller.commandPending,
            child: switch (destination) {
              _DestinationView.today => Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  _DayToolbar(
                    controller,
                    narrow: narrow,
                    onCreateEvent:
                        actionController?.canDirect == true &&
                            controller.snapshot?.calendar != null
                        ? () => _openCalendarEditor()
                        : null,
                  ),
                  if (snapshot.calendar?.error != null)
                    Padding(
                      padding: EdgeInsets.only(bottom: 16),
                      child: FloeSquircle(
                        fill: FloePalette.amber50,
                        padding: EdgeInsets.all(FloeSpace.base),
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(
                              snapshot.calendar!.lastSuccessAt == null
                                  ? AppLocalizations.of(context)
                                        .calendarCouldNotBeCollectedCheckAccess
                                  : AppLocalizations.of(
                                      context,
                                    ).showingSavedEventsCalendarChangesCouldNot,
                              style: FloeType.bodySmall.copyWith(
                                color: FloePalette.neutral600,
                              ),
                            ),
                            FloeTextLink(
                              label: AppLocalizations.of(context)
                                  .manageConnection,
                              onPressed: () => _selectDestination(
                                _DestinationView.connections,
                              ),
                            ),
                          ],
                        ),
                      ),
                    ),
                  Expanded(child: _content(narrow, snapshot)),
                ],
              ),
              _DestinationView.tasks => _TasksScreen(
                snapshot: snapshot,
                disabled: controller.commandPending,
                onComplete: _setTaskCompleted,
                onOpen: (task) => setState(() => selectedTaskId = task.id),
                onDelete: controller.deleteItem,
              ),
              _DestinationView.connections => SizedBox.shrink(),
              _DestinationView.activity => SizedBox.shrink(),
              _DestinationView.settings => SizedBox.shrink(),
              _DestinationView.notes => _NotesScreen(
                notes: snapshot.items.whereType<NoteItem>().toList(),
                narrow: narrow,
                onCreate: _createNote,
                pending: controller.commandPending,
              ),
            },
          ),
        ),
      ],
    );
  }

  Widget _content(bool narrow, DaySnapshot snapshot) {
    final actions = actionController;
    final showReviews =
        actions != null && actions.actions.any((action) => action.needsReview);
    final primary = CalendarAgenda(
      key: PageStorageKey('calendar-agenda'),
      snapshot: snapshot,
      loading: controller.loadState == DayLoadState.loading,
      onConnections: () => _selectDestination(_DestinationView.connections),
      onCreateEvent:
          actionController?.canDirect == true && snapshot.calendar != null
          ? (startsAt) => _openCalendarEditor(startsAt)
          : null,
      draftStartsAt: draftEventStart,
      canModify: (event) => actionController?.canModify(event) == true,
      onEditEvent: (event) => _editCalendarEvent(event),
      onDeleteEvent: _deleteCalendarEvent,
      onMoveEvent: _moveCalendarEvent,
    );
    final rail = Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (showReviews) ...[
          ReviewRequestPanel(
            controller: actions,
            connection: () => controller.loadState == DayLoadState.ready
                ? controller.snapshot?.calendar
                : null,
          ),
          SizedBox(height: FloeSpace.lg),
        ],
        CalendarContextRail(
          snapshot: snapshot,
          disabled: controller.commandPending,
          complete: _setTaskCompleted,
          onTasks: () => _selectDestination(_DestinationView.tasks),
          onOpenTask: (task) => setState(() => selectedTaskId = task.id),
        ),
        if (agentController != null) ...[
          const SizedBox(height: FloeSpace.lg),
          FloeActionCard(
            focusNode: assistantEntryFocus,
            onPressed: _openAssistant,
            leading: const FloeMascot(size: 32),
            title: Text(AppLocalizations.of(context).agentEntry),
            description: Text(AppLocalizations.of(context).agentEntryHint),
            trailing: const Icon(LucideIcons.arrowRight),
          ),
        ],
      ],
    );
    return LayoutBuilder(
      builder: (context, constraints) {
        if (MediaQuery.sizeOf(context).width <= 960) {
          if (assistantOpen && agentController != null) {
            return AgentPanel(
              controller: agentController!,
              onOpenAction: actionController == null ? null : _openAgentAction,
              onClose: _closeAssistant,
            );
          }
          return Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Expanded(flex: 7, child: primary),
              SizedBox(height: narrow ? 16 : 24),
              Expanded(flex: 3, child: SingleChildScrollView(child: rail)),
            ],
          );
        }
        return Row(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Expanded(flex: 7, child: primary),
            SizedBox(width: FloeSpace.lg),
            SizedBox(
              width: ((constraints.maxWidth - 24) * 0.3).clamp(
                288,
                double.infinity,
              ),
              child: assistantOpen && agentController != null
                  ? AgentPanel(
                      controller: agentController!,
                      onOpenAction: actionController == null
                          ? null
                          : _openAgentAction,
                      onClose: _closeAssistant,
                    )
                  : SingleChildScrollView(child: rail),
            ),
          ],
        );
      },
    );
  }

  void _selectDestination(_DestinationView value) {
    if (value == _DestinationView.settings) {
      if (agentController?.session == null && agentController?.busy == false) {
        unawaited(agentController?.load());
      }
    }
    setState(() {
      assistantOpen = false;
      destination = value;
      selectedTaskId = null;
    });
  }

  Future<void> _openAssistant() async {
    final agent = agentController;
    if (agent == null) return;
    if (agent.session == null && !agent.busy) unawaited(agent.load());
    if (MediaQuery.sizeOf(context).width > 960) {
      setState(() => assistantOpen = true);
      return;
    }
    await showFloeSheet<void>(
      context,
      (context) => SizedBox(
        height: MediaQuery.sizeOf(context).height * .88,
        child: AgentPanel(
          controller: agent,
          onOpenAction: actionController == null ? null : _openAgentAction,
          onClose: () => Navigator.pop(context),
        ),
      ),
    );
  }

  Future<void> _openAgentAction(String actionId) async {
    final actions = actionController;
    final agent = agentController;
    if (actions == null ||
        agent == null ||
        actions.personId != agent.personId) {
      return;
    }
    unawaited(actions.load());
    await showFloeDialog<void>(
      context,
      (_) => ActionReviewDialog(
        controller: actions,
        actionId: actionId,
        connection: () => controller.loadState == DayLoadState.ready
            ? controller.snapshot?.calendar
            : null,
      ),
    );
  }

  AgentCalendarSources? _agentCalendarSources() {
    final snapshot = controller.snapshot;
    final connection = snapshot?.calendar;
    if (controller.loadState != DayLoadState.ready ||
        snapshot == null ||
        connection == null) {
      return null;
    }
    return AgentCalendarSources(
      personId: snapshot.personId,
      connection: connection,
    );
  }

  void _closeAssistant() {
    setState(() => assistantOpen = false);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted && destination == _DestinationView.today) {
        assistantEntryFocus.requestFocus();
      }
    });
  }

  Future<void> _openCalendarEditor([DateTime? startsAt]) async {
    final actions = actionController;
    final connection = controller.snapshot?.calendar;
    if (actions == null || !actions.canDirect || connection == null) return;
    final date = controller.query.date;
    final now = DateTime.now();
    final initialStart =
        startsAt ??
        (DateUtils.isSameDay(date, now)
            ? DateTime(now.year, now.month, now.day, now.hour + 1)
            : DateTime(date.year, date.month, date.day, 9));
    setState(() => draftEventStart = initialStart);
    await showFloeDialog<void>(
      context,
      (_) => CalendarEventComposer(
        controller: actions,
        connection: () => controller.snapshot?.calendar,
        initialStart: initialStart,
      ),
    );
    if (mounted && draftEventStart == initialStart) {
      setState(() => draftEventStart = null);
    }
  }

  Future<void> _editCalendarEvent(EventItem event) async {
    final actions = actionController;
    if (actions == null || !actions.canModify(event)) return;
    await showFloeDialog<void>(
      context,
      (_) => CalendarEventComposer(
        controller: actions,
        connection: () => controller.snapshot?.calendar,
        event: event,
      ),
    );
  }

  Future<void> _moveCalendarEvent(EventItem event, DateTime start) =>
      _changeCalendarEvent(event, start: start);

  Future<void> _deleteCalendarEvent(EventItem event) async {
    final confirmed = await showFloeDialog<bool>(
      context,
      (dialogContext) => FloeDetailDialog(
        title: 'Delete event?',
        children: [
          Text(
            '“${event.title}” will be removed from ${event.calendarName ?? 'your calendar'}.',
          ),
          const SizedBox(height: FloeSpace.base),
          Row(
            mainAxisAlignment: MainAxisAlignment.end,
            children: [
              FloeButton.text(
                onPressed: () => Navigator.of(dialogContext).pop(false),
                child: const Text('Cancel'),
              ),
              const SizedBox(width: FloeSpace.md),
              FloeButton.filled(
                onPressed: () => Navigator.of(dialogContext).pop(true),
                child: const Text('Delete event'),
              ),
            ],
          ),
        ],
      ),
    );
    if (confirmed == true && mounted) {
      await _changeCalendarEvent(event, delete: true);
    }
  }

  Future<void> _changeCalendarEvent(
    EventItem event, {
    DateTime? start,
    bool delete = false,
  }) async {
    final actions = actionController;
    if (actions == null || !actions.canModify(event)) return;
    final startsAt = (start ?? event.startsAt).toLocal();
    final result = await actions.direct(
      calendarId: event.calendarId!,
      title: event.title,
      startsAt: startsAt,
      endsAt: startsAt.add(event.endsAt.difference(event.startsAt)),
      timezone: calendarStorageTimezone(startsAt.timeZoneOffset),
      eventId: event.id,
      eventRevision: event.revision,
      delete: delete,
    );
    if (!mounted) return;
    FloeToastHost.of(context).show(
      title: result != null && actions.collection[result.id] == 'failed'
          ? 'Saved. Calendar refresh failed; retry in Activity.'
          : result?.status == CalendarActionStatus.succeeded
          ? (delete ? 'Event deleted' : 'Event moved')
          : 'Change not confirmed. Check Activity and reload your calendar.',
    );
  }

  Future<void> _setTaskCompleted(TaskItem task, bool completed) async {
    await controller.setTaskCompleted(task, completed);
    if (!mounted || controller.errorMessage != null) return;
    FloeToastHost.of(context).show(
      title: completed
          ? AppLocalizations.of(context).taskCompleted
          : AppLocalizations.of(context).taskMarkedIncomplete,
      actionLabel: AppLocalizations.of(context).undo,
      onAction: () {
        if (mounted) controller.setTaskCompleted(task, !completed);
      },
    );
  }

  Future<bool> _createNote(String content) async {
    if (controller.commandPending) return false;
    if (!await controller.submitCapture(content)) return false;
    final saved = await controller.classify(NoteDraft(content: content));
    if (saved && mounted) {
      FloeToastHost.of(context)
          .show(title: AppLocalizations.of(context).savedInFloe);
    }
    return saved;
  }
}
