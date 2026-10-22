import 'package:floe_client/app/floe_squircle.dart';
import 'package:floe_client/features/connections/presentation/connections_controller.dart';
import 'package:floe_client/features/connections/domain/connection_models.dart'
    show SourceRef;
import 'package:intl/intl.dart';

import 'dart:async';

import 'package:floe_client/l10n/app_localizations.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter/foundation.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import 'package:floe_client/app/design_tokens.dart';
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
import 'package:floe_client/features/actions/application/calendar_action_controller.dart';
import 'package:floe_client/features/actions/presentation/calendar_action_proposal.dart';
import 'package:floe_client/features/day/application/personal_day_controller.dart';
import 'package:floe_client/features/day/domain/day_models.dart';
import 'package:floe_client/features/day/presentation/day_appearance.dart';
import 'package:floe_client/features/day/presentation/calendar_agenda.dart';
import 'package:floe_client/features/day/presentation/day_overview.dart';
import 'package:floe_client/features/day/presentation/calendar_context_rail.dart';
import 'package:floe_client/features/day/presentation/manual_calendar_activity.dart';
import 'package:floe_client/features/connections/presentation/connector_screen.dart';
import 'package:floe_client/app/floe_feedback.dart';
import 'package:floe_client/features/settings/presentation/settings_screen.dart';
import 'package:floe_client/features/conversation/application/agent_conversation_gateway.dart';
import 'package:floe_client/features/conversation/application/conversation_controller.dart';
import 'package:floe_client/features/conversation/domain/agent_interaction.dart';
import 'package:floe_client/app/runtime/local_owner_gateways_scope.dart';
import 'package:floe_client/features/conversation/presentation/agent_panel.dart';
import 'package:floe_client/features/conversation/presentation/conversation_composer_draft.dart';
import 'package:floe_client/features/voice/presentation/voice_screen.dart';

part 'personal_day/navigation.dart';
part 'personal_day/tasks.dart';
part 'personal_day/notes.dart';
part 'personal_day/day_row.dart';
part 'personal_day/feedback.dart';

enum _DestinationView { today, tasks, notes, activity, connections, settings }

final class _CalendarStatusPresentation {
  const _CalendarStatusPresentation({
    required this.agendaOwnsEmptyStatus,
    required this.observationComplete,
    required this.label,
  });

  final bool agendaOwnsEmptyStatus;
  final bool observationComplete;
  final String label;

  bool get showFreshnessAction =>
      !agendaOwnsEmptyStatus && !observationComplete;
}

const _primaryDestinations = [
  _DestinationView.today,
  _DestinationView.tasks,
  _DestinationView.notes,
];

class PersonalDayScreen extends StatefulWidget {
  const PersonalDayScreen({
    super.key,
    required this.gateway,
    required this.query,
    required this.ownerGateways,
    this.agentGateway,
    this.connectionsController,
  });
  final DayGateway gateway;
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
  OperationPolicyController? operationPolicyController;
  ConversationController? agentController;
  late final ConversationComposerDraft conversationComposerDraft =
      ConversationComposerDraft();
  bool assistantOpen = false;
  bool compactTimeline = false;
  SourceRef? selectedConnectionSource;
  AgentAssistantFeatureSourceTarget? assistantFeatureSourceTarget;
  Future<void> Function()? onAssistantFeatureConfigured;
  final assistantEntryFocus = FocusNode();
  late final Listenable screenState;
  _DestinationView destination = _DestinationView.today;
  String? selectedTaskId;
  DateTime? draftEventStart;
  bool appActive = true;
  String? connectionSelection;

