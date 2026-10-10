import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/infrastructure/platform/client_support_directory.dart';

void main() {
  final platformDefaultPath = Platform.isWindows
      ? r'C:\platform\application-support'
      : '/platform/application-support';
  final platformDefault = Directory(platformDefaultPath);
  final absoluteOverride = Platform.isWindows
      ? r'C:\floe qa run'
      : '/tmp/floe qa run';
  final parentTraversal = Platform.isWindows
      ? r'C:\tmp\qa\..\existing'
      : '/tmp/qa/../existing';

  group('resolveClientSupportDirectoryForTesting', () {
    test(
      'uses the absolute Debug override without resolving the default',
      () async {
        var providerCalls = 0;

        final result = await resolveClientSupportDirectoryForTesting(
          buildMode: ClientBuildMode.debug,
          environment: {
            debugSupportDirectoryEnvironmentVariable: absoluteOverride,
          },
          platformDefaultProvider: () async {
            providerCalls++;
            return platformDefault;
          },
        );

        expect(result.path, absoluteOverride);
        expect(providerCalls, 0);
      },
    );

    test('uses the platform path once when Debug has no override', () async {
      var providerCalls = 0;

      final result = await resolveClientSupportDirectoryForTesting(
        buildMode: ClientBuildMode.debug,
        environment: const {},
        platformDefaultProvider: () async {
          providerCalls++;
          return platformDefault;
        },
      );

      expect(result.path, platformDefault.path);
      expect(providerCalls, 1);
    });

    test(
      'rejects an empty Debug override before resolving the default',
      () async {
        var providerCalls = 0;

        await expectLater(
          resolveClientSupportDirectoryForTesting(
            buildMode: ClientBuildMode.debug,
            environment: const {debugSupportDirectoryEnvironmentVariable: ''},
            platformDefaultProvider: () async {
              providerCalls++;
              return platformDefault;
            },
          ),
          throwsA(isA<InvalidDebugSupportDirectoryOverride>()),
        );

        expect(providerCalls, 0);
      },
    );

    test(
      'rejects a relative Debug override before resolving the default',
      () async {
        var providerCalls = 0;

        await expectLater(
          resolveClientSupportDirectoryForTesting(
            buildMode: ClientBuildMode.debug,
            environment: const {
              debugSupportDirectoryEnvironmentVariable: 'qa/fresh-profile',
            },
            platformDefaultProvider: () async {
              providerCalls++;
              return platformDefault;
            },
          ),
          throwsA(isA<InvalidDebugSupportDirectoryOverride>()),
        );

        expect(providerCalls, 0);
      },
    );

    test(
      'rejects parent-directory traversal before resolving the default',
      () async {
        var providerCalls = 0;

        await expectLater(
          resolveClientSupportDirectoryForTesting(
            buildMode: ClientBuildMode.debug,
            environment: {
              debugSupportDirectoryEnvironmentVariable: parentTraversal,
            },
            platformDefaultProvider: () async {
              providerCalls++;
              return platformDefault;
            },
          ),
          throwsA(isA<InvalidDebugSupportDirectoryOverride>()),
        );

        expect(providerCalls, 0);
      },
    );

    for (final buildMode in [
      ClientBuildMode.profile,
      ClientBuildMode.release,
    ]) {
      test(
        '${buildMode.name} ignores the override and resolves the default once',
        () async {
          var providerCalls = 0;

          final result = await resolveClientSupportDirectoryForTesting(
            buildMode: buildMode,
            environment: const {
              debugSupportDirectoryEnvironmentVariable: 'not/an/absolute/path',
            },
            platformDefaultProvider: () async {
              providerCalls++;
              return platformDefault;
            },
          );

          expect(result.path, platformDefault.path);
          expect(providerCalls, 1);
        },
      );
    }
  });
}
