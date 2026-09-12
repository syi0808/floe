import 'dart:async';

import 'package:flutter/material.dart';

import '../../app/design_tokens.dart';
import '../../app/floe_button.dart';
import '../../app/floe_badge.dart';
import '../../app/floe_feedback.dart';
import '../../app/floe_input.dart';
import '../../app/floe_loading.dart';
import '../../app/floe_squircle.dart';
import '../agent/agent_vault_gateway.dart';
import 'local_server_client.dart';

class LocalServerPanel extends StatefulWidget {
  const LocalServerPanel({
    super.key,
    required this.client,
    this.pairingGateway,
  });
  final LocalServerClient client;
  final RemotePairingGateway? pairingGateway;
  @override
  State<LocalServerPanel> createState() => _LocalServerPanelState();
}

class _LocalServerPanelState extends State<LocalServerPanel> {
  final address = TextEditingController(text: 'http://127.0.0.1:8431');
  ServerConnection? connection;
  String? proof;
  String? code;
  String? pairingAddress;
  ServerPairingStart? pairing;
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
    } on AgentVaultException catch (error) {
      if (mounted) status = _error(error.failure);
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
    final gateway = widget.pairingGateway;
    if (gateway == null) {
      throw const AgentVaultException('vault_unavailable');
    }
    final base = LocalServerClient.normalizeAddress(address.text);
    final attempt = ++generation;
    final issuer = await gateway.prepareRemotePairing(
      personId: widget.client.personId,
    );
    final response = await widget.client.startPairingStrict(
      base,
      issuerKeyId: issuer.keyId,
      issuerPublicKey: issuer.publicKey,
    );
    if (!mounted || attempt != generation) {
      await widget.client.request(
        base,
        '/pair/cancel',
        body: {
          'schema_version': 1,
          'pairing_id': response.pairingId,
          'proof': response.proof,
        },
      );
      return;
    }
    final challenge = RemotePairingChallenge(
      schemaVersion: 1,
      pairingId: response.pairingId,
      challengeId: response.challengeId,
      challengeB64Url: response.challengeB64Url,
      producerSignature: response.producerSignature,
      producer: RemoteProducerIdentity.fromJson(response.producer),
      issuer: RemoteOwnerPublicKey.fromJson(response.issuer),
      expiresAtUnixMs: response.expiresAt.millisecondsSinceEpoch,
    );
    final route = widget.client.pairingRoute(
      address: base,
      clientId: response.pairingId,
    );
    try {
      await gateway.confirmRemotePairing(
        personId: widget.client.personId,
        route: route,
        challenge: challenge,
        pollingProof: response.proof,
      );
    } on Object {
      await widget.client.request(
        base,
        '/pair/cancel',
        body: {
          'schema_version': 1,
          'pairing_id': response.pairingId,
          'proof': response.proof,
        },
      );
      rethrow;
    }
    pairingAddress = base;
    pairing = response;
    proof = response.proof;
    code = response.code;
    status = 'Compare this code and approve in the server dashboard.';
    setState(() {});
    unawaited(_poll(attempt, base, response.proof, route, challenge));
  });

  Future<void> _poll(
    int attempt,
    String base,
    String pendingProof,
    Map<String, Object?> route,
    RemotePairingChallenge challenge,
  ) async {
    final gateway = widget.pairingGateway;
    if (gateway == null) return;
    final deadline = pairing?.expiresAt ?? DateTime.now();
    try {
      while (mounted &&
          attempt == generation &&
          DateTime.now().isBefore(deadline)) {
        await Future<void>.delayed(const Duration(seconds: 2));
        if (!mounted || attempt != generation) return;
        final response = await gateway.remotePairingStatus(
          personId: widget.client.personId,
          route: route,
          pairingId: challenge.pairingId,
          pollingProof: pendingProof,
        );
        if (!mounted || attempt != generation) return;
        if (response.status == 'rejected') {
          _finishPairing('Pairing was rejected in the server dashboard.');
          return;
        }
        if (response.status == 'expired') {
          _finishPairing('Pairing expired. Start a new connection request.');
          return;
        }
        if (response.status != 'approved') continue;
        final token = response.token;
        final clientId = response.clientId ?? response.pairingId;
        if (token == null || clientId != response.pairingId) {
          throw const ServerConnectionException('invalid_response');
        }
        final finalized = await gateway.finalizeRemotePairing(
          personId: widget.client.personId,
          route: route,
          pairingId: challenge.pairingId,
          pollingProof: pendingProof,
          challenge: challenge,
        );
        if (finalized.status != 'approved' || finalized.token == null) {
          throw const ServerConnectionException('invalid_response');
        }
        final saved = ServerConnection(
          address: base,
          token: finalized.token!,
          clientId: finalized.clientId ?? finalized.pairingId,
          personId: widget.client.personId,
          deviceId: widget.client.deviceId,
        );
        await widget.client.checkConnection(saved);
        if (!mounted || attempt != generation) return;
        await widget.client.save(saved);
        if (!mounted || attempt != generation) return;
        setState(() {
          connection = saved;
          proof = null;
          code = null;
          pairing = null;
          address.text = base;
          status = 'Connected to Floe server';
        });
        return;
      }
      if (mounted && attempt == generation) {
        _finishPairing('Pairing expired. Start a new connection request.');
      }
    } on AgentVaultException catch (error) {
      if (mounted && attempt == generation) {
        await _abortPairing(_error(error.failure));
      }
    } on Object {
      if (mounted && attempt == generation) {
        await _abortPairing(
          'Pairing could not finish. Check the dashboard and try again.',
        );
      }
    }
  }

  void _finishPairing(String message) {
    if (!mounted) return;
    setState(() {
      proof = null;
      code = null;
      pairing = null;
      status = message;
    });
  }

  Future<void> _abortPairing(String message) async {
    final base = pairingAddress;
    final pendingProof = proof;
    final pairingId = pairing?.pairingId;
    generation++;
    _finishPairing(message);
    if (base == null || pendingProof == null) return;
    try {
      await widget.client.request(
        base,
        '/pair/cancel',
        body: {
          'schema_version': 1,
          'pairing_id': pairingId,
          'proof': pendingProof,
        },
      );
    } on Object {
      // The server expires abandoned pairings even when cancellation fails.
    }
  }

  Future<void> _cancel({bool notify = true}) async {
    generation++;
    final pendingProof = proof;
    final base = pairingAddress;
    final pairingId = pairing?.pairingId;
    proof = null;
    code = null;
    pairing = null;
    if (pendingProof != null && base != null) {
      try {
        await widget.client.request(
          base,
          '/pair/cancel',
          body: {
            'schema_version': 1,
            'pairing_id': pairingId,
            'proof': pendingProof,
          },
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
  Widget build(BuildContext context) => FloeCard(
    child: FloeLoadingOverlay(
      loading: busy,
      label: status,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const Text('Remote server connection', style: FloeType.headline),
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
          Semantics(
            liveRegion: true,
            child: FloeBadge(
              label: status,
              tone: connection != null
                  ? FloeBadgeTone.success
                  : proof != null
                  ? FloeBadgeTone.info
                  : status.contains('Loading')
                  ? FloeBadgeTone.neutral
                  : status.contains('Not connected')
                  ? FloeBadgeTone.warning
                  : FloeBadgeTone.danger,
            ),
          ),
          if (code != null) ...[
            const SizedBox(height: 16),
            SelectableText(
              code!,
              style: FloeType.display.copyWith(fontSize: 28, letterSpacing: 4),
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
  'unauthorized' => 'App access was revoked or pairing expired. Forget the connection and pair again.',
  'pairing_in_progress' => 'Another pairing request is pending. Reject it in the dashboard or wait for it to expire.',
  'vault_unavailable' => 'Unlock the local vault before pairing this device.',
  'policy_denied' =>
    'The server challenge did not match this device. Start pairing again.',
  'conflict' => 'Pairing changed before it could finish. Start pairing again.',
  'credential_expired' =>
    'The server pairing expired. Start a new connection request.',
  'invalid_saved_connection' =>
    'Saved connection is invalid. Forget it and pair again.',
  _ => 'Could not reach the Floe server. Check its address and that it is running.',
};