  @override
  void initState() {
    super.initState();
    controller = PersonalDayController(
      gateway: widget.gateway,
      query: widget.query,
    );
    WidgetsBinding.instance.addObserver(this);
    appActive =
        WidgetsBinding.instance.lifecycleState == null ||
        WidgetsBinding.instance.lifecycleState == AppLifecycleState.resumed;
    widget.connectionsController?.addListener(_connectionsChanged);
    unawaited(_loadInitialDay());
    final runtime = widget.ownerGateways.runtime;
    final actionGateway = widget.ownerGateways.operationAuthorization;
    if (actionGateway != null && runtime != null) {
      operationPolicyController = OperationPolicyController(
        gateway: actionGateway,
        runtime: runtime,
      );
    }
    screenState = Listenable.merge([
      controller,
      ?operationPolicyController,
      ?widget.ownerGateways.runtime,
    ]);
    final agentGateway = widget.agentGateway;
    if (agentGateway != null) {
      agentController = ConversationController(
        gateway: agentGateway,
        owners: widget.ownerGateways,
        personId: widget.query.personId,
      );
    }
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    widget.connectionsController?.removeListener(_connectionsChanged);
    controller.dispose();
    operationPolicyController?.dispose();
    agentController?.dispose();
    conversationComposerDraft.dispose();
    assistantEntryFocus.dispose();
    super.dispose();
  }

  Future<void> _loadInitialDay() async {
    await controller.load();
    if (mounted) _updateRefreshVisibility();
  }

