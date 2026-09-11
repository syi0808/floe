import 'dart:async';

import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';
import 'package:url_launcher/url_launcher.dart';

import '../../../app/design_tokens.dart';
import '../../../app/floe_badge.dart';
import '../../../app/floe_button.dart';
import '../../../app/floe_feedback.dart';
import '../../../app/floe_input.dart';
import '../../../app/floe_loading.dart';
import '../../../app/floe_primitives.dart';
import '../../../app/floe_squircle.dart';
import '../../server/local_server_client.dart';
import 'connector_status_presentation.dart';

typedef ConnectorAuthorizationLauncher = Future<bool> Function(Uri uri);

Future<bool> _launchConnectorAuthorization(Uri uri) =>
    launchUrl(uri, mode: LaunchMode.externalApplication);

class ServerConnectorPanel extends StatefulWidget {
  const ServerConnectorPanel({
    super.key,
    required this.connector,
    required this.connection,
    required this.client,
    required this.onBack,
    required this.onChanged,
    this.authorizationLauncher = _launchConnectorAuthorization,
    this.pollInterval = const Duration(seconds: 2),
  });

  final ServerConnector connector;
  final ServerConnection connection;
  final LocalServerClient client;
  final VoidCallback onBack;
  final Future<void> Function() onChanged;
  final ConnectorAuthorizationLauncher authorizationLauncher;
  final Duration pollInterval;

  @override
  State<ServerConnectorPanel> createState() => _ServerConnectorPanelState();
}

class _ServerConnectorPanelState extends State<ServerConnectorPanel> {
  final secret = TextEditingController();
  final scope = <String, TextEditingController>{};
  ServerConnectorAttempt? attempt;
  String? error;
  bool busy = false;
  int pollGeneration = 0;
  Timer? pollTimer;

  @override
  void initState() {
    super.initState();
    _syncScope();
  }

