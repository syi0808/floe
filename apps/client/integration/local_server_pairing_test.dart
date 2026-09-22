import 'dart:convert';
import 'dart:io';

import 'package:floe_client/features/connections/application/local_server_client.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/features/connections/application/remote_pairing_gateway.dart';
import 'package:floe_client/features/connections/domain/remote_owner_models.dart';

import '../test/support/server_credentials.dart';
import 'support/disposable_product_profile.dart';

void main() {
  test('strict server pairing uses native owner envelopes and memory-only credential persistence', () async {
    final library = File('../../target/debug/libfloe_ffi.dylib').absolute;
    if (!Platform.isMacOS || !library.existsSync()) {
      markTestSkipped(
        'Build floe-ffi on macOS before running this native integration.',
      );
      return;
    }
    final profile = await DisposableProductProfile.create();
    addTearDown(profile.cleanup);
    final temporary = profile.root;
    final personId = profile.personId;
    final deviceId = profile.deviceId;
    final runtime = await profile.open(library.path);
    await runtime.vault.createVault(personId);
    await profile.recordVault();
    final pairing = NativeRemotePairingGateway(
      runtime.remotePairingV2,
      expectedPersonId: personId,
      expectedDeviceId: deviceId,
    );
    final socket = await ServerSocket.bind(InternetAddress.loopbackIPv4, 0);
    final port = socket.port;
    await socket.close();
    final address = 'http://127.0.0.1:$port';
    final binary = '${temporary.path}/floe-server';
    final environmentFile = File('${temporary.path}/server.env');
    await environmentFile.writeAsString('');
    final build = await Process.run('go', [
      'build',
      '-o',
      binary,
      './cmd/floe-server',
    ], workingDirectory: '../../server');
    expect(build.exitCode, 0, reason: build.stderr.toString());
    final process = await Process.start(
      binary,
      [],
      environment: {
        'FLOE_SERVER_DATA': '${temporary.path}/node',
        'FLOE_SERVER_ADDRESS': '127.0.0.1:$port',
        'FLOE_INFERENCE_CONFIG': '',
        'FLOE_ENV_FILE': environmentFile.path,
        'FLOE_GITHUB_OAUTH_CLIENT_ID': '',
        'FLOE_SLACK_OAUTH_CLIENT_ID': '',
        'FLOE_GOOGLE_OAUTH_CLIENT_ID': '',
        'FLOE_MICROSOFT_OAUTH_CLIENT_ID': '',
      },
    );
    final http = HttpClient()..findProxy = (_) => 'DIRECT';
    addTearDown(() async {
      http.close(force: true);
      process.kill(ProcessSignal.sigterm);
      await process.exitCode.timeout(const Duration(seconds: 10));
    });
    await process.stderr
        .transform(utf8.decoder)
        .transform(const LineSplitter())
        .firstWhere((line) => line.contains('listening'))
        .timeout(const Duration(seconds: 10));
    Cookie? cookie;
    var csrf = '';
    Future<Map<String, dynamic>> manage(
      String path, [
      Map<String, Object?>? body,
    ]) async {
      final request = await http.openUrl(
        body == null ? 'GET' : 'POST',
        Uri.parse('$address/manage/api/$path'),
      );
      request.headers.contentType = ContentType.json;
      request.headers.set('Origin', address);
      request.headers.set('X-Floe-CSRF', csrf);
      if (cookie != null) request.cookies.add(cookie!);
      if (body != null) request.write(jsonEncode(body));
      final response = await request.close();
      expect(response.statusCode, 200);
      if (response.cookies.isNotEmpty) cookie = response.cookies.first;
      return jsonDecode(await utf8.decoder.bind(response).join())
          as Map<String, dynamic>;
    }

    final admin = await File('${temporary.path}/node/admin-token')
        .readAsString();
    await manage('login', {'token': admin});
    csrf = (await manage('state'))['csrf'] as String;
    final store = MemoryServerCredentials();
    final client = LocalServerClient(
      store: store,
      personId: personId,
      deviceId: deviceId,
    );
    final owner = await pairing.prepareRemotePairing();
    final pair = await client.startPairingStrict(
      address,
      issuerKeyId: owner.keyId,
      issuerPublicKey: owner.publicKey,
    );
    final challenge = RemotePairingChallenge(
      schemaVersion: 1,
      pairingId: pair.pairingId,
      challengeId: pair.challengeId,
      challengeB64Url: pair.challengeB64Url,
      producerSignature: pair.producerSignature,
      producer: RemoteProducerIdentity.fromJson(pair.producer),
      issuer: RemoteOwnerPublicKey.fromJson(pair.issuer),
      expiresAtUnixMs: pair.expiresAt.millisecondsSinceEpoch,
    );
    final target = PairingTarget(address);
    final confirmed = await pairing.confirmRemotePairing(
      target: target,
      challenge: challenge,
      pollingProof: pair.proof,
    );
    expect(confirmed.status, 'local_confirmed');
    expect(confirmed.token, isNull);
    final pending = await pairing.remotePairingStatus(
      target: target,
      pairingId: pair.pairingId,
      pollingProof: pair.proof,
    );
    expect(pending.status, 'local_confirmed');
    expect(pending.token, isNull);
    await manage('pair/approve', {
      'schema_version': 1,
      'pairing_id': pair.pairingId,
      'issuer_fingerprint': owner.fingerprint,
    });
    final approved = await pairing.finalizeRemotePairing(
      target: target,
      pairingId: pair.pairingId,
      pollingProof: pair.proof,
      challenge: challenge,
    );
    expect(approved.status, 'approved');
    final connection = ServerConnection(
      address: address,
      token: approved.token!,
      clientId: approved.clientId!,
      personId: approved.personId,
      deviceId: approved.deviceId,
    );
    await client.save(connection);
    final saved = await client.connection();
    expect(
      saved != null &&
          jsonEncode(saved.toJson()) == jsonEncode(connection.toJson()),
      isTrue,
      reason: 'The exact approved connection must be reread before release.',
    );
    await pairing.releaseApprovedPairing(pair.pairingId);
    await client.checkConnection(connection);
    final purposes = await client.purposes(connection);
    expect(purposes.keys, InferencePurpose.values);
    expect(purposes.values.every((purpose) => !purpose.available), true);
    expect(await client.privacyActivity(connection), isEmpty);
    await manage('client/delete', {'id': connection.clientId});
    await expectLater(
      client.checkConnection(connection),
      throwsA(
        isA<ServerConnectionException>().having(
          (error) => error.code,
          'code',
          'unauthorized',
        ),
      ),
    );
  }, timeout: const Timeout(Duration(minutes: 2)));
}
