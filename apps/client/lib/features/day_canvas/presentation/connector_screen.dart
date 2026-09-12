import 'dart:async';

import 'package:floe_client/l10n/app_localizations.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../../app/design_tokens.dart';
import '../../../app/floe_badge.dart';
import '../../../app/floe_button.dart';
import '../../../app/floe_feedback.dart';
import '../../../app/floe_input.dart';
import '../../../app/floe_selection.dart';
import '../../../app/floe_primitives.dart';
import '../../../app/floe_squircle.dart';
import '../../server/local_server_client.dart';
import '../application/calendar_gateway.dart';
import '../domain/day_models.dart';
import 'calendar_panel.dart';
import 'connector_status_presentation.dart';
import 'server_connector_panel.dart';
import '../../agent/agent_calendar_expert_dialog.dart';
import '../../agent/agent_calendar_sources.dart';
import '../../agent/agent_controller.dart';
import '../../agent/agent_vault_gateway.dart';
import '../../agent/agent_connections.dart';
import '../../agent/agent_personal_access.dart';
import '../../agent/agent_personal_access_settings.dart';
import '../../../infrastructure/native/apple_context_gateway.dart';
import '../../../infrastructure/native/macos_context_gateway.dart';

class ConnectorScreen extends StatefulWidget {
  const ConnectorScreen({
    super.key,
    required this.gateway,
    required this.query,
    required this.connection,
    required this.onChanged,
    this.serverClient,
    this.deviceId,
    this.platform,
    this.agentController,
    this.calendarSources,
    this.calendarSourceChanges,
    this.agentVaultGateway,
    this.appleContext,
    this.macOSContext,
    this.daySnapshot,
    this.initialDeviceCalendarDetail = false,
  });

  final CalendarGateway? gateway;
  final DayQuery query;
  final CalendarConnection? connection;
  final Future<void> Function() onChanged;
  final LocalServerClient? serverClient;
  final String? deviceId;
  final TargetPlatform? platform;
  final AgentController? agentController;
  final AgentCalendarSources? Function()? calendarSources;
  final Listenable? calendarSourceChanges;
  final NativeAgentVaultGateway? agentVaultGateway;
  final AppleContextApi? appleContext;
  final MacOSContextApi? macOSContext;
  final DaySnapshot? daySnapshot;
  final bool initialDeviceCalendarDetail;

  @override
  State<ConnectorScreen> createState() => _ConnectorScreenState();
}

class _ConnectorScreenState extends State<ConnectorScreen> {
  bool deviceCalendarDetail = false;
  String? selectedServerConnectorId;
  ServerConnection? serverConnection;
  ServerConnectorCatalog? catalog;
  String? catalogError;
  bool loadingCatalog = false;
  bool loadingLocalConnections = false;
  List<AgentConnection>? localConnections;
  String? localConnectionFailure;
  String? activatingCalendarConnectorId;
  String? calendarSelectionError;
  String? selectedLocalConnectionId;

  bool get supportsDeviceCalendar =>
      effectivePlatform == TargetPlatform.iOS ||
      effectivePlatform == TargetPlatform.macOS ||
      effectivePlatform == TargetPlatform.android;

  String get deviceCalendarProvider =>
      effectivePlatform == TargetPlatform.android ? 'android' : 'event_kit';

  CalendarConnection? get deviceCalendarConnection {
    final connection = widget.connection;
    return connection?.provider == deviceCalendarProvider ? connection : null;
  }

  ServerConnectorStatus get deviceCalendarStatus {
    if (widget.gateway == null) return ServerConnectorStatus.unavailable;
    final connection = deviceCalendarConnection;
    if (connection?.error != null) return ServerConnectorStatus.error;
    return connection == null
        ? ServerConnectorStatus.available
        : ServerConnectorStatus.connected;
  }

  TargetPlatform get effectivePlatform =>
      widget.platform ?? defaultTargetPlatform;

  @override
  void initState() {
    super.initState();
    deviceCalendarDetail = widget.initialDeviceCalendarDetail;
    unawaited(_loadCatalog());
    unawaited(_loadLocalConnections());
  }