  @override
  void didUpdateWidget(ServerConnectorPanel oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.connector.id != widget.connector.id ||
        oldWidget.connector.scope != widget.connector.scope) {
      for (final controller in scope.values) {
        controller.dispose();
      }
      scope.clear();
      _syncScope();
    }
  }

  void _syncScope() {
    for (final field in widget.connector.scopeFields) {
      final value = widget.connector.scope[field];
      scope[field] = TextEditingController(
        text: value is List ? value.join(', ') : value?.toString() ?? '',
      );
    }
  }

  @override
  void dispose() {
    pollGeneration++;
    pollTimer?.cancel();
    secret.dispose();
    for (final controller in scope.values) {
      controller.dispose();
    }
    super.dispose();
  }

  Map<String, Object?> _scopeValue() => {
    for (final entry in scope.entries)
      entry.key: entry.key == 'entities'
          ? entry.value.text
                .split(',')
                .map((value) => value.trim())
                .where((value) => value.isNotEmpty)
                .toList(growable: false)
          : entry.value.text.trim(),
  };

  bool get _scopeComplete => scope.entries.every(
    (entry) => entry.key == 'thread' || entry.value.text.trim().isNotEmpty,
  );

  Future<void> _run(Future<void> Function() operation) async {
    if (busy) return;
    setState(() {
      busy = true;
      error = null;
    });
    try {
      await FloeLoading.run(operation);
    } on ServerConnectionException catch (failure) {
      if (mounted) setState(() => error = connectorErrorMessage(failure.code));
    } on Object {
      if (mounted) {
        setState(() => error = 'The connector request could not be completed.');
      }
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }

  Future<void> _connect() => _run(() async {
    if (!_scopeComplete || widget.connector.isSecret && secret.text.isEmpty) {
      setState(() => error = 'Complete the required connection fields.');
      return;
    }
    final oneShotSecret = widget.connector.isSecret ? secret.text : null;
    try {
      final next = await widget.client.connectConnector(
        connection: widget.connection,
        connectorId: widget.connector.id,
        scope: _scopeValue(),
        secret: oneShotSecret,
      );
      if (!mounted) return;
      setState(() => attempt = next);
      if (next.status == ServerConnectorStatus.connecting) {
        final authorizationUrl = next.authorizationUrl;
        if (authorizationUrl == null ||
            !_validAuthorizationUrl(authorizationUrl) ||
            !await widget.authorizationLauncher(Uri.parse(authorizationUrl))) {
          if (mounted) {
            setState(
              () => error = 'Could not open the authorization page. Cancel this attempt and try again.',
            );
          }
          return;
        }
        unawaited(_poll(next));
      } else {
        await widget.onChanged();
      }
    } finally {
      secret.clear();
    }
  });

  Future<void> _poll(ServerConnectorAttempt pending) async {
    final generation = ++pollGeneration;
    final deadline = DateTime.now().add(const Duration(minutes: 5));
    try {
      while (mounted &&
          generation == pollGeneration &&
          DateTime.now().isBefore(deadline)) {
        await _waitForPoll();
        if (!mounted || generation != pollGeneration) return;
        final next = await widget.client.connectorAttempt(
          connection: widget.connection,
          connectorId: widget.connector.id,
          attemptId: pending.id,
        );
        if (!mounted || generation != pollGeneration) return;
        setState(() => attempt = next);
        if (next.status == ServerConnectorStatus.connecting) continue;
        if (next.status == ServerConnectorStatus.error) {
          setState(
            () => error = connectorErrorMessage(
              next.errorCode ?? 'connector_authorization_unavailable',
            ),
          );
        }
        await widget.onChanged();
        return;
      }
      if (mounted && generation == pollGeneration) {
        setState(
          () => error =
              'Authorization timed out. Cancel this attempt and try again.',
        );
      }
    } on ServerConnectionException catch (failure) {
      if (mounted && generation == pollGeneration) {
        setState(() => error = connectorErrorMessage(failure.code));
      }
    }
  }

  Future<void> _cancel() => _run(() async {
    final pending = attempt;
    if (pending == null) return;
    pollGeneration++;
    pollTimer?.cancel();
    pollTimer = null;
    final next = await widget.client.cancelConnectorAttempt(
      connection: widget.connection,
      connectorId: widget.connector.id,
      attemptId: pending.id,
    );
    if (!mounted) return;
    setState(() => attempt = next);
    await widget.onChanged();
  });

  Future<void> _waitForPoll() {
    final completer = Completer<void>();
    pollTimer = Timer(widget.pollInterval, () {
      pollTimer = null;
      completer.complete();
    });
    return completer.future;
  }

  Future<void> _updateScope() => _run(() async {
    if (!_scopeComplete) {
      setState(() => error = 'Complete the required scope fields.');
      return;
    }
    final connectionId = widget.connector.connectionId;
    final connectionRevision = widget.connector.connectionRevision;
    if (connectionId == null || connectionRevision == null) {
      throw const ServerConnectionException('connection_changed');
    }
    await widget.client.updateConnectorScope(
      connection: widget.connection,
      connectorId: widget.connector.id,
      connectionId: connectionId,
      connectionRevision: connectionRevision,
      scope: _scopeValue(),
    );
    await widget.onChanged();
  });

  Future<void> _disconnect() => _run(() async {
    final confirmed = await showFloeDialog<bool>(
      context,
      (context) => FloeDialog(
        title: Text('Disconnect ${widget.connector.name}?'),
        content: const Text(
          'Floe will remove this Person-owned connection and ask the server to revoke its stored credential.',
        ),
        actions: [
          FloeButton.text(
            onPressed: () => Navigator.pop(context, false),
            child: const Text('Cancel'),
          ),
          FloeButton.filled(
            onPressed: () => Navigator.pop(context, true),
            child: const Text('Disconnect'),
          ),
        ],
      ),
    );
    if (confirmed != true || !mounted) return;
    final connectionId = widget.connector.connectionId;
    final connectionRevision = widget.connector.connectionRevision;
    if (connectionId == null || connectionRevision == null) {
      throw const ServerConnectionException('connection_changed');
    }
    await widget.client.disconnectConnector(
      connection: widget.connection,
      connectorId: widget.connector.id,
      connectionId: connectionId,
      connectionRevision: connectionRevision,
    );
    await widget.onChanged();
  });

  ServerConnectorStatus get displayStatus =>
      attempt?.status ?? widget.connector.status;

  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      Align(
        alignment: Alignment.centerLeft,
        child: FloeTextLink(
          label: 'Back to connections',
          icon: LucideIcons.arrowLeft,
          onPressed: widget.onBack,
        ),
      ),
      SizedBox(height: FloeSpace.lg),
      FloeSquircle(
        padding: EdgeInsets.all(FloeSpace.xl),
        child: FloeLoadingOverlay(
          loading: busy,
          label: 'Updating ${widget.connector.name}',
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Wrap(
                spacing: FloeSpace.md,
                runSpacing: FloeSpace.sm,
                crossAxisAlignment: WrapCrossAlignment.center,
                children: [
                  Text(widget.connector.name, style: FloeType.headline),
                  FloeBadge(
                    label: connectorStatusLabel(displayStatus),
                    tone: connectorStatusTone(displayStatus),
                  ),
                ],
              ),
              SizedBox(height: FloeSpace.sm),
              Text(
                'Connection owner · this Person\nAttempt device · ${widget.client.deviceId}',
                style: FloeType.bodySmall.copyWith(
                  color: FloePalette.neutral600,
                  height: 1.6,
                ),
              ),
              SizedBox(height: FloeSpace.lg),
              if (!widget.connector.available)
                const FloeInfoNote(
                  text: 'This provider is known to Floe but is unavailable until its server configuration is installed.',
                ),
              if (error != null) ...[
                if (!widget.connector.available)
                  SizedBox(height: FloeSpace.base),
                FloeInfoNote(text: error!),
              ],
              if (widget.connector.scopeFields.isNotEmpty) ...[
                SizedBox(height: FloeSpace.lg),
                Text('Source scope', style: FloeType.controlLabel),
                SizedBox(height: FloeSpace.md),
                for (final field in widget.connector.scopeFields) ...[
                  FloeInput(
                    key: Key('connector-scope-$field'),
                    label: _scopeLabel(field),
                    controller: scope[field]!,
                    enabled: !busy && widget.connector.available,
                    autocorrect: false,
                    enableSuggestions: false,
                  ),
                  SizedBox(height: FloeSpace.md),
                ],
              ],
              if (widget.connector.isSecret &&
                  displayStatus != ServerConnectorStatus.connected) ...[
                FloeInput(
                  key: const Key('connector-secret'),
                  label: 'Access token',
                  controller: secret,
                  enabled: !busy && widget.connector.available,
                  obscureText: true,
                  autocorrect: false,
                  enableSuggestions: false,
                ),
                SizedBox(height: FloeSpace.sm),
                Text(
                  'Sent once to your Floe server and never saved by this app.',
                  style: FloeType.bodySmall.copyWith(
                    color: FloePalette.neutral600,
                  ),
                ),
                SizedBox(height: FloeSpace.md),
              ],
              Wrap(
                spacing: FloeSpace.sm,
                runSpacing: FloeSpace.sm,
                children: [
                  if ({
                        ServerConnectorStatus.available,
                        ServerConnectorStatus.error,
                      }.contains(displayStatus) &&
                      widget.connector.connectionId == null &&
                      widget.connector.capabilities.connect)
                    FloeButton.filled(
                      onPressed: busy || !widget.connector.available
                          ? null
                          : _connect,
                      child: Text(
                        widget.connector.isSecret
                            ? 'Connect securely'
                            : 'Continue to authorize',
                      ),
                    ),
                  if (displayStatus == ServerConnectorStatus.connecting &&
                      attempt != null &&
                      widget.connector.capabilities.cancel)
                    FloeButton.outlined(
                      onPressed: busy ? null : _cancel,
                      child: const Text('Cancel connection'),
                    ),
                  if (displayStatus == ServerConnectorStatus.connected &&
                      widget.connector.capabilities.scopeUpdate)
                    FloeButton.outlined(
                      onPressed: busy ? null : _updateScope,
                      child: const Text('Update scope'),
                    ),
                  if ((displayStatus == ServerConnectorStatus.connected ||
                          displayStatus == ServerConnectorStatus.error ||
                          displayStatus == ServerConnectorStatus.connecting &&
                              attempt == null) &&
                      widget.connector.capabilities.disconnect)
                    FloeButton.text(
                      onPressed: busy ? null : _disconnect,
                      child: const Text('Disconnect'),
                    ),
                ],
              ),
              SizedBox(height: FloeSpace.lg),
              Text('Server boundary', style: FloeType.controlLabel),
              SizedBox(height: FloeSpace.xs),
              Text(
                'OAuth state, PKCE verifier, token exchange, refresh tokens, and stored credentials stay on the Floe server.',
                style: FloeType.bodySmall.copyWith(
                  color: FloePalette.neutral600,
                  height: 1.6,
                ),
              ),
            ],
          ),
        ),
      ),
    ],
  );
}

