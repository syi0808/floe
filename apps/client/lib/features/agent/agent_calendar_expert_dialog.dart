import 'package:flutter/material.dart';

import '../../app/design_tokens.dart';
import '../../app/floe_badge.dart';
import '../../app/floe_button.dart';
import '../../app/floe_feedback.dart';
import '../../app/floe_primitives.dart';
import '../../app/floe_selection.dart';
import '../../app/floe_squircle.dart';
import '../../l10n/app_localizations.dart';
import 'agent_calendar_experts.dart';
import 'agent_calendar_sources.dart';
import 'agent_controller.dart';
import 'agent_vault_gateway.dart';

class AgentCalendarSettings extends StatefulWidget {
  const AgentCalendarSettings({
    super.key,
    required this.controller,
    this.sources,
    this.sourceChanges,
  });

  final AgentController controller;
  final AgentCalendarSources? Function()? sources;
  final Listenable? sourceChanges;

  @override
  State<AgentCalendarSettings> createState() => _AgentCalendarSettingsState();
}

class _AgentCalendarSettingsState extends State<AgentCalendarSettings> {
  late Listenable _changes;
  final Set<String> _selected = {};
  String? _fingerprint;
  String? _editingSetupId;
  bool _editing = false;
  bool _connectionChanged = false;

  AgentController get controller => widget.controller;

  AgentCalendarSources? get _sources {
    if (controller.vaultState != AgentVaultState.ready) return null;
    final sources = widget.sources?.call();
    return sources?.personId == controller.personId && sources!.usable
        ? sources
        : null;
  }

  @override
  void initState() {
    super.initState();
    _listen();
  }

  void _listen() {
    _fingerprint = _sources?.fingerprint;
    _changes = Listenable.merge([controller, ?widget.sourceChanges]);
    _changes.addListener(_changed);
  }

  @override
  void didUpdateWidget(AgentCalendarSettings oldWidget) {
    super.didUpdateWidget(oldWidget);
    _changes.removeListener(_changed);
    _resetEditor();
    _listen();
  }

  void _changed() {
    final current = _sources?.fingerprint;
    if (current != _fingerprint ||
        controller.vaultState != AgentVaultState.ready) {
      _connectionChanged = _fingerprint != null;
      _fingerprint = current;
      _resetEditor();
    }
    if (mounted) setState(() {});
  }

  void _resetEditor() {
    _selected.clear();
    _editingSetupId = null;
    _editing = false;
  }

  bool _fresh() {
    if (_sources?.fingerprint == _fingerprint) return true;
    _changed();
    return false;
  }

  void _startSetup() => setState(() {
    _selected.clear();
    _editingSetupId = null;
    _editing = true;
  });