  @override
  void didUpdateWidget(ConnectorScreen oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.serverClient != widget.serverClient) {
      unawaited(_loadCatalog());
    }
    if (oldWidget.agentController != widget.agentController ||
        oldWidget.appleContext != widget.appleContext ||
        oldWidget.macOSContext != widget.macOSContext ||
        oldWidget.deviceId != widget.deviceId) {
      unawaited(_loadLocalConnections());
    }
    if (oldWidget.initialDeviceCalendarDetail !=
        widget.initialDeviceCalendarDetail) {
      deviceCalendarDetail = widget.initialDeviceCalendarDetail;
    }
  }

  Future<void> _loadLocalConnections() async {
    final controller = widget.agentController;
    final apple = widget.appleContext;
    if (controller == null && apple == null) {
      if (mounted) setState(() => localConnections = const []);
      return;
    }
    if (mounted) setState(() => loadingLocalConnections = true);
    try {
      if (controller != null) await controller.loadConnections();
      final values = <AgentConnection>[];
      if (effectivePlatform == TargetPlatform.macOS &&
          controller?.connections != null &&
          widget.macOSContext == null) {
        values.addAll(
          controller!.connections!.where(
            (connection) => connection.descriptor.provider == 'attention.macos',
          ),
        );
      }
      final macOS = widget.macOSContext;
      if (macOS != null) {
        final deviceId = widget.deviceId ?? 'local-macos';
        final view = await macOS.readAttention();
        values.add(_macOSAttentionConnection(deviceId, view));
      }
      if (apple != null) {
        values.addAll(
          (await apple.connections()).map(AgentConnection.fromJson),
        );
      }
      if (!mounted) return;
      setState(() {
        localConnections = List.unmodifiable(values);
        localConnectionFailure = null;
      });
    } on Object catch (error) {
      if (!mounted) return;
      setState(() {
        localConnections = const [];
        localConnectionFailure = error.toString();
      });
    } finally {
      if (mounted) setState(() => loadingLocalConnections = false);
    }
  }

  Future<void> _loadCatalog() async {
    final client = widget.serverClient;
    if (client == null) {
      if (mounted) {
        setState(() {
          serverConnection = null;
          catalog = null;
          catalogError = null;
          loadingCatalog = false;
        });
      }
      return;
    }
    if (mounted) {
      setState(() {
        loadingCatalog = true;
        catalogError = null;
      });
    }
    try {
      final connection = await client.connection();
      final nextCatalog = connection == null
          ? null
          : await client.connectorCatalog(connection);
      if (connection != null && nextCatalog != null) {
        await _synchronizeServerCalendar(connection, nextCatalog);
      }
      if (!mounted) return;
      setState(() {
        serverConnection = connection;
        catalog = nextCatalog;
        if (connection == null) catalogError = 'pair_required';
      });
    } on ServerConnectionException catch (error) {
      if (!mounted) return;
      setState(() => catalogError = error.code);
    } on Object {
      if (!mounted) return;
      setState(() => catalogError = 'server_unavailable');
    } finally {
      if (mounted) setState(() => loadingCatalog = false);
    }
  }

  Future<void> _synchronizeServerCalendar(
    ServerConnection server,
    ServerConnectorCatalog nextCatalog,
  ) async {
    final gateway = widget.gateway;
    if (gateway == null) return;
    final connected = nextCatalog.connectors
        .where(
          (connector) =>
              connector.status == ServerConnectorStatus.connected &&
              const {
                'calendar.google',
                'calendar.microsoft',
              }.contains(connector.id),
        )
        .toList(growable: false);
    ServerConnector? selected;
    final current = widget.connection;
    final currentProvider = current?.provider;
    for (final connector in connected) {
      if (_calendarProvider(connector.id) == currentProvider &&
          connector.connectionId == current?.connectionId) {
        selected = connector;
        break;
      }
    }
    if (selected == null && current == null && connected.length == 1) {
      selected = connected.single;
    }
    if (selected == null) {
      if (const {
        'google_calendar',
        'microsoft_calendar',
      }.contains(currentProvider)) {
        await gateway.disconnectCalendar(widget.query);
        await widget.onChanged();
      }
      return;
    }
    if (await _bindServerCalendar(gateway, server, selected)) {
      await widget.onChanged();
    }
  }

  Future<bool> _bindServerCalendar(
    CalendarGateway gateway,
    ServerConnection server,
    ServerConnector selected,
  ) async {
    final calendarId = selected.scope['calendar_id'];
    final connectionId = selected.connectionId;
    final revision = selected.connectionRevision;
    if (calendarId is! String ||
        calendarId.isEmpty ||
        connectionId == null ||
        revision == null) {
      throw const ServerConnectionException('invalid_response');
    }
    final provider = _calendarProvider(selected.id);
    final current = widget.connection;
    if (current?.connectionId == connectionId &&
        current?.revision == revision &&
        current?.deviceId == server.deviceId &&
        current?.provider == provider &&
        current?.selectedCalendarIds.length == 1 &&
        current?.selectedCalendarIds.single == calendarId) {
      return false;
    }
    await gateway.bindCalendarConnection(
      connectionId: connectionId,
      connectionRevision: revision,
      deviceId: server.deviceId,
      provider: provider,
      calendars: [
        CalendarChoice(calendarId, selected.name, provider: provider),
      ],
      query: widget.query,
    );
    return true;
  }

  ServerConnector? get selectedServerConnector {
    final selected = selectedServerConnectorId;
    if (selected == null) return null;
    for (final connector in catalog?.connectors ?? const <ServerConnector>[]) {
      if (connector.id == selected) return connector;
    }
    return null;
  }

  List<ServerConnector> get connectedServerCalendars =>
      (catalog?.connectors ?? const <ServerConnector>[])
          .where(
            (connector) =>
                connector.status == ServerConnectorStatus.connected &&
                const {
                  'calendar.google',
                  'calendar.microsoft',
                }.contains(connector.id),
          )
          .toList(growable: false);

  bool _isActiveServerCalendar(ServerConnector connector) {
    final current = widget.connection;
    return current != null &&
        current.provider == _calendarProvider(connector.id) &&
        current.connectionId == connector.connectionId;
  }

  Future<void> _activateServerCalendar(ServerConnector connector) async {
    final gateway = widget.gateway;
    final server = serverConnection;
    if (gateway == null ||
        server == null ||
        activatingCalendarConnectorId != null) {
      return;
    }
    setState(() {
      activatingCalendarConnectorId = connector.id;
      calendarSelectionError = null;
    });
    try {
      if (await _bindServerCalendar(gateway, server, connector)) {
        await widget.onChanged();
      }
    } on Object {
      if (mounted) {
        setState(() {
          calendarSelectionError = 'This calendar could not be selected. Refresh Connections and try again.';
        });
      }
    } finally {
      if (mounted) setState(() => activatingCalendarConnectorId = null);
    }
  }

  @override
  Widget build(BuildContext context) {
    if (deviceCalendarDetail) return _deviceCalendarDetail(context);
    final localConnection = _selectedLocalConnection;
    if (localConnection != null) {
      return _localConnectionDetail(context, localConnection);
    }
    final serverConnector = selectedServerConnector;
    if (serverConnector != null && serverConnection != null) {
      return ServerConnectorPanel(
        connector: serverConnector,
        connection: serverConnection!,
        client: widget.serverClient!,
        agentVaultGateway: widget.agentVaultGateway,
        onBack: () => setState(() => selectedServerConnectorId = null),
        onChanged: _loadCatalog,
      );
    }
    return _catalog(context);
  }

  AgentConnection? get _selectedLocalConnection {
    final id = selectedLocalConnectionId;
    if (id == null) return null;
    for (final connection in localConnections ?? const <AgentConnection>[]) {
      if (connection.descriptor.id == id) return connection;
    }
    return null;
  }

  Widget _localConnectionDetail(
    BuildContext context,
    AgentConnection connection,
  ) => _AppleConnectionDetail(
    connection: connection,
    personId: widget.query.personId,
    deviceId: connection.descriptor.execution.deviceId ?? widget.deviceId ?? '',
    agentVaultGateway: widget.agentVaultGateway,
    appleContext: widget.appleContext,
    macOSContext: widget.macOSContext,
    daySnapshot: widget.daySnapshot,
    onBack: () => setState(() => selectedLocalConnectionId = null),
    onChanged: _loadLocalConnections,
  );

  Widget _catalog(BuildContext context) {
    final strings = AppLocalizations.of(context);
    final serverConnectors = catalog?.connectors ?? const <ServerConnector>[];
    final nativeConnections = localConnections ?? const <AgentConnection>[];
    final connectedCount =
        serverConnectors
            .where((item) => item.status == ServerConnectorStatus.connected)
            .length +
        (supportsDeviceCalendar &&
                deviceCalendarStatus == ServerConnectorStatus.connected
            ? 1
            : 0) +
        nativeConnections
            .where(
              (connection) =>
                  connection.state != AgentConnectionState.unsupported,
            )
            .length;
    final cards = <({ServerConnectorStatus status, Widget card})>[
      if (supportsDeviceCalendar)
        (
          status: deviceCalendarStatus,
          card: _ConnectorCard(
            key: Key(
              effectivePlatform == TargetPlatform.android
                  ? 'connector-calendar-android'
                  : 'connector-calendar-apple',
            ),
            icon: LucideIcons.calendarDays,
            name: _deviceCalendarName(strings, effectivePlatform),
            description: _deviceCalendarDescription(strings, effectivePlatform),
            status: deviceCalendarStatus,
            onPressed: () => setState(() => deviceCalendarDetail = true),
          ),
        ),
      for (final connector in serverConnectors)
        (
          status: connector.status,
          card: _ConnectorCard(
            key: Key('connector-${connector.id}'),
            icon: _connectorIcon(connector.id),
            name: connector.name,
            description: _connectorDescription(connector),
            status: connector.status,
            onPressed: () =>
                setState(() => selectedServerConnectorId = connector.id),
          ),
        ),
      for (final connection in nativeConnections)
        (
          status: _serverStatus(connection.state),
          card: _ConnectorCard(
            key: Key('connector-${connection.descriptor.id}'),
            icon: _nativeConnectionIcon(connection.descriptor.provider),
            name: _nativeConnectionName(connection.descriptor.provider),
            description: _nativeConnectionDescription(
              connection.descriptor.provider,
            ),
            status: _serverStatus(connection.state),
            onPressed: () => setState(
              () => selectedLocalConnectionId = connection.descriptor.id,
            ),
          ),
        ),
    ];
    final availableCards = cards
        .where((item) => item.status != ServerConnectorStatus.unavailable)
        .map((item) => item.card)
        .toList(growable: false);
    final unavailableCards = cards
        .where((item) => item.status == ServerConnectorStatus.unavailable)
        .map((item) => item.card)
        .toList(growable: false);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(strings.connections, style: FloeType.pageTitle),
        SizedBox(height: FloeSpace.md),
        Text(
          strings.manageTheServicesThatBringContextTo,
          style: FloeType.body.copyWith(color: FloePalette.neutral600),
        ),
        SizedBox(height: 36),
        Text(
          connectedCount == 0
              ? strings.availableServices
              : strings.connectedServicesCount(connectedCount),
          style: FloeType.title,
        ),
        if (catalogError != null) ...[
          SizedBox(height: FloeSpace.base),
          FloeInfoNote(
            text: catalogError == 'pair_required'
                ? 'Pair this device with Floe server in Settings to connect server services.'
                : 'Server services could not be loaded. Check the server connection in Settings.',
          ),
        ],
        if (localConnectionFailure != null) ...[
          SizedBox(height: FloeSpace.base),
          const FloeInfoNote(
            text: 'Apple connections could not be loaded. Check system access and try again.',
          ),
        ],
        if (connectedServerCalendars.isNotEmpty) ...[
          SizedBox(height: FloeSpace.lg),
          _calendarProviderSelection(context),
        ],
        SizedBox(height: FloeSpace.base),
        if (cards.isEmpty && !loadingCatalog)
          const FloeSquircle(
            padding: EdgeInsets.all(FloeSpace.lg),
            child: Text('No connector providers are available on this device.'),
          )
        else if (availableCards.isNotEmpty)
          _ConnectorCardGrid(cards: availableCards),
        if (unavailableCards.isNotEmpty) ...[
          SizedBox(height: 36),
          Text(strings.unavailableServices, style: FloeType.title),
          SizedBox(height: FloeSpace.base),
          _ConnectorCardGrid(cards: unavailableCards),
        ],
        if (loadingCatalog) ...[
          SizedBox(height: FloeSpace.lg),
          const LinearProgressIndicator(key: Key('connector-catalog-loading')),
        ],
        if (loadingLocalConnections) ...[
          SizedBox(height: FloeSpace.lg),
          const LinearProgressIndicator(
            key: ValueKey('local-connections-loading'),
          ),
        ],
      ],
    );
  }

  Widget _calendarProviderSelection(BuildContext context) {
    final serverCalendars = connectedServerCalendars;
    final activeServer = serverCalendars
        .where(_isActiveServerCalendar)
        .firstOrNull;
    final activeDevice = deviceCalendarConnection != null;
    final needsChoice =
        serverCalendars.length > 1 && activeServer == null && !activeDevice;
    final activeName =
        activeServer?.name ??
        (activeDevice
            ? _deviceCalendarName(
                AppLocalizations.of(context),
                effectivePlatform,
              )
            : null);
    return FloeSquircle(
      key: const Key('calendar-provider-selection'),
      fill: FloePalette.neutral0,
      borderColor: needsChoice
          ? FloePalette.warning600
          : FloePalette.neutral200,
      borderWidth: 1,
      padding: const EdgeInsets.all(FloeSpace.lg),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text('Calendar used by Floe', style: FloeType.title),
          SizedBox(height: FloeSpace.sm),
          Text(
            activeName == null
                ? 'Choose which connected calendar Floe and Schedule should use.'
                : '$activeName supplies calendar context to Floe and Schedule.',
            style: FloeType.bodySmall.copyWith(color: FloePalette.neutral600),
          ),
          SizedBox(height: FloeSpace.base),
          if (activeDevice)
            _CalendarProviderOption(
              key: const Key('calendar-provider-device'),
              name: _deviceCalendarName(
                AppLocalizations.of(context),
                effectivePlatform,
              ),
              active: true,
              onPressed: null,
            )
          else if (supportsDeviceCalendar && widget.gateway != null)
            _CalendarProviderOption(
              key: const Key('calendar-provider-device'),
              name: _deviceCalendarName(
                AppLocalizations.of(context),
                effectivePlatform,
              ),
              active: false,
              actionLabel: 'Choose calendars',
              onPressed: () => setState(() => deviceCalendarDetail = true),
            ),
          for (final connector in serverCalendars) ...[
            if (activeDevice || serverCalendars.indexOf(connector) > 0)
              SizedBox(height: FloeSpace.sm),
            _CalendarProviderOption(
              key: Key('calendar-provider-${connector.id}'),
              name: connector.name,
              active: _isActiveServerCalendar(connector),
              loading: activatingCalendarConnectorId == connector.id,
              onPressed: _isActiveServerCalendar(connector)
                  ? null
                  : () => _activateServerCalendar(connector),
            ),
          ],
          if (calendarSelectionError != null) ...[
            SizedBox(height: FloeSpace.base),
            FloeInfoNote(text: calendarSelectionError!),
          ],
        ],
      ),
    );
  }

  Widget _deviceCalendarDetail(BuildContext context) => Column(
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      Align(
        alignment: Alignment.centerLeft,
        child: FloeTextLink(
          label: AppLocalizations.of(context).backToConnections,
          icon: LucideIcons.arrowLeft,
          onPressed: () => setState(() => deviceCalendarDetail = false),
        ),
      ),
      SizedBox(height: FloeSpace.lg),
      Text(
        _deviceCalendarName(AppLocalizations.of(context), effectivePlatform),
        style: FloeType.headline,
      ),
      if (widget.deviceId != null) ...[
        SizedBox(height: FloeSpace.sm),
        Text(
          'Device binding · ${widget.deviceId}',
          style: FloeType.bodySmall.copyWith(color: FloePalette.neutral600),
        ),
      ],
      SizedBox(height: FloeSpace.lg),
      if (widget.gateway != null)
        CalendarPanel(
          gateway: widget.gateway!,
          query: widget.query,
          connection: deviceCalendarConnection,
          onChanged: widget.onChanged,
          platform: effectivePlatform,
        )
      else
        FloeSquircle(
          padding: EdgeInsets.all(FloeSpace.lg),
          child: Text(
            AppLocalizations.of(context)
                .calendarIntegrationIsUnavailableInThisPreview,
          ),
        ),
      if (effectivePlatform == TargetPlatform.macOS &&
          widget.agentController != null) ...[
        const SizedBox(height: FloeSpace.lg),
        FloeSquircle(
          padding: const EdgeInsets.all(FloeSpace.lg),
          child: AgentCalendarSettings(
            controller: widget.agentController!,
            sources: widget.calendarSources,
            sourceChanges: widget.calendarSourceChanges,
          ),
        ),
      ],
      SizedBox(height: FloeSpace.lg),
      FloeInfoNote(
        text: effectivePlatform == TargetPlatform.android
            ? AppLocalizations.of(context).androidCalendarDeviceBoundary
            : AppLocalizations.of(context).appleCalendarDeviceBoundary,
      ),
    ],
  );
}

