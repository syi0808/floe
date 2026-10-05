import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_badge.dart';
import 'package:floe_client/app/floe_button.dart';
import 'package:floe_client/app/floe_squircle.dart';
import 'package:floe_client/features/connections/application/calendar_system_access_gateway.dart';
import 'package:floe_client/features/connections/domain/connection_models.dart';
import 'package:floe_client/l10n/app_localizations.dart';

/// Baseline macOS permission display. This is not a Floe access decision.
final class CalendarSystemAccessCard extends StatefulWidget {
  const CalendarSystemAccessCard({
    super.key,
    required this.gateway,
    required this.enabled,
  });
  final CalendarSystemAccessGateway gateway;
  final bool enabled;

  static bool appliesTo(IntegrationSummary? integration) =>
      defaultTargetPlatform == TargetPlatform.macOS &&
      integration?.serviceKind == 'apple_calendar';

  @override
  State<CalendarSystemAccessCard> createState() =>
      _CalendarSystemAccessCardState();
}

final class _CalendarSystemAccessCardState
    extends State<CalendarSystemAccessCard>
    with WidgetsBindingObserver {
  CalendarSystemAccess? _status;
  bool _loading = false;
  bool _openingSettings = false;
  bool _settingsFailed = false;
  int _generation = 0;
  int _settingsGeneration = 0;
  bool get _active =>
      WidgetsBinding.instance.lifecycleState == null ||
      WidgetsBinding.instance.lifecycleState == AppLifecycleState.resumed;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    if (_active) unawaited(_inspect());
  }

  @override
  void didUpdateWidget(CalendarSystemAccessCard oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.gateway != widget.gateway) {
      ++_generation;
      _status = null;
      _loading = false;
      ++_settingsGeneration;
      _openingSettings = false;
      _settingsFailed = false;
      if (_active) unawaited(_inspect());
    }
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed) {
      unawaited(_inspect());
    } else {
      // A late pre-resume observation cannot overwrite a new OS status.
      ++_generation;
    }
  }

  Future<void> _inspect() async {
    final generation = ++_generation;
    final gateway = widget.gateway;
    setState(() => _loading = true);
    CalendarSystemAccess status;
    try {
      status = await gateway.inspect();
    } catch (_) {
      status = CalendarSystemAccess.unavailable;
    }
    if (!mounted || generation != _generation || gateway != widget.gateway) {
      return;
    }
    setState(() {
      _status = status;
      _loading = false;
    });
  }

  Future<void> _openSettings() async {
    if (_openingSettings || !widget.enabled) return;
    final gateway = widget.gateway;
    final generation = ++_settingsGeneration;
    setState(() {
      _openingSettings = true;
      _settingsFailed = false;
    });
    bool failed = false;
    try {
      await gateway.openSettings();
    } catch (_) {
      failed = true;
    }
    if (!mounted ||
        generation != _settingsGeneration ||
        gateway != widget.gateway) {
      return;
    }
    setState(() {
      _openingSettings = false;
      _settingsFailed = failed;
    });
    // Opening Settings is not proof of permission. Resume observes the OS again.
  }

  @override
  void dispose() {
    ++_generation;
    ++_settingsGeneration;
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final label = switch (_status) {
      CalendarSystemAccess.allowed => 'Allowed',
      CalendarSystemAccess.notRequested => 'Not requested',
      CalendarSystemAccess.denied ||
      CalendarSystemAccess.writeOnly => 'Needs attention',
      CalendarSystemAccess.restricted => 'Restricted',
      CalendarSystemAccess.unavailable => 'Unavailable',
      null => 'Not checked',
    };
    final tone = switch (_status) {
      CalendarSystemAccess.allowed => FloeBadgeTone.success,
      CalendarSystemAccess.denied ||
      CalendarSystemAccess.restricted ||
      CalendarSystemAccess.writeOnly => FloeBadgeTone.warning,
      _ => FloeBadgeTone.neutral,
    };
    return FloeSquircle(
      key: const ValueKey('calendar-system-access'),
      size: FloeSquircleSize.md,
      fill: FloePalette.neutral50,
      borderWidth: 0,
      padding: const EdgeInsets.all(FloeSpace.base),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              const Expanded(
                child: Text('System access', style: FloeType.controlLabel),
              ),
              if (_loading)
                const SizedBox(
                  width: 16,
                  height: 16,
                  child: CircularProgressIndicator(strokeWidth: 2),
                )
              else
                FloeBadge(
                  key: const ValueKey('calendar-system-access-status'),
                  label: label,
                  tone: tone,
                ),
            ],
          ),
          const SizedBox(height: FloeSpace.xs),
          Text(
            'macOS Calendar access makes calendars available to Floe. It does not grant any Floe feature permission.',
            style: FloeType.bodySmall.copyWith(
              color: FloePalette.neutral600,
              height: 1.5,
            ),
          ),
          if (_status == CalendarSystemAccess.denied ||
              _status == CalendarSystemAccess.writeOnly) ...[
            const SizedBox(height: FloeSpace.sm),
            Align(
              alignment: AlignmentDirectional.centerStart,
              child: FloeButton.outlined(
                key: const ValueKey('calendar-system-access-recover'),
                onPressed: widget.enabled && !_openingSettings
                    ? _openSettings
                    : null,
                child: Text(AppLocalizations.of(context).manageAccess),
              ),
            ),
          ],
          if (_settingsFailed) ...[
            const SizedBox(height: FloeSpace.sm),
            Text(
              'Could not open System Settings. Open Privacy & Security → Calendars to manage access.',
              style: FloeType.bodySmall.copyWith(color: FloePalette.neutral600),
            ),
          ],
        ],
      ),
    );
  }
}