  Future<void> _startAdditionalSetup() async {
    final sources = _sources;
    if (!controller.canManageCalendarExperts || sources == null) return;
    final selected = <String>{};
    var saving = false;
    await showFloeDialog<void>(
      context,
      (dialogContext) => StatefulBuilder(
        builder: (context, setDialogState) {
          Future<void> save() async {
            if (saving ||
                !_fresh() ||
                selected.isEmpty ||
                selected.length > 4 ||
                !sources.containsScope(sources.provider, selected)) {
              return;
            }
            setDialogState(() => saving = true);
            await controller.installCalendarExpert(
              setupId: sources.connectionId,
              provider: sources.provider,
              calendarIds: selected.toList(),
              connectionScope: sources.connectionScope,
              connectionRevision: sources.revision,
            );
            if (!dialogContext.mounted) return;
            if (controller.calendarExpertFailure == null) {
              Navigator.pop(dialogContext);
            } else {
              setDialogState(() => saving = false);
            }
          }

          return FloeDialog(
            title: const Text('Add Calendar scope'),
            content: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Text(
                  'Choose up to four calendars for this access scope.',
                  style: FloeType.body.copyWith(
                    color: FloePalette.neutral600,
                    height: 1.4,
                  ),
                ),
                const SizedBox(height: FloeSpace.md),
                for (final source in sources.calendars)
                  FloeCheckboxTile(
                    key: ValueKey('calendar-choice-${source.id}'),
                    title: Text(source.name),
                    subtitle: source.error == null
                        ? null
                        : const Text(
                            'This calendar is temporarily unavailable.',
                          ),
                    value: selected.contains(source.id),
                    onChanged:
                        !saving &&
                            source.error == null &&
                            (selected.contains(source.id) ||
                                selected.length < 4)
                        ? (checked) => setDialogState(() {
                            if (checked == true) {
                              selected.add(source.id);
                            } else {
                              selected.remove(source.id);
                            }
                          })
                        : null,
                  ),
                const SizedBox(height: FloeSpace.xs),
                Text(
                  '${selected.length} of 4 selected',
                  style: FloeType.bodySmall.copyWith(
                    color: FloePalette.neutral600,
                  ),
                ),
                const SizedBox(height: FloeSpace.md),
                FloeSquircle(
                  size: FloeSquircleSize.md,
                  fill: FloePalette.primary50,
                  borderWidth: 0,
                  padding: const EdgeInsets.all(FloeSpace.base),
                  child: Text(
                    'Floe may read event details from only these calendars and prepare suggestions. Calendar changes remain controlled separately in Action permissions.',
                    style: FloeType.body.copyWith(height: 1.4),
                  ),
                ),
              ],
            ),
            actions: [
              FloeButton.text(
                onPressed: saving ? null : () => Navigator.pop(dialogContext),
                child: const Text('Cancel'),
              ),
              FloeButton.filled(
                key: const ValueKey('calendar-access-save'),
                onPressed: !saving && selected.isNotEmpty ? save : null,
                child: const Text('Allow access'),
              ),
            ],
          );
        },
      ),
    );
  }

  void _startChange(AgentCalendarSetupReceipt setup, AgentCalendarView view) =>
      setState(() {
        _selected
          ..clear()
          ..addAll(view.calendarIds);
        _editingSetupId = setup.setupId;
        _editing = true;
      });

  Future<void> _save() async {
    final sources = _sources;
    if (!_fresh() ||
        sources == null ||
        _selected.isEmpty ||
        _selected.length > 4 ||
        !sources.containsScope(sources.provider, _selected)) {
      return;
    }
    final setupId = _editingSetupId;
    if (setupId == null) {
      await controller.installCalendarExpert(
        setupId: sources.connectionId,
        provider: sources.provider,
        calendarIds: _selected.toList(),
        connectionScope: sources.connectionScope,
        connectionRevision: sources.revision,
      );
    } else {
      await controller.changeCalendarAccessScope(
        setupId: setupId,
        provider: sources.provider,
        calendarIds: _selected.toList(),
        connectionScope: sources.connectionScope,
        connectionRevision: sources.revision,
      );
    }
    if (mounted && controller.calendarExpertFailure == null) {
      setState(_resetEditor);
    }
  }

  Future<void> _remove(AgentCalendarSetupReceipt setup) async {
    final confirmed = await showFloeDialog<bool>(
      context,
      (context) => FloeDialog(
        title: const Text('Remove Calendar access?'),
        content: const Text(
          'Floe will stop using these calendars in new conversations. The source connection and action history are not deleted.',
        ),
        actions: [
          FloeButton.text(
            onPressed: () => Navigator.pop(context, false),
            child: const Text('Cancel'),
          ),
          FloeButton.filled(
            key: const ValueKey('calendar-access-remove-confirm'),
            onPressed: () => Navigator.pop(context, true),
            child: const Text('Remove access'),
          ),
        ],
      ),
    );
    if (confirmed == true) await controller.removeCalendarAccess(setup.setupId);
  }

  @override
  void dispose() {
    _changes.removeListener(_changed);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final strings = AppLocalizations.of(context);
    final ready = controller.vaultState == AgentVaultState.ready;
    final current = controller.calendarExperts;
    final pending = controller.pendingCalendarSetup;
    final sources = _sources;
    final canManage = controller.canManageCalendarExperts;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        const Text('Data Floe can use', style: FloeType.title),
        const SizedBox(height: FloeSpace.xs),
        Text(
          'Access is granted to a specific source and scope. It does not allow Floe to change external data automatically.',
          style: FloeType.body.copyWith(color: FloePalette.neutral600),
        ),
        const SizedBox(height: FloeSpace.base),
        if (!ready)
          Text(
            controller.vaultState == AgentVaultState.unavailable
                ? strings.agentStorageUnavailable
                : strings.agentStorageLocked,
          )
        else ...[
          if (controller.calendarExpertFailure != null)
            Semantics(
              liveRegion: true,
              child: Text(
                'Floe could not confirm this access change. Refresh before trying again.',
              ),
            ),
          if (_connectionChanged)
            const Text(
              'Your Calendar connection changed. Review its scope before making another change.',
            ),
          if (pending != null) _pendingChange(pending, sources, canManage),
          if (pending == null && current != null)
            for (final setup in current.setups)
              Padding(
                padding: const EdgeInsets.only(bottom: FloeSpace.md),
                child: _accessCard(
                  setup,
                  current.views.singleWhere(
                    (entry) => entry.handle == setup.viewHandle,
                  ),
                  current,
                  sources,
                  canManage,
                ),
              ),
          if (pending == null && !_editing && (current?.setups.isEmpty ?? true))
            _emptyCalendarCard(sources, canManage),
          if (pending == null && _editing) _editor(sources, canManage),
          if (pending == null &&
              !_editing &&
              (current?.setups.isNotEmpty ?? false))
            Align(
              alignment: Alignment.centerLeft,
              child: FloeButton.outlined(
                key: const ValueKey('calendar-access-add'),
                onPressed: canManage && sources != null
                    ? _startAdditionalSetup
                    : null,
                child: const Text('Add another Calendar scope'),
              ),
            ),
          if (pending != null || controller.calendarExpertFailure != null) ...[
            const SizedBox(height: FloeSpace.sm),
            Align(
              alignment: Alignment.centerLeft,
              child: FloeButton.text(
                key: const ValueKey('calendar-access-refresh'),
                onPressed: canManage ? controller.loadCalendarExperts : null,
                child: const Text('Refresh access'),
              ),
            ),
          ],
        ],
      ],
    );
  }

  Widget _emptyCalendarCard(
    AgentCalendarSources? sources,
    bool canManage,
  ) => FloeSquircle(
    size: FloeSquircleSize.md,
    fill: FloePalette.neutral50,
    borderWidth: 0,
    padding: const EdgeInsets.all(FloeSpace.base),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text(
          'Calendars',
          style: FloeType.controlLabel.copyWith(fontWeight: FontWeight.w600),
        ),
        const SizedBox(height: FloeSpace.xs),
        Text(
          sources == null
              ? 'Connect and choose calendars in Connections first.'
              : '${_calendarProviderName(sources.provider)} is connected. Let Floe read selected event details and prepare scheduling suggestions.',
          style: FloeType.body.copyWith(
            color: FloePalette.neutral600,
            height: 1.4,
          ),
        ),
        const SizedBox(height: FloeSpace.md),
        Align(
          alignment: Alignment.centerLeft,
          child: FloeButton.filled(
            key: const ValueKey('calendar-access-setup'),
            onPressed: canManage && sources != null ? _startSetup : null,
            child: const Text('Set up Calendar access'),
          ),
        ),
      ],
    ),
  );

  Widget _pendingChange(
    AgentCalendarSetup pending,
    AgentCalendarSources? sources,
    bool canManage,
  ) => FloeSquircle(
    size: FloeSquircleSize.md,
    fill: FloePalette.warning50,
    borderWidth: 0,
    padding: const EdgeInsets.all(FloeSpace.base),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        const Text(
          'Calendar access needs attention',
          style: FloeType.controlLabel,
        ),
        const SizedBox(height: FloeSpace.xs),
        const Text(
          'Floe could not confirm whether the requested access was saved. Refresh first, or retry only this exact request.',
        ),
        const SizedBox(height: FloeSpace.md),
        Wrap(
          spacing: FloeSpace.sm,
          children: [
            FloeButton.outlined(
              key: const ValueKey('calendar-access-retry'),
              onPressed:
                  canManage &&
                      (sources?.containsScope(
                            pending.provider,
                            pending.calendarIds,
                          ) ??
                          false)
                  ? controller.retryCalendarSetup
                  : null,
              child: const Text('Try exact request again'),
            ),
            FloeButton.text(
              key: const ValueKey('calendar-access-discard'),
              onPressed: canManage
                  ? controller.discardUncommittedCalendarSetup
                  : null,
              child: const Text('Cancel unconfirmed change'),
            ),
          ],
        ),
      ],
    ),
  );

  Widget _accessCard(
    AgentCalendarSetupReceipt setup,
    AgentCalendarView view,
    AgentCalendarExperts current,
    AgentCalendarSources? sources,
    bool canManage,
  ) {
    final connected =
        sources?.containsScope(view.provider, view.calendarIds) ?? false;
    final active = current.accessEnabled(setup);
    final scopeNames = _scopeNames(sources, view);
    final status = !connected
        ? 'Needs attention'
        : active
        ? 'Active'
        : 'Paused';
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
                  'Calendars',
                  style: FloeType.controlLabel.copyWith(
                    fontWeight: FontWeight.w600,
                  ),
                ),
              ),
              FloeBadge(
                label: status,
                tone: !connected
                    ? FloeBadgeTone.warning
                    : active
                    ? FloeBadgeTone.success
                    : FloeBadgeTone.neutral,
              ),
            ],
          ),
          const SizedBox(height: FloeSpace.xs),
          Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Expanded(
                child: Text(
                  scopeNames.join(', '),
                  style: FloeType.body.copyWith(color: FloePalette.neutral600),
                ),
              ),
              const SizedBox(width: FloeSpace.sm),
              FloeBadge(
                key: ValueKey('calendar-scope-count-${setup.setupId}'),
                label:
                    '${scopeNames.length} ${scopeNames.length == 1 ? 'calendar' : 'calendars'}',
                compact: true,
              ),
            ],
          ),
          const SizedBox(height: FloeSpace.xs),
          const Text('Read event details and prepare suggestions.'),
          const SizedBox(height: FloeSpace.md),
          Wrap(
            spacing: FloeSpace.sm,
            runSpacing: FloeSpace.sm,
            children: [
              FloeButton.outlined(
                key: ValueKey('calendar-access-change-${setup.setupId}'),
                onPressed: canManage && connected
                    ? () => _startChange(setup, view)
                    : null,
                child: const Text('Change'),
              ),
              FloeButton.outlined(
                key: ValueKey('calendar-access-toggle-${setup.setupId}'),
                onPressed: canManage && (active || connected)
                    ? () => controller.setCalendarAccessEnabled(
                        setup.setupId,
                        !active,
                      )
                    : null,
                child: Text(active ? 'Pause' : 'Resume'),
              ),
              FloeButton.text(
                key: ValueKey('calendar-access-remove-${setup.setupId}'),
                onPressed: canManage ? () => _remove(setup) : null,
                child: const Text('Remove access'),
              ),
            ],
          ),
        ],
      ),
    );
  }

  Widget _editor(AgentCalendarSources? sources, bool canManage) => Column(
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      Text(
        _editingSetupId == null ? 'Choose calendars' : 'Change Calendar scope',
        style: FloeType.controlLabel,
      ),
      const SizedBox(height: FloeSpace.sm),
      if (sources != null)
        for (final source in sources.calendars)
          FloeCheckboxTile(
            key: ValueKey('calendar-choice-${source.id}'),
            title: Text(source.name),
            subtitle: source.error == null
                ? null
                : const Text('This calendar is temporarily unavailable.'),
            value: _selected.contains(source.id),
            onChanged:
                canManage &&
                    source.error == null &&
                    (_selected.contains(source.id) || _selected.length < 4)
                ? (selected) {
                    if (!_fresh()) return;
                    setState(() {
                      if (selected == true) {
                        _selected.add(source.id);
                      } else {
                        _selected.remove(source.id);
                      }
                    });
                  }
                : null,
          ),
      Text('${_selected.length} of 4 selected'),
      const SizedBox(height: FloeSpace.md),
      FloeSquircle(
        size: FloeSquircleSize.md,
        fill: FloePalette.primary50,
        borderWidth: 0,
        padding: EdgeInsets.all(FloeSpace.base),
        child: Text(
          'Floe may read event details from only these calendars and prepare suggestions. Calendar changes remain controlled separately in Action permissions.',
          style: FloeType.body.copyWith(height: 1.4),
        ),
      ),
      const SizedBox(height: FloeSpace.md),
      Wrap(
        spacing: FloeSpace.sm,
        children: [
          FloeButton.filled(
            key: const ValueKey('calendar-access-save'),
            onPressed: canManage && _selected.isNotEmpty ? _save : null,
            child: Text(
              _editingSetupId == null ? 'Allow access' : 'Save changes',
            ),
          ),
          FloeButton.text(
            key: const ValueKey('calendar-access-cancel'),
            onPressed: canManage ? () => setState(_resetEditor) : null,
            child: const Text('Cancel'),
          ),
        ],
      ),
    ],
  );

  List<String> _scopeNames(
    AgentCalendarSources? sources,
    AgentCalendarView view,
  ) => [
    for (final identifier in view.calendarIds)
      sources?.provider == view.provider
          ? sources!.calendars
                    .where((entry) => entry.id == identifier)
                    .singleOrNull
                    ?.name ??
                'Unavailable calendar'
          : 'Unavailable calendar',
  ];
}

String _calendarProviderName(String provider) => switch (provider) {
  'event_kit' => 'Apple Calendar',
  'google_calendar' => 'Google Calendar',
  'microsoft_calendar' => 'Microsoft Calendar',
  'android' => 'Android Calendar',
  'fixture' => 'Demo Calendar',
  _ => throw ArgumentError.value(provider, 'provider'),
};
