import 'dart:async';

import 'package:flutter/material.dart';

import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_button.dart';
import 'package:floe_client/app/floe_badge.dart';
import 'package:floe_client/app/floe_feedback.dart';
import 'package:floe_client/app/floe_input.dart';
import 'package:floe_client/app/floe_loading.dart';
import 'package:floe_client/app/floe_squircle.dart';
import 'package:floe_client/app/runtime/agent_vault_gateway.dart';
import 'package:floe_client/features/connections/application/local_server_client.dart';
import 'package:floe_client/features/connections/application/remote_pairing_gateway.dart';
import 'package:floe_client/features/connections/application/remote_owner_operation.dart';
import 'package:floe_client/features/connections/domain/remote_owner_models.dart';
import 'package:floe_client/features/connections/presentation/server_connector_panel.dart'
    show connectorErrorMessage;

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
  Future<void> Function()? retryPairing;
  Future<void>? pairingDrive;
  int drivingGeneration = 0;

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
    final issuer = await gateway.prepareRemotePairing();
    if (!mounted || attempt != generation) return;
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
    final target = PairingTarget(base);
    pairingAddress = base;
    pairing = response;
    proof = response.proof;
    code = response.code;
    var confirmed = false;
    retryPairing = () {
      if (drivingGeneration == attempt && pairingDrive != null) {
        return pairingDrive!;
      }
      drivingGeneration = attempt;
      final pending = _drive(
        attempt,
        target,
        response.proof,
        challenge,
        confirm: () async {
          if (confirmed) return;
          await gateway.confirmRemotePairing(
            target: target,
            challenge: challenge,
            pollingProof: response.proof,
          );
          confirmed = true;
        },
      );
      pairingDrive = pending;
      return pending.whenComplete(() {
        if (drivingGeneration == attempt) pairingDrive = null;
      });
    };
    status = 'Compare this code and approve in the server dashboard.';
    setState(() {});
    unawaited(retryPairing!());
  });

  Future<void> _drive(
    int attempt,
    PairingTarget target,
    String pendingProof,
    RemotePairingChallenge challenge, {
    required Future<void> Function() confirm,
  }) async {
    final gateway = widget.pairingGateway;
    if (gateway == null) return;
    bool current() => mounted && attempt == generation;
    try {
      await confirm();
      for (var observation = 0; observation < 240 && current(); observation++) {
        final report = await gateway.remotePairingStatus(
          target: target,
          pairingId: challenge.pairingId,
          pollingProof: pendingProof,
        );
        if (!current()) return;
        if (report.pairingId != challenge.pairingId ||
            report.personId != widget.client.personId ||
            report.deviceId != widget.client.deviceId) {
          throw const ServerConnectionException('connection_identity_mismatch');
        }
        if (report.status == 'approved') {
          final finalized = await gateway.finalizeRemotePairing(
            target: target,
            pairingId: challenge.pairingId,
            pollingProof: pendingProof,
            challenge: challenge,
          );
          if (!current()) return;
          if (finalized.status != 'approved' ||
              finalized.token == null ||
              finalized.clientId != challenge.pairingId ||
              finalized.pairingId != challenge.pairingId ||
              finalized.personId != widget.client.personId ||
              finalized.deviceId != widget.client.deviceId) {
            throw const ServerConnectionException(
              'connection_identity_mismatch',
            );
          }
          final saved = ServerConnection(
            address: target.baseUrl,
            token: finalized.token!,
            clientId: finalized.clientId!,
            personId: finalized.personId,
            deviceId: finalized.deviceId,
          );
          await widget.client.save(saved);
          final persisted = await widget.client.connection();
          if (persisted == null ||
              persisted.address != saved.address ||
              persisted.token != saved.token ||
              persisted.clientId != saved.clientId ||
              persisted.personId != saved.personId ||
              persisted.deviceId != saved.deviceId) {
            throw const ServerConnectionException('invalid_saved_connection');
          }
          await widget.client.checkConnection(persisted);
          await gateway.releaseApprovedPairing(challenge.pairingId);
          if (!current()) return;
          setState(() {
            connection = persisted;
            proof = null;
            code = null;
            pairing = null;
            retryPairing = null;
            address.text = target.baseUrl;
            status = 'Connected to Floe server';
          });
          return;
        }
        if (!{'pending', 'local_confirmed'}.contains(report.status)) {
          _finishPairing(switch (report.status) {
            'rejected' => 'Pairing was rejected in the server dashboard.',
            'repair_required' =>
              'The server asked to pair again. Start a new request.',
            _ => 'Pairing expired. Start a new connection request.',
          });
          return;
        }
        await Future<void>.delayed(const Duration(seconds: 1));
      }
      if (current()) {
        setState(
          () =>
              status = 'Pairing observation timed out. Retry the same request.',
        );
      }
    } on RemoteOperationPending {
      if (current()) {
        setState(
          () => status =
              'Pairing result needs reconciliation. Retry the same request.',
        );
      }
    } on AgentVaultException catch (failure) {
      if (current()) setState(() => status = _error(failure.failure));
    } on ServerConnectionException catch (failure) {
      if (current()) {
        setState(() => status = connectorErrorMessage(failure.code));
      }
    } on Object {
      if (current()) {
        setState(
          () => status = 'Pairing is not saved yet. Retry the same request.',
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
      retryPairing = null;
      status = message;
    });
  }

  Future<void> _cancel({bool notify = true}) async {
    generation++;
    retryPairing = null;
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
          if (retryPairing != null)
            FloeButton.text(
              key: const Key('server-pairing-retry'),
              onPressed: busy ? null : () => _run(retryPairing!),
              child: const Text('Retry pairing result'),
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
