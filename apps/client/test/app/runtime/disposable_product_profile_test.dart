import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

import '../../../integration/support/disposable_product_profile.dart';

void main() {
  test('unused disposable profile removes only its private files', () async {
    final profile = await DisposableProductProfile.create();
    expect(
      await File('${profile.root.path}/local_device_id').readAsString(),
      profile.deviceId,
    );
    await profile.cleanup();
    expect(await profile.root.exists(), isFalse);
  });

  test(
    'malformed Vault marker retains evidence without Keychain deletion',
    () async {
      final profile = await DisposableProductProfile.create();
      addTearDown(() => profile.root.delete(recursive: true));
      final directory = Directory(
        '${profile.databasePath}.agent-vaults/${profile.personId}',
      );
      await directory.create(recursive: true);
      await File('${directory.path}/vault.id').writeAsString('invalid');
      await expectLater(profile.cleanup(), throwsStateError);
      expect(await profile.root.exists(), isTrue);
    },
  );

  test('symlink Vault marker cannot authorize Keychain cleanup', () async {
    final profile = await DisposableProductProfile.create();
    addTearDown(() => profile.root.delete(recursive: true));
    final directory = Directory(
      '${profile.databasePath}.agent-vaults/${profile.personId}',
    );
    await directory.create(recursive: true);
    final target = File('${profile.root.path}/not-a-vault.id');
    await target.writeAsString('00000000-0000-4000-8000-000000000001');
    await Link('${directory.path}/vault.id').create(target.path);
    await expectLater(profile.cleanup(), throwsStateError);
    expect(await target.exists(), isTrue);
  });
}
