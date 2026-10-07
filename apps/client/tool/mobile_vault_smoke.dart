import 'dart:io';

import 'package:floe_client/app/runtime/app_runtime.dart';
import 'package:flutter/material.dart';
import 'package:path_provider/path_provider.dart';

// This entry point is an explicit, opt-in exercise of an existing disposable
// profile. It never discovers a product profile or creates profile identity.
const _exercise = bool.fromEnvironment('FLOE_VAULT_SMOKE_EXERCISE');
const _databasePath = String.fromEnvironment('FLOE_VAULT_SMOKE_DATABASE');
const _personId = String.fromEnvironment('FLOE_VAULT_SMOKE_PERSON_ID');
const _deviceId = String.fromEnvironment('FLOE_VAULT_SMOKE_DEVICE_ID');

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  var outcome = 'MOBILE_RUNTIME_SMOKE_FAILED';
  try {
    await _validateExplicitProfile();
    for (var attempt = 0; attempt < 2; attempt++) {
      final gateway = await _openProfile();
      try {
        await gateway.startRuntime();
        if (!gateway.runtimeController.ready)
          throw StateError('Runtime is not ready');
        if (attempt == 0) {
          AppRuntime? contender;
          try {
            contender = await _openProfile();
            await contender.startRuntime();
            throw StateError('A second Runtime owner was admitted');
          } on AppRuntimeException {
            // The installation lock rejects a concurrent host.
          } finally {
            await contender?.close();
          }
        }
      } finally {
        await gateway.close();
      }
    }
    outcome = 'MOBILE_RUNTIME_SMOKE_PASSED';
  } catch (error) {
    debugPrint('MOBILE_RUNTIME_SMOKE_ERROR: $error');
  }
  debugPrint(outcome);
  runApp(
    MaterialApp(
      home: Scaffold(body: Center(child: Text(outcome))),
    ),
  );
}

Future<AppRuntime> _openProfile() => AppRuntime.open(
  libraryPath: AppRuntime.resolveLibraryPath(),
  databasePath: _databasePath,
  deviceId: _deviceId,
  personId: _personId,
);

Future<void> _validateExplicitProfile() async {
  if (!_exercise ||
      _databasePath.isEmpty ||
      !_databasePath.startsWith('/') ||
      _personId == '00000000-0000-0000-0000-000000000000' ||
      !RegExp(r'^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$')
          .hasMatch(_personId) ||
      _deviceId.isEmpty ||
      _deviceId.length > 128 ||
      _deviceId.contains(RegExp(r'\s')) ||
      _deviceId.runes.any(
        (value) => value < 32 || value >= 127 && value <= 159,
      )) {
    throw StateError(
      'Explicit exercise, existing database path, Person UUID and device identity are required.',
    );
  }
  final support = await getApplicationSupportDirectory();
  final root = Directory('${support.path}/mobile-vault-smoke');
  final people = Directory('${root.path}/people');
  final person = Directory('${people.path}/$_personId');
  for (final directory in [root, people, person]) {
    if (await FileSystemEntity.type(directory.path, followLinks: false) !=
        FileSystemEntityType.directory) {
      throw StateError('The disposable diagnostic profile must already exist.');
    }
  }
  final database = File('${person.path}/floe.db');
  if (_databasePath != database.path ||
      await FileSystemEntity.type(database.path, followLinks: false) !=
          FileSystemEntityType.file ||
      await database.length() == 0) {
    throw StateError(
      'The explicit database must be the existing diagnostic-only Person profile.',
    );
  }
  final identity = File('${root.path}/local_device_id');
  if (await FileSystemEntity.type(identity.path, followLinks: false) !=
          FileSystemEntityType.file ||
      await identity.length() > 128 ||
      await identity.readAsString() != _deviceId) {
    throw StateError(
      'The existing diagnostic device identity does not match the explicit input.',
    );
  }
}
