import 'package:flutter/material.dart';

import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_button.dart';
import 'package:floe_client/app/floe_feedback.dart';
import 'package:floe_client/app/floe_primitives.dart';
import 'package:floe_client/app/floe_selection.dart';
import 'package:floe_client/app/floe_squircle.dart';
import 'package:floe_client/features/connections/application/connection_observe_gateway.dart';
import 'package:floe_client/features/connections/application/native_personal_source_gateway.dart';
import 'package:floe_client/features/connections/domain/connection_observe.dart';
import 'package:floe_client/features/connections/domain/source_connection.dart';

final class PersonalObserveControl extends StatefulWidget {
  const PersonalObserveControl({
    super.key,
    required this.gateway,
    required this.connectorId,
    required this.connectionId,
    required this.sourceRevision,
  });

  final ConnectionObserveGateway gateway;
  final String connectorId;
  final String connectionId;
  final int sourceRevision;

  @override
  State<PersonalObserveControl> createState() => _PersonalObserveControlState();
}

final class _PersonalObserveControlState extends State<PersonalObserveControl> {
  ConnectionObserveOverview? overview;
  Object? failure;
  bool busy = false;

  @override
  void initState() {
    super.initState();
    _inspect();
  }

  @override
  void didUpdateWidget(PersonalObserveControl oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.connectorId != widget.connectorId ||
        oldWidget.connectionId != widget.connectionId ||
        oldWidget.sourceRevision != widget.sourceRevision) {
      _inspect();
    }
  }

  Future<void> _inspect() async {
    try {
      final value = await widget.gateway.inspect(
        connectorId: widget.connectorId,
        connectionId: widget.connectionId,
      );
      if (mounted) setState(() => overview = value);
    } on Object catch (error) {
      if (mounted) setState(() => failure = error);
    }
  }

  Future<void> _setEnabled(bool enabled) async {
    if (busy) return;
    setState(() {
      busy = true;
      failure = null;
    });
    try {
      final expected = enabled
          ? await widget.gateway.review(
              connectorId: widget.connectorId,
              connectionId: widget.connectionId,
            )
          : null;
      if (expected != null && mounted) {
        final confirmed = await showFloeDialog<bool>(
          context,
          (context) => FloeDialog(
            title: const Text('Allow Floe to use this connection?'),
            content: Text('Views: ${expected.members.join(', ')}'),
            actions: [
              FloeButton.text(
                onPressed: () => Navigator.pop(context, false),
                child: const Text('Cancel'),
              ),
              FloeButton.filled(
                onPressed: () => Navigator.pop(context, true),
                child: const Text('Allow'),
              ),
            ],
          ),
        );
        if (confirmed != true) return;
      }
      final value = await widget.gateway.setEnabled(
        connectorId: widget.connectorId,
        connectionId: widget.connectionId,
        enabled: enabled,
        expected: expected,
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
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        FloeSwitchTile(
          key: const ValueKey('personal-use-with-floe'),
          title: 'Use with Floe',
          subtitle: current == null
              ? 'Checking Observe permission…'
              : current.enabled
              ? 'Floe can use this connection.'
              : 'Review is required before Floe can use this connection.',
          value: current?.enabled ?? false,
          onChanged: busy || current == null ? null : _setEnabled,
        ),
        if (failure != null)
          const Text('Observe permission could not be updated.'),
      ],
    );
  }
}

final class PersonalSingletonSourceCard extends StatefulWidget {
  const PersonalSingletonSourceCard({
    super.key,
    required this.title,
    required this.description,
    required this.connectorId,
    required this.sourceGateway,
    required this.observeGateway,
    this.requestPermission,
  });

  final String title;
  final String description;
  final String connectorId;
  final NativePersonalSourceGateway sourceGateway;
  final ConnectionObserveGateway observeGateway;
  final Future<bool> Function()? requestPermission;

  @override
  State<PersonalSingletonSourceCard> createState() =>
      _PersonalSingletonSourceCardState();
}