AgentConnection _macOSAttentionConnection(
  String deviceId,
  Map<String, dynamic> view,
) => AgentConnection.fromJson({
  'descriptor': {
    'schema_version': 1,
    'id': 'attention.macos',
    'version': '1.0.0',
    'provider': 'attention.macos',
    'execution': {'kind': 'device', 'device_id': deviceId},
    'capabilities': [
      {
        'schema_version': 1,
        'id': 'attention.coarse.read',
        'version': '1.0.0',
        'authority': 'observe',
        'required_scopes': ['session_observation'],
        'output_view_id': 'attention.coarse',
      },
    ],
    'views': [
      {
        'schema_version': 1,
        'id': 'attention.coarse',
        'version': '1.0.0',
        'data_class': 'personal',
        'retention': 'ephemeral',
        'freshness_ttl_ms': 120000,
        'max_items': 1,
        'max_bytes': 8192,
        'provenance_required': true,
      },
    ],
  },
  'connection': {
    'schema_version': 1,
    'connector_id': 'attention.macos',
    'state': view['state'] == 'unknown' ? 'unavailable' : 'ready',
    'granted_scopes': ['session_observation'],
    'observed_at_unix_ms': view['observed_at_unix_ms'],
    'last_success_at_unix_ms': view['observed_at_unix_ms'],
    if (view['state'] == 'unknown')
      'last_failure': {
        'kind': 'permission_denied',
        'observed_at_unix_ms': view['observed_at_unix_ms'],
      },
  },
  'views': [
    {
      'schema_version': 1,
      'view_id': 'attention.coarse',
      'source_handle': view['source_handle'],
      'observed_at_unix_ms': view['observed_at_unix_ms'],
      'expires_at_unix_ms': view['expires_at_unix_ms'],
      'item_count': view['state'] == 'unknown' ? 0 : 1,
      'byte_count': 256,
      'provenance_count': view['evidence_handles'] is List
          ? (view['evidence_handles'] as List).length
          : 0,
    },
  ],
});

