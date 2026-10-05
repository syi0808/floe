import 'package:intl/intl.dart';
import 'package:floe_client/features/day/domain/day_models.dart';

import 'calendar_system_access_card.dart';
import 'calendar_collection_summary.dart';
import 'service_presentation.dart';

import 'package:flutter/material.dart';
import 'package:floe_client/l10n/app_localizations.dart';

import 'resource_groups.dart';

import 'package:lucide_icons_flutter/lucide_icons.dart';
import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_badge.dart';
import 'package:floe_client/app/floe_button.dart';
import 'package:floe_client/app/floe_feedback.dart';
import 'package:floe_client/app/floe_primitives.dart';
import 'package:floe_client/app/floe_selection.dart';
import 'package:floe_client/app/floe_squircle.dart';
import 'package:floe_client/features/connections/domain/connection_models.dart';
import 'package:floe_client/features/connections/presentation/connections_controller.dart';

/// Service presentation; reviewed references and authority stay in Connections.
final class SourceConnectionPanel extends StatefulWidget {
  const SourceConnectionPanel({
    super.key,
    required this.controller,
    required this.source,
    this.integration,
    this.calendarCoverage,
  });
  final ConnectionsController controller;
  final SourceSummary source;
  final IntegrationSummary? integration;
  final DayCalendarCoverage? calendarCoverage;
  @override
  State<SourceConnectionPanel> createState() => _SourceConnectionPanelState();
}

final class _SourceConnectionPanelState extends State<SourceConnectionPanel> {
  bool opening = false;

  @override
  void initState() {
    super.initState();
    _offerInitialSelection();
  }

  @override
  void didUpdateWidget(SourceConnectionPanel oldWidget) {
    super.didUpdateWidget(oldWidget);
    _offerInitialSelection();
  }

