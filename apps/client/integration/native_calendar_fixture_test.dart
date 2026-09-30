import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

void main() {
  test('Calendar FFI scenarios use the bundled native subject fixture', () async {
    expect(Platform.isMacOS, isTrue, reason: 'Requires a macOS host.');
    final ffi = File('../../target/debug/libfloe_ffi.dylib').absolute;
    expect(
      ffi.existsSync(),
      isTrue,
      reason: 'Build floe-ffi from this source.',
    );
    final temporary = await Directory.systemTemp.createTemp(
      'floe-calendar-fixture-',
    );
    addTearDown(() => temporary.delete(recursive: true));
    final bundle = '${temporary.path}/FloeCalendarFixtureValidation.app';
    final executable = '$bundle/Contents/MacOS/flutter_tester';
    final frameworks = '$bundle/Contents/Frameworks';
    await Directory('$bundle/Contents/MacOS').create(recursive: true);
    await Directory(frameworks).create();
    await File(Platform.resolvedExecutable).copy(executable);
    await File('../../tools/validation/native-calendar-fixture-Info.plist')
        .copy('$bundle/Contents/Info.plist');

    Future<void> checked(String command, List<String> arguments) async {
      final result = await Process.run(command, arguments);
      expect(result.exitCode, 0, reason: '${result.stdout}\n${result.stderr}');
    }

    await checked('/bin/chmod', ['700', temporary.path]);
    final assets = '${temporary.path}/flutter_assets';
    await checked('flutter', [
      'build',
      'bundle',
      '--debug',
      '--no-pub',
      '--target-platform=darwin',
      '--target=integration/support/native_calendar_fixture_host.dart',
      '--asset-dir=$assets',
    ]);
    final architecture = (await Process.run('uname', [
      '-m',
    ])).stdout.toString().trim();
    final nativeLibrary = '$frameworks/libfloe_eventkit.dylib';
    await checked('xcrun', [
      'swiftc',
      '-emit-library',
      '-warnings-as-errors',
      '-target',
      '$architecture-apple-macosx12.0',
      '../../crates/adapters/providers/tests/fixtures/NativeCalendarFixture.swift',
      '-o',
      nativeLibrary,
    ]);
    await checked('install_name_tool', [
      '-id',
      '@rpath/libfloe_eventkit.dylib',
      nativeLibrary,
    ]);
    await checked('codesign', ['--force', '--sign', '-', nativeLibrary]);
    await checked('codesign', ['--force', '--sign', '-', bundle]);
    await checked('codesign', ['--verify', '--deep', '--strict', bundle]);
    final process = await Process.start(
      executable,
      [
        '--disable-vm-service',
        '--enable-software-rendering',
        '--non-interactive',
        '--flutter-assets-dir=$assets',
        '--icu-data-file-path=${File(Platform.resolvedExecutable).parent.path}/icudtl.dat',
        '--packages=${File('.dart_tool/package_config.json').absolute.path}',
        '$assets/kernel_blob.bin',
      ],
      environment: {'FLOE_VALIDATION_FFI': ffi.path},
    );
    addTearDown(() async {
      process.kill(ProcessSignal.sigterm);
      await process.exitCode;
    });
    final output = process.stdout
        .transform(const SystemEncoding().decoder)
        .join();
    final errors = process.stderr
        .transform(const SystemEncoding().decoder)
        .join();
    final result = await process.exitCode;
    final transcript = await output;
    expect(result, 0, reason: '$transcript\n${await errors}');
    for (final sentinel in [
      'DAY_MIRROR_SOURCE_CONTINUITY_PASSED',
      'ALL_AVAILABLE_SOURCE_AUTHORITY_PASSED',
      'WIDE_SOURCE_BOUNDED_PUBLICATION_PASSED',
    ]) {
      expect(transcript, contains(sentinel));
    }
    expect('VALIDATION_PROFILE_REMOVED'.allMatches(transcript), hasLength(3));
    expect(
      await File('$bundle/Contents/MacOS/creates.txt').exists(),
      isFalse,
      reason: 'No native Calendar writes are authorized by these scenarios.',
    );
    stdout.write(transcript);
  }, timeout: const Timeout(Duration(minutes: 5)));
}
