part of '../settings_screen.dart';

class _DataPrivacy extends StatefulWidget {
  const _DataPrivacy({
    required this.controller,
    required this.serverClient,
    required this.onManageMemory,
    this.androidContext,
    this.calendarSources,
    this.calendarSourceChanges,
  });

  final AgentController controller;
  final LocalServerClient? serverClient;
  final VoidCallback onManageMemory;
  final AndroidContextApi? androidContext;
  final AgentCalendarSources? Function()? calendarSources;
  final Listenable? calendarSourceChanges;

  @override
  State<_DataPrivacy> createState() => _DataPrivacyState();
}

class _DataPrivacyState extends State<_DataPrivacy> {
  bool loading = false;
  bool registryRequested = false;
  bool calendarRequested = false;
  bool memoryRequested = false;
  bool savedMemoryRequested = false;
  bool connectionsRequested = false;
  bool serverConnectionsRequested = false;
  bool androidConnectionsRequested = false;
  bool androidHealthBusy = false;
  bool androidCalendarBusy = false;
  List<AgentConnection>? serverConnections;
  List<AgentConnection>? androidConnections;
  List<AndroidCalendarOption>? androidCalendars;
  Set<String> androidSelectedCalendars = {};
  Object? serverConnectionFailure;
  Object? androidConnectionFailure;
  Object? androidCalendarFailure;

  AgentController get controller => widget.controller;

  @override
  void initState() {
    super.initState();
    controller.addListener(_controllerChanged);
    WidgetsBinding.instance.addPostFrameCallback((_) => _load());
  }

