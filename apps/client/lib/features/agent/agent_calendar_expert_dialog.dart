import 'package:flutter/material.dart';

import '../../app/design_tokens.dart';
import '../../app/floe_button.dart';
import '../../app/floe_feedback.dart';
import '../../app/floe_squircle.dart';
import '../../l10n/app_localizations.dart';
import 'agent_calendar_sources.dart';
import 'agent_controller.dart';
import 'agent_vault_gateway.dart';

class AgentCalendarExpertDialog extends StatefulWidget {
  const AgentCalendarExpertDialog({
    super.key,
    required this.controller,
    this.sources,
    this.sourceChanges,
  });
  final AgentController controller;
  final AgentCalendarSources? Function()? sources;
  final Listenable? sourceChanges;

  @override
  State<AgentCalendarExpertDialog> createState() =>
      _AgentCalendarExpertDialogState();
}

class _AgentCalendarExpertDialogState extends State<AgentCalendarExpertDialog> {
  late Listenable _changes;
  final Set<String> _selected = {};
  String? _fingerprint;
  bool _confirmed = false;
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
  void didUpdateWidget(AgentCalendarExpertDialog oldWidget) {
    super.didUpdateWidget(oldWidget);
    _changes.removeListener(_changed);
    _selected.clear();
    _confirmed = false;
    _listen();
  }

  void _changed() {
    final current = _sources?.fingerprint;
    if (current != _fingerprint ||
        controller.vaultState != AgentVaultState.ready) {
      _connectionChanged = _fingerprint != null;
      _fingerprint = current;
      _selected.clear();
      _confirmed = false;
    }
    if (mounted) setState(() {});
  }

  @override
  void dispose() {
    _changes.removeListener(_changed);
    super.dispose();
  }

  bool _fresh() {
    if (_sources?.fingerprint == _fingerprint) return true;
    _changed();
    return false;
  }

  bool _alreadyInstalled(AgentCalendarSources sources) =>
      controller.calendarExperts?.views.any(
        (view) =>
            view.provider == sources.provider &&
            view.calendarIds.length == _selected.length &&
            view.calendarIds.every(_selected.contains) &&
            controller.calendarExperts!.setups.any(
              (setup) => setup.viewHandle == view.handle,
            ),
      ) ??
      false;

