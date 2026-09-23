import 'package:floe_client/features/connections/application/remote_access_gateway.dart';
import 'package:floe_client/features/connections/domain/remote_owner_models.dart';

import 'dart:async';

import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';
import 'package:url_launcher/url_launcher.dart';

import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_badge.dart';
import 'package:floe_client/app/floe_button.dart';
import 'package:floe_client/app/floe_feedback.dart';
import 'package:floe_client/app/floe_input.dart';
import 'package:floe_client/app/floe_loading.dart';
import 'package:floe_client/app/floe_primitives.dart';
import 'package:floe_client/app/floe_squircle.dart';
import 'package:floe_client/features/connections/application/local_server_client.dart';
import 'package:floe_client/app/runtime/agent_vault_gateway.dart';
import 'package:floe_client/features/connections/application/connector_authorization_gateway.dart';
import 'package:floe_client/features/connections/presentation/connector_status_presentation.dart';

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
    this.remoteAccessGateway,
    required this.authorization,
    this.authorizationLauncher = _launchConnectorAuthorization,
  });

  final ServerConnector connector;
  final ServerConnection connection;
  final LocalServerClient client;
  final VoidCallback onBack;
  final Future<void> Function() onChanged;
  final RemoteAccessGateway? remoteAccessGateway;
  final ConnectorAuthorizationLauncher authorizationLauncher;
  final ConnectorAuthorizationGateway authorization;

  @override
  State<ServerConnectorPanel> createState() => _ServerConnectorPanelState();
}

