import 'package:flutter/material.dart';

import '../../app/design_tokens.dart';
import '../../app/floe_badge.dart';
import '../../app/floe_button.dart';
import '../../app/floe_squircle.dart';
import 'agent_connections.dart';

final class AgentConnectionSettings extends StatelessWidget {
  const AgentConnectionSettings({
    super.key,
    required this.connections,
    required this.loading,
    required this.failed,
    required this.onRefresh,
  });

  final List<AgentConnection> connections;
  final bool loading;
  final bool failed;
  final Future<void> Function() onRefresh;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Row(
          children: [
            const Expanded(child: Text('Connections', style: FloeType.title)),
            FloeButton.text(
              key: const ValueKey('connections-refresh'),
              onPressed: loading ? null : onRefresh,
              child: const Text('Refresh'),
            ),
          ],
        ),
        const SizedBox(height: FloeSpace.xs),
        Text(
          'Source health, execution location and read scope are reported independently from action authority.',
          style: FloeType.body.copyWith(color: FloePalette.neutral600),
        ),
        const SizedBox(height: FloeSpace.base),
        if (failed) ...[
          const Text('Some connection status is temporarily unavailable.'),
          if (connections.isNotEmpty) const SizedBox(height: FloeSpace.sm),
        ],
        if (loading && connections.isEmpty)
          const Text('Loading connection status…')
        else if (connections.isEmpty && !failed)
          const Text('No connected sources are available yet.')
        else
          for (final connection in connections)
            Padding(
              padding: const EdgeInsets.only(bottom: FloeSpace.sm),
              child: _ConnectionCard(connection: connection),
            ),
      ],
    );
  }
}

final class _ConnectionCard extends StatelessWidget {
  const _ConnectionCard({required this.connection});

  final AgentConnection connection;

  @override
  Widget build(BuildContext context) {
    final status = _status(connection.state);
    final lastSuccess = connection.lastSuccessAt;
    return FloeSquircle(
      size: FloeSquircleSize.md,
      fill: FloePalette.neutral50,
      borderWidth: 0,
      padding: const EdgeInsets.all(FloeSpace.base),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              Expanded(
                child: Text(
                  _provider(connection.descriptor.provider),
                  style: FloeType.controlLabel.copyWith(
                    fontWeight: FontWeight.w600,
                  ),
                ),
              ),
              FloeBadge(label: status.$1, tone: status.$2),
            ],
          ),
          const SizedBox(height: FloeSpace.xs),
          Text(
            [
              connection.descriptor.execution.kind == 'device'
                  ? 'Runs on this device'
                  : 'Runs on server',
              '${connection.views.length} available ${connection.views.length == 1 ? 'view' : 'views'}',
              if (lastSuccess != null) 'Last success ${_relative(lastSuccess)}',
            ].join(' · '),
            style: FloeType.body.copyWith(color: FloePalette.neutral600),
          ),
          if (connection.failure != null) ...[
            const SizedBox(height: FloeSpace.xs),
            Text(
              _failure(connection.failure!.kind),
              style: FloeType.bodySmall.copyWith(color: FloePalette.warning600),
            ),
          ],
          const SizedBox(height: FloeSpace.xs),
          Text(
            connection.grantedScopes.isEmpty
                ? 'No active read scope.'
                : 'Read scope: ${connection.grantedScopes.join(', ')}. Actions require separate approval.',
            style: FloeType.bodySmall.copyWith(color: FloePalette.neutral600),
          ),
        ],
      ),
    );
  }

  (String, FloeBadgeTone) _status(
    AgentConnectionState state,
  ) => switch (state) {
    AgentConnectionState.ready => ('Ready', FloeBadgeTone.success),
    AgentConnectionState.degraded => ('Partial', FloeBadgeTone.warning),
    AgentConnectionState.pending => ('Pending', FloeBadgeTone.neutral),
    AgentConnectionState.disconnected => (
      'Disconnected',
      FloeBadgeTone.neutral,
    ),
    AgentConnectionState.unavailable => ('Unavailable', FloeBadgeTone.warning),
    AgentConnectionState.revoked => ('Access revoked', FloeBadgeTone.warning),
    AgentConnectionState.unsupported => ('Unsupported', FloeBadgeTone.neutral),
  };

  String _provider(String provider) => switch (provider) {
    'apple_event_kit' => 'Apple Calendar',
    'android_calendar' => 'Android Calendar',
    'android_contacts' => 'Android Contacts',
    'health_connect' => 'Health Connect',
    'google_calendar' => 'Google Calendar',
    'microsoft_calendar' => 'Microsoft Calendar',
    'microsoft' => 'Microsoft',
    'slack' => 'Slack',
    'google_drive' => 'Google Drive',
    'github' => 'GitHub',
    'home_assistant' => 'Home Assistant',
    'gmail' => 'Gmail',
    'fixture' => 'Demo Calendar',
    _ => 'Connected source',
  };

  String _failure(String failure) => switch (failure) {
    'permission_denied' => 'Source permission is no longer available.',
    'unsupported_entitlement' => 'This source is not supported on this device.',
    'stale' => 'The last observation is stale. Refresh the source.',
    'partial_fetch' => 'Some source data could not be refreshed.',
    'rate_limited' => 'The provider temporarily limited refreshes.',
    'credential_expired' => 'Provider access expired. Reconnect the source.',
    _ => 'The source could not provide current data.',
  };

  String _relative(DateTime time) {
    final elapsed = DateTime.now().toUtc().difference(time);
    if (elapsed.isNegative || elapsed.inMinutes < 1) return 'just now';
    if (elapsed.inHours < 1) return '${elapsed.inMinutes}m ago';
    if (elapsed.inDays < 1) return '${elapsed.inHours}h ago';
    return '${elapsed.inDays}d ago';
  }
}
