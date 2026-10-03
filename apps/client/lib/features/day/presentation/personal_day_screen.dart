import 'package:floe_client/features/connections/presentation/connections_controller.dart';
import 'package:intl/intl.dart';

import 'dart:async';

import 'package:floe_client/l10n/app_localizations.dart';
import 'package:flutter/material.dart';
import 'package:flutter/foundation.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_action_card.dart';
import 'package:floe_client/app/floe_selection.dart';
import 'package:floe_client/app/floe_button.dart';
import 'package:floe_client/app/floe_input.dart';
import 'package:floe_client/app/floe_mascot.dart';
import 'package:floe_client/app/floe_loading.dart';
import 'package:floe_client/app/floe_motion.dart';
import 'package:floe_client/app/floe_primitives.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/app/floe_toast.dart';
import 'package:floe_client/features/day/application/day_gateway.dart';
import 'package:floe_client/features/actions/application/calendar_action_gateway.dart';
import 'package:floe_client/features/actions/application/calendar_action_controller.dart';
import 'package:floe_client/features/actions/presentation/calendar_action_panel.dart';
import 'package:floe_client/features/actions/presentation/calendar_action_proposal.dart';
import 'package:floe_client/features/day/application/personal_day_controller.dart';
import 'package:floe_client/features/day/domain/day_models.dart';
import 'package:floe_client/features/actions/domain/calendar_action.dart';
import 'package:floe_client/features/day/presentation/day_appearance.dart';
import 'package:floe_client/features/day/presentation/calendar_agenda.dart';
import 'package:floe_client/features/day/presentation/calendar_context_rail.dart';
import 'package:floe_client/features/connections/presentation/connector_screen.dart';
import 'package:floe_client/app/floe_feedback.dart';
import 'package:floe_client/features/settings/presentation/settings_screen.dart';
import 'package:floe_client/features/conversation/application/agent_conversation_gateway.dart';
import 'package:floe_client/features/conversation/application/conversation_controller.dart';
import 'package:floe_client/features/conversation/domain/agent_interaction.dart';
import 'package:floe_client/app/runtime/local_owner_gateways_scope.dart';
import 'package:floe_client/app/runtime/agent_vault_gateway.dart';
import 'package:floe_client/features/conversation/presentation/agent_panel.dart';

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
    this.calendarActions,
    this.agentGateway,
    this.connectionsController,
    this.ownerGateways = const LocalOwnerGateways(),
  });
  final DayGateway gateway;
  final CalendarActionGateway? calendarActions;
  final DayQuery query;
  final AgentConversationGateway? agentGateway;
  final ConnectionsController? connectionsController;
  final LocalOwnerGateways ownerGateways;
  @override
  State<PersonalDayScreen> createState() => _PersonalDayScreenState();
}

