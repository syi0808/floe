import 'dart:io';

import 'package:floe_client/app/local_identity.dart';
import 'package:floe_client/features/agent/agent_vault_gateway.dart';
import 'package:floe_client/features/day_canvas/application/ffi_day_gateway.dart';
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
      final gateway = await FfiDayGateway.open(
        libraryPath: FfiDayGateway.resolveLibraryPath(),
        databasePath: '${directory.path}/floe.db',
        deviceId: 'mobile-vault-smoke',
      );
      try {
        final previous = await gateway.secureAgent.vaultStatus(
          defaultLocalPersonId,
        );
        final state = previous == AgentVaultState.missing
            ? await gateway.secureAgent.createVault(defaultLocalPersonId)
            : await gateway.secureAgent.unlockVault(defaultLocalPersonId);
        if (state != AgentVaultState.ready) {
          throw StateError('Vault is not ready');
        }
        if (attempt == 0) {
          final contender = await FfiDayGateway.open(
            libraryPath: FfiDayGateway.resolveLibraryPath(),
            databasePath: '${directory.path}/floe.db',
            deviceId: 'mobile-vault-smoke',
          );
          try {
            var rejected = false;
            try {
              await contender.secureAgent.unlockVault(defaultLocalPersonId);
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
        await gateway.secureAgent.lockVault(defaultLocalPersonId);
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
