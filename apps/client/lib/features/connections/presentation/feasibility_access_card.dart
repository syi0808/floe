import 'package:flutter/material.dart';

import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_button.dart';
import 'package:floe_client/app/floe_squircle.dart';
import 'package:floe_client/features/settings/domain/feasibility_access.dart';

final class PersonalFeasibilityAccessCard extends StatefulWidget {
  const PersonalFeasibilityAccessCard({
    super.key,
    required this.gateway,
    required this.personId,
    required this.requestQuery,
    required this.requestPermission,
    required this.inspectSubject,
  });

  final FeasibilityAccessGateway gateway;
  final String personId;
  final Future<FeasibilityQuery?> Function() requestQuery;
  final Future<bool> Function() requestPermission;
  final Future<Map<String, dynamic>> Function() inspectSubject;

  @override
  State<PersonalFeasibilityAccessCard> createState() =>
      _PersonalFeasibilityAccessCardState();
}

final class _PersonalFeasibilityAccessCardState
    extends State<PersonalFeasibilityAccessCard> {
  FeasibilityAccessOverview? overview;
  Object? failure;
  bool busy = false;

  @override
  void initState() {
    super.initState();
    _inspect();
  }

  Future<void> _inspect() async {
    try {
      final value = await widget.gateway.inspectFeasibility(widget.personId);
      if (mounted) setState(() => overview = value);
    } on Object catch (error) {
      if (mounted) setState(() => failure = error);
    }
  }

  Future<void> _review() async {
    if (busy) return;
    final query = await widget.requestQuery();
    if (query == null || !mounted) return;
    setState(() {
      busy = true;
      failure = null;
    });
    try {
      if (!await widget.requestPermission()) {
        throw const FormatException('Feasibility permission was not granted.');
      }
      final subject = await widget.inspectSubject();
      final fingerprint = subject['subject_fingerprint'];
      if (fingerprint is! String ||
          !RegExp(r'^[0-9a-f]{64}$').hasMatch(fingerprint)) {
        throw const FormatException('Feasibility subject is unavailable.');
      }
      final inspected = await widget.gateway.inspectFeasibility(
        widget.personId,
      );
      final value = await widget.gateway.reviewFeasibility(
        widget.personId,
        query: query,
        reviewedPreview: inspected.withNativeSubjectFingerprint(fingerprint),
      );
      if (mounted) setState(() => overview = value);
    } on Object catch (error) {
      if (mounted) setState(() => failure = error);
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }

  Future<void> _setEnabled(bool enabled) async {
    if (busy) return;
    setState(() {
      busy = true;
      failure = null;
    });
    try {
      final value = await widget.gateway.setFeasibilityEnabled(
        widget.personId,
        enabled,
      );
      if (mounted) setState(() => overview = value);
    } on Object catch (error) {
      if (mounted) setState(() => failure = error);
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final current = overview;
    final enabled = current?.state == 'active' && !current!.reviewRequired;
    final paused = current?.state == 'paused';
    return FloeSquircle(
      padding: const EdgeInsets.all(FloeSpace.lg),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text('Trip feasibility access', style: FloeType.title),
          const SizedBox(height: FloeSpace.xs),
          Text(
            'Use your next timed event and an explicitly chosen destination. Apple MapKit and WeatherKit may contact their providers for the estimate.',
            style: FloeType.body,
          ),
          const SizedBox(height: FloeSpace.sm),
          Text(
            current == null
                ? failure == null
                      ? 'Checking availability…'
                      : 'Trip feasibility is unavailable on this device.'
                : enabled
                ? 'Enabled for the reviewed event query.'
                : paused
                ? 'Paused.'
                : 'Review required.',
            style: FloeType.bodySmall,
          ),
          if (failure != null)
            Text('Review could not be completed.', style: FloeType.bodySmall),
          const SizedBox(height: FloeSpace.sm),
          Row(
            children: [
              if (paused)
                FloeButton.outlined(
                  onPressed: busy ? null : () => _setEnabled(true),
                  loading: busy,
                  child: const Text('Re-enable'),
                )
              else
                FloeButton.outlined(
                  key: const ValueKey('personal-feasibility-review'),
                  onPressed: busy ? null : _review,
                  loading: busy,
                  child: Text(
                    enabled ? 'Use with Floe is on' : 'Turn on Use with Floe',
                  ),
                ),
              if (enabled) ...[
                const SizedBox(width: FloeSpace.sm),
                FloeButton.outlined(
                  onPressed: busy ? null : () => _setEnabled(false),
                  child: const Text('Pause'),
                ),
              ],
            ],
          ),
        ],
      ),
    );
  }
}