final class _AppleConnectionDetail extends StatefulWidget {
  const _AppleConnectionDetail({
    required this.connection,
    required this.personId,
    required this.deviceId,
    required this.agentVaultGateway,
    required this.appleContext,
    required this.macOSContext,
    required this.daySnapshot,
    required this.onBack,
    required this.onChanged,
  });

  final AgentConnection connection;
  final String personId;
  final String deviceId;
  final NativeAgentVaultGateway? agentVaultGateway;
  final AppleContextApi? appleContext;
  final MacOSContextApi? macOSContext;
  final DaySnapshot? daySnapshot;
  final VoidCallback onBack;
  final Future<void> Function() onChanged;

  @override
  State<_AppleConnectionDetail> createState() => _AppleConnectionDetailState();
}

final class _AppleConnectionDetailState extends State<_AppleConnectionDetail> {
  bool busy = false;
  String? failure;

  String get provider => widget.connection.descriptor.provider;
  bool get blocked => {
    AgentConnectionState.revoked,
    AgentConnectionState.unavailable,
    AgentConnectionState.unsupported,
  }.contains(widget.connection.state);

  bool _hasObserveCapability(String id) =>
      widget.connection.descriptor.capabilities.any(
        (capability) =>
            capability.id == id && capability.authority == 'observe',
      );