  void _offerInitialSelection() {
    if (widget.source.selectedResources.isNotEmpty ||
        !(widget.source.availability == 'available' ||
            (widget.controller.operation?.state == 'completed' &&
                widget.controller.operation?.source?.sourceRef ==
                    widget.source.sourceRef)) ||
        !{'calendar', 'contacts'}.contains(widget.integration?.category) ||
        !widget.source.allowedActions.contains('configure'))
      return;
    final source = widget.source;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted ||
          busy ||
          widget.source.sourceRef != source.sourceRef ||
          widget.source.revision != source.revision ||
          widget.source.selectedResources.isNotEmpty)
        return;
      if (widget.controller.claimInitialResourceSelection(widget.source)) {
        _chooseResources();
      }
    });
  }

  ServicePresentation get presentation => widget.integration != null
      ? ServicePresentation.forIntegration(
          widget.integration!,
          AppLocalizations.of(context),
        )
      : ServicePresentation.forSource(widget.source);
  String get name => presentation.name;
  bool get calendar => widget.integration?.category == 'calendar';
  bool get busy => opening || widget.controller.busy;

  DayCalendarSourceCoverage? get _calendarCoverage => widget
      .calendarCoverage
      ?.sources
      .where((value) => value.sourceRef == widget.source.sourceRef.value)
      .singleOrNull;

  String _resourceDescription(ResourceSummary resource) {
    if (!calendar) return resource.label;
    // Identity joins only. Historical read evidence never implies current access.
    final coverage = _calendarCoverage?.resources
        .where((value) => value.resourceRef == resource.resourceRef.value)
        .singleOrNull;
    final strings = AppLocalizations.of(context);
    if (coverage == null) {
      return '${resource.label}\n${strings.collectionStatusUnavailable}';
    }
    final lines = <String>[resource.label];
    if (coverage.failure case final failure?) {
      lines.add(switch (failure) {
        DayCalendarFailure.source_changed => strings.collectionSourceChanged,
        DayCalendarFailure.source_fenced => strings.collectionSourcePaused,
        _ =>
          coverage.lastSuccessAt == null
              ? strings.collectionFailedWithoutSavedData
              : strings.couldNotCollectEventsShowingTheLast,
      });
    }
    if (coverage.lastSuccessAt case final success?) {
      final timestamp = DateFormat.yMMMd(strings.localeName)
          .add_jm()
          .format(success.toLocal());
      lines.add('${strings.lastSuccessfulRead}: $timestamp');
    } else {
      lines.add(strings.notCollectedYet);
    }
    return lines.join('\n');
  }

  Future<void> _chooseResources() async {
    if (busy) return;
    final source = widget.source;
    setState(() => opening = true);
    final review = await widget.controller.prepareSource(source);
    if (!mounted) return;
    setState(() => opening = false);
    if (widget.source.sourceRef != source.sourceRef ||
        widget.source.revision != source.revision ||
        review == null ||
        review.sourceRef != source.sourceRef ||
        review.sourceRevision != source.revision)
      return;
    await showFloeDialog<void>(
      context,
      (_) => _ResourceDialog(
        controller: widget.controller,
        review: review,
        title: calendar ? 'Choose calendars' : 'Choose resources',
      ),
    );
  }

  Future<void> _useWithFloe(bool enabled) async {
    if (busy) return;
    final source = widget.source;
    if (!enabled) {
      await widget.controller.pauseObserve(source);
      return;
    }
    await showFloeDialog<void>(
      context,
      (_) => _AccessDialog(
        controller: widget.controller,
        source: source,
        name: name,
      ),
    );
  }

  Future<void> _disconnect() async {
    if (busy) return;
    final source = widget.source;
    final confirmed = await showFloeDialog<bool>(
      context,
      (context) => FloeDialog(
        title: Text('Disconnect $name?'),
        content: const Text(
          'Floe will stop using this source. Data in the original service is not deleted.',
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
    if (confirmed == true &&
        mounted &&
        widget.source.sourceRef == source.sourceRef)
      await widget.controller.disconnectSource(source);
  }

  @override
  Widget build(BuildContext context) {
    final source = widget.source;
    final enabled = source.observeState == 'enabled';
    return FloeCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          ServiceDetailHeader(
            presentation: presentation,
            status: FloeBadge(
              label: switch (source.availability) {
                'available' => 'Connected',
                'permission_required' => 'Finish setup',
                'identity_changed' => 'Reconnect needed',
                'disconnected' => 'Disconnected',
                _ => 'Unavailable',
              },
              tone: source.availability == 'available'
                  ? FloeBadgeTone.success
                  : FloeBadgeTone.warning,
            ),
          ),
          const SizedBox(height: FloeSpace.lg),
          if (widget.controller.configurationNoticeFor(source.sourceRef)
              case final notice?) ...[
            FloeInfoNote(
              text: notice.hadSelection
                  ? AppLocalizations.of(context)
                        .configurationNotSavedNeedsReview
                  : AppLocalizations.of(context)
                        .configurationNotSavedInitialSelection,
            ),
            const SizedBox(height: FloeSpace.lg),
          ],
          if (CalendarSystemAccessCard.appliesTo(widget.integration)) ...[
            CalendarSystemAccessCard(
              gateway: widget.controller.calendarSystemAccess,
              enabled: !busy,
            ),
            const SizedBox(height: FloeSpace.lg),
          ],
          Text(
            calendar
                ? AppLocalizations.of(context)
                      .connectedCalendarCount(source.selectedResources.length)
                : 'Selected resources · ${source.selectedResources.length}',
            style: calendar ? FloeType.titleLarge : FloeType.title,
          ),
          const SizedBox(height: FloeSpace.sm),
          ConnectionResourceGroups(
            columns: calendar,
            ungroupedLabel: calendar
                ? AppLocalizations.of(context).calendarAccountFallback
                : 'Resources',
            items: [
              for (final resource in source.selectedResources)
                (
                  group: resource.group,
                  child: Padding(
                    padding: const EdgeInsets.symmetric(vertical: FloeSpace.sm),
                    child: FloeIconText(
                      icon: Icon(
                        calendar ? LucideIcons.calendar : LucideIcons.check,
                        size: 15,
                        color: FloePalette.primary500,
                      ),
                      text: _resourceDescription(resource),
                      gap: 10,
                      style: FloeType.bodySmall.copyWith(height: 1.7),
                    ),
                  ),
                ),
            ],
          ),
          if (source.selectedResources.isEmpty)
            const Text('Choose what you want to connect.'),
          if (calendar && source.selectedResources.isNotEmpty) ...[
            const SizedBox(height: 28),
            const FloeDivider(),
            const SizedBox(height: 20),
            CalendarCollectionSummary(
              coverage: _calendarCoverage,
              selectedResourceRefs: source.selectedResources
                  .map((resource) => resource.resourceRef.value)
                  .toList(growable: false),
            ),
          ],
          const SizedBox(height: FloeSpace.base),
          Wrap(
            spacing: FloeSpace.sm,
            runSpacing: FloeSpace.sm,
            children: [
              if (source.allowedActions.contains('configure'))
                FloeButton.outlined(
                  onPressed: busy ? null : _chooseResources,
                  child: Text(
                    calendar ? 'Choose calendars' : 'Choose resources',
                  ),
                ),
              if (source.allowedActions.contains('disconnect'))
                FloeButton.text(
                  onPressed: busy ? null : _disconnect,
                  child: const Text('Disconnect'),
                ),
            ],
          ),
          const SizedBox(height: FloeSpace.lg),
          FloeSwitchTile(
            value: enabled,
            onChanged:
                busy ||
                    (enabled
                        ? !source.allowedActions.contains('pause_observe')
                        : !source.allowedActions.contains(
                            'prepare_observe_review',
                          ))
                ? null
                : _useWithFloe,
            title: 'Use with Floe',
            subtitle: enabled
                ? 'Floe can use the data you have allowed.'
                : source.observeState == 'review_required'
                ? 'Review access after changing this connection.'
                : 'Choose whether Floe may use this source to assist you.',
          ),
          if (enabled &&
              source.allowedActions.contains('prepare_observe_review'))
            FloeButton.text(
              onPressed: busy ? null : () => _useWithFloe(true),
              child: const Text('Review access'),
            ),
        ],
      ),
    );
  }
}