  @override
  void didUpdateWidget(_DataPrivacy oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.controller != controller) {
      oldWidget.controller.removeListener(_controllerChanged);
      controller.addListener(_controllerChanged);
      registryRequested = false;
      calendarRequested = false;
      memoryRequested = false;
      savedMemoryRequested = false;
      connectionsRequested = false;
    }
    if (oldWidget.serverClient != widget.serverClient) {
      serverConnectionsRequested = false;
      serverConnections = null;
      serverConnectionFailure = null;
    }
    if (oldWidget.androidContext != widget.androidContext) {
      androidConnectionsRequested = false;
      androidConnections = null;
      androidConnectionFailure = null;
      androidCalendars = null;
      androidSelectedCalendars = {};
      androidCalendarFailure = null;
    }
    _load();
  }

  void _controllerChanged() {
    if (!controller.busy) _load();
  }

  Future<void> _load() async {
    if (loading ||
        !controller.canManageRegistry &&
            !controller.canReviewMemory &&
            !controller.canReadMemory &&
            !controller.canReadConnections &&
            widget.serverClient == null &&
            widget.androidContext == null) {
      return;
    }
    loading = true;
    if (controller.canManageRegistry && !registryRequested) {
      registryRequested = true;
      await controller.loadRegistry();
    }
    if (controller.hasCalendarExpertManagement &&
        !calendarRequested &&
        controller.canManageCalendarExperts) {
      calendarRequested = true;
      await controller.loadCalendarExperts();
    }
    if (controller.hasMemoryReview &&
        !memoryRequested &&
        controller.canReviewMemory) {
      memoryRequested = true;
      await controller.loadMemoryReview();
    }
    if (controller.hasMemory &&
        !savedMemoryRequested &&
        controller.canReadMemory) {
      savedMemoryRequested = true;
      await controller.loadMemory();
    }
    if (controller.hasConnections &&
        !connectionsRequested &&
        controller.canReadConnections) {
      connectionsRequested = true;
      await controller.loadConnections();
    }
    if (!serverConnectionsRequested && widget.serverClient != null) {
      serverConnectionsRequested = true;
      try {
        final connection = await widget.serverClient!.connection();
        final values = connection == null
            ? const <Map<String, dynamic>>[]
            : await widget.serverClient!.connections(connection);
        serverConnections = List.unmodifiable(
          values.map(AgentConnection.fromJson),
        );
        serverConnectionFailure = null;
      } on Object catch (error) {
        serverConnections = const [];
        serverConnectionFailure = error;
      }
      if (mounted) setState(() {});
    }
    if (!androidConnectionsRequested && widget.androidContext != null) {
      androidConnectionsRequested = true;
      try {
        final values = await widget.androidContext!.connections();
        androidConnections = List.unmodifiable(
          values.map(AgentConnection.fromJson),
        );
        androidConnectionFailure = null;
        await _loadAndroidCalendarConfiguration(androidConnections!);
      } on Object catch (error) {
        androidConnections = const [];
        androidConnectionFailure = error;
      }
      if (mounted) setState(() {});
    }
    loading = false;
  }

  Future<void> _refreshConnections() async {
    connectionsRequested = false;
    serverConnectionsRequested = false;
    serverConnectionFailure = null;
    androidConnectionsRequested = false;
    androidConnectionFailure = null;
    androidCalendarFailure = null;
    if (widget.serverClient != null) serverConnections = null;
    if (widget.androidContext != null) androidConnections = null;
    await _load();
  }

  Future<void> _loadAndroidCalendarConfiguration(
    List<AgentConnection> connections,
  ) async {
    final gateway = widget.androidContext;
    AgentConnection? calendar;
    for (final connection in connections) {
      if (connection.descriptor.provider == 'android_calendar') {
        calendar = connection;
        break;
      }
    }
    if (gateway == null ||
        calendar == null ||
        calendar.state == AgentConnectionState.revoked ||
        calendar.state == AgentConnectionState.unsupported) {
      androidCalendars = null;
      androidSelectedCalendars = {};
      return;
    }
    try {
      final calendars = await gateway.listCalendars();
      final selected = await gateway.selectedCalendars();
      androidCalendars = List.unmodifiable(calendars);
      androidSelectedCalendars = selected.toSet();
      androidCalendarFailure = null;
    } on Object catch (error) {
      androidCalendars = const [];
      androidSelectedCalendars = {};
      androidCalendarFailure = error;
    }
  }

  Future<void> _allowAndroidCalendar(AgentConnection connection) async {
    final gateway = widget.androidContext;
    if (gateway == null || androidCalendarBusy) return;
    setState(() {
      androidCalendarBusy = true;
      androidCalendarFailure = null;
    });
    try {
      if (connection.state == AgentConnectionState.revoked) {
        final granted = await gateway.requestPermission(
          AndroidContextSource.calendar,
        );
        if (!granted) {
          throw StateError('Android Calendar access was not granted.');
        }
      }
      androidConnectionsRequested = false;
      await _load();
    } on Object catch (error) {
      androidCalendarFailure = error;
    } finally {
      if (mounted) setState(() => androidCalendarBusy = false);
    }
  }

  Future<void> _toggleAndroidCalendar(AndroidCalendarOption calendar) async {
    final gateway = widget.androidContext;
    if (gateway == null || androidCalendarBusy) return;
    final selected = {...androidSelectedCalendars};
    if (!selected.remove(calendar.id)) {
      if (selected.length == 4) return;
      selected.add(calendar.id);
    }
    setState(() {
      androidCalendarBusy = true;
      androidCalendarFailure = null;
    });
    try {
      androidSelectedCalendars = (await gateway.setSelectedCalendars(
        selected.toList(),
      )).toSet();
      if (androidSelectedCalendars.isNotEmpty) {
        final now = DateTime.now().toUtc();
        await gateway.readCalendar(
          rangeStart: now,
          rangeEnd: now.add(const Duration(days: 14)),
        );
      }
      androidConnectionsRequested = false;
      await _load();
    } on Object catch (error) {
      androidCalendarFailure = error;
    } finally {
      if (mounted) setState(() => androidCalendarBusy = false);
    }
  }

  Future<void> _refreshAndroidWellbeing(AgentConnection connection) async {
    final gateway = widget.androidContext;
    if (gateway == null || androidHealthBusy) return;
    setState(() {
      androidHealthBusy = true;
      androidConnectionFailure = null;
    });
    try {
      if (connection.state == AgentConnectionState.revoked) {
        final granted = await gateway.requestPermission(
          AndroidContextSource.health,
        );
        if (!granted) {
          throw StateError('Health Connect access was not granted.');
        }
      }
      await gateway.readWellbeing();
    } on Object catch (error) {
      androidConnectionFailure = error;
    } finally {
      androidConnectionsRequested = false;
      try {
        await _load();
      } finally {
        if (mounted) setState(() => androidHealthBusy = false);
      }
    }
  }

  @override
  void dispose() {
    controller.removeListener(_controllerChanged);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: controller,
    builder: (context, _) {
      final localConnections = controller.hasConnections
          ? controller.connections
          : const <AgentConnection>[];
      final remoteConnections = widget.serverClient == null
          ? const <AgentConnection>[]
          : serverConnections;
      final deviceConnections = widget.androidContext == null
          ? const <AgentConnection>[]
          : androidConnections;
      final connections = [
        ...?localConnections,
        ...?deviceConnections,
        ...?remoteConnections,
      ];
      final connectionLoading =
          controller.hasConnections && localConnections == null ||
          widget.androidContext != null && deviceConnections == null ||
          widget.serverClient != null && remoteConnections == null;
      AgentConnection? healthConnection;
      AgentConnection? androidCalendarConnection;
      for (final connection in connections) {
        if (connection.descriptor.provider == 'health_connect') {
          healthConnection = connection;
          break;
        }
      }
      for (final connection in connections) {
        if (connection.descriptor.provider == 'android_calendar') {
          androidCalendarConnection = connection;
          break;
        }
      }
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(
            'Data & privacy',
            style: FloeType.headlineLarge.copyWith(fontSize: 22),
          ),
          const SizedBox(height: FloeSpace.sm),
          Text(
            'Control what Floe may use and where assisted processing may happen.',
            style: FloeType.body.copyWith(color: FloePalette.neutral600),
          ),
          const SizedBox(height: FloeSpace.lg),
          FloeSquircle(
            padding: const EdgeInsets.all(FloeSpace.lg),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                if (controller.hasConnections ||
                    widget.serverClient != null ||
                    widget.androidContext != null)
                  AgentConnectionSettings(
                    connections: connections,
                    loading: connectionLoading,
                    failed:
                        controller.connectionFailure != null ||
                        serverConnectionFailure != null ||
                        androidConnectionFailure != null ||
                        androidCalendarFailure != null,
                    onRefresh: _refreshConnections,
                  ),
                if (androidCalendarConnection != null) ...[
                  const SizedBox(height: FloeSpace.base),
                  Text(
                    'Choose up to four Android calendars. Selection stays on this device and reads never change Calendar.',
                    style: FloeType.bodySmall.copyWith(
                      color: FloePalette.neutral600,
                    ),
                  ),
                  const SizedBox(height: FloeSpace.xs),
                  if (androidCalendarConnection.state ==
                      AgentConnectionState.revoked)
                    Align(
                      alignment: Alignment.centerLeft,
                      child: FloeButton.outlined(
                        key: const ValueKey('android-calendar-allow'),
                        onPressed: () =>
                            _allowAndroidCalendar(androidCalendarConnection!),
                        loading: androidCalendarBusy,
                        child: const Text('Allow Android Calendar'),
                      ),
                    )
                  else if (androidCalendars case final calendars?)
                    if (calendars.isEmpty)
                      const Text('No readable Android calendars are available.')
                    else
                      Wrap(
                        spacing: FloeSpace.xs,
                        runSpacing: FloeSpace.xs,
                        children: [
                          for (final calendar in calendars)
                            if (androidSelectedCalendars.contains(calendar.id))
                              FloeButton.filled(
                                key: ValueKey(
                                  'android-calendar-${calendar.id}',
                                ),
                                size: FloeButtonSize.compact,
                                onPressed: androidCalendarBusy
                                    ? null
                                    : () => _toggleAndroidCalendar(calendar),
                                child: Text(calendar.displayName),
                              )
                            else
                              FloeButton.outlined(
                                key: ValueKey(
                                  'android-calendar-${calendar.id}',
                                ),
                                size: FloeButtonSize.compact,
                                onPressed:
                                    androidCalendarBusy ||
                                        androidSelectedCalendars.length == 4
                                    ? null
                                    : () => _toggleAndroidCalendar(calendar),
                                child: Text(calendar.displayName),
                              ),
                        ],
                      ),
                ],
                if (healthConnection != null) ...[
                  const SizedBox(height: FloeSpace.sm),
                  Text(
                    'Health records stay on this device. Floe receives only a short-lived capacity and recovery summary.',
                    style: FloeType.bodySmall.copyWith(
                      color: FloePalette.neutral600,
                    ),
                  ),
                  const SizedBox(height: FloeSpace.xs),
                  Align(
                    alignment: Alignment.centerLeft,
                    child: FloeButton.outlined(
                      key: const ValueKey('android-health-refresh'),
                      onPressed:
                          healthConnection.state ==
                              AgentConnectionState.unsupported
                          ? null
                          : () => _refreshAndroidWellbeing(healthConnection!),
                      loading: androidHealthBusy,
                      child: Text(
                        healthConnection.state == AgentConnectionState.revoked
                            ? 'Allow Health Connect'
                            : 'Refresh wellbeing',
                      ),
                    ),
                  ),
                ],
                if ((controller.hasConnections ||
                        widget.serverClient != null ||
                        widget.androidContext != null) &&
                    controller.hasCalendarExpertManagement)
                  const Padding(
                    padding: EdgeInsets.symmetric(vertical: FloeSpace.lg),
                    child: FloeDivider(height: 1),
                  ),
                if (controller.hasCalendarExpertManagement)
                  AgentCalendarSettings(
                    controller: controller,
                    sources: widget.calendarSources,
                    sourceChanges: widget.calendarSourceChanges,
                  ),
                if (!controller.hasConnections &&
                    widget.serverClient == null &&
                    widget.androidContext == null &&
                    !controller.hasCalendarExpertManagement)
                  const Text('No connected data sources are available yet.'),
              ],
            ),
          ),
          if (controller.hasMemory) ...[
            const SizedBox(height: FloeSpace.lg),
            AgentMemorySettingsCard(
              controller: controller,
              onManage: widget.onManageMemory,
            ),
          ],
          const SizedBox(height: FloeSpace.lg),
          _AiProcessing(client: widget.serverClient),
          if (controller.vaultState != AgentVaultState.ready) ...[
            const SizedBox(height: FloeSpace.sm),
            Text(
              'Data access will appear when your private data is unlocked.',
              style: FloeType.body.copyWith(
                color: FloePalette.neutral600,
                height: 1.4,
              ),
            ),
          ],
        ],
      );
    },
  );
}