  Future<void> _recover() async {
    if (busy) return;
    setState(() {
      busy = true;
      failure = null;
    });
    try {
      final apple = widget.appleContext;
      switch (provider) {
        case 'apple_contacts':
          if (apple == null) {
            throw UnsupportedError('Apple Contacts unavailable.');
          }
          if (widget.connection.state == AgentConnectionState.revoked) {
            if (!await apple.requestPermission(AppleContextSource.contacts)) {
              throw StateError('Apple Contacts permission was not granted.');
            }
          }
          await apple.readContacts();
        case 'apple_feasibility':
          if (apple is! AppleFeasibilitySubjectApi) {
            throw UnsupportedError('Apple Location access unavailable.');
          }
          if (!await (apple as AppleFeasibilitySubjectApi)
              .requestFeasibilityPermission()) {
            throw StateError('Location permission was not granted.');
          }
        case 'apple_health':
          if (apple is! AppleHealthSubjectApi) {
            throw UnsupportedError('Apple Health unavailable.');
          }
          if (!await (apple as AppleHealthSubjectApi)
              .requestWellbeingPermission()) {
            throw StateError('Apple Health permission was not granted.');
          }
          await (apple as AppleContextApi).readWellbeing();
        case 'apple_screen_time':
          if (apple == null) {
            throw UnsupportedError('Apple Screen Time unavailable.');
          }
          await apple.screenTimeCapability();
        case 'attention.macos':
          final macOS = widget.macOSContext;
          if (macOS == null) throw UnsupportedError('Attention unavailable.');
          await macOS.inspectAttentionSubject(widget.deviceId);
          await macOS.readAttention();
        default:
          throw UnsupportedError('This connection is not editable.');
      }
      await widget.onChanged();
    } on Object catch (error) {
      if (mounted) setState(() => failure = error.toString());
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }

  Future<PersonalFeasibilityQuery?> _requestQuery() async {
    final snapshot = widget.daySnapshot;
    final eventId = snapshot?.nextEventId;
    if (snapshot == null || eventId == null) return null;
    final event = snapshot.items
        .whereType<EventItem>()
        .where((item) => item.id == eventId)
        .firstOrNull;
    if (event == null || event.isAllDay) return null;
    return showFloeDialog<PersonalFeasibilityQuery>(
      context,
      (_) => _PersonalFeasibilityDialog(event: event),
    );
  }

  @override
  Widget build(BuildContext context) {
    final gateway = widget.agentVaultGateway;
    final scoped = gateway == null
        ? null
        : _ScopedPersonalAccessGateway(
            gateway,
            personId: widget.personId,
            connectionId: widget.connection.descriptor.id == 'contacts.apple'
                ? 'contacts.apple.local'
                : widget.connection.descriptor.id == 'feasibility.apple'
                ? 'feasibility.apple.local'
                : widget.connection.descriptor.id == 'health.apple'
                ? 'health.apple.local'
                : 'attention.macos.local',
          );
    final title = _nativeConnectionName(provider);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Align(
          alignment: Alignment.centerLeft,
          child: FloeTextLink(
            label: AppLocalizations.of(context).backToConnections,
            icon: LucideIcons.arrowLeft,
            onPressed: widget.onBack,
          ),
        ),
        SizedBox(height: FloeSpace.lg),
        Text(title, style: FloeType.headline),
        SizedBox(height: FloeSpace.sm),
        Text(
          'Connection ${widget.connection.descriptor.id} · ${widget.deviceId}',
          style: FloeType.bodySmall.copyWith(color: FloePalette.neutral600),
        ),
        SizedBox(height: FloeSpace.lg),
        FloeSquircle(
          padding: const EdgeInsets.all(FloeSpace.lg),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text('System access', style: FloeType.title),
              const SizedBox(height: FloeSpace.xs),
              Text(_connectionStateText(widget.connection.state)),
              const SizedBox(height: FloeSpace.sm),
              FloeButton.outlined(
                key: const ValueKey('apple-connection-recover'),
                onPressed:
                    busy ||
                        widget.connection.state ==
                            AgentConnectionState.unsupported
                    ? null
                    : _recover,
                loading: busy,
                child: Text(blocked ? 'Allow access' : 'Refresh access'),
              ),
              if (failure != null) ...[
                const SizedBox(height: FloeSpace.xs),
                Text(failure!, style: FloeType.bodySmall),
              ],
            ],
          ),
        ),
        if (!blocked &&
            provider == 'apple_contacts' &&
            _hasObserveCapability('contacts.identity.read') &&
            scoped != null &&
            widget.appleContext is AppleContextSubjectApi) ...[
          SizedBox(height: FloeSpace.lg),
          PersonalContactsAccessCard(
            gateway: scoped,
            personId: widget.personId,
            readContacts: () => widget.appleContext!.readContacts(),
            inspectSubject: (handles) =>
                (widget.appleContext! as AppleContextSubjectApi)
                    .inspectContactsSubject(handles),
          ),
        ],
        if (!blocked &&
            provider == 'attention.macos' &&
            _hasObserveCapability('attention.coarse.read') &&
            scoped != null &&
            widget.macOSContext != null) ...[
          SizedBox(height: FloeSpace.lg),
          PersonalAttentionAccessCard(
            gateway: scoped,
            personId: widget.personId,
            inspectSubject: () =>
                widget.macOSContext!.inspectAttentionSubject(widget.deviceId),
          ),
        ],
        if (!blocked &&
            provider == 'apple_feasibility' &&
            _hasObserveCapability('schedule.feasibility.read') &&
            scoped != null &&
            widget.appleContext is AppleFeasibilitySubjectApi) ...[
          SizedBox(height: FloeSpace.lg),
          PersonalFeasibilityAccessCard(
            gateway: scoped,
            personId: widget.personId,
            requestQuery: _requestQuery,
            requestPermission: () =>
                (widget.appleContext! as AppleFeasibilitySubjectApi)
                    .requestFeasibilityPermission(),
            inspectSubject: () =>
                (widget.appleContext! as AppleFeasibilitySubjectApi)
                    .inspectFeasibilitySubject(),
          ),
        ],
        if (!blocked &&
            provider == 'apple_health' &&
            _hasObserveCapability('health.derived.read') &&
            scoped != null &&
            widget.appleContext is AppleHealthSubjectApi) ...[
          SizedBox(height: FloeSpace.lg),
          PersonalWellbeingAccessCard(
            gateway: scoped,
            personId: widget.personId,
            requestPermission: () =>
                (widget.appleContext! as AppleHealthSubjectApi)
                    .requestWellbeingPermission(),
            inspectSubject: () =>
                (widget.appleContext! as AppleHealthSubjectApi)
                    .inspectWellbeingSubject(),
          ),
        ],
      ],
    );
  }
}

