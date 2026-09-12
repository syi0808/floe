part of '../settings_screen.dart';

final class _CalendarGrantChoice {
  const _CalendarGrantChoice({
    required this.connectorId,
    required this.connectionId,
    required this.resource,
    required this.label,
  });

  final String connectorId;
  final String connectionId;
  final String resource;
  final String label;

  String get value => '$connectorId\u0000$connectionId\u0000$resource';
}

List<_CalendarGrantChoice> _calendarChoices(List<Map<String, dynamic>> values) {
  final choices = <_CalendarGrantChoice>[];
  for (final value in values) {
    final connectorId = value['connector_id'];
    final connectionId = value['connection_id'];
    final scope = value['scope'];
    if (connectorId is! String ||
        connectionId is! String ||
        scope is! Map ||
        !{'calendar.google', 'calendar.microsoft'}.contains(connectorId)) {
      continue;
    }
    final resource = scope['calendar_id'];
    if (resource is! String || resource.isEmpty) continue;
    choices.add(
      _CalendarGrantChoice(
        connectorId: connectorId,
        connectionId: connectionId,
        resource: resource,
        label: '$connectorId · $resource',
      ),
    );
  }
  final seen = <String>{};
  return List.unmodifiable(choices.where((choice) => seen.add(choice.value)));
}

final class _RemoteViewChoice {
  const _RemoteViewChoice({
    required this.viewId,
    required this.connectorId,
    required this.connectionId,
  });

  final String viewId;
  final String connectorId;
  final String connectionId;

  String get resource => '$viewId:$connectionId';
  String get value => '$viewId\u0000$connectorId\u0000$connectionId';
  String get label => '$viewId · $connectorId';
}

List<_RemoteViewChoice> _remoteViewChoices(List<Map<String, dynamic>> values) {
  const supported = {'mail.communication', 'work.context', 'life.logistics'};
  final choices = <_RemoteViewChoice>[];
  for (final value in values) {
    final descriptor = value['descriptor'];
    final connection = value['connection'];
    if (descriptor is! Map || connection is! Map) continue;
    final connectorId = descriptor['id'];
    final connectionId = connection['connection_id'];
    final capabilities = descriptor['capabilities'];
    if (connectorId is! String ||
        connectionId is! String ||
        capabilities is! List) {
      continue;
    }
    for (final capability in capabilities) {
      if (capability is! Map) continue;
      final viewId = capability['output_view_id'];
      if (viewId is String && supported.contains(viewId)) {
        choices.add(
          _RemoteViewChoice(
            viewId: viewId,
            connectorId: connectorId,
            connectionId: connectionId,
          ),
        );
      }
    }
  }
  final seen = <String>{};
  return List.unmodifiable(choices.where((choice) => seen.add(choice.value)));
}

const _remoteConsumers = <String>[
  'assistant',
  'floe.builtin.commitments',
  'floe.builtin.communication',
  'floe.builtin.work-context',
  'floe.builtin.life-logistics',
];

class _RemoteAuthorityEnrollment extends StatefulWidget {
  const _RemoteAuthorityEnrollment({
    required this.client,
    required this.agentVaultGateway,
  });

  final LocalServerClient client;
  final NativeAgentVaultGateway agentVaultGateway;

  @override
  State<_RemoteAuthorityEnrollment> createState() =>
      _RemoteAuthorityEnrollmentState();
}