  void _updateRefreshVisibility() => controller.setAutomaticRefreshActive(
    mounted &&
        appActive &&
        destination == _DestinationView.today &&
        selectedTaskId == null,
  );

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    appActive = state == AppLifecycleState.resumed;
    _updateRefreshVisibility();
  }

  void _connectionsChanged() {
    final connections = widget.connectionsController;
    if (!mounted ||
        connections == null ||
        !connections.ready ||
        connections.overview == null)
      return;
    final sources =
        connections.overview!.sources
            .map(
              (source) =>
                  '${source.sourceRef.value}:${source.revision}:${source.availability}:${source.selectedResources.map((resource) => resource.resourceRef.value).join(',')}',
            )
            .toList()
          ..sort();
    final selection = sources.join('|');
    if (connectionSelection == selection) return;
    connectionSelection = selection;
    // Reproject cached evidence after configuration changes, even off the Day tab.
    // This is a query; only the existing active-Day policy admits acquisition.
    unawaited(controller.load());
    unawaited(controller.refreshIfStale(force: true));
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: screenState,
    builder: (context, _) => LayoutBuilder(
      builder: (context, constraints) {
        final narrow = constraints.maxWidth <= 780;
        final navigation = _AdaptiveNavigation(
          selected: destination,
          onSelected: _selectDestination,
          onAssistant: _openAssistant,
          assistantFocus: assistantEntryFocus,
        );
        return FloeScaffold(
          backgroundColor: FloePalette.neutral25,
          body: SafeArea(
            child: Column(
              children: [
                // Keep the header, content, and dock slots in fixed positions
                // across breakpoints so destination state stays mounted.
                Visibility(
                  key: const Key('compact-header-slot'),
                  visible: narrow,
                  child: navigation.compactHeader(context),
                ),
                Expanded(
                  child: Stack(
                    key: const Key('personal-day-middle-viewport'),
                    children: [
                      Positioned.fill(child: _page(narrow)),
                      Positioned(
                        top: 24,
                        bottom: 20,
                        left: 16,
                        width: 80,
                        child: Visibility(
                          key: const Key('desktop-navigation-slot'),
                          visible: !narrow,
                          child: navigation.desktopRail(),
                        ),
                      ),
                      Positioned(
                        top: narrow ? 0 : 16,
                        right: 12,
                        bottom: narrow ? 0 : 16,
                        width: constraints.maxWidth <= 960
                            ? constraints.maxWidth - (narrow ? 24 : 132)
                            : 420,
                        child: FloePanelPresence(
                          visible: assistantOpen,
                          child: CallbackShortcuts(
                            bindings: {
                              const SingleActivator(LogicalKeyboardKey.escape):
                                  _closeAssistant,
                            },
                            child: FocusScope(
                              autofocus: true,
                              child: _assistantPanel(_closeAssistant),
                            ),
                          ),
                        ),
                      ),
                    ],
                  ),
                ),
                Visibility(
                  key: const Key('compact-dock-slot'),
                  visible: narrow,
                  child: navigation.compactDock(),
                ),
              ],
            ),
          ),
        );
      },
    ),
  );

  Widget _page(bool narrow) {
    final padding = narrow
        ? const EdgeInsets.symmetric(horizontal: FloeSpace.md)
        : const EdgeInsets.fromLTRB(120, 24, 36, 24);
    final workspace = FloeScreenEntrance(
      identity: selectedTaskId ?? destination,
      child: _workspace(narrow),
    );
    if (destination == _DestinationView.settings) {
      return Padding(padding: padding, child: workspace);
    }
    if (destination != _DestinationView.today || selectedTaskId != null) {
      return SingleChildScrollView(
        key: PageStorageKey(
          'workspace-${destination.name}-${selectedTaskId ?? 'list'}',
        ),
        padding: padding,
        child: workspace,
      );
    }
    return Padding(padding: padding, child: workspace);
  }

  Widget _fillDay(Widget child) =>
      destination == _DestinationView.today ? Expanded(child: child) : child;

  Widget _workspace(bool narrow) {
    if (destination == _DestinationView.connections) {
      return ConnectorScreen(
        controller: widget.connectionsController,
        initialSourceRef: selectedConnectionSource,
        calendarCoverage: controller.calendarCoverage,
      );
    }
    if (destination == _DestinationView.settings) {
      return SettingsScreen(
        connectionsController: widget.connectionsController,
        operationPolicyController: operationPolicyController,
        runtime: widget.ownerGateways.runtime,
        assistantFeatureController: widget.ownerGateways.assistantFeatures,
        memoryController: widget.ownerGateways.memory,
        assistantFeatureSourceTarget: assistantFeatureSourceTarget,
        onAssistantFeatureConfigured: onAssistantFeatureConfigured,
        platform: defaultTargetPlatform,
      );
    }
    if (destination == _DestinationView.activity) {
      return ManualCalendarActivity(gateway: widget.gateway);
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
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Align(
            alignment: Alignment.centerLeft,
            child: FloeButton.text(
              onPressed: () {
                setState(() => selectedTaskId = null);
                _updateRefreshVisibility();
              },
              icon: const Icon(LucideIcons.arrowLeft, size: 18),
              child: Text(AppLocalizations.of(context).back),
            ),
          ),
          const SizedBox(height: 12),
          _TaskDetailScreen(
            task: selectedTask,
            snapshot: snapshot,
            narrow: narrow,
            disabled: controller.commandPending,
            onComplete: _setTaskCompleted,
          ),
        ],
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
              _DestinationView.today => LayoutBuilder(
                builder: (context, constraints) {
                  final status = _calendarStatusFor(snapshot);
                  final compactCalendar = constraints.maxWidth <= 800;
                  final showToolbarStatus =
                      compactTimeline &&
                      compactCalendar &&
                      status.showFreshnessAction;
                  void openDayDetails() => _openDayDetails(snapshot);
                  return Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      _DayToolbar(
                        controller,
                        narrow: narrow,
                        onCreateEvent:
                            widget.ownerGateways.runtime?.ready == true
                            ? () => _openCalendarEditor()
                            : null,
                        statusLabel: showToolbarStatus ? status.label : null,
                        onStatusPressed: showToolbarStatus
                            ? openDayDetails
                            : null,
                      ),
                      Expanded(
                        child: _content(
                          narrow,
                          snapshot,
                          status: status,
                          showToolbarStatus: showToolbarStatus,
                          onDayDetails: openDayDetails,
                        ),
                      ),
                    ],
                  );
                },
              ),
              _DestinationView.tasks => _TasksScreen(
                snapshot: snapshot,
                disabled: controller.commandPending,
                onComplete: _setTaskCompleted,
                onOpen: _openTask,
                onDelete: controller.deleteItem,
              ),
              _DestinationView.connections => SizedBox.shrink(),
              _DestinationView.activity => SizedBox.shrink(),
              _DestinationView.settings => SizedBox.shrink(),
              _DestinationView.notes => _NotesScreen(
                date: snapshot.date,
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

  _CalendarStatusPresentation _calendarStatusFor(DaySnapshot snapshot) {
    final hasEvents = snapshot.items.any((item) => item is EventItem);
    final agendaOwnsEmptyStatus =
        !hasEvents &&
        controller.loadState != DayLoadState.loading &&
        draftEventStart == null;
    final observationComplete =
        controller.hasCurrentObservation &&
        hasCompleteCalendarCoverage(snapshot, controller.query);
    final label =
        !controller.hasCurrentObservation ||
            snapshot.calendarCoverage?.sources.isNotEmpty == true
        ? AppLocalizations.of(context).dayStatusNeedsRefresh
        : AppLocalizations.of(context).dayCalendarStatusUnavailable;
    return _CalendarStatusPresentation(
      agendaOwnsEmptyStatus: agendaOwnsEmptyStatus,
      observationComplete: observationComplete,
      label: label,
    );
  }

  Widget _content(
    bool narrow,
    DaySnapshot snapshot, {
    required _CalendarStatusPresentation status,
    required bool showToolbarStatus,
    required VoidCallback onDayDetails,
  }) {
    final calendarChangesAvailable =
        widget.ownerGateways.runtime?.ready == true;
    final primary = CalendarAgenda(
      key: PageStorageKey('calendar-agenda'),
      snapshot: snapshot,
      query: controller.query,
      loading: controller.loadState == DayLoadState.loading,
      observationCurrent: controller.hasCurrentObservation,
      onConnections: () => _selectDestination(_DestinationView.connections),
      // The fixed Day toolbar is the full-screen Refresh action.
      onRefresh: null,
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
        if (!narrow)
          DayOverview(
            snapshot: snapshot,
            query: controller.query,
            loading: controller.loadState == DayLoadState.loading,
            focusOnly: true,
            observationCurrent: controller.hasCurrentObservation,
            onEdit: calendarChangesAvailable ? _editCalendarEvent : null,
            onDelete: calendarChangesAvailable ? _deleteCalendarEvent : null,
          ),
        CalendarContextRail(
          snapshot: snapshot,
          query: controller.query,
          observationCurrent: controller.hasCurrentObservation,
          hideEmptyStatus: status.agendaOwnsEmptyStatus,
          disabled: controller.commandPending,
          complete: _setTaskCompleted,
          onTasks: () => _selectDestination(_DestinationView.tasks),
          onOpenTask: _openTask,
        ),
      ],
    );
    return LayoutBuilder(
      builder: (context, constraints) {
        if (constraints.maxWidth <= 800) {
          final copy = AppLocalizations.of(context);
          return Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              ConstrainedBox(
                constraints: BoxConstraints(
                  maxHeight: constraints.maxHeight / 2,
                ),
                child: SingleChildScrollView(
                  primary: false,
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      Wrap(
                        alignment: WrapAlignment.spaceBetween,
                        crossAxisAlignment: WrapCrossAlignment.center,
                        spacing: 8,
                        children: [
                          Text(
                            compactTimeline
                                ? copy.dayTimeline
                                : copy.daySchedule,
                            style: FloeType.title,
                          ),
                          FloeButton.text(
                            onPressed: () => setState(
                              () => compactTimeline = !compactTimeline,
                            ),
                            icon: Icon(
                              compactTimeline
                                  ? LucideIcons.list
                                  : LucideIcons.calendarDays,
                              size: 16,
                            ),
                            child: Text(
                              compactTimeline
                                  ? copy.daySchedule
                                  : copy.dayTimeline,
                            ),
                          ),
                          if (compactTimeline && !showToolbarStatus)
                            FloeButton.icon(
                              key: const Key('day-details-opener'),
                              tooltip: copy.dayDetails,
                              onPressed: onDayDetails,
                              icon: const Icon(LucideIcons.ellipsis, size: 18),
                            ),
                        ],
                      ),
                    ],
                  ),
                ),
              ),
              const SizedBox(height: 8),
              Expanded(
                child: compactTimeline
                    ? LayoutBuilder(
                        builder: (context, size) => SingleChildScrollView(
                          child: SizedBox(
                            height: size.maxHeight < 420 ? 420 : size.maxHeight,
                            child: primary,
                          ),
                        ),
                      )
                    : SingleChildScrollView(
                        key: PageStorageKey(
                          'day-overview-${controller.query.date.toIso8601String()}',
                        ),
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.stretch,
                          children: [
                            CalendarCoverageDetails(
                              observationCurrent:
                                  controller.hasCurrentObservation,
                              snapshot: snapshot,
                              query: controller.query,
                            ),
                            const SizedBox(height: 16),
                            DayOverview(
                              observationCurrent:
                                  controller.hasCurrentObservation,
                              snapshot: snapshot,
                              query: controller.query,
                              loading:
                                  controller.loadState == DayLoadState.loading,
                              onEdit: calendarChangesAvailable
                                  ? _editCalendarEvent
                                  : null,
                              onDelete: calendarChangesAvailable
                                  ? _deleteCalendarEvent
                                  : null,
                            ),
                            const SizedBox(height: 16),
                            CalendarContextRail(
                              snapshot: snapshot,
                              query: controller.query,
                              showCoverage: false,
                              observationCurrent:
                                  controller.hasCurrentObservation,
                              disabled: controller.commandPending,
                              complete: _setTaskCompleted,
                              onTasks: () =>
                                  _selectDestination(_DestinationView.tasks),
                              onOpenTask: _openTask,
                            ),
                          ],
                        ),
                      ),
              ),
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
              child: SingleChildScrollView(child: rail),
            ),
          ],
        );
      },
    );
  }

  void _openDayDetails(DaySnapshot snapshot) {
    final copy = AppLocalizations.of(context);
    showFloeSheet<void>(
      context,
      (sheetContext) => SafeArea(
        child: SizedBox(
          height: MediaQuery.sizeOf(sheetContext).height * .7,
          child: FloeCard(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Row(
                  children: [
                    Expanded(
                      child: Text(copy.dayDetails, style: FloeType.title),
                    ),
                    FloeButton.icon(
                      key: const Key('day-details-close'),
                      tooltip: copy.close,
                      onPressed: () => Navigator.pop(sheetContext),
                      icon: const Icon(LucideIcons.x, size: 18),
                    ),
                  ],
                ),
                Expanded(
                  child: SingleChildScrollView(
                    child: AnimatedBuilder(
                      animation: controller,
                      builder: (context, _) => CalendarContextRail(
                        snapshot: controller.snapshot ?? snapshot,
                        query: controller.query,
                        observationCurrent: controller.hasCurrentObservation,
                        disabled: controller.commandPending,
                        complete: _setTaskCompleted,
                        onTasks: () {
                          Navigator.pop(sheetContext);
                          _selectDestination(_DestinationView.tasks);
                        },
                        onOpenTask: (task) {
                          Navigator.pop(sheetContext);
                          _openTask(task);
                        },
                      ),
                    ),
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }

  void _openTask(TaskItem task) {
    setState(() => selectedTaskId = task.id);
    _updateRefreshVisibility();
  }

  void _selectDestination(_DestinationView value, {SourceRef? sourceRef}) {
    setState(() {
      assistantOpen = false;
      selectedConnectionSource = sourceRef;
      destination = value;
      selectedTaskId = null;
      if (value != _DestinationView.settings) {
        assistantFeatureSourceTarget = null;
        onAssistantFeatureConfigured = null;
      }
    });
    _updateRefreshVisibility();
  }

  Widget _assistantPanel(VoidCallback close) {
    final agent = agentController;
    if (agent == null) {
      return FloeCard(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Row(
              children: [
                const Expanded(child: Text('Floe', style: FloeType.titleLarge)),
                FloeButton.icon(
                  tooltip: 'Close',
                  onPressed: close,
                  icon: const Icon(LucideIcons.x),
                ),
              ],
            ),
            const Spacer(),
            Text(
              AppLocalizations.of(context).conversationUnavailable,
              style: FloeType.body,
            ),
            const SizedBox(height: 16),
            FloeButton.outlined(
              onPressed: () {
                close();
                _selectDestination(_DestinationView.settings);
              },
              icon: const Icon(LucideIcons.settings, size: 18),
              child: Text(AppLocalizations.of(context).settings),
            ),
            const Spacer(),
          ],
        ),
      );
    }
    return AgentPanel(
      controller: agent,
      composerDraft: conversationComposerDraft,
      dayGateway: widget.gateway,
      onOpenVoice: _openVoice,
      onOpenConnections: () => _openAgentSourceReview(null),
      onOpenSourceReview: _openAgentSourceReview,
      onOpenAssistantFeatureSettings: _openAssistantFeatureSettings,
      onClose: close,
    );
  }

  Future<void> _openVoice() async {
    await showFloeDialog<void>(
      context,
      (context) => Dialog(
        insetPadding: const EdgeInsets.all(16),
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 560, maxHeight: 640),
          child: VoiceScreen(
            ownerGateways: widget.ownerGateways,
            agentGateway: widget.agentGateway,
            personId: widget.query.personId,
            onClose: () => Navigator.of(context).pop(),
          ),
        ),
      ),
      barrierDismissible: false,
    );
  }

  Future<void> _openAssistant() async {
    if (assistantOpen) {
      _closeAssistant();
      return;
    }
    if (MediaQuery.sizeOf(context).width > 960) {
      agentController?.attachView();
      setState(() => assistantOpen = true);
      return;
    }
    await showFloeSheet<void>(
      context,
      (context) => SizedBox(
        height: MediaQuery.sizeOf(context).height * .88,
        child: _assistantPanel(() => Navigator.pop(context)),
      ),
    );
    if (mounted) assistantEntryFocus.requestFocus();
  }

  void _openAgentSourceReview(AgentInteractionTarget? target) {
    if (MediaQuery.sizeOf(context).width <= 960) {
      Navigator.of(context).maybePop();
    }
    _selectDestination(
      _DestinationView.connections,
      sourceRef: switch (target) {
        AgentSourceReviewTarget(:final review) => review.sourceRef,
        AgentNavigationTarget(:final sourceRef) => sourceRef,
        _ => null,
      },
    );
  }

  void _openAssistantFeatureSettings(
    AgentAssistantFeatureSourceTarget target,
    Future<void> Function() onConfigured,
  ) {
    if (MediaQuery.sizeOf(context).width <= 960) {
      Navigator.of(context).maybePop();
    }
    setState(() {
      assistantFeatureSourceTarget = target;
      onAssistantFeatureConfigured = () async {
        await onConfigured();
        if (!mounted) return;
        final current = assistantFeatureSourceTarget;
        if (current?.review.reviewRef.matches(target.review.reviewRef) !=
            true) {
          return;
        }
        setState(() {
          assistantFeatureSourceTarget = null;
          onAssistantFeatureConfigured = null;
        });
      };
    });
    _selectDestination(_DestinationView.settings);
  }

  void _closeAssistant() {
    agentController?.detachView();
    setState(() => assistantOpen = false);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) {
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
    final date = controller.query.date;
    final now = DateTime.now();
    final createStart =
        initialStart ??
        (DateUtils.isSameDay(date, now)
            ? DateTime(now.year, now.month, now.day, now.hour + 1)
            : DateTime(date.year, date.month, date.day, 9));
    if (event == null) setState(() => draftEventStart = createStart);
    final saved = await showFloeDialog<ManualCalendarOperationReceipt>(
      context,
      (_) => CalendarEventComposer(
        gateway: widget.gateway,
        initialStart: event == null ? createStart : initialStart,
        event: event,
      ),
    );
    if (mounted && saved != null) {
      _showManualCalendarOperationOutcome(
        saved,
        success: event == null ? 'Event created' : 'Event saved',
      );
      if (saved.status == ManualCalendarOperationStatus.succeeded) {
        await controller.load();
      }
    }
    if (event == null && mounted && draftEventStart == createStart) {
      setState(() => draftEventStart = null);
    }
  }

  Future<void> _editCalendarEvent(EventItem event) async {
    if (event.actionTarget == null) return;
    await _showCalendarComposer(event: event);
  }

  Future<void> _moveCalendarEvent(EventItem event, DateTime start) async {
    final target = event.actionTarget;
    if (target == null) return;
    try {
      final result = await widget.gateway.executeExternalCalendarOperation(
        UpdateManualCalendarEvent(
          eventRef: target.eventId,
          expectedRevision: target.expectedRevision,
          title: event.title,
          startsAt: start.toUtc(),
          endsAt: start.toUtc().add(event.endsAt.difference(event.startsAt)),
          timezone:
              event.timezone ?? calendarStorageTimezone(start.timeZoneOffset),
        ),
      );
      _showManualCalendarOperationOutcome(result, success: 'Event moved');
      if (mounted && result.status == ManualCalendarOperationStatus.succeeded) {
        await controller.load();
      }
    } on Object {
      _showManualCalendarOperationOutcome(null, success: 'Event moved');
    }
  }

  Future<void> _deleteCalendarEvent(EventItem event) async {
    if (event.actionTarget == null) return;
    final confirmed = await showFloeDialog<bool>(
      context,
      (dialogContext) => FloeDetailDialog(
        title: 'Delete event?',
        children: [
          Text(
            '“${event.title}” will be removed from ${event.calendarLabel ?? 'its Calendar source'}.',
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
      await _deleteCalendarEventAtOwner(event);
    }
  }

  Future<void> _deleteCalendarEventAtOwner(EventItem event) async {
    final target = event.actionTarget;
    if (target == null) return;
    try {
      final result = await widget.gateway.executeExternalCalendarOperation(
        DeleteManualCalendarEvent(
          eventRef: target.eventId,
          expectedRevision: target.expectedRevision,
        ),
      );
      _showManualCalendarOperationOutcome(result, success: 'Event deleted');
    } on Object {
      _showManualCalendarOperationOutcome(null, success: 'Event deleted');
    }
  }

  void _showManualCalendarOperationOutcome(
    ManualCalendarOperationReceipt? operation, {
    required String success,
  }) {
    if (!mounted) return;
    final title = switch (operation?.status) {
      ManualCalendarOperationStatus.succeeded =>
        operation!.collectionPending
            ? '$success. Day update is pending.'
            : success,
      ManualCalendarOperationStatus.pending =>
        '$success request accepted. Check Activity for updates.',
      ManualCalendarOperationStatus.executing =>
        'Calendar change is in progress. Check Activity for updates.',
      ManualCalendarOperationStatus.blocked =>
        'Calendar change was blocked before dispatch.',
      ManualCalendarOperationStatus.notApplied =>
        'Calendar change was not applied.',
      ManualCalendarOperationStatus.unknown => 'Calendar outcome is unknown. Reconcile it in Activity before retrying.',
      null =>
        'Calendar outcome was not confirmed. Check Activity before retrying.',
    };
    FloeToastHost.of(context).show(title: title);
  }

  Future<void> _setTaskCompleted(TaskItem task, bool completed) async {
    final acknowledged = await controller.setTaskCompleted(task, completed);
    if (!mounted || acknowledged == null) return;
    final changedTask = _taskById(acknowledged, task.id);
    FloeToastHost.of(context).show(
      title: completed
          ? AppLocalizations.of(context).taskCompleted
          : AppLocalizations.of(context).taskMarkedIncomplete,
      actionLabel: changedTask == null
          ? null
          : AppLocalizations.of(context).undo,
      onAction: changedTask == null
          ? null
          : () {
              if (mounted) controller.setTaskCompleted(changedTask, !completed);
            },
    );
  }

  Future<bool> _createNote(String content) async {
    if (controller.commandPending) return false;
    // Retain an acknowledged capture when classification acknowledgement is
    // lost, so Save retries classification instead of creating another capture.
    if (controller.pendingCapture?.originalInput != content &&
        !await controller.submitCapture(content))
      return false;
    final saved = await controller.classify(NoteDraft(content: content));
    if (saved && mounted) {
      final copy = AppLocalizations.of(context);
      final outsideSelectedDay = !DateUtils.isSameDay(
        controller.query.date,
        controller.query.now.toLocal(),
      );
      FloeToastHost.of(context).show(
        title: outsideSelectedDay ? copy.noteSavedToToday : copy.savedInFloe,
        actionLabel: outsideSelectedDay ? copy.viewNotes : null,
        onAction: outsideSelectedDay
            ? () {
                if (mounted) controller.goToday();
              }
            : null,
      );
    }
    return saved;
  }
}
