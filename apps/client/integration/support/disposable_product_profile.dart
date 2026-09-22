import 'dart:io';

import 'package:floe_client/app/runtime/app_runtime.dart';
import 'package:floe_client/features/conversation/application/agent_request_id.dart';

final class DisposableProductProfile {
  DisposableProductProfile._(this.root, this.personId, this.deviceId);

  final Directory root;
  final String personId;
  final String deviceId;
  AppRuntime? _runtime;
  String? _vaultId;

  String get databasePath => '${root.path}/people/$personId/floe.db';
  String get _vaultDirectory => '$databasePath.agent-vaults/$personId';

  static Future<DisposableProductProfile> create() async {
    final root = await Directory.systemTemp.createTemp(
      'floe-product-validation-',
    );
    final profile = DisposableProductProfile._(
      root,
      newAgentRequestId(),
      'validation-${newAgentRequestId()}',
    );
    await _checkedProcess('/bin/chmod', ['700', root.path]);
    await Directory('${root.path}/people/${profile.personId}')
        .create(recursive: true);
    await File('${root.path}/local_device_id').writeAsString(profile.deviceId);
    return profile;
  }

  Future<AppRuntime> open(String libraryPath) async {
    if (_runtime != null) throw StateError('Validation runtime already open.');
    return _runtime = await AppRuntime.open(
      libraryPath: libraryPath,
      databasePath: databasePath,
      deviceId: deviceId,
    );
  }

  Future<void> recordVault() async {
    final marker = File('$_vaultDirectory/vault.id');
    for (final path in [
      root.path,
      '${root.path}/people',
      '${root.path}/people/$personId',
      '$databasePath.agent-vaults',
      _vaultDirectory,
    ]) {
      if (await FileSystemEntity.type(path, followLinks: false) !=
          FileSystemEntityType.directory) {
        throw StateError('Not an owned validation directory: $path');
      }
    }
    if (await FileSystemEntity.type(marker.path, followLinks: false) !=
        FileSystemEntityType.file) {
      throw StateError('Not an owned validation Vault marker: ${marker.path}');
    }
    final vaultId = await marker.readAsString();
    if (!RegExp(
          r'^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$',
        ).hasMatch(vaultId) ||
        (_vaultId != null && _vaultId != vaultId)) {
      throw StateError(
        'Validation Vault identity changed; retain ${root.path}',
      );
    }
    _vaultId = vaultId;
  }

  Future<void> closeRuntime() async {
    await _runtime?.close();
    _runtime = null;
  }

  Future<void> cleanup() async {
    try {
      await closeRuntime();
      if (await Directory(_vaultDirectory).exists()) {
        await recordVault();
        final account = '$personId/$_vaultId';
        final arguments = ['-s', 'com.floe.agent-vault.v1', '-a', account];
        final deleted = await Process.run('/usr/bin/security', [
          'delete-generic-password',
          ...arguments,
        ]);
        if (deleted.exitCode != 0 && deleted.exitCode != 44) {
          throw StateError('Exact Keychain deletion failed for $account');
        }
        final absent = await Process.run('/usr/bin/security', [
          'find-generic-password',
          ...arguments,
        ]);
        if (absent.exitCode != 44) {
          throw StateError('Exact Keychain absence not verified for $account');
        }
        stdout.writeln('VALIDATION_EXACT_VAULT_KEY_ABSENT');
      } else if (_vaultId != null) {
        throw StateError('Recorded validation Vault marker disappeared.');
      }
      await root.delete(recursive: true);
      if (await root.exists()) throw StateError('Validation files remain.');
      stdout.writeln('VALIDATION_PROFILE_REMOVED');
    } on Object catch (error) {
      throw StateError(
        '$error; retained ${root.path}; inspect its exact $personId/vault.id '
        'marker before any service/account cleanup. Never reset shared data.',
      );
    }
  }
}

Future<void> _checkedProcess(String executable, List<String> arguments) async {
  final result = await Process.run(executable, arguments);
  if (result.exitCode != 0) throw StateError('$executable failed.');
}
