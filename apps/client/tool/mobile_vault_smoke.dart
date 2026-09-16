import 'dart:io';

import 'package:floe_client/app/local_identity.dart';
import 'package:floe_client/app/runtime/agent_vault_gateway.dart';
import 'package:floe_client/app/runtime/app_runtime.dart';
import 'package:flutter/material.dart';
import 'package:path_provider/path_provider.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  var outcome = 'MOBILE_VAULT_SMOKE_FAILED';
  try {
    final support = await getApplicationSupportDirectory();
    final directory = Directory('${support.path}/mobile-vault-smoke');
    await directory.create(recursive: true);
    for (var attempt = 0; attempt < 2; attempt++) {
      final gateway = await AppRuntime.open(
        libraryPath: AppRuntime.resolveLibraryPath(),
        databasePath: '${directory.path}/floe.db',
        deviceId: 'mobile-vault-smoke',
      );
      try {
        final previous = await gateway.vault.vaultStatus(
          defaultLocalPersonId,
        );
        final state = previous == AgentVaultState.missing
            ? await gateway.vault.createVault(defaultLocalPersonId)
            : await gateway.vault.unlockVault(defaultLocalPersonId);
        if (state != AgentVaultState.ready) {
          throw StateError('Vault is not ready');
        }
        if (attempt == 0) {
          final contender = await AppRuntime.open(
            libraryPath: AppRuntime.resolveLibraryPath(),
            databasePath: '${directory.path}/floe.db',
            deviceId: 'mobile-vault-smoke',
          );
          try {
            var rejected = false;
            try {
              await contender.vault.unlockVault(defaultLocalPersonId);
            } on AgentVaultException catch (error) {
              rejected = error.failure == 'conflict';
            }
            if (!rejected) {
              throw StateError('A second vault owner was admitted');
            }
          } finally {
            await contender.close();
          }
        }
        await gateway.vault.lockVault(defaultLocalPersonId);
      } finally {
        await gateway.close();
      }
    }
    outcome = 'MOBILE_VAULT_SMOKE_PASSED';
  } catch (error) {
    debugPrint('MOBILE_VAULT_SMOKE_ERROR: $error');
  }
  debugPrint(outcome);
  runApp(
    MaterialApp(
      home: Scaffold(body: Center(child: Text(outcome))),
    ),
  );
}