  Future<void> _install() async {
    final sources = _sources;
    if (!_fresh() ||
        sources == null ||
        !_confirmed ||
        _selected.isEmpty ||
        _selected.length > 4 ||
        !sources.containsScope(sources.provider, _selected) ||
        _alreadyInstalled(sources)) {
      return;
    }
    await controller.installCalendarExpert(
      provider: sources.provider,
      calendarIds: _selected.toList(),
    );
    if (mounted && controller.pendingCalendarSetup == null) {
      setState(() {
        _selected.clear();
        _confirmed = false;
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final strings = AppLocalizations.of(context);
    final ready = controller.vaultState == AgentVaultState.ready;
    final current = controller.calendarExperts;
    final sources = _sources;
    final pending = controller.pendingCalendarSetup;
    final canManage = controller.canManageCalendarExperts;
    return FloeDetailDialog(
      title: strings.agentCalendarTitle,
      loading: controller.busy,
      children: [
        Text(strings.agentCalendarBoundary),
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
              child: Text(strings.agentRegistryFailure),
            ),
          if (_connectionChanged)
            Semantics(
              liveRegion: true,
              child: Text(strings.agentCalendarChanged),
            ),
          if (sources == null) Text(strings.agentCalendarConnect),
          if (pending != null) ...[
            Text(
              strings.agentCalendarPending,
              style: const TextStyle(fontWeight: FontWeight.w600),
            ),
            ..._scopeLabels(
              strings,
              sources,
              pending.provider,
              pending.calendarIds,
            ),
            _consent(
              strings,
              canManage &&
                  (sources?.containsScope(
                        pending.provider,
                        pending.calendarIds,
                      ) ??
                      false),
            ),
            FloeButton.outlined(
              key: const ValueKey('calendar-setup-retry'),
              onPressed:
                  canManage &&
                      _confirmed &&
                      (sources?.containsScope(
                            pending.provider,
                            pending.calendarIds,
                          ) ??
                          false)
                  ? () {
                      if (_fresh() &&
                          (_sources?.containsScope(
                                pending.provider,
                                pending.calendarIds,
                              ) ??
                              false)) {
                        controller.retryCalendarSetup();
                      }
                    }
                  : null,
              child: Text(strings.agentCalendarRetry),
            ),
            if (current != null)
              FloeButton.text(
                key: const ValueKey('calendar-setup-discard'),
                onPressed: canManage
                    ? controller.discardUncommittedCalendarSetup
                    : null,
                child: Text(strings.agentCalendarDiscard),
              ),
          ] else if (sources != null && current != null) ...[
            Text(
              sources.provider == 'event_kit'
                  ? strings.agentCalendarApple
                  : strings.agentCalendarFixture,
            ),
            Text(
              strings.agentCalendarChoose,
              style: const TextStyle(fontWeight: FontWeight.w600),
            ),
            for (final source in sources.calendars)
              CheckboxListTile(
                key: ValueKey('calendar-choice-${source.id}'),
                contentPadding: EdgeInsets.zero,
                controlAffinity: ListTileControlAffinity.leading,
                title: Text(source.name),
                subtitle: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(source.id),
                    if (source.error != null)
                      Text(strings.agentCalendarSourceUnavailable),
                  ],
                ),
                value: _selected.contains(source.id),
                onChanged:
                    canManage &&
                        (_selected.contains(source.id) || _selected.length < 4)
                    ? (selected) {
                        if (!_fresh()) return;
                        setState(() {
                          if (selected == true) {
                            _selected.add(source.id);
                          } else {
                            _selected.remove(source.id);
                          }
                          _confirmed = false;
                        });
                      }
                    : null,
              ),
            Text(strings.agentCalendarSelected(_selected.length)),
            _consent(strings, canManage && _selected.isNotEmpty),
            if (_alreadyInstalled(sources) && _selected.isNotEmpty)
              Text(strings.agentCalendarExists),
            FloeButton.filled(
              key: const ValueKey('calendar-setup-install'),
              onPressed:
                  canManage &&
                      _confirmed &&
                      _selected.isNotEmpty &&
                      !_alreadyInstalled(sources)
                  ? _install
                  : null,
              child: Text(strings.agentCalendarInstall),
            ),
          ],
          if (current != null && current.views.isNotEmpty) ...[
            const SizedBox(height: FloeSpace.lg),
            Text(
              strings.agentCalendarInstalled,
              style: const TextStyle(fontWeight: FontWeight.w600),
            ),
            for (final view in current.views)
              Padding(
                padding: const EdgeInsets.only(top: FloeSpace.md),
                child: FloeSquircle(
                  size: FloeSquircleSize.md,
                  padding: const EdgeInsets.all(FloeSpace.md),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      ..._scopeLabels(
                        strings,
                        sources,
                        view.provider,
                        view.calendarIds,
                      ),
                      if (!(sources?.containsScope(
                            view.provider,
                            view.calendarIds,
                          ) ??
                          false))
                        Text(strings.agentCalendarScopeUnavailable),
                      SwitchListTile.adaptive(
                        key: ValueKey('calendar-scope-${view.handle}'),
                        contentPadding: EdgeInsets.zero,
                        title: Text(strings.agentCalendarScopeEnabled),
                        value: view.enabled,
                        onChanged:
                            canManage &&
                                pending == null &&
                                (view.enabled ||
                                    (sources?.containsScope(
                                          view.provider,
                                          view.calendarIds,
                                        ) ??
                                        false))
                            ? (enabled) {
                                if (!enabled ||
                                    (_fresh() &&
                                        (_sources?.containsScope(
                                              view.provider,
                                              view.calendarIds,
                                            ) ??
                                            false))) {
                                  controller.configureCalendarView(
                                    view.handle,
                                    enabled,
                                  );
                                }
                              }
                            : null,
                      ),
                      Text(strings.agentCalendarSeparateEnablement),
                    ],
                  ),
                ),
              ),
          ],
          const SizedBox(height: FloeSpace.base),
          FloeButton.outlined(
            key: const ValueKey('calendar-setup-refresh'),
            onPressed: canManage ? controller.loadCalendarExperts : null,
            child: Text(strings.agentRegistryRefresh),
          ),
        ],
      ],
    );
  }

  Widget _consent(AppLocalizations strings, bool enabled) => CheckboxListTile(
    key: const ValueKey('calendar-setup-consent'),
    contentPadding: EdgeInsets.zero,
    controlAffinity: ListTileControlAffinity.leading,
    title: Text(strings.agentCalendarConsent),
    value: _confirmed,
    onChanged: enabled
        ? (value) {
            if (_fresh()) setState(() => _confirmed = value == true);
          }
        : null,
  );

  List<Widget> _scopeLabels(
    AppLocalizations strings,
    AgentCalendarSources? sources,
    String provider,
    List<String> identifiers,
  ) => [
    Text(
      provider == 'event_kit'
          ? strings.agentCalendarApple
          : strings.agentCalendarFixture,
    ),
    for (final identifier in identifiers) ...[
      Text(
        sources?.provider == provider
            ? sources!.calendars
                      .where((entry) => entry.id == identifier)
                      .singleOrNull
                      ?.name ??
                  strings.agentCalendarMissingName
            : strings.agentCalendarMissingName,
      ),
      Text(identifier),
    ],
  ];
}
