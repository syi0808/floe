import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:path_provider/path_provider.dart';

const String debugSupportDirectoryEnvironmentVariable =
    'FLOE_DEBUG_SUPPORT_DIRECTORY';

@visibleForTesting
enum ClientBuildMode { debug, profile, release }

final class InvalidDebugSupportDirectoryOverride implements Exception {
  const InvalidDebugSupportDirectoryOverride(this.reason);

  final String reason;

  @override
  String toString() =>
      'Invalid $debugSupportDirectoryEnvironmentVariable: $reason';
}

/// Resolves the client's support root without opening or migrating a profile.
///
/// The injected mode, environment, and provider are only a test seam. Production
/// callers use [resolveClientSupportDirectory], which takes its mode from the
/// compile-time Flutter flags and its environment from the current process.
@visibleForTesting
Future<Directory> resolveClientSupportDirectoryForTesting({
  required ClientBuildMode buildMode,
  required Map<String, String> environment,
  required Future<Directory> Function() platformDefaultProvider,
}) async {
  final overridePath = _debugSupportDirectoryOverride(
    buildMode: buildMode,
    environment: environment,
  );
  if (overridePath != null) return Directory(overridePath);
  return platformDefaultProvider();
}

String? _debugSupportDirectoryOverride({
  required ClientBuildMode buildMode,
  required Map<String, String> environment,
}) {
  if (buildMode != ClientBuildMode.debug ||
      !environment.containsKey(debugSupportDirectoryEnvironmentVariable)) {
    return null;
  }

  final override = environment[debugSupportDirectoryEnvironmentVariable]!;
  if (override.trim().isEmpty) {
    throw const InvalidDebugSupportDirectoryOverride(
      'the value must be a non-empty absolute path',
    );
  }
  if (override.contains('\u0000')) {
    throw const InvalidDebugSupportDirectoryOverride(
      'the path must not contain a NUL character',
    );
  }
  final isAbsolute = Platform.isWindows
      ? RegExp(r'^[A-Za-z]:[\\/]').hasMatch(override) ||
            override.startsWith(r'\\')
      : override.startsWith(Platform.pathSeparator);
  if (!isAbsolute) {
    throw const InvalidDebugSupportDirectoryOverride(
      'the value must be an absolute path',
    );
  }
  if (override.split(RegExp(r'[\\/]')).contains('..')) {
    throw const InvalidDebugSupportDirectoryOverride(
      'the path must not contain a parent-directory component',
    );
  }

  return override;
}

/// Shared platform resolution for client storage and persistent diagnostics.
Future<Directory> resolveClientSupportDirectory() =>
    resolveClientSupportDirectoryForTesting(
      buildMode: kDebugMode
          ? ClientBuildMode.debug
          : kProfileMode
          ? ClientBuildMode.profile
          : ClientBuildMode.release,
      environment: Platform.environment,
      platformDefaultProvider: getApplicationSupportDirectory,
    );
