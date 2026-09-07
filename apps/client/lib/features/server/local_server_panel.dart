import 'dart:async';

import 'package:flutter/material.dart';

import '../../app/floe_button.dart';
import '../../app/floe_feedback.dart';
import '../../app/floe_input.dart';
import '../../app/floe_loading.dart';
import '../../app/floe_squircle.dart';
import 'local_server_client.dart';

class LocalServerPanel extends StatefulWidget {
  const LocalServerPanel({super.key, required this.client});
  final LocalServerClient client;
  @override
  State<LocalServerPanel> createState() => _LocalServerPanelState();
}

class _LocalServerPanelState extends State<LocalServerPanel> {
  final address = TextEditingController(text: 'http://127.0.0.1:8431');
  ServerConnection? connection;
  String? proof;
  String? code;
  String? pairingAddress;
  String status = 'Loading saved connection…';
  bool busy = true;
  int generation = 0;

  @override
  void initState() {
    super.initState();
    unawaited(_load());
  }

  Future<void> _load() async {
    await _run(() async {
      final saved = await widget.client.connection();
      if (!mounted) return;
      connection = saved;
      if (saved == null) {
        status = 'Not connected';
        return;
      }
      address.text = saved.address;
      await widget.client.checkConnection(saved);
      status = 'Connected to Floe server';
    });
  }

  Future<void> _run(Future<void> Function() operation) async {
    if (mounted) setState(() => busy = true);
    try {
      await FloeLoading.run(operation);
    } on ServerConnectionException catch (error) {
      if (mounted) status = _error(error.code);
    } on Object {
      if (mounted) {
        status =
            'Could not access the secure credential store. Please try again.';
      }
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }

  Future<void> _pair() => _run(() async {
    final base = LocalServerClient.normalizeAddress(address.text);
    final attempt = ++generation;
    final response = await widget.client.request(base, '/pair/start', body: {});
    final pendingProof = response['proof'] as String;
    if (!mounted || attempt != generation) {
      await widget.client.request(
        base,
        '/pair/cancel',
        body: {'proof': pendingProof},
      );
      return;
    }
    pairingAddress = base;
    proof = pendingProof;
    code = response['code'] as String;
    status = 'Compare this code and approve in the server dashboard.';
    setState(() {});
    unawaited(_poll(attempt, base, pendingProof));
  });

  Future<void> _poll(int attempt, String base, String pendingProof) async {
    final deadline = DateTime.now().add(const Duration(minutes: 5));
    try {
      while (mounted &&
          attempt == generation &&
          DateTime.now().isBefore(deadline)) {
        await Future<void>.delayed(const Duration(seconds: 2));
        if (!mounted || attempt != generation) return;
        final response = await widget.client.request(
          base,
          '/pair/poll',
          body: {'proof': pendingProof},
        );
        if (!mounted || attempt != generation) return;
        if (response['status'] != 'approved') continue;
        final saved = ServerConnection(
          address: base,
          token: response['token'] as String,
          clientId: response['client_id'] as String,
        );
        await widget.client.checkConnection(saved);
        if (!mounted || attempt != generation) return;
        await widget.client.save(saved);
        if (!mounted || attempt != generation) return;
        setState(() {
          connection = saved;
          proof = null;
          code = null;
          address.text = base;
          status = 'Connected to Floe server';
        });
        await widget.client.request(
          base,
          '/pair/cancel',
          body: {'proof': pendingProof},
        );
        return;
      }
      if (mounted && attempt == generation) {
        setState(() {
          proof = null;
          code = null;
          status = 'Pairing expired. Start a new connection request.';
        });
      }
    } on Object {
      if (mounted && attempt == generation && connection == null) {
        setState(() {
          proof = null;
          code = null;
          status =
              'Pairing could not finish. Check the dashboard and try again.';
        });
      }
    }
  }

  Future<void> _cancel({bool notify = true}) async {
    generation++;
    final pendingProof = proof;
    final base = pairingAddress;
    proof = null;
    code = null;
    if (pendingProof != null && base != null) {
      try {
        await widget.client.request(
          base,
          '/pair/cancel',
          body: {'proof': pendingProof},
        );
      } on Object {
        if (mounted && notify) {
          setState(
            () => status = 'Pairing stopped. The server request will expire automatically.',
          );
        }
        return;
      }
    }
    if (mounted && notify) setState(() => status = 'Pairing cancelled');
  }

  @override
  void dispose() {
    unawaited(_cancel(notify: false));
    address.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => FloeSquircle(
    padding: const EdgeInsets.all(24),
    child: FloeLoadingOverlay(
      loading: busy,
      label: status,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const Text(
            'Remote server connection',
            style: TextStyle(fontSize: 20, fontWeight: FontWeight.w600),
          ),
          const SizedBox(height: 12),
          const Text(
            'Connect Floe to your server for assisted features. Service credentials stay on the server; app access is saved in Keychain.',
          ),
          const SizedBox(height: 20),
          FloeInput(
            key: const Key('server-address'),
            label: 'Server address',
            controller: address,
            enabled: !busy && proof == null && connection == null,
            autocorrect: false,
            enableSuggestions: false,
          ),
          const SizedBox(height: 16),
          Semantics(liveRegion: true, child: Text(status)),
          if (code != null) ...[
            const SizedBox(height: 16),
            SelectableText(
              code!,
              style: const TextStyle(
                fontSize: 28,
                fontWeight: FontWeight.w600,
                letterSpacing: 4,
              ),
            ),
            const SizedBox(height: 12),
            const FloeInfoNote(
              text: 'Approve only when this code matches the dashboard. No calendar data is sent when pairing.',
            ),
          ],
          const SizedBox(height: 16),
          Wrap(
            spacing: 10,
            runSpacing: 10,
            children: [
              if (connection == null && proof == null)
                FloeButton.filled(
                  onPressed: busy ? null : _pair,
                  child: const Text('Pair this device'),
                ),
              FloeButton.outlined(
                onPressed: busy
                    ? null
                    : () => _run(
                        () => widget.client.openDashboard(
                          pairingAddress ?? address.text,
                        ),
                      ),
                child: const Text('Open dashboard'),
              ),
              if (proof != null)
                FloeButton.text(
                  onPressed: busy ? null : _cancel,
                  child: const Text('Cancel pairing'),
                ),
              if (connection != null)
                FloeButton.outlined(
                  onPressed: busy ? null : _load,
                  child: const Text('Check connection'),
                ),
              if (proof == null)
                FloeButton.text(
                  onPressed: busy
                      ? null
                      : () => _run(() async {
                          await _cancel();
                          await widget.client.store.delete();
                          if (!mounted) return;
                          connection = null;
                          status = 'Forgot this connection. Revoke its access in the dashboard if no longer needed.';
                        }),
                  child: const Text('Forget connection'),
                ),
            ],
          ),
        ],
      ),
    ),
  );
}

String _error(String code) => switch (code) {
  'invalid_address' =>
    'Enter a local HTTP address such as http://127.0.0.1:8431.',
  'authorization_required' => 'App access was revoked or pairing expired. Forget the connection and pair again.',
  'pairing_in_progress' => 'Another pairing request is pending. Reject it in the dashboard or wait for it to expire.',
  'invalid_saved_connection' =>
    'Saved connection is invalid. Forget it and pair again.',
  _ => 'Could not reach the Floe server. Check its address and that it is running.',
};