final class _PersonalSingletonSourceCardState
    extends State<PersonalSingletonSourceCard> {
  SourceConnection? source;
  Object? failure;
  bool busy = false;

  @override
  void initState() {
    super.initState();
    _inspect();
  }

  Future<void> _inspect() async {
    try {
      final value = await widget.sourceGateway.inspect(widget.connectorId);
      if (mounted) setState(() => source = value);
    } on Object catch (error) {
      if (mounted) setState(() => failure = error);
    }
  }

  Future<void> _setup() async {
    if (busy) return;
    setState(() {
      busy = true;
      failure = null;
    });
    try {
      if (widget.requestPermission case final requestPermission?) {
        if (!await requestPermission()) {
          throw const FormatException('System access was not granted.');
        }
      }
      final value = await widget.sourceGateway.setup(
        connectorId: widget.connectorId,
        expectedRevision: source?.revision,
        selectedHandles: const [],
      );
      if (mounted) setState(() => source = value);
    } on Object catch (error) {
      if (mounted) setState(() => failure = error);
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }

  @override
  Widget build(BuildContext context) => FloeSquircle(
    padding: const EdgeInsets.all(FloeSpace.lg),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(widget.title, style: FloeType.title),
        const SizedBox(height: FloeSpace.xs),
        Text(widget.description, style: FloeType.body),
        const SizedBox(height: FloeSpace.sm),
        FloeButton.outlined(
          onPressed: busy ? null : _setup,
          loading: busy,
          child: Text(source == null ? 'Set up source' : 'Refresh source'),
        ),
        if (failure != null) const Text('Source setup could not be completed.'),
        if (source case final current?) ...[
          const SizedBox(height: FloeSpace.sm),
          PersonalObserveControl(
            gateway: widget.observeGateway,
            connectorId: widget.connectorId,
            connectionId: current.connectionId,
            sourceRevision: current.revision,
          ),
        ],
      ],
    ),
  );
}

final class PersonalContactsSourceCard extends StatefulWidget {
  const PersonalContactsSourceCard({
    super.key,
    required this.sourceGateway,
    required this.observeGateway,
    required this.readContacts,
  });

  final NativePersonalSourceGateway sourceGateway;
  final ConnectionObserveGateway observeGateway;
  final Future<Map<String, dynamic>> Function() readContacts;

  @override
  State<PersonalContactsSourceCard> createState() =>
      _PersonalContactsSourceCardState();
}

final class _PersonalContactsSourceCardState
    extends State<PersonalContactsSourceCard> {
  List<Map<String, String>> identities = const [];
  Set<String> selected = {};
  SourceConnection? source;
  Object? failure;
  bool busy = false;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    try {
      final value = await widget.readContacts();
      final raw = value['identities'];
      if (raw is! List) throw const FormatException('Invalid Contacts list');
      final parsed = <Map<String, String>>[];
      for (final item in raw) {
        if (item is! Map ||
            item['identity_handle'] is! String ||
            item['display_name'] is! String) {
          throw const FormatException('Invalid contact identity');
        }
        parsed.add({
          'handle': item['identity_handle']! as String,
          'name': item['display_name']! as String,
        });
      }
      final current = await widget.sourceGateway.inspect('contacts.apple');
      if (mounted) {
        setState(() {
          identities = List.unmodifiable(parsed);
          source = current;
          selected =
              current?.resources.map((resource) => resource.handle).toSet() ??
              {};
        });
      }
    } on Object catch (error) {
      if (mounted) setState(() => failure = error);
    }
  }

  Future<void> _save() async {
    if (busy || selected.isEmpty) return;
    setState(() {
      busy = true;
      failure = null;
    });
    try {
      final value = await widget.sourceGateway.setup(
        connectorId: 'contacts.apple',
        expectedRevision: source?.revision,
        selectedHandles: selected.toList()..sort(),
      );
      if (mounted) setState(() => source = value);
    } on Object catch (error) {
      if (mounted) setState(() => failure = error);
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }

  @override
  Widget build(BuildContext context) => FloeSquircle(
    padding: const EdgeInsets.all(FloeSpace.lg),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text('Contacts source', style: FloeType.title),
        const SizedBox(height: FloeSpace.xs),
        const Text('Choose the contacts available to this connection.'),
        const SizedBox(height: FloeSpace.sm),
        for (final identity in identities)
          FloeCheckboxTile(
            value: selected.contains(identity['handle']),
            onChanged: busy
                ? null
                : (value) => setState(() {
                    if (value == true) {
                      selected.add(identity['handle']!);
                    } else {
                      selected.remove(identity['handle']);
                    }
                  }),
            title: Text(identity['name']!),
          ),
        FloeButton.outlined(
          onPressed: busy || selected.isEmpty ? null : _save,
          loading: busy,
          child: const Text('Save selection'),
        ),
        if (failure != null)
          const Text('Contacts source could not be updated.'),
        if (source case final current?) ...[
          const SizedBox(height: FloeSpace.sm),
          PersonalObserveControl(
            gateway: widget.observeGateway,
            connectorId: 'contacts.apple',
            connectionId: current.connectionId,
            sourceRevision: current.revision,
          ),
        ],
      ],
    ),
  );
}