class _ServerConnectorPanelState extends State<ServerConnectorPanel> {
  final secret = TextEditingController();
  final scope = <String, TextEditingController>{};
  ServerConnectorAttempt? attempt;
  String? error;
  bool busy = false;
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
      unawaited(
        _drive(
          await widget.authorization.startAuthorization(
            connection: widget.connection,
            connectorId: widget.connector.id,
            attempt: next,
          ),
        ),
      );
    } finally {
      secret.clear();
    }
  });

  /// Relays observations until Connections says the Operation settled.
  ///
  /// The deadline, the retry cadence, the generation guard and the terminal
  /// transition all belong to Rust Connections; this loop only observes,
  /// displays and relays.
  Future<void> _drive(AuthorizationDirective directive) async {
    var next = directive;
    while (mounted) {
      switch (next) {
        case OpenAuthorizationPage(:final authorizationUrl, :final pollAfter):
          if (!await widget.authorizationLauncher(
            Uri.parse(authorizationUrl),
          )) {
            if (mounted) {
              setState(
                () => error = 'Could not open the authorization page. Cancel this attempt and try again.',
              );
            }
            return;
          }
          await _waitForPoll(pollAfter);
        case ObserveAgain(:final pollAfter):
          await _waitForPoll(pollAfter);
        case AuthorizationSettled(:final state, :final errorCode):
          if (!mounted) return;
          if (state != 'connected') {
            setState(
              () => error = connectorErrorMessage(
                errorCode ?? 'connector_authorization_unavailable',
              ),
            );
          }
          await widget.onChanged();
          return;
      }
      if (!mounted) return;
      final pending = attempt;
      if (pending == null) return;
      final observed = await widget.client.connectorAttempt(
        connection: widget.connection,
        connectorId: widget.connector.id,
        attemptId: pending.id,
      );
      if (!mounted) return;
      setState(() => attempt = observed);
      next = await widget.authorization.observeAuthorization(
        connection: widget.connection,
        connectorId: widget.connector.id,
        attempt: observed,
      );
    }
  }

  Future<void> _cancel() => _run(() async {
    final pending = attempt;
    if (pending == null) return;
    pollTimer?.cancel();
    pollTimer = null;
    await widget.authorization.cancelAuthorization(
      connection: widget.connection,
      connectorId: widget.connector.id,
    );
    final next = await widget.client.cancelConnectorAttempt(
      connection: widget.connection,
      connectorId: widget.connector.id,
      attemptId: pending.id,
    );
    if (!mounted) return;
    setState(() => attempt = next);
    await widget.onChanged();
  });

  Future<void> _waitForPoll(Duration interval) {
    final completer = Completer<void>();
    pollTimer = Timer(interval, () {
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
              if (_showConnectionGrants) ...[
                SizedBox(height: FloeSpace.lg),
                _ServerConnectionGrants(
                  connector: widget.connector,
                  connection: widget.connection,
                  client: widget.client,
                  gateway: widget.remoteAccessGateway!,
                ),
              ],
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
              if (displayStatus == ServerConnectorStatus.connecting)
                if (attempt?.userCode case final userCode?) ...[
                  SizedBox(height: FloeSpace.base),
                  FloeInfoNote(
                    text:
                        'Enter this code on the authorization page: $userCode',
                  ),
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
                'OAuth flow state, token exchange, refresh tokens, and stored credentials stay on the Floe server.',
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

  bool get _showConnectionGrants =>
      widget.remoteAccessGateway != null &&
      widget.connector.status == ServerConnectorStatus.connected &&
      widget.connector.connectionId != null &&
      (widget.connector.id == 'calendar.google' ||
          widget.connector.id == 'calendar.microsoft' ||
          _remoteViewsFor(widget.connector.id).isNotEmpty);
}

final class _ServerConnectionGrants extends StatefulWidget {
  const _ServerConnectionGrants({
    required this.connector,
    required this.connection,
    required this.client,
    required this.gateway,
  });

  final ServerConnector connector;
  final ServerConnection connection;
  final LocalServerClient client;
  final RemoteAccessGateway gateway;

  @override
  State<_ServerConnectionGrants> createState() =>
      _ServerConnectionGrantsState();
}

final class _ServerConnectionGrantsState
    extends State<_ServerConnectionGrants> {
  RemoteCalendarGrantPreview? calendarPreview;
  RemoteCalendarGrantOverview? calendarOverview;
  RemoteViewGrantPreview? viewPreview;
  RemoteViewGrantOverview? viewOverview;
  String? selectedView;
  String consumer = 'assistant';
  String? error;
  bool busy = false;

  @override
  void didUpdateWidget(_ServerConnectionGrants oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.connector.id != widget.connector.id ||
        oldWidget.connector.connectionId != widget.connector.connectionId) {
      calendarPreview = null;
      calendarOverview = null;
      viewPreview = null;
      viewOverview = null;
      selectedView = null;
      error = null;
    }
  }

  String get connectionId => widget.connector.connectionId!;
  List<String> get views => _remoteViewsFor(widget.connector.id);
  bool get isCalendar =>
      widget.connector.id == 'calendar.google' ||
      widget.connector.id == 'calendar.microsoft';
  String? get calendarResource {
    final value = widget.connector.scope['calendar_id'];
    return value is String && value.isNotEmpty ? value : null;
  }

  Future<void> _run(Future<void> Function() operation) async {
    if (busy) return;
    setState(() {
      busy = true;
      error = null;
    });
    try {
      await operation();
    } on AgentVaultException catch (failure) {
      if (mounted) setState(() => error = _grantError(failure.failure));
    } on Object {
      if (mounted) {
        setState(
          () => error = 'The connection permission could not be updated.',
        );
      }
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }

  bool _stable(String expected) => expected == connectionId;

  Future<void> _previewCalendar() => _run(() async {
    final resource = calendarResource;
    if (resource == null || !_stable(connectionId)) {
      throw const FormatException('connection_changed');
    }
    final preview = await widget.gateway.previewRemoteCalendarGrant(
      connectorId: widget.connector.id,
      connectionId: connectionId,
      resource: resource,
    );
    if (!_stable(preview.connectionId)) {
      throw const FormatException('connection_changed');
    }
    if (mounted) setState(() => calendarPreview = preview);
  });

  Future<void> _reviewCalendar() => _run(() async {
    final preview = calendarPreview;
    if (preview == null || !_stable(preview.connectionId)) {
      throw const FormatException('connection_changed');
    }
    final overview = await widget.gateway.reviewRemoteCalendarGrant(
      connectorId: widget.connector.id,
      connectionId: connectionId,
      resource: preview.resource,
      expectedProducerFingerprint: preview.producer.fingerprint,
      expectedSourceAuthority: preview.sourceAuthority,
      expectedGrantId: preview.grantId,
      expectedGrantAuthority: preview.grantAuthority,
      expectedConsumerPolicy: preview.consumerPolicy,
    );
    if (overview.connectionId != null && !_stable(overview.connectionId!)) {
      throw const FormatException('connection_changed');
    }
    if (mounted) setState(() => calendarOverview = overview);
  });

  Future<void> _pauseCalendar() => _run(() async {
    final overview = calendarOverview;
    if (overview == null ||
        calendarPreview?.connectionId != connectionId ||
        overview.connectionId != null && !_stable(overview.connectionId!)) {
      throw const FormatException('connection_changed');
    }
    final paused = await widget.gateway.pauseRemoteCalendarGrant(
      grantId: overview.grantId,
      expectedAuthority: overview.grantAuthority,
    );
    if (paused.grantId != overview.grantId ||
        paused.connectionId != connectionId ||
        overview.connectionId != connectionId) {
      throw const FormatException('connection_changed');
    }
    if (mounted) setState(() => calendarOverview = paused);
  });

  Future<void> _previewView() => _run(() async {
    final viewId = selectedView;
    if (viewId == null || !_stable(connectionId)) {
      throw const FormatException('connection_changed');
    }
    final preview = await widget.gateway.previewRemoteViewGrant(
      viewId: viewId,
      connectorId: widget.connector.id,
      connectionId: connectionId,
      resource: '$viewId:$connectionId',
      consumer: consumer,
    );
    if (!_stable(preview.connectionId)) {
      throw const FormatException('connection_changed');
    }
    if (mounted) setState(() => viewPreview = preview);
  });

  Future<void> _reviewView() => _run(() async {
    final preview = viewPreview;
    if (preview == null || !_stable(preview.connectionId)) {
      throw const FormatException('connection_changed');
    }
    final overview = await widget.gateway.reviewRemoteViewGrant(
      viewId: preview.viewId,
      connectorId: widget.connector.id,
      connectionId: connectionId,
      resource: preview.resource,
      consumer: preview.consumer,
      expectedProducerFingerprint: preview.producer.fingerprint,
      expectedSourceAuthority: preview.sourceAuthority,
      expectedConnectionRevision: preview.connectionRevision,
      expectedProviderIdentity: preview.providerIdentity,
      expectedRecipient: preview.recipient,
    );
    if (!_stable(overview.connectionId) ||
        overview.connectionRevision != preview.connectionRevision) {
      throw const FormatException('connection_changed');
    }
    if (mounted) setState(() => viewOverview = overview);
  });

  Future<void> _pauseView() => _run(() async {
    final overview = viewOverview;
    if (overview == null ||
        viewPreview?.connectionId != connectionId ||
        !_stable(overview.connectionId)) {
      throw const FormatException('connection_changed');
    }
    final paused = await widget.gateway.pauseRemoteViewGrant(
      grantId: overview.grantId,
      expectedAuthority: overview.grantAuthority,
    );
    if (paused.grantId != overview.grantId ||
        !_stable(paused.connectionId) ||
        !_stable(overview.connectionId) ||
        paused.connectionRevision != overview.connectionRevision) {
      throw const FormatException('connection_changed');
    }
    if (mounted) setState(() => viewOverview = paused);
  });

  @override
  Widget build(BuildContext context) => FloeSquircle(
    fill: FloePalette.neutral50,
    borderWidth: 0,
    padding: const EdgeInsets.all(FloeSpace.base),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        const Text('Feature permissions', style: FloeType.controlLabel),
        const SizedBox(height: FloeSpace.xs),
        Text(
          'Permissions are scoped to this exact server connection. Preview is read-only; review creates the selected grant.',
          style: FloeType.bodySmall.copyWith(color: FloePalette.neutral600),
        ),
        if (isCalendar) ...[
          const SizedBox(height: FloeSpace.md),
          const Text('Calendar access', style: FloeType.controlLabel),
          Text('Resource: ${calendarResource ?? 'Unavailable'}'),
          _grantButtons(
            previewKey: const ValueKey('connection-calendar-preview'),
            previewLabel: 'Preview calendar permission',
            reviewKey: const ValueKey('connection-calendar-review'),
            reviewLabel: 'Review calendar permission',
            pauseKey: const ValueKey('connection-calendar-pause'),
            pauseLabel: 'Pause calendar permission',
            canPreview: calendarResource != null,
            hasPreview: calendarPreview != null,
            hasActive: calendarOverview?.state == 'active',
            preview: _previewCalendar,
            review: _reviewCalendar,
            pause: _pauseCalendar,
          ),
          if (calendarPreview != null)
            Text('Verified source: ${calendarPreview!.resource}'),
          if (calendarOverview != null)
            Text('Grant status: ${calendarOverview!.state}'),
        ],
        if (views.isNotEmpty) ...[
          const SizedBox(height: FloeSpace.md),
          const Text('Remote views', style: FloeType.controlLabel),
          DropdownButtonFormField<String>(
            key: const ValueKey('connection-view-selection'),
            initialValue: selectedView,
            items: [
              for (final view in views)
                DropdownMenuItem(value: view, child: Text(view)),
            ],
            onChanged: busy
                ? null
                : (value) => setState(() {
                    selectedView = value;
                    viewPreview = null;
                    viewOverview = null;
                  }),
            decoration: const InputDecoration(labelText: 'View'),
          ),
          DropdownButtonFormField<String>(
            key: const ValueKey('connection-view-consumer'),
            initialValue: consumer,
            items: const [
              DropdownMenuItem(value: 'assistant', child: Text('assistant')),
              DropdownMenuItem(
                value: 'floe.builtin.work-context',
                child: Text('work-context'),
              ),
              DropdownMenuItem(
                value: 'floe.builtin.communication',
                child: Text('communication'),
              ),
              DropdownMenuItem(
                value: 'floe.builtin.life-logistics',
                child: Text('life-logistics'),
              ),
            ],
            onChanged: busy
                ? null
                : (value) => setState(() {
                    consumer = value!;
                    viewPreview = null;
                    viewOverview = null;
                  }),
            decoration: const InputDecoration(labelText: 'Consumer'),
          ),
          _grantButtons(
            previewKey: const ValueKey('connection-view-preview'),
            previewLabel: 'Preview remote permission',
            reviewKey: const ValueKey('connection-view-review'),
            reviewLabel: 'Review remote permission',
            pauseKey: const ValueKey('connection-view-pause'),
            pauseLabel: 'Pause remote permission',
            canPreview: selectedView != null,
            hasPreview: viewPreview != null,
            hasActive: viewOverview?.state == 'active',
            preview: _previewView,
            review: _reviewView,
            pause: _pauseView,
          ),
          if (viewPreview != null)
            Text('Verified source: ${viewPreview!.resource}'),
          if (viewOverview != null)
            Text('Grant status: ${viewOverview!.state}'),
        ],
        if (error != null) ...[
          const SizedBox(height: FloeSpace.xs),
          Text(
            error!,
            style: FloeType.bodySmall.copyWith(color: FloePalette.error600),
          ),
        ],
      ],
    ),
  );

  Widget _grantButtons({
    required Key previewKey,
    required String previewLabel,
    required Key reviewKey,
    required String reviewLabel,
    required Key pauseKey,
    required String pauseLabel,
    required bool canPreview,
    required bool hasPreview,
    required bool hasActive,
    required Future<void> Function() preview,
    required Future<void> Function() review,
    required Future<void> Function() pause,
  }) => Wrap(
    spacing: FloeSpace.sm,
    children: [
      FloeButton.text(
        key: previewKey,
        onPressed: busy || !canPreview ? null : preview,
        child: Text(previewLabel),
      ),
      if (hasPreview)
        FloeButton.filled(
          key: reviewKey,
          onPressed: busy ? null : review,
          child: Text(reviewLabel),
        ),
      if (hasActive)
        FloeButton.text(
          key: pauseKey,
          onPressed: busy ? null : pause,
          child: Text(pauseLabel),
        ),
    ],
  );
}

List<String> _remoteViewsFor(String connectorId) => switch (connectorId) {
  'gmail' || 'microsoft.mail' => ['mail.communication'],
  'slack' || 'microsoft.teams' || 'github.repository' => ['work.context'],
  'home_assistant.selected' => ['life.logistics'],
  _ => const [],
};

String _grantError(String code) => switch (code) {
  'policy_denied' =>
    'The connection changed. Refresh Connections and review again.',
  'vault_unavailable' => 'Unlock the local vault before reviewing permissions.',
  'conflict' => 'This permission changed elsewhere. Refresh Connections.',
  _ => 'The connection permission could not be updated.',
};

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
