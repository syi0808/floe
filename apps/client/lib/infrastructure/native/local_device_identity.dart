import 'dart:io';

import 'package:path_provider/path_provider.dart';

final class LocalDeviceIdentity {
  const LocalDeviceIdentity(this.id);

  final String id;

  static Future<LocalDeviceIdentity> openExisting() async {
    final supportDirectory = await getApplicationSupportDirectory();
    final file = File('${supportDirectory.path}/local_device_id');
    final type = await FileSystemEntity.type(file.path, followLinks: false);
    if (type == FileSystemEntityType.file && await file.length() <= 512) {
      final existing = (await file.readAsString()).trim();
      if (_validIdentifier(existing)) return LocalDeviceIdentity(existing);
      throw const FormatException(
        'The existing local device identity is invalid.',
      );
    }
    if (type != FileSystemEntityType.notFound)
      throw const FormatException(
        'The existing device identity is unreadable or invalid.',
      );
    throw const FormatException(
      'The existing device identity is missing. Profile setup is required.',
    );
  }
}

bool _validIdentifier(String value) =>
    value.isNotEmpty && value.length <= 128 && !value.contains(RegExp(r'\s'));
