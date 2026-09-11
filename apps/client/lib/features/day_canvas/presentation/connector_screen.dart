import 'dart:async';

import 'package:floe_client/l10n/app_localizations.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../../app/design_tokens.dart';
import '../../../app/floe_badge.dart';
import '../../../app/floe_feedback.dart';
import '../../../app/floe_primitives.dart';
import '../../../app/floe_squircle.dart';
import '../../server/local_server_client.dart';
import '../application/calendar_gateway.dart';
import '../domain/day_models.dart';
import 'calendar_panel.dart';
import 'connector_status_presentation.dart';
import 'server_connector_panel.dart';

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
  });

  final CalendarGateway? gateway;
  final DayQuery query;
  final CalendarConnection? connection;
  final Future<void> Function() onChanged;
  final LocalServerClient? serverClient;
  final String? deviceId;
  final TargetPlatform? platform;

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
    unawaited(_loadCatalog());
  }

  @override
  void didUpdateWidget(ConnectorScreen oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.serverClient != widget.serverClient) {
      unawaited(_loadCatalog());
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
    final currentProvider = widget.connection?.provider;
    for (final connector in connected) {
      if (_calendarProvider(connector.id) == currentProvider) {
        selected = connector;
        break;
      }
    }
    if (selected == null && connected.length == 1) selected = connected.single;
    if (selected == null) {
      if (connected.isEmpty &&
          const {
            'google_calendar',
            'microsoft_calendar',
          }.contains(currentProvider)) {
        await gateway.disconnectCalendar(widget.query);
        await widget.onChanged();
      }
      return;
    }
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
      return;
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
    await widget.onChanged();
  }

  ServerConnector? get selectedServerConnector {
    final selected = selectedServerConnectorId;
    if (selected == null) return null;
    for (final connector in catalog?.connectors ?? const <ServerConnector>[]) {
      if (connector.id == selected) return connector;
    }
    return null;
  }

  @override
  Widget build(BuildContext context) {
    if (deviceCalendarDetail) return _deviceCalendarDetail(context);
    final serverConnector = selectedServerConnector;
    if (serverConnector != null && serverConnection != null) {
      return ServerConnectorPanel(
        connector: serverConnector,
        connection: serverConnection!,
        client: widget.serverClient!,
        onBack: () => setState(() => selectedServerConnectorId = null),
        onChanged: _loadCatalog,
      );
    }
    return _catalog(context);
  }

  Widget _catalog(BuildContext context) {
    final strings = AppLocalizations.of(context);
    final serverConnectors = catalog?.connectors ?? const <ServerConnector>[];
    final connectedCount =
        serverConnectors
            .where((item) => item.status == ServerConnectorStatus.connected)
            .length +
        (supportsDeviceCalendar &&
                deviceCalendarStatus == ServerConnectorStatus.connected
            ? 1
            : 0);
    final cards = <Widget>[
      if (supportsDeviceCalendar)
        _ConnectorCard(
          key: Key(
            effectivePlatform == TargetPlatform.android
                ? 'connector-calendar-android'
                : 'connector-calendar-apple',
          ),
          icon: LucideIcons.calendarDays,
          name: _deviceCalendarName(strings, effectivePlatform),
          description: _deviceCalendarDescription(strings, effectivePlatform),
          status: deviceCalendarStatus,
          binding: widget.deviceId == null
              ? null
              : 'Bound to this device · ${widget.deviceId}',
          onPressed: () => setState(() => deviceCalendarDetail = true),
        ),
      for (final connector in serverConnectors)
        _ConnectorCard(
          key: Key('connector-${connector.id}'),
          icon: _connectorIcon(connector.id),
          name: connector.name,
          description: _connectorDescription(connector),
          status: connector.status,
          binding: 'Floe server · Person-owned',
          onPressed: () =>
              setState(() => selectedServerConnectorId = connector.id),
        ),
    ];
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
        SizedBox(height: FloeSpace.base),
        if (cards.isEmpty && !loadingCatalog)
          const FloeSquircle(
            padding: EdgeInsets.all(FloeSpace.lg),
            child: Text('No connector providers are available on this device.'),
          )
        else
          LayoutBuilder(
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
          ),
        if (loadingCatalog) ...[
          SizedBox(height: FloeSpace.lg),
          const LinearProgressIndicator(key: Key('connector-catalog-loading')),
        ],
      ],
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
      SizedBox(height: FloeSpace.lg),
      FloeInfoNote(
        text: effectivePlatform == TargetPlatform.android
            ? AppLocalizations.of(context).androidCalendarDeviceBoundary
            : AppLocalizations.of(context).appleCalendarDeviceBoundary,
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
    this.binding,
  });

  final IconData icon;
  final String name;
  final String description;
  final ServerConnectorStatus status;
  final String? binding;
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
          if (binding != null) ...[
            SizedBox(height: FloeSpace.base),
            Text(
              binding!,
              maxLines: 2,
              overflow: TextOverflow.ellipsis,
              style: FloeType.micro.copyWith(color: FloePalette.neutral500),
            ),
          ],
        ],
      ),
    ),
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