class _RemoteAuthorityEnrollmentState
    extends State<_RemoteAuthorityEnrollment> {
  ServerConnection? connection;
  RemoteProducerInspection? inspection;
  RemoteEnrollmentStatus? enrollment;
  RemoteCalendarGrantPreview? grantPreview;
  RemoteCalendarGrantOverview? grantOverview;
  List<_CalendarGrantChoice> calendarChoices = const [];
  _CalendarGrantChoice? selectedCalendar;
  List<_RemoteViewChoice> remoteChoices = const [];
  _RemoteViewChoice? selectedRemote;
  String remoteConsumer = 'assistant';
  RemoteViewGrantPreview? remotePreview;
  RemoteViewGrantOverview? remoteOverview;
  bool busy = true;
  String? failure;

  @override
  void initState() {
    super.initState();
    unawaited(_load());
  }

  Future<void> _load() async {
    try {
      final saved = await widget.client.connection();
      if (!mounted) return;
      List<_CalendarGrantChoice> choices = const [];
      List<_RemoteViewChoice> remote = const [];
      if (saved != null) {
        try {
          final values = await widget.client.connections(saved);
          choices = _calendarChoices(values);
          remote = _remoteViewChoices(values);
        } on Object {
          choices = const [];
          remote = const [];
        }
      }
      setState(() {
        connection = saved;
        calendarChoices = choices;
        remoteChoices = remote;
        busy = false;
        failure = saved == null
            ? 'Pair this app before reviewing a server.'
            : null;
      });
    } on ServerConnectionException catch (error) {
      if (!mounted) return;
      setState(() {
        busy = false;
        failure = error.code == 'invalid_saved_connection'
            ? 'The saved pairing is unavailable. Pair this app again.'
            : 'The saved server pairing could not be loaded.';
      });
    } on Object {
      if (!mounted) return;
      setState(() {
        busy = false;
        failure = 'The saved server pairing could not be loaded.';
      });
    }
  }

  Future<Map<String, Object?>?> _route() async {
    final saved = connection ??= await widget.client.connection();
    if (saved == null) {
      if (mounted) {
        setState(() => failure = 'Pair this app before reviewing a server.');
      }
      return null;
    }
    return widget.client.authorityRoute(saved);
  }

  Future<void> _inspect() async {
    setState(() {
      busy = true;
      failure = null;
    });
    try {
      final route = await _route();
      if (route == null) return;
      final result = await widget.agentVaultGateway.inspectRemoteProducer(
        personId: connection!.personId,
        route: route,
      );
      if (mounted) setState(() => inspection = result);
    } on AgentVaultException catch (error) {
      if (mounted) setState(() => failure = _failure(error.failure));
    } on Object {
      if (mounted) {
        setState(() => failure = 'The server identity could not be inspected.');
      }
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }

  Future<void> _reviewAndEnroll() async {
    final reviewed = inspection;
    final saved = connection;
    if (reviewed == null || saved == null) return;
    setState(() {
      busy = true;
      failure = null;
    });
    try {
      final status = await widget.agentVaultGateway
          .reviewAndEnrollRemoteProducer(
            personId: saved.personId,
            route: widget.client.authorityRoute(saved),
            producer: reviewed.producer,
          );
      if (mounted) setState(() => enrollment = status);
    } on AgentVaultException catch (error) {
      if (mounted) setState(() => failure = _failure(error.failure));
    } on Object {
      if (mounted) {
        setState(
          () => failure =
              'The server identity changed or enrollment was refused.',
        );
      }
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }

  Future<void> _refreshStatus() async {
    final saved = connection;
    final current = enrollment;
    if (saved == null || current == null) return;
    setState(() {
      busy = true;
      failure = null;
    });
    try {
      final status = await widget.agentVaultGateway.remoteEnrollmentStatus(
        personId: saved.personId,
        route: widget.client.authorityRoute(saved),
        enrollmentId: current.enrollmentId,
      );
      if (mounted) setState(() => enrollment = status);
    } on AgentVaultException catch (error) {
      if (mounted) {
        setState(() {
          enrollment = null;
          failure = _failure(error.failure);
        });
      }
    } on Object {
      if (mounted) {
        setState(() {
          enrollment = null;
          failure = 'The server enrollment status could not be read.';
        });
      }
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }

  String _failure(String code) => switch (code) {
    'policy_denied' =>
      'The inspected producer identity changed. Inspect it again.',
    'vault_unavailable' =>
      'Unlock the local vault before enrolling a producer.',
    'credential_expired' => 'Pair this app again before reviewing the server.',
    _ => 'Server enrollment could not be completed.',
  };

  Future<void> _previewGrant() async {
    final saved = connection;
    final selected = selectedCalendar;
    if (saved == null || selected == null) return;
    setState(() {
      busy = true;
      failure = null;
    });
    try {
      final preview = await widget.agentVaultGateway.previewRemoteCalendarGrant(
        personId: saved.personId,
        route: widget.client.authorityRoute(saved),
        connectorId: selected.connectorId,
        connectionId: selected.connectionId,
        resource: selected.resource,
      );
      if (mounted) setState(() => grantPreview = preview);
    } on Object {
      if (mounted)
        setState(
          () => failure = 'The selected calendar source is not reviewable.',
        );
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }

  Future<void> _reviewGrant() async {
    final saved = connection;
    final preview = grantPreview;
    if (saved == null || preview == null) return;
    setState(() {
      busy = true;
      failure = null;
    });
    try {
      final overview = await widget.agentVaultGateway.reviewRemoteCalendarGrant(
        personId: saved.personId,
        route: widget.client.authorityRoute(saved),
        connectorId: preview.connectorId,
        connectionId: preview.connectionId,
        resource: preview.resource,
        expectedProducerFingerprint: preview.producer.fingerprint,
      );
      if (mounted) setState(() => grantOverview = overview);
    } on Object {
      if (mounted)
        setState(
          () => failure =
              'The source changed; inspect it again before reviewing.',
        );
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }

  Future<void> _pauseGrant() async {
    final saved = connection;
    final overview = grantOverview;
    if (saved == null || overview == null) return;
    setState(() {
      busy = true;
      failure = null;
    });
    try {
      final paused = await widget.agentVaultGateway.pauseRemoteCalendarGrant(
        personId: saved.personId,
        grantId: overview.grantId,
        expectedAuthority: overview.grantAuthority,
      );
      if (mounted) setState(() => grantOverview = paused);
    } on Object {
      if (mounted)
        setState(() => failure = 'The calendar grant could not be paused.');
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }

  Future<void> _previewRemoteGrant() async {
    final saved = connection;
    final selected = selectedRemote;
    if (saved == null || selected == null) return;
    setState(() {
      busy = true;
      failure = null;
    });
    try {
      final preview = await widget.agentVaultGateway.previewRemoteViewGrant(
        personId: saved.personId,
        route: widget.client.authorityRoute(saved),
        viewId: selected.viewId,
        connectorId: selected.connectorId,
        connectionId: selected.connectionId,
        resource: selected.resource,
        consumer: remoteConsumer,
      );
      if (mounted) setState(() => remotePreview = preview);
    } on Object {
      if (mounted) {
        setState(
          () => failure = 'The selected remote source is not reviewable.',
        );
      }
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }

  Future<void> _reviewRemoteGrant() async {
    final saved = connection;
    final selected = selectedRemote;
    final preview = remotePreview;
    if (saved == null || selected == null || preview == null) return;
    setState(() {
      busy = true;
      failure = null;
    });
    try {
      final overview = await widget.agentVaultGateway.reviewRemoteViewGrant(
        personId: saved.personId,
        route: widget.client.authorityRoute(saved),
        viewId: selected.viewId,
        connectorId: selected.connectorId,
        connectionId: selected.connectionId,
        resource: selected.resource,
        consumer: preview.consumer,
        expectedProducerFingerprint: preview.producer.fingerprint,
        expectedSourceAuthority: preview.sourceAuthority,
        expectedConnectionRevision: preview.connectionRevision,
        expectedProviderIdentity: preview.providerIdentity,
        expectedRecipient: preview.recipient,
      );
      if (mounted) setState(() => remoteOverview = overview);
    } on Object {
      if (mounted) {
        setState(
          () => failure =
              'The source changed; inspect it again before reviewing.',
        );
      }
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }

  Future<void> _pauseRemoteGrant() async {
    final saved = connection;
    final overview = remoteOverview;
    if (saved == null || overview == null) return;
    setState(() {
      busy = true;
      failure = null;
    });
    try {
      final paused = await widget.agentVaultGateway.pauseRemoteViewGrant(
        personId: saved.personId,
        grantId: overview.grantId,
        expectedAuthority: overview.grantAuthority,
      );
      if (mounted) setState(() => remoteOverview = paused);
    } on Object {
      if (mounted)
        setState(() => failure = 'The remote grant could not be paused.');
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final producer = inspection?.producer;
    final status = enrollment;
    return FloeSquircle(
      padding: const EdgeInsets.all(FloeSpace.lg),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          const Text('Server authority enrollment', style: FloeType.title),
          const SizedBox(height: FloeSpace.xs),
          Text(
            'Review this producer identity before enrolling this paired server for owner authorization. Compare its fingerprint with the authenticated server dashboard; pairing alone is not an approval.',
            style: FloeType.body.copyWith(
              color: FloePalette.neutral600,
              height: 1.5,
            ),
          ),
          const SizedBox(height: FloeSpace.lg),
          if (producer != null) ...[
            _authorityValue('Producer fingerprint', producer.fingerprint),
            _authorityValue('Producer instance', producer.instanceId),
            _authorityValue('Producer audience', producer.audience),
            _authorityValue(
              'Local owner fingerprint',
              inspection?.ownerFingerprint ?? 'Unlock vault to display',
            ),
          ],
          if (status != null) ...[
            const SizedBox(height: FloeSpace.sm),
            _authorityValue(
              'Enrollment status',
              status.active
                  ? 'Active'
                  : status.adminApproved
                  ? 'Approved; awaiting activation'
                  : 'Pending explicit server admin approval',
            ),
          ],
          const SizedBox(height: FloeSpace.lg),
          const Text('Calendar source consent', style: FloeType.controlLabel),
          const SizedBox(height: FloeSpace.xs),
          Text(
            'Enter one exact paired calendar connection and resource. Preview is read-only; review is the explicit consent that creates the grant.',
            style: FloeType.body.copyWith(color: FloePalette.neutral600),
          ),
          DropdownButtonFormField<String>(
            key: const ValueKey('remote-calendar-selection'),
            value: selectedCalendar?.value,
            decoration: const InputDecoration(labelText: 'Paired calendar'),
            items: [
              for (final choice in calendarChoices)
                DropdownMenuItem<String>(
                  value: choice.value,
                  child: Text(choice.label),
                ),
            ],
            onChanged: busy || calendarChoices.isEmpty
                ? null
                : (value) {
                    final matches = calendarChoices.where(
                      (candidate) => candidate.value == value,
                    );
                    final choice = matches.isEmpty ? null : matches.first;
                    setState(() {
                      selectedCalendar = choice;
                      grantPreview = null;
                      grantOverview = null;
                    });
                  },
          ),
          if (calendarChoices.isEmpty)
            Text(
              'No authenticated server calendar source is available for review.',
              style: FloeType.body.copyWith(color: FloePalette.neutral600),
            ),
          if (grantPreview != null) ...[
            _authorityValue(
              'Source preview',
              '${grantPreview!.connectorId}/${grantPreview!.resource}',
            ),
            _authorityValue(
              'Source execution owner',
              grantPreview!.executionOwner,
            ),
            _authorityValue(
              'Verified provider identity',
              grantPreview!.providerIdentity,
            ),
            _authorityValue('Recipient', grantPreview!.recipient),
            _authorityValue(
              'Preview producer fingerprint',
              grantPreview!.producer.fingerprint,
            ),
          ],
          if (grantOverview != null)
            _authorityValue('Grant status', grantOverview!.state),
          Wrap(
            spacing: FloeSpace.sm,
            children: [
              FloeButton.text(
                key: const ValueKey('remote-calendar-preview'),
                onPressed: busy || selectedCalendar == null
                    ? null
                    : _previewGrant,
                child: const Text('Preview source'),
              ),
              if (grantPreview != null)
                FloeButton.filled(
                  key: const ValueKey('remote-calendar-review'),
                  onPressed: busy ? null : _reviewGrant,
                  child: const Text('Review calendar grant'),
                ),
              if (grantOverview case final overview?
                  when overview.state == 'active')
                FloeButton.text(
                  key: const ValueKey('remote-calendar-pause'),
                  onPressed: busy ? null : _pauseGrant,
                  child: const Text('Pause calendar grant'),
                ),
            ],
          ),
          const SizedBox(height: FloeSpace.lg),
          const Text('Remote view consent', style: FloeType.controlLabel),
          const SizedBox(height: FloeSpace.xs),
          Text(
            'Inspect the signed account source, then explicitly review one consumer grant. Resources are account-scoped as view_id:connection_id; folders and projects are not accepted.',
            style: FloeType.body.copyWith(color: FloePalette.neutral600),
          ),
          DropdownButtonFormField<String>(
            key: const ValueKey('remote-view-selection'),
            value: selectedRemote?.value,
            decoration: const InputDecoration(labelText: 'Remote account view'),
            items: [
              for (final choice in remoteChoices)
                DropdownMenuItem<String>(
                  value: choice.value,
                  child: Text(choice.label),
                ),
            ],
            onChanged: busy || remoteChoices.isEmpty
                ? null
                : (value) {
                    final matches = remoteChoices.where(
                      (candidate) => candidate.value == value,
                    );
                    setState(() {
                      selectedRemote = matches.isEmpty ? null : matches.first;
                      remotePreview = null;
                      remoteOverview = null;
                    });
                  },
          ),
          DropdownButtonFormField<String>(
            key: const ValueKey('remote-view-consumer'),
            value: remoteConsumer,
            decoration: const InputDecoration(labelText: 'Consumer'),
            items: [
              for (final consumer in _remoteConsumers)
                DropdownMenuItem<String>(
                  value: consumer,
                  child: Text(consumer.split('.').last),
                ),
            ],
            onChanged: busy
                ? null
                : (value) {
                    if (value == null) return;
                    setState(() {
                      remoteConsumer = value;
                      remotePreview = null;
                      remoteOverview = null;
                    });
                  },
          ),
          if (selectedRemote != null)
            _authorityValue('Exact account resource', selectedRemote!.resource),
          if (remotePreview != null) ...[
            _authorityValue(
              'Verified source',
              '${remotePreview!.viewId}/${remotePreview!.connectorId}',
            ),
            _authorityValue(
              'Connection revision',
              '${remotePreview!.connectionRevision}',
            ),
            _authorityValue(
              'Source execution owner',
              remotePreview!.executionOwner,
            ),
            _authorityValue(
              'Provider identity',
              remotePreview!.providerIdentity,
            ),
            _authorityValue('Consumer', remotePreview!.consumer),
            _authorityValue(
              'Producer fingerprint',
              remotePreview!.producer.fingerprint,
            ),
          ],
          if (remoteOverview != null)
            _authorityValue('Remote grant status', remoteOverview!.state),
          Wrap(
            spacing: FloeSpace.sm,
            children: [
              FloeButton.text(
                key: const ValueKey('remote-view-preview'),
                onPressed:
                    busy || selectedRemote == null || status?.active != true
                    ? null
                    : _previewRemoteGrant,
                child: const Text('Inspect remote source'),
              ),
              if (remotePreview != null)
                FloeButton.filled(
                  key: const ValueKey('remote-view-review'),
                  onPressed: busy ? null : _reviewRemoteGrant,
                  child: const Text('Review remote grant'),
                ),
              if (remoteOverview case final overview?
                  when overview.state == 'active')
                FloeButton.text(
                  key: const ValueKey('remote-view-pause'),
                  onPressed: busy ? null : _pauseRemoteGrant,
                  child: const Text('Pause remote grant'),
                ),
            ],
          ),
          const SizedBox(height: FloeSpace.sm),
          Wrap(
            spacing: FloeSpace.sm,
            runSpacing: FloeSpace.sm,
            children: [
              FloeButton.text(
                key: const ValueKey('remote-authority-inspect'),
                onPressed: busy || connection == null ? null : _inspect,
                child: const Text('Inspect producer'),
              ),
              if (producer != null)
                FloeButton.filled(
                  key: const ValueKey('remote-authority-review'),
                  onPressed: busy ? null : _reviewAndEnroll,
                  child: const Text('Review and enroll'),
                ),
              if (status != null)
                FloeButton.text(
                  key: const ValueKey('remote-authority-refresh'),
                  onPressed: busy ? null : _refreshStatus,
                  child: const Text('Refresh status'),
                ),
            ],
          ),
          if (failure case final message?) ...[
            const SizedBox(height: FloeSpace.sm),
            Text(
              message,
              style: FloeType.body.copyWith(color: FloePalette.error600),
            ),
          ],
        ],
      ),
    );
  }

  Widget _authorityValue(String label, String value) => Padding(
    padding: const EdgeInsets.only(bottom: FloeSpace.xs),
    child: SelectableText('$label: $value', style: FloeType.body),
  );
}