final class _PersonalFeasibilityDialog extends StatefulWidget {
  const _PersonalFeasibilityDialog({required this.event});

  final EventItem event;

  @override
  State<_PersonalFeasibilityDialog> createState() =>
      _PersonalFeasibilityDialogState();
}

final class _PersonalFeasibilityDialogState
    extends State<_PersonalFeasibilityDialog> {
  final latitude = TextEditingController();
  final longitude = TextEditingController();
  AppleTravelMode travelMode = AppleTravelMode.transit;

  @override
  void dispose() {
    latitude.dispose();
    longitude.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => FloeDialog(
    title: const Text('Review trip feasibility'),
    content: SizedBox(
      width: 420,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(widget.event.title, style: FloeType.body),
          const SizedBox(height: FloeSpace.sm),
          FloeInput(
            key: const ValueKey('connection-feasibility-latitude'),
            label: 'Destination latitude',
            controller: latitude,
            keyboardType: const TextInputType.numberWithOptions(
              decimal: true,
              signed: true,
            ),
          ),
          const SizedBox(height: FloeSpace.sm),
          FloeInput(
            key: const ValueKey('connection-feasibility-longitude'),
            label: 'Destination longitude',
            controller: longitude,
            keyboardType: const TextInputType.numberWithOptions(
              decimal: true,
              signed: true,
            ),
          ),
          const SizedBox(height: FloeSpace.sm),
          FloeRadioGroup<AppleTravelMode>(
            value: travelMode,
            onChanged: (value) {
              if (value != null) setState(() => travelMode = value);
            },
            child: Column(
              children: [
                for (final mode in AppleTravelMode.values)
                  FloeRadioTile(value: mode, title: Text(mode.name)),
              ],
            ),
          ),
        ],
      ),
    ),
    actions: [
      FloeButton.text(
        onPressed: () => Navigator.pop(context),
        child: const Text('Cancel'),
      ),
      FloeButton.filled(
        key: const ValueKey('connection-feasibility-confirm'),
        onPressed: () {
          final parsedLatitude = double.tryParse(latitude.text.trim());
          final parsedLongitude = double.tryParse(longitude.text.trim());
          if (parsedLatitude == null ||
              parsedLatitude < -90 ||
              parsedLatitude > 90 ||
              parsedLongitude == null ||
              parsedLongitude < -180 ||
              parsedLongitude > 180) {
            return;
          }
          Navigator.pop(
            context,
            PersonalFeasibilityQuery(
              eventHandle: 'event:${widget.event.id}',
              evidenceHandles: ['calendar.event:${widget.event.id}'],
              destinationLatitude: parsedLatitude,
              destinationLongitude: parsedLongitude,
              eventStartUnixMs: widget.event.startsAt
                  .toUtc()
                  .millisecondsSinceEpoch,
              eventEndUnixMs: widget.event.endsAt
                  .toUtc()
                  .millisecondsSinceEpoch,
              travelMode: travelMode.name,
            ),
          );
        },
        child: const Text('Review access'),
      ),
    ],
  );
}

