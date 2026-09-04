import 'package:flutter/material.dart';
import 'package:intl/intl.dart';

import '../../../l10n/app_localizations.dart';
import '../../../app/design_tokens.dart';
import '../../../app/floe_button.dart';
import '../../../app/floe_feedback.dart';
import '../../../app/floe_squircle.dart';
import '../application/focus_controller.dart';
import '../application/focus_gateway.dart';
import '../application/ffi_day_gateway.dart';
import '../domain/day_models.dart';
import '../domain/focus_models.dart';

Future<void> openFocusDialog(
  BuildContext context,
  FocusGateway gateway,
  DayQuery query,
) => showFloeDialog<void>(
  context,
  (_) => FocusDialog(gateway: gateway, query: query),
);

class FocusDialog extends StatefulWidget {
  const FocusDialog({super.key, required this.gateway, required this.query});
  final FocusGateway gateway;
  final DayQuery query;

  @override
  State<FocusDialog> createState() => _FocusDialogState();
}

class _FocusDialogState extends State<FocusDialog> {
  late final FocusController controller;
  final start = TextEditingController(text: '09:00');
  final end = TextEditingController(text: '18:00');
  final duration = TextEditingController(text: '60');
  final model = TextEditingController(
    text: const String.fromEnvironment('FLOE_INFERENCE_TARGET'),
  );
  bool allowExternal = false;
  bool dirty = false;
  bool invalidPreference = false;
  bool serverUnavailable = false;

  @override
  void initState() {
    super.initState();
    controller = FocusController(gateway: widget.gateway, query: widget.query);
    _load();
  }

  Future<void> _load() async {
    if (widget.gateway case FfiDayGateway gateway) {
      try {
        final connection = await gateway.serverClient.connection();
        if (mounted && model.text.isEmpty && connection != null) {
          model.text = connection.target;
        }
      } on Object {
        serverUnavailable = true;
      }
    }
    if (await controller.load() && mounted) _restoreFields();
  }

  void _restoreFields() {
    final value = controller.preference?.value;
    setState(() {
      start.text = _time(value?.startMinute ?? 540);
      end.text = _time(value?.endMinute ?? 1080);
      duration.text = (value?.durationMinutes ?? 60).toString();
      dirty = false;
      invalidPreference = false;
    });
  }

  void _edit(String _) {
    setState(() {
      dirty = true;
      invalidPreference = false;
    });
    controller.clearProposal();
  }

  Future<void> _save() async {
    final startMinute = _parseTime(start.text);
    final endMinute = _parseTime(end.text);
    final minutes = int.tryParse(duration.text.trim());
    if (startMinute == null ||
        endMinute == null ||
        minutes == null ||
        startMinute >= endMinute ||
        minutes < 15 ||
        minutes > 240 ||
        minutes > endMinute - startMinute) {
      setState(() => invalidPreference = true);
      return;
    }
    if (await controller.save(
          FocusPreferenceValue(
            startMinute: startMinute,
            endMinute: endMinute,
            durationMinutes: minutes,
          ),
        ) &&
        mounted) {
      _restoreFields();
    }
  }

  Future<void> _delete() async {
    if (await controller.save(null) && mounted) _restoreFields();
  }

