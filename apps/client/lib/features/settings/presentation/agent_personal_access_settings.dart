import 'package:flutter/material.dart';

import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_button.dart';
import 'package:floe_client/app/floe_squircle.dart';
import 'package:floe_client/features/settings/domain/agent_personal_access.dart';

final class PersonalAttentionAccessCard extends StatefulWidget {
  const PersonalAttentionAccessCard({
    super.key,
    required this.gateway,
    required this.personId,
    this.requestPermission,
    this.inspectSubject,
  });

  final AgentPersonalAccessGateway gateway;
  final String personId;
  final Future<bool> Function()? requestPermission;
  final Future<Map<String, dynamic>> Function()? inspectSubject;

  @override
  State<PersonalAttentionAccessCard> createState() =>
      _PersonalAttentionAccessCardState();
}

final class _PersonalAttentionAccessCardState
    extends State<PersonalAttentionAccessCard> {
  PersonalAccessOverview? overview;
  Object? failure;
  bool busy = false;
  Set<String> selectedConsumers = {'assistant', 'attention.expert'};

  @override
  void initState() {
    super.initState();
    _inspect();
  }

  Future<void> _inspect() async {
    try {
      final value = await widget.gateway.inspectPersonalAttention(
        widget.personId,
      );
      if (mounted) {
        setState(() {
          overview = value;
          if (value.consumers.isNotEmpty) {
            selectedConsumers = value.consumers.toSet();
          }
        });
      }
    } on Object catch (error) {
      if (mounted) setState(() => failure = error);
    }
  }

  Future<void> _change(Future<PersonalAccessOverview> Function() action) async {
    if (busy) return;
    setState(() {
      busy = true;
      failure = null;
    });
    try {
      final value = await action();
      if (mounted) setState(() => overview = value);
    } on Object catch (error) {
      if (mounted) setState(() => failure = error);
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }

  Future<void> _review() async {
    await _change(() async {
      final requestPermission = widget.requestPermission;
      if (requestPermission != null && !await requestPermission()) {
        throw const FormatException('Attention permission was not granted.');
      }
      final inspectSubject = widget.inspectSubject;
      if (inspectSubject != null) {
        final subject = await inspectSubject();
        final fingerprint = subject['subject_fingerprint'];
        if (fingerprint is! String || fingerprint.isEmpty) {
          throw const FormatException('Attention subject is unavailable.');
        }
      }
      final current = overview;
      if (current == null) {
        throw const FormatException('Attention preview unavailable.');
      }
      return widget.gateway.reviewPersonalAttention(
        widget.personId,
        reviewedPreview: current,
        consumers: selectedConsumers.toList()..sort(),
      );
    });
  }

  @override
  Widget build(BuildContext context) {
    final current = overview;
    final enabled = current?.state == 'active' && !current!.reviewRequired;
    return FloeSquircle(
      padding: const EdgeInsets.all(FloeSpace.lg),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text('Attention access', style: FloeType.title),
          const SizedBox(height: FloeSpace.xs),
          Text(
            'Floe can use coarse device-local attention only after explicit review. No app or window names are collected.',
            style: FloeType.body,
          ),
          const SizedBox(height: FloeSpace.sm),
          Text(
            current == null
                ? failure == null
                      ? 'Checking availability…'
                      : 'Attention access is unavailable on this device.'
                : enabled
                ? 'Enabled on this device.'
                : current.state == 'paused'
                ? 'Paused.'
                : 'Review required.',
            style: FloeType.bodySmall,
          ),
          if (current != null && current.consumers.isNotEmpty)
            Text(
              'Approved consumers: ${current.consumers.join(', ')}',
              style: FloeType.bodySmall,
            ),
          if (current != null && current.nativeSubjectFingerprint != null) ...[
            const SizedBox(height: FloeSpace.xs),
            const Text('Allow attention data for:'),
            for (final consumer in const ['assistant', 'attention.expert'])
              CheckboxListTile(
                dense: true,
                contentPadding: EdgeInsets.zero,
                value: selectedConsumers.contains(consumer),
                onChanged: busy
                    ? null
                    : (selected) => setState(() {
                        if (selected == true) {
                          selectedConsumers.add(consumer);
                        } else {
                          selectedConsumers.remove(consumer);
                        }
                      }),
                title: Text(consumer),
              ),
          ],
          const SizedBox(height: FloeSpace.sm),
          Row(
            children: [
              FloeButton.outlined(
                onPressed:
                    busy ||
                        current?.nativeSubjectFingerprint == null ||
                        selectedConsumers.isEmpty
                    ? null
                    : _review,
                loading: busy,
                child: Text(enabled ? 'Review again' : 'Review and enable'),
              ),
              if (enabled) ...[
                const SizedBox(width: FloeSpace.sm),
                FloeButton.outlined(
                  onPressed: busy
                      ? null
                      : () => _change(
                          () => widget.gateway.setPersonalAttentionEnabled(
                            widget.personId,
                            false,
                          ),
                        ),
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

final class PersonalContactsAccessCard extends StatefulWidget {
  const PersonalContactsAccessCard({
    super.key,
    required this.gateway,
    required this.personId,
    required this.readContacts,
    required this.inspectSubject,
  });

  final AgentPersonalAccessGateway gateway;
  final String personId;
  final Future<Map<String, dynamic>> Function() readContacts;
  final Future<Map<String, dynamic>> Function(List<String> handles)
  inspectSubject;

  @override
  State<PersonalContactsAccessCard> createState() =>
      _PersonalContactsAccessCardState();
}

final class PersonalFeasibilityAccessCard extends StatefulWidget {
  const PersonalFeasibilityAccessCard({
    super.key,
    required this.gateway,
    required this.personId,
    required this.requestQuery,
    required this.requestPermission,
    required this.inspectSubject,
  });

  final AgentPersonalAccessGateway gateway;
  final String personId;
  final Future<PersonalFeasibilityQuery?> Function() requestQuery;
  final Future<bool> Function() requestPermission;
  final Future<Map<String, dynamic>> Function() inspectSubject;

  @override
  State<PersonalFeasibilityAccessCard> createState() =>
      _PersonalFeasibilityAccessCardState();
}

final class _PersonalFeasibilityAccessCardState
    extends State<PersonalFeasibilityAccessCard> {
  PersonalAccessOverview? overview;
  Object? failure;
  bool busy = false;

  @override
  void initState() {
    super.initState();
    _inspect();
  }

  Future<void> _inspect() async {
    try {
      final value = await widget.gateway.inspectPersonalFeasibility(
        widget.personId,
      );
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
      final inspected = await widget.gateway.inspectPersonalFeasibility(
        widget.personId,
      );
      final value = await widget.gateway.reviewPersonalFeasibility(
        widget.personId,
        query: query,
        reviewedPreview: inspected.withNativeSubjectFingerprint(fingerprint),
        consumers: const ['assistant'],
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
      final value = await widget.gateway.setPersonalFeasibilityEnabled(
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
                  child: Text(enabled ? 'Review again' : 'Review and enable'),
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

final class PersonalWellbeingAccessCard extends StatefulWidget {
  const PersonalWellbeingAccessCard({
    super.key,
    required this.gateway,
    required this.personId,
    required this.requestPermission,
    required this.inspectSubject,
  });

  final AgentPersonalAccessGateway gateway;
  final String personId;
  final Future<bool> Function() requestPermission;
  final Future<Map<String, dynamic>> Function() inspectSubject;

  @override
  State<PersonalWellbeingAccessCard> createState() =>
      _PersonalWellbeingAccessCardState();
}

final class _PersonalWellbeingAccessCardState
    extends State<PersonalWellbeingAccessCard> {
  PersonalAccessOverview? overview;
  Object? failure;
  bool busy = false;

  @override
  void initState() {
    super.initState();
    _inspect();
  }

  Future<void> _inspect() async {
    try {
      final value = await widget.gateway.inspectPersonalWellbeing(
        widget.personId,
      );
      if (mounted) setState(() => overview = value);
    } on Object catch (error) {
      if (mounted) setState(() => failure = error);
    }
  }

  Future<void> _review() async {
    if (busy) return;
    setState(() {
      busy = true;
      failure = null;
    });
    try {
      if (!await widget.requestPermission()) {
        throw const FormatException(
          'Health authorization review could not be completed.',
        );
      }
      final subject = await widget.inspectSubject();
      final fingerprint = subject['subject_fingerprint'];
      if (fingerprint is! String ||
          !RegExp(r'^[0-9a-f]{64}$').hasMatch(fingerprint)) {
        throw const FormatException('Health subject is unavailable.');
      }
      final inspected = await widget.gateway.inspectPersonalWellbeing(
        widget.personId,
      );
      final value = await widget.gateway.reviewPersonalWellbeing(
        widget.personId,
        reviewedPreview: inspected,
        nativeSubjectFingerprint: fingerprint,
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
      final value = await widget.gateway.setPersonalWellbeingEnabled(
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
          Text('Wellbeing access', style: FloeType.title),
          const SizedBox(height: FloeSpace.xs),
          Text(
            'Review access to a short-lived derived capacity and recovery summary from the last 36 hours of Apple Health sleep, steps, and exercise data. Raw health samples stay on this device; only the derived summary is available to the device-local assistant.',
            style: FloeType.body,
          ),
          const SizedBox(height: FloeSpace.sm),
          Text(
            current == null
                ? failure == null
                      ? 'Checking availability…'
                      : 'Wellbeing is unavailable on this device.'
                : enabled
                ? 'Enabled for device-local wellbeing summaries.'
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
                  key: const ValueKey('personal-wellbeing-review'),
                  onPressed: busy ? null : _review,
                  loading: busy,
                  child: Text(enabled ? 'Review again' : 'Review and enable'),
                ),
              if (enabled) ...[
                const SizedBox(width: FloeSpace.sm),
                FloeButton.outlined(
                  key: const ValueKey('personal-wellbeing-pause'),
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

final class _PersonalContactsAccessCardState
    extends State<PersonalContactsAccessCard> {
  List<Map<String, String>> identities = const [];
  Set<String> selected = {};
  PersonalAccessOverview? preview;
  Object? failure;
  bool busy = false;

  @override
  void initState() {
    super.initState();
    _loadContacts();
  }

  Future<void> _loadContacts() async {
    try {
      final value = await widget.readContacts();
      final raw = value['identities'];
      if (raw is! List) throw const FormatException('Invalid Contacts list.');
      final parsed = <Map<String, String>>[];
      for (final item in raw) {
        if (item is! Map ||
            item['identity_handle'] is! String ||
            item['display_name'] is! String) {
          throw const FormatException('Invalid contact identity.');
        }
        parsed.add({
          'handle': item['identity_handle']! as String,
          'name': item['display_name']! as String,
        });
      }
      if (mounted) setState(() => identities = List.unmodifiable(parsed));
    } on Object catch (error) {
      if (mounted) setState(() => failure = error);
    }
  }

  Future<void> _review() async {
    if (busy || selected.isEmpty) return;
    setState(() {
      busy = true;
      failure = null;
    });
    try {
      final inspected = await widget.inspectSubject(selected.toList()..sort());
      final fingerprint = inspected['subject_fingerprint'];
      if (fingerprint is! String) {
        throw const FormatException('Contacts subject is unavailable.');
      }
      final value = await widget.gateway.inspectPersonalContacts(
        widget.personId,
        selected.toList()..sort(),
      );
      if (value.nativeSubjectFingerprint != fingerprint) {
        throw const FormatException('Contacts changed during review.');
      }
      if (mounted) setState(() => preview = value);
    } on Object catch (error) {
      if (mounted) setState(() => failure = error);
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }

  Future<void> _enable() async {
    final current = preview;
    if (busy || current == null) return;
    setState(() {
      busy = true;
      failure = null;
    });
    try {
      final value = await widget.gateway.reviewPersonalContacts(
        widget.personId,
        selectedHandles: selected.toList()..sort(),
        reviewedPreview: current,
        consumers: const ['assistant'],
      );
      if (mounted) setState(() => preview = value);
    } on Object catch (error) {
      if (mounted) setState(() => failure = error);
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final current = preview;
    final enabled = current?.state == 'active' && !current!.reviewRequired;
    return FloeSquircle(
      padding: const EdgeInsets.all(FloeSpace.lg),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text('Contacts access', style: FloeType.title),
          const SizedBox(height: FloeSpace.xs),
          Text(
            'Choose specific contacts. Floe never requests an all-contacts grant for this access.',
            style: FloeType.body,
          ),
          const SizedBox(height: FloeSpace.sm),
          if (failure != null)
            Text(
              'Contacts are unavailable on this device.',
              style: FloeType.bodySmall,
            )
          else if (identities.isEmpty)
            Text('No readable contacts found.', style: FloeType.bodySmall)
          else
            for (final identity in identities)
              CheckboxListTile(
                dense: true,
                contentPadding: EdgeInsets.zero,
                value: selected.contains(identity['handle']),
                onChanged: busy
                    ? null
                    : (value) => setState(() {
                        if (value == true) {
                          selected.add(identity['handle']!);
                        } else {
                          selected.remove(identity['handle']);
                        }
                        preview = null;
                      }),
                title: Text(identity['name']!),
              ),
          if (current != null)
            Text(
              enabled
                  ? 'Enabled for the selected contacts.'
                  : 'Selection ready for review.',
              style: FloeType.bodySmall,
            ),
          const SizedBox(height: FloeSpace.sm),
          Row(
            children: [
              FloeButton.outlined(
                onPressed: busy || selected.isEmpty ? null : _review,
                loading: busy,
                child: const Text('Review selection'),
              ),
              if (current != null && !enabled) ...[
                const SizedBox(width: FloeSpace.sm),
                FloeButton.outlined(
                  onPressed: busy ? null : _enable,
                  child: const Text('Enable'),
                ),
              ],
            ],
          ),
        ],
      ),
    );
  }
}