final class _ScopedPersonalAccessGateway implements AgentPersonalAccessGateway {
  _ScopedPersonalAccessGateway(
    this.delegate, {
    required this.personId,
    required this.connectionId,
  });

  final AgentPersonalAccessGateway delegate;
  final String personId;
  final String connectionId;

  PersonalAccessOverview _check(PersonalAccessOverview value) {
    if (value.personId != personId || value.connectionId != connectionId) {
      throw const FormatException('Personal access connection changed.');
    }
    return value;
  }

  @override
  Future<PersonalAccessOverview> inspectPersonalAttention(String id) async =>
      _check(await delegate.inspectPersonalAttention(id));

  @override
  Future<PersonalAccessOverview> reviewPersonalAttention(
    String id, {
    required PersonalAccessOverview reviewedPreview,
    required List<String> consumers,
  }) async => _check(
    await delegate.reviewPersonalAttention(
      id,
      reviewedPreview: reviewedPreview,
      consumers: consumers,
    ),
  );

  @override
  Future<PersonalAccessOverview> setPersonalAttentionEnabled(
    String id,
    bool enabled,
  ) async => _check(await delegate.setPersonalAttentionEnabled(id, enabled));

  @override
  Future<PersonalAccessOverview> inspectPersonalFeasibility(String id) async =>
      _check(await delegate.inspectPersonalFeasibility(id));

  @override
  Future<PersonalAccessOverview> reviewPersonalFeasibility(
    String id, {
    required PersonalFeasibilityQuery query,
    required PersonalAccessOverview reviewedPreview,
    required List<String> consumers,
  }) async => _check(
    await delegate.reviewPersonalFeasibility(
      id,
      query: query,
      reviewedPreview: reviewedPreview,
      consumers: consumers,
    ),
  );

  @override
  Future<PersonalAccessOverview> setPersonalFeasibilityEnabled(
    String id,
    bool enabled,
  ) async => _check(await delegate.setPersonalFeasibilityEnabled(id, enabled));

  @override
  Future<PersonalAccessOverview> inspectPersonalWellbeing(String id) async =>
      _check(await delegate.inspectPersonalWellbeing(id));

  @override
  Future<PersonalAccessOverview> reviewPersonalWellbeing(
    String id, {
    required PersonalAccessOverview reviewedPreview,
    required String nativeSubjectFingerprint,
  }) async => _check(
    await delegate.reviewPersonalWellbeing(
      id,
      reviewedPreview: reviewedPreview,
      nativeSubjectFingerprint: nativeSubjectFingerprint,
    ),
  );

  @override
  Future<PersonalAccessOverview> setPersonalWellbeingEnabled(
    String id,
    bool enabled,
  ) async => _check(await delegate.setPersonalWellbeingEnabled(id, enabled));

  @override
  Future<PersonalAccessOverview> inspectPersonalContacts(
    String id,
    List<String> selectedHandles,
  ) async =>
      _check(await delegate.inspectPersonalContacts(id, selectedHandles));

  @override
  Future<PersonalAccessOverview> reviewPersonalContacts(
    String id, {
    required List<String> selectedHandles,
    required PersonalAccessOverview reviewedPreview,
    required List<String> consumers,
  }) async => _check(
    await delegate.reviewPersonalContacts(
      id,
      selectedHandles: selectedHandles,
      reviewedPreview: reviewedPreview,
      consumers: consumers,
    ),
  );
}

String _nativeConnectionName(String provider) => switch (provider) {
  'apple_contacts' => 'Apple Contacts',
  'attention.macos' || 'apple_screen_time' => 'Attention',
  'apple_feasibility' => 'Apple Location, ETA & Weather',
  'apple_health' => 'Wellbeing',
  _ => 'Apple connection',
};

String _nativeConnectionDescription(String provider) => switch (provider) {
  'apple_contacts' => 'Choose bounded contact identities for Floe.',
  'attention.macos' ||
  'apple_screen_time' => 'Use coarse device attention signals.',
  'apple_feasibility' => 'Review location, route and weather estimates.',
  'apple_health' => 'Use a derived wellbeing summary from Apple Health.',
  _ => 'Manage this Apple connection.',
};

IconData _nativeConnectionIcon(String provider) => switch (provider) {
  'apple_contacts' => LucideIcons.contact,
  'attention.macos' || 'apple_screen_time' => LucideIcons.focus,
  'apple_feasibility' => LucideIcons.map,
  'apple_health' => LucideIcons.heartPulse,
  _ => LucideIcons.circleHelp,
};

ServerConnectorStatus _serverStatus(AgentConnectionState state) =>
    switch (state) {
      AgentConnectionState.ready ||
      AgentConnectionState.degraded => ServerConnectorStatus.connected,
      AgentConnectionState.unsupported ||
      AgentConnectionState.unavailable => ServerConnectorStatus.unavailable,
      _ => ServerConnectorStatus.available,
    };

