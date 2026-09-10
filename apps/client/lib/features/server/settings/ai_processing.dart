part of '../settings_screen.dart';

class _AiProcessing extends StatefulWidget {
  const _AiProcessing({required this.client});
  final LocalServerClient? client;

  @override
  State<_AiProcessing> createState() => _AiProcessingState();
}

class _AiProcessingState extends State<_AiProcessing> {
  ServerConnection? connection;
  Map<InferencePurpose, InferencePurposeAvailability>? purposes;
  List<InferenceAuditRecord>? activity;
  bool loading = true;
  String? failure;

  Set<String> get _externalRecipients =>
      purposes?.values
          .map((purpose) => purpose.recipient)
          .whereType<String>()
          .toSet() ??
      const {};

  bool get _externalConsentActive {
    final saved = connection;
    final recipients = _externalRecipients;
    return saved != null &&
        recipients.isNotEmpty &&
        recipients.every(saved.coversExternalRecipient);
  }

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    try {
      final saved = await widget.client?.connection();
      if (!mounted) return;
      Map<InferencePurpose, InferencePurposeAvailability>? availability;
      List<InferenceAuditRecord>? recentActivity;
      String? routeFailure;
      if (saved != null && widget.client != null) {
        try {
          availability = await widget.client!.purposes(saved);
        } on Object {
          routeFailure = 'Paired, but route availability could not be checked.';
        }
        try {
          recentActivity = await widget.client!.privacyActivity(saved);
        } on Object {
          routeFailure ??=
              'Paired, but recent processing activity is unavailable.';
        }
      }
      if (!mounted) return;
      setState(() {
        connection = saved;
        purposes = availability;
        activity = recentActivity;
        loading = false;
        failure = routeFailure;
      });
    } on Object {
      if (mounted) {
        setState(() {
          loading = false;
          failure = 'Processing settings could not be loaded.';
        });
      }
    }
  }

  Future<void> _setExternal(bool enabled) async {
    final current = connection;
    final client = widget.client;
    if (current == null || client == null) return;
    setState(() => loading = true);
    try {
      final updated = current.withExternalConsent(
        enabled,
        recipients: _externalRecipients,
      );
      await client.save(updated);
      if (mounted) {
        setState(() {
          connection = updated;
          failure = null;
        });
      }
    } on Object {
      if (mounted) {
        setState(
          () => failure = 'External processing consent could not be saved.',
        );
      }
    } finally {
      if (mounted) setState(() => loading = false);
    }
  }

  @override
  Widget build(BuildContext context) => FloeSquircle(
    padding: const EdgeInsets.all(FloeSpace.lg),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        const Text('AI processing', style: FloeType.title),
        const SizedBox(height: FloeSpace.xs),
        Text(
          'Floe chooses a permitted route for each task. Conversations do not select a model.',
          style: FloeType.body.copyWith(
            color: FloePalette.neutral600,
            height: 1.5,
          ),
        ),
        const SizedBox(height: FloeSpace.lg),
        Text(
          'Processing locations',
          style: FloeType.controlLabel.copyWith(fontWeight: FontWeight.w600),
        ),
        const SizedBox(height: FloeSpace.sm),
        _ProcessingGroup(
          children: [
            const _ProcessingRow(
              title: 'On this device',
              detail:
                  'Local data preparation and available local intelligence.',
              status: 'Preferred',
            ),
            const FloeDivider(height: FloeSpace.lg),
            _ProcessingRow(
              title: 'On your Floe Server',
              detail: connection == null
                  ? 'Pair a server in Floe Server settings to add assisted routes.'
                  : 'The paired server receives only the context required for a task.',
              status: connection == null ? 'Not paired' : 'Paired',
            ),
          ],
        ),
        if (connection != null && purposes != null) ...[
          const SizedBox(height: FloeSpace.lg),
          Text(
            'Task routes',
            style: FloeType.controlLabel.copyWith(fontWeight: FontWeight.w600),
          ),
          const SizedBox(height: FloeSpace.xs),
          Text(
            'Floe selects among these routes based on the work and your consent.',
            style: FloeType.body.copyWith(
              color: FloePalette.neutral600,
              height: 1.5,
            ),
          ),
          const SizedBox(height: FloeSpace.sm),
          _ProcessingGroup(
            children: [
              for (final purpose in InferencePurpose.values) ...[
                _ProcessingRow(
                  title: switch (purpose) {
                    InferencePurpose.quickResponse => 'Quick responses',
                    InferencePurpose.everydayAssistance =>
                      'Everyday assistance',
                    InferencePurpose.deepWork => 'Deep work',
                  },
                  detail: purposes![purpose]!.recipient != null
                      ? 'External recipient: ${purposes![purpose]!.recipient}. Selected automatically when needed.'
                      : 'Processed on your Floe Server when selected automatically.',
                  status: !purposes![purpose]!.available
                      ? 'Unavailable'
                      : purposes![purpose]!.requiresExternalConsent &&
                            !connection!.coversExternalRecipient(
                              purposes![purpose]!.recipient,
                            )
                      ? 'Needs consent'
                      : 'Available',
                ),
                if (purpose != InferencePurpose.values.last)
                  const FloeDivider(height: FloeSpace.lg),
              ],
            ],
          ),
        ],
        if (connection != null) ...[
          const FloeDivider(height: FloeSpace.xl),
          FloeSwitchTile(
            key: const ValueKey('external-model-consent'),
            title: 'Allow external model providers',
            subtitle: 'Allows the paired server to send the minimum required context to a provider it manages. Turn this off to withdraw consent without disconnecting the server.',
            value: _externalConsentActive,
            onChanged: loading || _externalRecipients.isEmpty
                ? null
                : _setExternal,
          ),
        ],
        if (failure case final message?) ...[
          const SizedBox(height: FloeSpace.sm),
          Text(
            message,
            style: FloeType.body.copyWith(color: FloePalette.error600),
          ),
        ],
        if (connection != null && activity != null) ...[
          const FloeDivider(height: FloeSpace.xl),
          Align(
            alignment: Alignment.centerLeft,
            child: FloeButton.text(
              key: const ValueKey('processing-activity-open'),
              onPressed: _showActivity,
              child: Text(
                activity!.isEmpty
                    ? 'View recent data use'
                    : 'View recent data use (${activity!.length})',
              ),
            ),
          ),
        ],
      ],
    ),
  );

  Future<void> _showActivity() => showFloeDialog<void>(
    context,
    (dialogContext) => FloeDialog(
      title: const Text('Recent data use'),
      maxWidth: 640,
      content: activity!.isEmpty
          ? Text(
              'No server model processing has been recorded since the server started.',
              style: FloeType.body.copyWith(
                color: FloePalette.neutral600,
                height: 1.5,
              ),
            )
          : _ProcessingGroup(
              children: [
                for (final record in activity!.take(5)) ...[
                  _ProcessingRow(
                    title: switch (record.purpose) {
                      'quick_response' => 'Quick response',
                      'everyday_assistance' => 'Everyday assistance',
                      'deep_work' => 'Deep work',
                      _ => 'Assisted processing',
                    },
                    detail:
                        '${record.dataClasses.join(', ')} · ${record.placement == 'remote' ? 'External provider' : 'Floe Server'} · Trace ${record.traceId.substring(0, 8)}',
                    status: record.outcome == 'completed'
                        ? 'Completed'
                        : 'Failed',
                  ),
                  if (record != activity!.take(5).last)
                    const FloeDivider(height: FloeSpace.lg),
                ],
              ],
            ),
      actions: [
        FloeButton.filled(
          key: const ValueKey('processing-activity-close'),
          onPressed: () => Navigator.pop(dialogContext),
          child: const Text('Done'),
        ),
      ],
    ),
  );
}

class _ProcessingGroup extends StatelessWidget {
  const _ProcessingGroup({required this.children});

  final List<Widget> children;

  @override
  Widget build(BuildContext context) => FloeSquircle(
    size: FloeSquircleSize.md,
    fill: FloePalette.neutral50,
    borderWidth: 0,
    padding: const EdgeInsets.all(FloeSpace.base),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: children,
    ),
  );
}

class _ProcessingRow extends StatelessWidget {
  const _ProcessingRow({
    required this.title,
    required this.detail,
    required this.status,
  });
  final String title;
  final String detail;
  final String status;

  @override
  Widget build(BuildContext context) => Row(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      Expanded(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(title, style: FloeType.controlLabel),
            const SizedBox(height: FloeSpace.xxs),
            Text(
              detail,
              style: FloeType.body.copyWith(
                color: FloePalette.neutral600,
                height: 1.4,
              ),
            ),
          ],
        ),
      ),
      const SizedBox(width: FloeSpace.md),
      FloeBadge(label: status, tone: _statusTone(status)),
    ],
  );
}