class _PersonalDayScreenState extends State<PersonalDayScreen>
    with WidgetsBindingObserver {
  late final PersonalDayController controller;
  CalendarActionController? actionController;
  ConversationController? agentController;
  bool assistantOpen = false;
  bool openDeviceCalendarDetail = false;
  AgentExpertBindingTarget? expertBindingTarget;
  Future<void> Function()? onBindingReplaced;
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
    unawaited(controller.load());
    if (widget.calendarActions case final gateway?) {
      actionController = CalendarActionController(
        gateway: gateway,
      )..load();
    }
    screenState = Listenable.merge([controller, ?actionController]);
    final agentGateway = widget.agentGateway;
    if (agentGateway != null) {
      agentController = ConversationController(
        gateway: agentGateway,
        owners: widget.ownerGateways,
        personId: widget.query.personId,
      )..addListener(_reloadActionAuthorityAfterVaultUnlock);
    }
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    controller.dispose();
    agentController?.removeListener(_reloadActionAuthorityAfterVaultUnlock);
    actionController?.dispose();
    agentController?.dispose();
    assistantEntryFocus.dispose();
    super.dispose();
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
      return ConnectorScreen(controller: widget.connectionsController);
    }
    if (destination == _DestinationView.settings) {
      return SettingsScreen(
        connectionsController: widget.connectionsController,
        actionController: actionController,
        agentController: agentController,
        expertBindingTarget: expertBindingTarget,
        onBindingReplaced: onBindingReplaced,
        platform: defaultTargetPlatform,
      );
    }
    if (destination == _DestinationView.activity) {
      final actions = actionController;
      return actions == null
          ? const Text('Activity is available in the native Floe app.')
          : ActivityPanel(controller: actions);
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
                    onCreateEvent: actionController?.calendarChangesAvailable == true &&
                            actionController?.busy == false
                        ? () => _openCalendarEditor()
                        : null,
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
    final calendarChangesAvailable = actions != null &&
        actions.calendarChangesAvailable && !actions.busy;
    final showReviews =
        actions != null && actions.actions.any((action) =>
          action.allowedActions.contains(ActionAllowedAction.approve) ||
          action.allowedActions.contains(ActionAllowedAction.reject) ||
          action.allowedActions.contains(ActionAllowedAction.cancel));
    final primary = CalendarAgenda(
      key: PageStorageKey('calendar-agenda'),
      snapshot: snapshot,
      query: controller.query,
      loading: controller.loadState == DayLoadState.loading,
      onConnections: () => _selectDestination(_DestinationView.connections),
      onRefresh: controller.canRefresh ? controller.refresh : null,
      onCreateEvent: !calendarChangesAvailable
          ? null
          : (startsAt) => _openCalendarEditor(startsAt),
      draftStartsAt: draftEventStart,
      onEditEvent: calendarChangesAvailable ? _editCalendarEvent : null,
      onDeleteEvent: calendarChangesAvailable ? _deleteCalendarEvent : null,
      onMoveEvent: calendarChangesAvailable ? _moveCalendarEvent : null,
    );
    final rail = Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (showReviews) ...[
          ReviewRequestPanel(
            controller: actions,
          ),
          SizedBox(height: FloeSpace.lg),
        ],
        CalendarContextRail(
          snapshot: snapshot,
          query: controller.query,
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
              onOpenConnections: () =>
                  _selectDestination(_DestinationView.connections),
              onOpenSourceReview: _openAgentSourceReview,
              onOpenExpertSettings: _openExpertSettings,
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
                      onOpenConnections: () =>
                          _selectDestination(_DestinationView.connections),
                      onOpenSourceReview: _openAgentSourceReview,
                      onOpenExpertSettings: _openExpertSettings,
                      onClose: _closeAssistant,
                    )
                  : SingleChildScrollView(child: rail),
            ),
          ],
        );
      },
    );
  }

  void _selectDestination(
    _DestinationView value, {
    bool openCalendarDetail = false,
  }) {
    if (value == _DestinationView.settings) {
      if (agentController?.session == null && agentController?.busy == false) {
        unawaited(agentController?.load());
      }
    }
    if (value == _DestinationView.connections) {
      if (agentController?.session == null && agentController?.busy == false) {
        unawaited(agentController?.load());
      }
    }
    setState(() {
      assistantOpen = false;
      openDeviceCalendarDetail = openCalendarDetail;
      destination = value;
      selectedTaskId = null;
    });
  }

  void _reloadActionAuthorityAfterVaultUnlock() {
    final agent = agentController;
    final actions = actionController;
    if (agent?.vaultState == AgentVaultState.ready &&
        actions?.error != null &&
        !actions!.busy) {
      unawaited(actions.load());
    }
  }

  Future<void> _openAssistant() async {
    final agent = agentController;
    if (agent == null) return;
    agent.attachView();
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
          onOpenConnections: () =>
              _selectDestination(_DestinationView.connections),
          onOpenSourceReview: _openAgentSourceReview,
          onOpenExpertSettings: _openExpertSettings,
          onClose: () {
            agent.detachView();
            Navigator.pop(context);
          },
        ),
      ),
    );
  }

  Future<void> _openAgentAction(String actionRef) async {
    final actions = actionController;
    if (actions == null) return;
    unawaited(actions.load());
    await showFloeDialog<void>(
      context,
      (_) => ActionReviewDialog(
        controller: actions,
        actionRef: actionRef,
      ),
    );
  }

  void _openAgentSourceReview() {
    if (MediaQuery.sizeOf(context).width <= 960) {
      Navigator.of(context).maybePop();
    }
    _selectDestination(_DestinationView.connections, openCalendarDetail: true);
  }

  void _openExpertSettings(
    AgentExpertBindingTarget target,
    Future<void> Function() onReplaced,
  ) {
    if (MediaQuery.sizeOf(context).width <= 960) {
      Navigator.of(context).maybePop();
    }
    setState(() {
      expertBindingTarget = target;
      onBindingReplaced = onReplaced;
    });
    _selectDestination(_DestinationView.settings);
  }

  void _closeAssistant() {
    agentController?.detachView();
    setState(() => assistantOpen = false);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted && destination == _DestinationView.today) {
        assistantEntryFocus.requestFocus();
      }
    });
  }

  Future<void> _openCalendarEditor([DateTime? startsAt]) async {
    await _showCalendarComposer(initialStart: startsAt);
  }

  Future<void> _showCalendarComposer({
    DateTime? initialStart,
    EventItem? event,
  }) async {
    final actions = actionController;
    if (actions == null || actions.busy || !actions.calendarChangesAvailable) return;
    final date = controller.query.date;
    final now = DateTime.now();
    final createStart =
        initialStart ??
        (DateUtils.isSameDay(date, now)
            ? DateTime(now.year, now.month, now.day, now.hour + 1)
            : DateTime(date.year, date.month, date.day, 9));
    if (event == null) setState(() => draftEventStart = createStart);
    await showFloeDialog<void>(
      context,
      (_) => CalendarEventComposer(
        controller: actions,
        initialStart: event == null ? createStart : initialStart,
        event: event,
      ),
    );
    if (event == null && mounted && draftEventStart == createStart) {
      setState(() => draftEventStart = null);
    }
  }

  Future<void> _editCalendarEvent(EventItem event) async {
    if (event.actionTarget == null) return;
    await _showCalendarComposer(event: event);
  }

  Future<void> _moveCalendarEvent(EventItem event, DateTime start) =>
      _showCalendarComposer(initialStart: start, event: event);

  Future<void> _deleteCalendarEvent(EventItem event) async {
    final actions = actionController;
    if (actions == null || actions.busy || !actions.calendarChangesAvailable ||
        event.actionTarget == null) return;
    final confirmed = await showFloeDialog<bool>(
      context,
      (dialogContext) => AnimatedBuilder(
        animation: actions,
        builder: (dialogContext, _) => FloeDetailDialog(
          title: 'Delete event?',
          children: [
            Text(
              '“${event.title}” will be removed from ${event.calendarLabel ?? 'its Calendar source'}.',
            ),
            if (!actions.calendarChangesAvailable)
              const Text('Calendar changes are unavailable. No writable Calendar destination could be confirmed.'),
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
                  onPressed: actions.busy || !actions.calendarChangesAvailable
                      ? null : () => Navigator.of(dialogContext).pop(true),
                  child: const Text('Delete event'),
                ),
              ],
            ),
          ],
        ),
      ),
    );
    if (confirmed == true && mounted) {
      await _deleteCalendarEventAtOwner(event);
    }
  }

  Future<void> _deleteCalendarEventAtOwner(EventItem event) async {
    final actions = actionController;
    final target = event.actionTarget;
    if (actions == null || actions.busy || !actions.calendarChangesAvailable ||
        target == null) return;
    try {
      final result = await actions.submit(
        DirectDelete(
          eventRef: target.eventId,
          expectedRevision: target.expectedRevision,
        ),
      );
      _showCalendarActionOutcome(result, success: 'Event deleted');
    } on Object {
      _showCalendarActionOutcome(null, success: 'Event deleted');
    }
  }

  void _showCalendarActionOutcome(
    CalendarAction? action, {
    required String success,
  }) {
    if (!mounted) return;
    final controller = actionController;
    final title = switch (action?.status.state) {
      CalendarActionState.succeeded =>
        action?.status.collection == ActionCollectionStatus.pending
            ? '$success. Day update is pending; check Activity.'
            : success,
      CalendarActionState.blocked =>
        'Blocked: ${action?.status.blockedReason?.name ?? 'The action owner blocked this request.'}',
      CalendarActionState.unknown =>
        'Unknown outcome: ${action?.status.unknownReason?.name ?? 'Reconcile this action in Activity.'}',
      CalendarActionState.failed =>
        'Not applied: ${action?.status.failedReason?.name ?? 'Check Activity.'}',
      null => controller?.error?.message ??
          'The action outcome was not confirmed. Check Activity.',
      final status => 'Action status: ${status.name}. Check Activity.',
    };
    FloeToastHost.of(context).show(
      title: title,
    );
  }

  Future<void> _setTaskCompleted(TaskItem task, bool completed) async {
    final acknowledged = await controller.setTaskCompleted(task, completed);
    if (!mounted || acknowledged == null) return;
    final changedTask = _taskById(acknowledged, task.id);
    FloeToastHost.of(context).show(
      title: completed
          ? AppLocalizations.of(context).taskCompleted
          : AppLocalizations.of(context).taskMarkedIncomplete,
      actionLabel: changedTask == null ? null : AppLocalizations.of(context).undo,
      onAction: changedTask == null ? null : () {
        if (mounted) controller.setTaskCompleted(changedTask, !completed);
      },
    );
  }

  Future<bool> _createNote(String content) async {
    if (controller.commandPending) return false;
    // Retain an acknowledged capture when classification acknowledgement is
    // lost, so Save retries classification instead of creating another capture.
    if (controller.pendingCapture?.originalInput != content &&
        !await controller.submitCapture(content)) return false;
    final saved = await controller.classify(NoteDraft(content: content));
    if (saved && mounted) {
      FloeToastHost.of(context)
          .show(title: AppLocalizations.of(context).savedInFloe);
    }
    return saved;
  }
}