bool _validAuthorizationUrl(String value) {
  final uri = Uri.tryParse(value);
  return uri != null &&
      uri.scheme == 'https' &&
      uri.host.isNotEmpty &&
      uri.userInfo.isEmpty;
}

String _scopeLabel(String value) => switch (value) {
  'owner' => 'Repository owner',
  'repository' => 'Repository',
  'channel' => 'Channel',
  'thread' => 'Thread (optional)',
  'folder_id' => 'Folder ID',
  'calendar_id' => 'Calendar ID',
  'team_id' => 'Team ID',
  'channel_id' => 'Channel ID',
  'base_url' => 'Home Assistant URL',
  'entities' => 'Entity IDs (comma-separated)',
  _ => value,
};

String connectorErrorMessage(String code) => switch (code) {
  'unauthorized' =>
    'Server authorization expired. Pair this device again in Settings.',
  'connection_owned_by_another_person' =>
    'This connector belongs to another Person on the server.',
  'connector_unavailable' =>
    'The provider is not configured on the Floe server.',
  'invalid_scope' ||
  'validation' => 'Check the source scope and credential, then try again.',
  'already_connected' || 'connection_changed' =>
    'The connection changed on the server. Return to Connections and refresh.',
  'credential_store_unavailable' || 'credential_scope_unavailable' =>
    'The server could not securely store this credential.',
  'connector_authorization_unavailable' =>
    'The provider authorization could not be completed.',
  'capability_not_supported' =>
    'This connector does not support that operation.',
  _ => 'The Floe server rejected the connector request.',
};