  @override
  void dispose() {
    controller.dispose();
    start.dispose();
    end.dispose();
    duration.dispose();
    model.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: controller,
    builder: (context, _) {
      final strings = AppLocalizations.of(context);
      final enabled = controller.loaded && !controller.pending;
      final preference = controller.preference;
      return FloeDetailDialog(
        title: strings.focusTitle,
        children: [
          Text(DateFormat.yMMMMd(strings.localeName).format(widget.query.date)),
          const SizedBox(height: 16),
          Text(
            strings.focusPreferenceTitle,
            style: const TextStyle(fontWeight: FontWeight.w600),
          ),
          const SizedBox(height: 12),
          Text(
            preference?.value == null
                ? strings.focusDefaults
                : strings.focusSource,
          ),
          if (preference?.value != null)
            Text(
              DateFormat.yMMMd(strings.localeName)
                  .add_Hm()
                  .format(preference!.updatedAt.toLocal()),
            ),
          const SizedBox(height: 16),
          Wrap(
            spacing: 12,
            runSpacing: 12,
            children: [
              SizedBox(
                width: 130,
                child: TextField(
                  controller: start,
                  enabled: enabled,
                  decoration: InputDecoration(labelText: strings.focusStart),
                  onChanged: _edit,
                ),
              ),
              SizedBox(
                width: 130,
                child: TextField(
                  controller: end,
                  enabled: enabled,
                  decoration: InputDecoration(labelText: strings.focusEnd),
                  onChanged: _edit,
                ),
              ),
              SizedBox(
                width: 160,
                child: TextField(
                  controller: duration,
                  enabled: enabled,
                  keyboardType: TextInputType.number,
                  decoration: InputDecoration(labelText: strings.focusDuration),
                  onChanged: _edit,
                ),
              ),
            ],
          ),
          const SizedBox(height: 12),
          Wrap(
            spacing: 8,
            runSpacing: 8,
            children: [
              FloeButton.outlined(
                onPressed: enabled ? _save : null,
                child: Text(strings.focusSave),
              ),
              if (preference?.value != null)
                FloeButton.text(
                  onPressed: enabled ? _delete : null,
                  child: Text(strings.focusDelete),
                ),
            ],
          ),
          if (invalidPreference) _notice(strings.focusInvalidPreference),
          const SizedBox(height: 24),
          FloeInfoNote(text: strings.focusDisclosure),
          if (serverUnavailable)
            const FloeInfoNote(
              text: 'Check your local server connection in Connections before requesting a suggestion.',
            ),
          const SizedBox(height: 16),
          TextField(
            controller: model,
            enabled: !controller.pending,
            autocorrect: false,
            enableSuggestions: false,
            decoration: InputDecoration(labelText: strings.focusModel),
            onChanged: (_) {
              setState(() {});
              controller.clearProposal();
            },
          ),
          const SizedBox(height: 12),
          CheckboxListTile(
            contentPadding: EdgeInsets.zero,
            title: Text(strings.focusAllowExternal),
            value: allowExternal,
            onChanged: controller.pending
                ? null
                : (value) {
                    setState(() => allowExternal = value ?? false);
                    controller.clearProposal();
                  },
          ),
          if (dirty) _notice(strings.focusDirty),
          FloeButton.filled(
            onPressed: enabled && !dirty && model.text.trim().isNotEmpty
                ? () => controller.suggest(
                    model.text,
                    allowExternal: allowExternal,
                  )
                : null,
            child: Text(strings.focusSuggest),
          ),
          if (controller.pending) ...[
            const SizedBox(height: 16),
            const LinearProgressIndicator(),
            Semantics(liveRegion: true, child: Text(strings.focusPending)),
          ],
          if (controller.errorCode case final code?) ...[
            _notice(_error(strings, code)),
            FloeButton.text(
              onPressed: controller.pending ? null : _load,
              child: Text(strings.focusReload),
            ),
          ],
          if (controller.proposal case final proposal?) ...[
            const SizedBox(height: 24),
            _Proposal(proposal: proposal),
          ],
        ],
      );
    },
  );

  Widget _notice(String text) => Padding(
    padding: const EdgeInsets.symmetric(vertical: 12),
    child: Semantics(
      liveRegion: true,
      child: Text(text, style: const TextStyle(color: FloePalette.amber700)),
    ),
  );
}

class _Proposal extends StatelessWidget {
  const _Proposal({required this.proposal});
  final FocusProposal proposal;

  @override
  Widget build(BuildContext context) {
    final strings = AppLocalizations.of(context);
    final offset = Duration(seconds: proposal.timezoneOffsetSeconds);
    final format = DateFormat.Hm(strings.localeName);
    final zone =
        'UTC${offset.isNegative ? '-' : '+'}${_time(offset.inMinutes.abs())}';
    return FloeSquircle(
      fill: FloePalette.primary50,
      borderWidth: 0,
      padding: const EdgeInsets.all(20),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Semantics(
            liveRegion: true,
            child: Text(
              '${format.format(proposal.startsAt.toUtc().add(offset))}–${format.format(proposal.endsAt.toUtc().add(offset))} · $zone',
              style: const TextStyle(fontSize: 20, fontWeight: FontWeight.w600),
            ),
          ),
          const SizedBox(height: 12),
          Text(proposal.reason),
          const SizedBox(height: 12),
          Text(strings.focusReadOnly),
          Text(
            proposal.model,
            style: const TextStyle(color: FloePalette.neutral600),
          ),
          if (proposal.calendarWarning) ...[
            const SizedBox(height: 12),
            FloeInfoNote(text: strings.focusCalendarWarning),
          ],
          const SizedBox(height: 16),
          Text(
            strings.focusEvidence,
            style: const TextStyle(fontWeight: FontWeight.w600),
          ),
          for (final source in proposal.evidence)
            Padding(
              padding: const EdgeInsets.only(top: 8),
              child: Text(source.label),
            ),
        ],
      ),
    );
  }
}

String _time(int minutes) =>
    '${(minutes ~/ 60).toString().padLeft(2, '0')}:${(minutes % 60).toString().padLeft(2, '0')}';

int? _parseTime(String source) {
  if (!RegExp(r'^\d{2}:\d{2}$').hasMatch(source.trim())) return null;
  final parts = source.trim().split(':').map(int.parse).toList();
  if (parts[0] > 24 || parts[1] > 59 || (parts[0] == 24 && parts[1] != 0)) {
    return null;
  }
  return parts[0] * 60 + parts[1];
}

String _error(AppLocalizations strings, String code) => switch (code) {
  'model_timeout' => strings.focusModelTimeout,
  'model_unavailable' => strings.focusModelUnavailable,
  'invalid_proposal' => strings.focusInvalidProposal,
  'no_focus_slot' => strings.focusNoSlot,
  'conflict' => strings.focusConflict,
  'external_transfer_denied' => strings.focusExternalDenied,
  'validation' => strings.focusInvalidInput,
  _ => strings.focusFailure,
};