final class _ResourceDialog extends StatefulWidget {
  const _ResourceDialog({
    required this.controller,
    required this.review,
    required this.title,
  });
  final ConnectionsController controller;
  final SourceReview review;
  final String title;
  @override
  State<_ResourceDialog> createState() => _ResourceDialogState();
}

final class _ResourceDialogState extends State<_ResourceDialog> {
  late final selected = widget.review.permittedChoices
      .where((choice) => choice.selected)
      .map((choice) => choice.resourceRef)
      .toSet();
  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: widget.controller,
    builder: (context, _) {
      final expired = !widget.review.expiresAt.isAfter(DateTime.now().toUtc());
      return FloeDialog(
        title: Text(widget.title),
        content: SizedBox(
          width: 420,
          child: SingleChildScrollView(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                if (widget.review.permittedChoices.isEmpty)
                  const Text(
                    'No resources are available yet. Add one in the original service, then reopen this selection.',
                  ),
                for (final choice in widget.review.permittedChoices)
                  FloeCheckboxTile(
                    title: Text(
                      choice.group == null
                          ? choice.label
                          : '${choice.group!.label} · ${choice.label}',
                    ),
                    value: selected.contains(choice.resourceRef),
                    onChanged: widget.controller.busy || expired
                        ? null
                        : (checked) => setState(() {
                            if (checked == true) {
                              selected.add(choice.resourceRef);
                            } else {
                              selected.remove(choice.resourceRef);
                            }
                          }),
                  ),
                const SizedBox(height: FloeSpace.sm),
                const Text(
                  'Changing this selection requires reviewing Floe access again.',
                ),
                if (expired)
                  const Text(
                    'This selection has expired. Close and open it again.',
                  ),
                if (widget.controller.failure case final failure?)
                  Text(failure),
              ],
            ),
          ),
        ),
        actions: [
          FloeButton.text(
            onPressed: () => Navigator.pop(context),
            child: const Text('Cancel'),
          ),
          FloeButton.filled(
            onPressed:
                widget.controller.busy ||
                    expired ||
                    selected.isEmpty ||
                    !widget.review.allowedActions.contains('configure')
                ? null
                : () async {
                    final confirmed = await widget.controller.configureSource(
                      widget.review,
                      selected.toList(growable: false),
                    );
                    if (context.mounted && confirmed != null) {
                      Navigator.pop(context);
                    }
                  },
            child: const Text('Save'),
          ),
        ],
      );
    },
  );
}