String _connectionStateText(AgentConnectionState state) => switch (state) {
  AgentConnectionState.ready => 'Ready on this device.',
  AgentConnectionState.degraded => 'Available with limited data.',
  AgentConnectionState.pending => 'Waiting for the first successful read.',
  AgentConnectionState.revoked => 'System access is revoked.',
  AgentConnectionState.unavailable => 'The source needs recovery.',
  AgentConnectionState.unsupported => 'This source is not supported here.',
  AgentConnectionState.disconnected => 'The source is disconnected.',
};

class _CalendarProviderOption extends StatelessWidget {
  const _CalendarProviderOption({
    super.key,
    required this.name,
    required this.active,
    required this.onPressed,
    this.actionLabel = 'Use for Floe & Schedule',
    this.loading = false,
  });

  final String name;
  final bool active;
  final VoidCallback? onPressed;
  final String actionLabel;
  final bool loading;

  @override
  Widget build(BuildContext context) => Row(
    children: [
      Icon(
        active ? LucideIcons.circleCheck : LucideIcons.circle,
        size: 20,
        color: active ? FloePalette.primary600 : FloePalette.neutral400,
      ),
      SizedBox(width: FloeSpace.sm),
      Expanded(child: Text(name, style: FloeType.body)),
      if (active)
        const FloeBadge(
          label: 'Active',
          tone: FloeBadgeTone.success,
          compact: true,
        )
      else
        FloeButton.outlined(
          size: FloeButtonSize.compact,
          loading: loading,
          onPressed: onPressed,
          child: Text(actionLabel),
        ),
    ],
  );
}

class _ConnectorCard extends StatelessWidget {
  const _ConnectorCard({
    super.key,
    required this.icon,
    required this.name,
    required this.description,
    required this.status,
    required this.onPressed,
  });

  final IconData icon;
  final String name;
  final String description;
  final ServerConnectorStatus status;
  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) => FloePressable(
    size: FloeSquircleSize.lg,
    fill: FloePalette.neutral0,
    borderColor: FloePalette.neutral200,
    borderWidth: 1,
    onPressed: onPressed,
    hoverFill: FloePalette.primary50,
    child: Padding(
      padding: EdgeInsets.all(FloeSpace.lg),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              FloeSquircle(
                size: FloeSquircleSize.md,
                fill: FloePalette.primary50,
                borderWidth: 0,
                padding: EdgeInsets.all(14),
                child: Icon(icon, size: 26, color: FloePalette.primary600),
              ),
              FloeBadge(
                label: connectorStatusLabel(status),
                tone: connectorStatusTone(status),
                compact: true,
              ),
            ],
          ),
          SizedBox(height: 20),
          Text(name, style: FloeType.title),
          SizedBox(height: FloeSpace.sm),
          Text(
            description,
            style: FloeType.bodySmall.copyWith(
              height: 1.6,
              color: FloePalette.neutral600,
            ),
          ),
        ],
      ),
    ),
  );
}

class _ConnectorCardGrid extends StatelessWidget {
  const _ConnectorCardGrid({required this.cards});

  final List<Widget> cards;

  @override
  Widget build(BuildContext context) => LayoutBuilder(
    builder: (context, constraints) {
      final width = constraints.maxWidth < 620
          ? constraints.maxWidth
          : constraints.maxWidth < 930
          ? (constraints.maxWidth - FloeSpace.lg) / 2
          : (constraints.maxWidth - FloeSpace.lg * 2) / 3;
      return Wrap(
        spacing: FloeSpace.lg,
        runSpacing: FloeSpace.lg,
        children: [
          for (final card in cards) SizedBox(width: width, child: card),
        ],
      );
    },
  );
}

IconData _connectorIcon(String id) {
  if (id.startsWith('calendar.')) return LucideIcons.calendarDays;
  if (id.contains('mail') || id == 'gmail') return LucideIcons.mail;
  if (id.startsWith('github')) return LucideIcons.gitFork;
  if (id.startsWith('slack') || id.endsWith('teams')) {
    return LucideIcons.messagesSquare;
  }
  if (id.startsWith('home_assistant')) return LucideIcons.house;
  if (id.contains('drive')) return LucideIcons.folder;
  return LucideIcons.plug;
}

String _connectorDescription(ServerConnector connector) =>
    switch (connector.id) {
      'gmail' || 'microsoft.mail' =>
        'Read selected mail context through your Floe server.',
      'github.issues' => 'Bring repository issues into your work context.',
      'slack.conversations' || 'microsoft.teams' =>
        'Read selected team conversations through your Floe server.',
      'google_drive.files' => 'Read files from a selected Drive folder.',
      'calendar.google' || 'calendar.microsoft' =>
        'Read a selected calendar through your Floe server.',
      'home_assistant.states' => 'Read selected Home Assistant entity states.',
      _ => 'Bring bounded context into Floe through your server.',
    };

String _calendarProvider(String connectorId) => switch (connectorId) {
  'calendar.google' => 'google_calendar',
  'calendar.microsoft' => 'microsoft_calendar',
  _ => throw ArgumentError.value(connectorId, 'connectorId'),
};

String _deviceCalendarName(AppLocalizations strings, TargetPlatform platform) =>
    switch (platform) {
      TargetPlatform.iOS => strings.appleCalendar,
      TargetPlatform.macOS => strings.macosCalendar,
      TargetPlatform.android => strings.androidCalendar,
      _ => strings.deviceCalendar,
    };

String _deviceCalendarDescription(
  AppLocalizations strings,
  TargetPlatform platform,
) => switch (platform) {
  TargetPlatform.iOS => strings.calendarsAlreadyOnThisIphoneOrIpad,
  TargetPlatform.macOS => strings.calendarsAlreadyOnThisMac,
  TargetPlatform.android => strings.selectedCalendarsOnThisAndroidDevice,
  _ => strings.calendarsAlreadyOnThisDevice,
};
