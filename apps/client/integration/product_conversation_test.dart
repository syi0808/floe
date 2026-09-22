import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

void main() {
  test('product Conversation uses bundled Foundation and survives reopen', () async {
    expect(
      Platform.isMacOS,
      isTrue,
      reason: 'Requires a supported Apple host.',
    );
    final ffi = File('../../target/debug/libfloe_ffi.dylib').absolute;
    expect(
      ffi.existsSync(),
      isTrue,
      reason: 'Build floe-ffi from this source.',
    );
    final temporary = await Directory.systemTemp.createTemp(
      'floe-product-host-',
    );
    addTearDown(() => temporary.delete(recursive: true));
    final bundle = '${temporary.path}/FloeProductValidation.app';
    final executable = '$bundle/Contents/MacOS/flutter_tester';
    final frameworks = '$bundle/Contents/Frameworks';
    await Directory('$bundle/Contents/MacOS').create(recursive: true);
    await Directory(frameworks).create();
    await File(Platform.resolvedExecutable).copy(executable);
    await File('../../tools/validation/product-conversation-Info.plist')
        .copy('$bundle/Contents/Info.plist');

    Future<void> checked(String command, List<String> arguments) async {
      final result = await Process.run(command, arguments);
      expect(result.exitCode, 0, reason: '${result.stdout}\n${result.stderr}');
    }

    final assets = '${temporary.path}/flutter_assets';
    await checked('flutter', [
      'build',
      'bundle',
      '--debug',
      '--no-pub',
      '--target-platform=darwin',
      '--target=integration/support/product_conversation_host.dart',
      '--asset-dir=$assets',
    ]);
    final architecture = (await Process.run('uname', [
      '-m',
    ])).stdout.toString().trim();
    final modelLibrary = '$frameworks/libfloe_local_model.dylib';
    await checked('xcrun', [
      'swiftc',
      '-emit-library',
      '-swift-version',
      '6',
      '-warnings-as-errors',
      '-target',
      '$architecture-apple-macosx12.0',
      'macos/LocalModel/LocalModel.swift',
      '-o',
      modelLibrary,
    ]);
    await checked('install_name_tool', [
      '-id',
      '@rpath/libfloe_local_model.dylib',
      modelLibrary,
    ]);
    await checked('codesign', ['--force', '--sign', '-', modelLibrary]);
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
    expect(transcript, contains('PRODUCT_CONVERSATION_PASSED'));
    expect(transcript, contains('VALIDATION_EXACT_VAULT_KEY_ABSENT'));
    expect(transcript, contains('VALIDATION_PROFILE_REMOVED'));
    stdout.write(transcript);
  }, timeout: const Timeout(Duration(minutes: 3)));
}