final class _AccessDialog extends StatefulWidget {
  const _AccessDialog({
    required this.controller,
    required this.source,
    required this.name,
  });
  final ConnectionsController controller;
  final SourceSummary source;
  final String name;
  @override
  State<_AccessDialog> createState() => _AccessDialogState();
}

final class _AccessDialogState extends State<_AccessDialog> {
  SourceProcessing processing = SourceProcessing.gatewayAllowed;
  ObserveReview? review;
  bool preparing = false;
  int generation = 0;
  @override
  void initState() {
    super.initState();
    _prepare();
  }

  Future<void> _prepare() async {
    final current = ++generation;
    setState(() {
      preparing = true;
      review = null;
    });
    final result = await widget.controller.prepareObserve(
      widget.source,
      processing,
    );
    if (!mounted || current != generation) return;
    setState(() {
      preparing = false;
      if (result != null &&
          result.sourceRef == widget.source.sourceRef &&
          result.sourceRevision == widget.source.revision &&
          result.processingDisclosure.views.every(
            (view) => view.requested?.processing == processing,
          ))
        review = result;
    });
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: widget.controller,
    builder: (context, _) {
      final value = review;
      final expired =
          value != null && !value.expiresAt.isAfter(DateTime.now().toUtc());
      final busy = preparing || widget.controller.busy;
      return FloeDialog(
        title: Text('Use ${widget.name} with Floe'),
        content: SizedBox(
          width: 460,
          child: SingleChildScrollView(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                const Text('Choose where Floe may process the data you allow.'),
                const SizedBox(height: FloeSpace.sm),
                FloeRadioGroup<SourceProcessing>(
                  value: processing,
                  onChanged: (choice) {
                    if (!busy && choice != null) {
                      setState(() => processing = choice);
                      _prepare();
                    }
                  },
                  child: Column(
                    children: [
                      FloeRadioTile(
                        value: SourceProcessing.gatewayAllowed,
                        enabled: !busy,
                        title: const Text('This device and my server'),
                      ),
                      FloeRadioTile(
                        value: SourceProcessing.deviceOnly,
                        enabled: !busy,
                        title: const Text('This device only'),
                      ),
                    ],
                  ),
                ),
                if (preparing) const Text('Preparing access details…'),
                if (value != null) ...[
                  for (final member in value.displayMembers) Text(member),
                  for (final view in value.processingDisclosure.views)
                    Padding(
                      padding: const EdgeInsets.only(top: FloeSpace.sm),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            view.isDerivedHealth
                                ? 'Derived wellbeing summary'
                                : view.viewId,
                            style: FloeType.controlLabel,
                          ),
                          Text(
                            '${view.dataClassLabel} data: ${view.dataCategories.join(', ')}',
                          ),
                          Text(
                            'Current access: ${view.current?.label ?? 'Not allowed'}',
                          ),
                          Text('Allow: ${view.requested!.label}'),
                        ],
                      ),
                    ),
                  if (value.processingDisclosure.views.any(
                    (view) => view.isDerivedHealth,
                  ))
                    const Text(
                      'Raw Health data stays on this device. Only the locally transformed wellbeing summary can be processed on your server.',
                    ),
                ],
                if (expired)
                  const Text(
                    'This review expired. Close and review access again.',
                  ),
                if (widget.controller.failure case final failure?)
                  Text(failure),
              ],
            ),
          ),
        ),
        actions: [
          FloeButton.text(
            onPressed: () => Navigator.pop(context),
            child: const Text('Cancel'),
          ),
          FloeButton.filled(
            onPressed:
                busy ||
                    value == null ||
                    expired ||
                    !value.allowedActions.contains('allow')
                ? null
                : () async {
                    final confirmed = await widget.controller.allowObserve(
                      value,
                    );
                    if (context.mounted && confirmed) Navigator.pop(context);
                  },
            child: const Text('Allow'),
          ),
        ],
      );
    },
  );
}
