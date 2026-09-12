import 'package:flutter/material.dart';

import '../../app/design_tokens.dart';
import '../../app/floe_button.dart';
import '../../app/floe_squircle.dart';
import 'agent_personal_access.dart';

final class PersonalAttentionAccessCard extends StatefulWidget {
  const PersonalAttentionAccessCard({
    super.key,
    required this.gateway,
    required this.personId,
  });

  final AgentPersonalAccessGateway gateway;
  final String personId;

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
                    : () => _change(
                        () => widget.gateway.reviewPersonalAttention(
                          widget.personId,
                          reviewedPreview: current!,
                          consumers: selectedConsumers.toList()..sort(),
                        ),
                      ),
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
