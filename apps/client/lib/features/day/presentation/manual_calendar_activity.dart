import 'dart:async';

import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_badge.dart';
import 'package:floe_client/app/floe_button.dart';
import 'package:floe_client/app/floe_loading.dart';
import 'package:floe_client/app/floe_squircle.dart';
import 'package:floe_client/features/day/application/day_gateway.dart';
import 'package:floe_client/features/day/domain/day_models.dart';

/// Day-owned status view for direct Calendar operations. An unresolved result
/// stays visible across navigation and can be reconciled without resubmitting.
class ManualCalendarActivity extends StatefulWidget {
  const ManualCalendarActivity({super.key, required this.gateway});

  final DayGateway gateway;

  @override
  State<ManualCalendarActivity> createState() => _ManualCalendarActivityState();
}

class _ManualCalendarActivityState extends State<ManualCalendarActivity>
    with WidgetsBindingObserver {
  List<ManualCalendarOperationReceipt> _operations = const [];
  final List<String?> _previousCursors = [];
  String? _cursor;
  String? _nextCursor;
  final Set<String> _busyOperations = {};
  bool _loading = false;
  bool _loaded = false;
  String? _error;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    unawaited(_load());
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed) unawaited(_load());
  }

  Future<void> _load({
    bool reset = false,
    bool useCursor = false,
    String? cursor,
    bool rememberCurrent = false,
    bool consumePrevious = false,
  }) async {
    if (_loading) return;
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final requestedCursor = reset ? null : (useCursor ? cursor : _cursor);
      final page = await widget.gateway.loadExternalCalendarOperations(
        cursor: requestedCursor,
      );
      if (!mounted) return;
      setState(() {
        if (reset) _previousCursors.clear();
        if (rememberCurrent) _previousCursors.add(_cursor);
        if (consumePrevious && _previousCursors.isNotEmpty) {
          _previousCursors.removeLast();
        }
        _cursor = requestedCursor;
        _operations = page.operations;
        _nextCursor = page.nextCursor;
        _loaded = true;
      });
    } on Object catch (failure) {
      if (!mounted) return;
      setState(() => _error = failure.toString());
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  Future<void> _inspect(ManualCalendarOperationReceipt operation) async {
    await _update(operation, () => widget.gateway
        .inspectExternalCalendarOperation(operation.operationRef));
  }

  Future<void> _reconcile(ManualCalendarOperationReceipt operation) async {
    await _update(
      operation,
      () => widget.gateway.reconcileExternalCalendarOperation(
        operation.operationRef,
        operation.revision,
      ),
    );
  }

  Future<void> _update(
    ManualCalendarOperationReceipt operation,
    Future<ManualCalendarOperationReceipt> Function() load,
  ) async {
    if (!_busyOperations.add(operation.operationRef)) return;
    setState(() => _error = null);
    try {
      final next = await load();
      if (!mounted) return;
      if (next.operationRef != operation.operationRef ||
          next.revision < operation.revision ||
          (next.revision == operation.revision &&
              (next.status != operation.status ||
                  next.collectionPending != operation.collectionPending))) {
        throw StateError('Calendar operation status regressed.');
      }
      setState(() {
        _operations = [
          for (final current in _operations)
            if (current.operationRef == next.operationRef) next else current,
        ];
      });
    } on Object catch (failure) {
      if (mounted) setState(() => _error = failure.toString());
    } finally {
      _busyOperations.remove(operation.operationRef);
      if (mounted) setState(() {});
    }
  }

  String _status(ManualCalendarOperationReceipt operation) =>
      switch (operation.status) {
        ManualCalendarOperationStatus.pending => 'Waiting to run',
        ManualCalendarOperationStatus.executing => 'In progress',
        ManualCalendarOperationStatus.blocked => 'Blocked; no Calendar change was confirmed',
        ManualCalendarOperationStatus.notApplied => 'Not applied',
        ManualCalendarOperationStatus.unknown =>
          'Outcome unknown. The Calendar may have changed; check status before trying again.',
        ManualCalendarOperationStatus.succeeded => operation.collectionPending
            ? 'Calendar change succeeded; Day update is pending.'
            : 'Calendar change succeeded.',
      };

  FloeBadgeTone _tone(ManualCalendarOperationStatus status) => switch (status) {
    ManualCalendarOperationStatus.succeeded => FloeBadgeTone.success,
    ManualCalendarOperationStatus.blocked ||
    ManualCalendarOperationStatus.notApplied => FloeBadgeTone.danger,
    _ => FloeBadgeTone.info,
  };

  @override
  Widget build(BuildContext context) => FloeLoadingOverlay(
    loading: _loading,
    label: 'Loading Calendar activity',
    blockInteraction: false,
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Row(
          children: [
            const Expanded(child: Text('Activity', style: FloeType.pageTitle)),
            FloeButton.icon(
              tooltip: 'Reload Calendar activity',
              onPressed: _loading ? null : () => unawaited(_load(reset: true)),
              icon: const Icon(LucideIcons.refreshCw, size: 18),
            ),
          ],
        ),
        const SizedBox(height: 10),
        const Text('Calendar changes you made from Day appear here.'),
        if (_error case final error?) ...[
          const SizedBox(height: 12),
          Text(error, style: FloeType.bodySmall),
        ],
        if (_loaded && !_loading && _operations.isEmpty) ...[
          const SizedBox(height: 24),
          const Text('No Calendar activity yet.'),
        ],
        if (_loaded && (_previousCursors.isNotEmpty || _nextCursor != null))
          Padding(
            padding: const EdgeInsets.only(top: 8),
            child: Wrap(
              spacing: 8,
              children: [
                if (_previousCursors.isNotEmpty)
                  FloeButton.text(
                    onPressed: _loading
                        ? null
                        : () => unawaited(_load(
                            useCursor: true,
                            cursor: _previousCursors.last,
                            consumePrevious: true,
                          )),
                    child: const Text('Previous page'),
                  ),
                if (_nextCursor case final next?)
                  FloeButton.text(
                    onPressed: _loading
                        ? null
                        : () => unawaited(_load(
                            useCursor: true,
                            cursor: next,
                            rememberCurrent: true,
                          )),
                    child: const Text('Next page'),
                  ),
              ],
            ),
          ),
        for (final operation in _operations)
          Padding(
            padding: const EdgeInsets.only(top: 12),
            child: FloeSquircle(
              padding: const EdgeInsets.all(16),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Text(
                    'Calendar operation · ${operation.operationRef}',
                    style: FloeType.controlLabel,
                  ),
                  const SizedBox(height: 8),
                  Align(
                    alignment: AlignmentDirectional.centerStart,
                    child: FloeBadge(
                      label: switch (operation.status) {
                        ManualCalendarOperationStatus.pending => 'Pending',
                        ManualCalendarOperationStatus.executing => 'In progress',
                        ManualCalendarOperationStatus.blocked => 'Blocked',
                        ManualCalendarOperationStatus.notApplied => 'Not applied',
                        ManualCalendarOperationStatus.unknown => 'Outcome unknown',
                        ManualCalendarOperationStatus.succeeded => 'Succeeded',
                      },
                      tone: _tone(operation.status),
                    ),
                  ),
                  const SizedBox(height: 8),
                  Text(_status(operation)),
                  const SizedBox(height: 8),
                  Wrap(
                    spacing: 8,
                    children: [
                      FloeButton.text(
                        onPressed: _busyOperations.contains(operation.operationRef)
                            ? null
                            : () => unawaited(_inspect(operation)),
                        child: const Text('Check status'),
                      ),
                      if (operation.status ==
                          ManualCalendarOperationStatus.unknown)
                        FloeButton.text(
                          onPressed: _busyOperations.contains(operation.operationRef)
                              ? null
                              : () => unawaited(_reconcile(operation)),
                          child: const Text('Reconcile'),
                        ),
                    ],
                  ),
                ],
              ),
            ),
          ),
      ],
    ),
  );
}
